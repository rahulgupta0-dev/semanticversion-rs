//! `Version` struct — parsing, display, coercion, and computed keys.
//!
//! ## Python mapping
//!
//! All logic faithfully mirrors `base.py::Version` @ commit `2cbbee3`.
//!
//! Key design decisions (see DECISIONS.md):
//! - D01: major/minor/patch stored as `u64` (Python uses arbitrary-precision int).
//! - D02: `Option<u64>` for minor/patch in partial versions (Python uses `None`).
//! - D06: NO `Ord`/`PartialOrd` trait on `Version` — see below.
//! - D09: `coerce()` ported directly; regex + string logic from base.py:225–302.
//! - D10: Error messages use single-quoted strings matching Python's ValueError format.
//! - D11: `PreReleaseIdent::Max` sentinel for no-prerelease ordering.
//!
//! ## CRITICAL: Why we do NOT impl Ord/PartialOrd on Version
//!
//! Python's `__eq__` includes build metadata (two versions with different builds are NOT equal).
//! Python's `__lt__`/`__gt__` use `_cmp_precedence_key` which EXCLUDES build metadata.
//! This means: `1.0.0+a != 1.0.0+b` but `NOT (1.0.0+a < 1.0.0+b)` and `NOT (1.0.0+a > 1.0.0+b)`.
//!
//! PyO3 rich-compare MUST return definite bools per this table (lt=false, le=true, gt=false, ge=true for build-diff).
//! NOT Python's NotImplemented — the original returns plain bools.
//! NotImplemented is only for cross-type operands (Version vs non-Version).
//!
//! Rust's `Ord` trait contract requires: `a == b ↔ a.cmp(b) == Equal`.
//! Our semantics VIOLATE this: two versions can be `ne` but have `Equal` precedence.
//! Therefore: **we implement `Ord`/`PartialOrd` only on the key types**, not on `Version` itself.
//! Version comparison is done via `.cmp_precedence_key()` directly (like Python's `__lt__`).
//!
//! Ground truth (confirmed 2026-08-01):
//!   `1.0.0+a == 1.0.0+b`  → False   (build included in __eq__)
//!   `1.0.0+a <  1.0.0+b`  → False   (same precedence key)
//!   `1.0.0+a <= 1.0.0+b`  → True    (same precedence key, <= is True)
//!   `1.0.0+a >  1.0.0+b`  → False   (same precedence key)
//!   `__lt__` returns bool `False`, never `NotImplemented` for same-type comparison.
//!   `hash(1.0.0+a) != hash(1.0.0+b)` (build IS included in hash).
//!
//! ## Regex definitions (base.py:81–82)
//!
//! Full version:
//!   `^(\d+)\.(\d+)\.(\d+)(?:-([0-9a-zA-Z.-]+))?(?:\+([0-9a-zA-Z.-]+))?$`
//!
//! Partial version:
//!   `^(\d+)(?:\.(\d+)(?:\.(\d+))?)?(?:-([0-9a-zA-Z.-]*))?(?:\+([0-9a-zA-Z.-]*))?$`
//!
//! Coerce base:
//!   `^\d+(?:\.\d+(?:\.\d+)?)?`

use std::fmt;
use std::hash::{Hash, Hasher};

use regex::Regex;

use crate::error::SemverError;
use crate::identifiers::{BuildIdent, PreReleaseIdent};

// ---------------------------------------------------------------------------
// Regex: compiled once via std::sync::OnceLock (stable since 1.70)
// ---------------------------------------------------------------------------

fn version_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(\d+)\.(\d+)\.(\d+)(?:-([0-9a-zA-Z.-]+))?(?:\+([0-9a-zA-Z.-]+))?$")
            .expect("version regex is valid — only panics if regex source is wrong, not user input")
    })
}

fn partial_version_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(\d+)(?:\.(\d+)(?:\.(\d+))?)?(?:-([0-9a-zA-Z.-]*))?(?:\+([0-9a-zA-Z.-]*))?$",
        )
        .expect("partial version regex is valid — only panics if regex source is wrong, not user input")
    })
}

fn coerce_base_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\d+(?:\.\d+(?:\.\d+)?)?")
            .expect("coerce base regex is valid — only panics if regex source is wrong, not user input")
    })
}

// ---------------------------------------------------------------------------
// Precedence key types
// ---------------------------------------------------------------------------

/// The key used for SemVer *precedence* comparison (ignores build metadata).
/// Mirrors Python's `_cmp_precedence_key` (base.py:123).
/// `[Max]` when there is no prerelease (so release > any prerelease).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PrecedenceKey {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub prerelease: Vec<PreReleaseIdent>,
}

/// The key used for stable *sorting* (includes build metadata).
/// Mirrors Python's `_sort_precedence_key` (base.py:125).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SortKey {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub prerelease: Vec<PreReleaseIdent>,
    pub build: Vec<BuildIdent>,
}

// ---------------------------------------------------------------------------
// Version struct
// ---------------------------------------------------------------------------

/// A parsed Semantic Version, faithful to `base.py::Version`.
///
/// ### Equality vs ordering
///
/// `PartialEq`/`Eq` includes build metadata — two versions with different builds are **not equal**.
/// `cmp_precedence_key()` excludes build metadata — versions differing only in build have the
/// **same precedence** (neither is less-than nor greater-than the other).
///
/// We do **not** implement `Ord`/`PartialOrd` on `Version` directly because that would
/// violate Rust's contract (`a == b ↔ a.cmp(b) == Equal`).
/// Use `v.cmp_precedence_key()` to compare versions by SemVer precedence.
#[derive(Debug, Clone)]
pub struct Version {
    pub major: u64,
    pub minor: Option<u64>,
    pub patch: Option<u64>,
    /// `None` = not specified (partial only).
    /// `Some([])` = no prerelease identifiers.
    pub prerelease: Option<Vec<PreReleaseIdent>>,
    /// `None` = not specified (partial only).
    /// `Some([])` = no build metadata.
    pub build: Option<Vec<BuildIdent>>,
    /// Whether this was parsed as a partial version (deprecated in Python 3.0).
    pub partial: bool,
}

impl Version {
    // -----------------------------------------------------------------------
    // Constructor from string
    // -----------------------------------------------------------------------

    /// Parse a standard version string.
    ///
    /// Mirrors `base.py::Version.__init__` → `Version.parse(version_string, partial=False)`.
    pub fn parse(s: &str) -> Result<Self, SemverError> {
        Self::parse_inner(s, false)
    }

    /// Parse a partial version string (deprecated feature, kept for test parity).
    ///
    /// Mirrors `base.py::Version.parse(version_string, partial=True)`.
    pub fn parse_partial(s: &str) -> Result<Self, SemverError> {
        Self::parse_inner(s, true)
    }

    fn parse_inner(s: &str, partial: bool) -> Result<Self, SemverError> {
        if s.is_empty() {
            return Err(SemverError::empty_version(s));
        }

        let re = if partial { partial_version_re() } else { version_re() };
        let caps = re.captures(s).ok_or_else(|| SemverError::invalid_version(s))?;

        let major_s = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let minor_s = caps.get(2).map(|m| m.as_str());
        let patch_s = caps.get(3).map(|m| m.as_str());
        let prerel_s = caps.get(4).map(|m| m.as_str());
        let build_s  = caps.get(5).map(|m| m.as_str());

        // Leading-zero checks (base.py:329–334) — granular error messages
        if has_leading_zero(major_s) {
            return Err(SemverError::leading_zero_major(s));
        }
        if minor_s.map(has_leading_zero).unwrap_or(false) {
            return Err(SemverError::leading_zero_minor(s));
        }
        if patch_s.map(has_leading_zero).unwrap_or(false) {
            return Err(SemverError::leading_zero_patch(s));
        }

        let major = major_s.parse::<u64>().map_err(|_| SemverError::invalid_version(s))?;
        let minor = minor_s
            .map(|v| v.parse::<u64>().map_err(|_| SemverError::invalid_version(s)))
            .transpose()?;
        let patch = patch_s
            .map(|v| v.parse::<u64>().map_err(|_| SemverError::invalid_version(s)))
            .transpose()?;

        // Prerelease parsing (base.py:340–350)
        let prerelease = match prerel_s {
            None if partial && build_s.is_none() => None,
            None => Some(vec![]),
            Some("") => Some(vec![]),
            Some(pr) => {
                let ids = parse_prerelease_identifiers_errmapped(pr, s)?;
                Some(ids)
            }
        };

        // Build parsing (base.py:352–361)
        let build = match build_s {
            None if partial => None,
            None => Some(vec![]),
            Some("") => Some(vec![]),
            Some(b) => {
                let ids = parse_build_identifiers_errmapped(b, s)?;
                Some(ids)
            }
        };

        Ok(Self { major, minor, patch, prerelease, build, partial })
    }

    // -----------------------------------------------------------------------
    // Constructor from components (base.py:84–119)
    // -----------------------------------------------------------------------

    /// Construct from explicit components (mirrors `Version(major=M, minor=m, ...)`).
    pub fn from_parts(
        major: u64,
        minor: u64,
        patch: u64,
        prerelease: Option<Vec<PreReleaseIdent>>,
        build: Option<Vec<BuildIdent>>,
    ) -> Self {
        Self {
            major,
            minor: Some(minor),
            patch: Some(patch),
            prerelease: Some(prerelease.unwrap_or_default()),
            build: Some(build.unwrap_or_default()),
            partial: false,
        }
    }

    // -----------------------------------------------------------------------
    // Validate
    // -----------------------------------------------------------------------

    /// Returns `true` if `s` is a valid SemVer version string.
    pub fn is_valid(s: &str) -> bool {
        Self::parse(s).is_ok()
    }

    // -----------------------------------------------------------------------
    // Coerce (base.py:225–302)
    // -----------------------------------------------------------------------

    /// Coerce an arbitrary version string into a valid SemVer.
    pub fn coerce(s: &str, partial: bool) -> Result<Self, SemverError> {
        let re = coerce_base_re();
        let m = re.find(s).ok_or_else(|| SemverError::invalid_coerce(s))?;
        let end = m.end();

        let mut version = s[..end].to_owned();

        if !partial {
            while version.matches('.').count() < 2 {
                version.push_str(".0");
            }
        }

        // Strip leading zeros (base.py:262–266)
        version = version
            .split('.')
            .map(|part| {
                let stripped = part.trim_start_matches('0');
                if stripped.is_empty() { "0" } else { stripped }
            })
            .collect::<Vec<_>>()
            .join(".");

        if end == s.len() {
            return Self::parse_inner(&version, partial);
        }

        let rest_raw = &s[end..];
        let rest: String = rest_raw.chars().map(|c| {
            if c.is_ascii_alphanumeric() || c == '+' || c == '.' || c == '-' { c } else { '-' }
        }).collect();

        let (prerel, build_str) = if rest.starts_with('+') {
            ("".to_owned(), rest[1..].to_owned())
        } else if rest.starts_with('.') {
            ("".to_owned(), rest[1..].to_owned())
        } else if rest.starts_with('-') {
            let inner = &rest[1..];
            if let Some(plus_pos) = inner.find('+') {
                (inner[..plus_pos].to_owned(), inner[plus_pos+1..].to_owned())
            } else {
                (inner.to_owned(), "".to_owned())
            }
        } else if let Some(plus_pos) = rest.find('+') {
            (rest[..plus_pos].to_owned(), rest[plus_pos+1..].to_owned())
        } else {
            (rest.clone(), "".to_owned())
        };

        let build_str = build_str.replace('+', ".");

        if !prerel.is_empty() {
            version = format!("{}-{}", version, prerel);
        }
        if !build_str.is_empty() {
            version = format!("{}+{}", version, build_str);
        }

        Self::parse_inner(&version, partial)
    }

    // -----------------------------------------------------------------------
    // Bump methods (base.py:133–179)
    // -----------------------------------------------------------------------

    /// Return the next major version (strips prerelease/build).
    pub fn next_major(&self) -> Self {
        let (minor, patch) = (self.minor.unwrap_or(0), self.patch.unwrap_or(0));
        let prerelease = self.prerelease.as_deref().unwrap_or(&[]);
        if !prerelease.is_empty() && minor == 0 && patch == 0 {
            Self::from_parts(self.major, 0, 0, None, None)
        } else {
            Self::from_parts(self.major + 1, 0, 0, None, None)
        }
    }

    /// Return the next minor version (strips prerelease/build).
    pub fn next_minor(&self) -> Self {
        let patch = self.patch.unwrap_or(0);
        let prerelease = self.prerelease.as_deref().unwrap_or(&[]);
        let minor = self.minor.unwrap_or(0);
        if !prerelease.is_empty() && patch == 0 {
            Self::from_parts(self.major, minor, 0, None, None)
        } else {
            Self::from_parts(self.major, minor + 1, 0, None, None)
        }
    }

    /// Return the next patch version (strips prerelease/build).
    pub fn next_patch(&self) -> Self {
        let minor = self.minor.unwrap_or(0);
        let patch = self.patch.unwrap_or(0);
        let prerelease = self.prerelease.as_deref().unwrap_or(&[]);
        if !prerelease.is_empty() {
            Self::from_parts(self.major, minor, patch, None, None)
        } else {
            Self::from_parts(self.major, minor, patch + 1, None, None)
        }
    }

    /// Return a new Version truncated up to build level (strips build metadata, keeps prerelease).
    /// Mirrors Python `v.truncate('prerelease')`.
    pub fn truncate_to_prerelease(&self) -> Self {
        Self {
            major: self.major,
            minor: self.minor,
            patch: self.patch,
            prerelease: self.prerelease.clone(),
            build: if self.partial { None } else { Some(vec![]) },
            partial: self.partial,
        }
    }

    /// Return a new Version truncated to patch level (strips prerelease and build metadata).
    /// Mirrors Python `v.truncate('patch')` or default `v.truncate()`.
    pub fn truncate_to_patch(&self) -> Self {
        Self {
            major: self.major,
            minor: self.minor,
            patch: self.patch,
            prerelease: if self.partial { None } else { Some(vec![]) },
            build: if self.partial { None } else { Some(vec![]) },
            partial: self.partial,
        }
    }

    /// Deprecated alias for `truncate_to_patch()`.
    pub fn truncate(&self) -> Self {
        self.truncate_to_patch()
    }

    // -----------------------------------------------------------------------
    // Precedence keys (base.py:424–463)
    // -----------------------------------------------------------------------

    /// Build the precedence key used for SemVer version comparison.
    ///
    /// **Build metadata is excluded** (SemVer §11).
    /// When there is no prerelease, we use `[Max]` so releases sort AFTER pre-releases.
    pub fn cmp_precedence_key(&self) -> PrecedenceKey {
        let prerelease: Vec<PreReleaseIdent> = match &self.prerelease {
            Some(ids) if !ids.is_empty() => ids.clone(),
            _ => vec![PreReleaseIdent::Max],
        };
        PrecedenceKey {
            major: self.major,
            minor: self.minor.unwrap_or(0),
            patch: self.patch.unwrap_or(0),
            prerelease,
        }
    }

    /// Build the full sort key, including build metadata.
    ///
    /// Use for `sorted(versions, key=lambda v: v.precedence_key)` style sorting.
    pub fn sort_precedence_key(&self) -> SortKey {
        let prerelease: Vec<PreReleaseIdent> = match &self.prerelease {
            Some(ids) if !ids.is_empty() => ids.clone(),
            _ => vec![PreReleaseIdent::Max],
        };
        let build: Vec<BuildIdent> = self.build.clone().unwrap_or_default();
        SortKey {
            major: self.major,
            minor: self.minor.unwrap_or(0),
            patch: self.patch.unwrap_or(0),
            prerelease,
            build,
        }
    }

    // -----------------------------------------------------------------------
    // Comparison helpers (mirrors Python's __lt__, __le__, __gt__, __ge__)
    // -----------------------------------------------------------------------

    /// Returns `true` if `self` has strictly lower SemVer precedence than `other`.
    /// Excludes build metadata (mirrors `__lt__`).
    pub fn precedence_lt(&self, other: &Self) -> bool {
        self.cmp_precedence_key() < other.cmp_precedence_key()
    }

    /// Returns `true` if `self` has lower or equal SemVer precedence than `other`.
    pub fn precedence_le(&self, other: &Self) -> bool {
        self.cmp_precedence_key() <= other.cmp_precedence_key()
    }

    /// Returns `true` if `self` has strictly higher SemVer precedence than `other`.
    pub fn precedence_gt(&self, other: &Self) -> bool {
        self.cmp_precedence_key() > other.cmp_precedence_key()
    }

    /// Returns `true` if `self` has higher or equal SemVer precedence than `other`.
    pub fn precedence_ge(&self, other: &Self) -> bool {
        self.cmp_precedence_key() >= other.cmp_precedence_key()
    }
}

// ---------------------------------------------------------------------------
// PartialEq / Eq — includes build metadata (base.py:477–491)
// ---------------------------------------------------------------------------

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.major == other.major
            && self.minor == other.minor
            && self.patch == other.patch
            && self.prerelease.as_deref().unwrap_or(&[])
                == other.prerelease.as_deref().unwrap_or(&[])
            && self.build.as_deref().unwrap_or(&[])
                == other.build.as_deref().unwrap_or(&[])
    }
}

impl Eq for Version {}

// ---------------------------------------------------------------------------
// Hash — mirrors base.py:419–422
// Hash includes build metadata (confirmed ground-truth: hash(1.0.0+a) != hash(1.0.0+b))
// ---------------------------------------------------------------------------

impl Hash for Version {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.major.hash(state);
        self.minor.hash(state);
        self.patch.hash(state);
        self.prerelease.as_deref().unwrap_or(&[]).hash(state);
        self.build.as_deref().unwrap_or(&[]).hash(state);
    }
}

impl Hash for PreReleaseIdent {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            PreReleaseIdent::Numeric(n) => { 0u8.hash(state); n.hash(state); }
            PreReleaseIdent::Alpha(s)   => { 1u8.hash(state); s.hash(state); }
            PreReleaseIdent::Max        => { 2u8.hash(state); }
        }
    }
}

impl Hash for BuildIdent {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            BuildIdent::Numeric(s) | BuildIdent::Alpha(s) => s.hash(state),
        }
    }
}

// ---------------------------------------------------------------------------
// Display (base.py:400–410)
// ---------------------------------------------------------------------------

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.major)?;
        if let Some(minor) = self.minor {
            write!(f, ".{}", minor)?;
        }
        if let Some(patch) = self.patch {
            write!(f, ".{}", patch)?;
        }

        let prerelease = self.prerelease.as_deref().unwrap_or(&[]);
        if !prerelease.is_empty() {
            let s = prerelease.iter().map(|id| match id {
                PreReleaseIdent::Numeric(n) => n.to_string(),
                PreReleaseIdent::Alpha(s)   => s.clone(),
                PreReleaseIdent::Max        => String::new(),
            }).collect::<Vec<_>>().join(".");
            write!(f, "-{}", s)?;
        } else if self.partial
            && self.prerelease == Some(vec![])
            && self.build.is_none()
        {
            // Trailing `-` with no identifiers and no build: `1.0.0-`
            write!(f, "-")?;
        }

        let build = self.build.as_deref().unwrap_or(&[]);
        if !build.is_empty() {
            let s = build.iter().map(|id| id.as_str()).collect::<Vec<_>>().join(".");
            write!(f, "+{}", s)?;
        } else if self.partial && self.build == Some(vec![]) {
            write!(f, "+")?;
        }

        Ok(())
    }
}

impl fmt::Display for PreReleaseIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PreReleaseIdent::Numeric(n) => write!(f, "{}", n),
            PreReleaseIdent::Alpha(s)   => write!(f, "{}", s),
            PreReleaseIdent::Max        => Ok(()),
        }
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Returns `true` if `s` is a non-zero all-digit string with a leading `0`.
fn has_leading_zero(s: &str) -> bool {
    s.len() > 1 && s.starts_with('0') && s.bytes().all(|b| b.is_ascii_digit())
}

/// Parse prerelease identifiers, mapping errors to `SemverError` with the full version string.
fn parse_prerelease_identifiers_errmapped(
    pr: &str,
    full_input: &str,
) -> Result<Vec<PreReleaseIdent>, SemverError> {
    use crate::identifiers::parse_prerelease_identifiers;
    parse_prerelease_identifiers(pr).map_err(|e| {
        // Reconstruct Python-style error messages
        if e.contains("Empty identifier") {
            // "Empty identifier in prerelease 'bad..id'" → "Invalid empty identifier '' in 'bad..id'"
            SemverError::empty_identifier("", pr)
        } else if e.contains("Leading zero") {
            // Extract the ident from the error string
            let ident = e.split_whitespace().last().unwrap_or(pr).trim_matches('\'').trim_matches('"');
            SemverError::leading_zero_identifier(ident)
        } else {
            SemverError::invalid_version(full_input)
        }
    })
}

/// Parse build identifiers, mapping errors to `SemverError`.
fn parse_build_identifiers_errmapped(
    b: &str,
    full_input: &str,
) -> Result<Vec<BuildIdent>, SemverError> {
    use crate::identifiers::parse_build_identifiers;
    parse_build_identifiers(b).map_err(|_| SemverError::invalid_version(full_input))
}

// ---------------------------------------------------------------------------
// Module-level functions
// ---------------------------------------------------------------------------

/// Returns `true` if `s` is a valid SemVer version string.
pub fn validate(s: &str) -> bool {
    Version::is_valid(s)
}

/// Compare two version strings by SemVer precedence.
///
/// Returns `Some(-1)`, `Some(0)`, or `Some(1)`.
/// Returns `None` for versions that cannot be compared (different types or parse error handled by caller).
///
/// **Ground truth (2026-08-01):** Python's `compare(v1, v2)` returns `NotImplemented` (the Python
/// sentinel object) when the versions differ ONLY in build metadata — meaning same precedence key
/// but `v1 != v2`. We return `None` for this case. For all other equal-precedence pairs (including
/// `1.0.0` vs `1.0.0` where both build fields are `()`), we return `Some(0)`.
///
/// Mirrors `base.compare(v1, v2)`.
pub fn compare(v1: &str, v2: &str) -> Result<Option<i32>, SemverError> {
    let a = Version::parse(v1)?;
    let b = Version::parse(v2)?;

    let key_cmp = a.cmp_precedence_key().cmp(&b.cmp_precedence_key());

    if key_cmp == std::cmp::Ordering::Equal && a != b {
        // Same precedence but different build — NotImplemented in Python
        return Ok(None);
    }

    use std::cmp::Ordering::*;
    Ok(Some(match key_cmp {
        Less    => -1,
        Equal   =>  0,
        Greater =>  1,
    }))
}
