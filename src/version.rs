//! `Version` struct — parsing, display, coercion, and computed keys.
//!
//! ## Python mapping
//!
//! All logic faithfully mirrors `base.py::Version` @ commit `2cbbee3`.
//!
//! Key design decisions (see DECISIONS.md):
//! - D01: major/minor/patch stored as `u64` (Python uses arbitrary-precision int).
//! - D02: `Option<u64>` for minor/patch in partial versions (Python uses `None`).
//! - D09: `coerce()` ported directly; regex + string logic from base.py:225–302.
//! - D10: Error messages include `{:?}` (Rust debug repr) for strings.
//! - D11: `PreReleaseIdent::Max` sentinel for no-prerelease ordering.
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
use crate::identifiers::{
    BuildIdent, PreReleaseIdent,
    parse_build_identifiers, parse_prerelease_identifiers,
};

// ---------------------------------------------------------------------------
// Regex: compiled once via std::sync::OnceLock (stable since 1.70)
// ---------------------------------------------------------------------------

fn version_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(\d+)\.(\d+)\.(\d+)(?:-([0-9a-zA-Z.-]+))?(?:\+([0-9a-zA-Z.-]+))?$")
            .expect("version regex is valid")
    })
}

fn partial_version_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(\d+)(?:\.(\d+)(?:\.(\d+))?)?(?:-([0-9a-zA-Z.-]*))?(?:\+([0-9a-zA-Z.-]*))?$",
        )
        .expect("partial version regex is valid")
    })
}

fn coerce_base_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\d+(?:\.\d+(?:\.\d+)?)?").expect("coerce base regex is valid")
    })
}

// ---------------------------------------------------------------------------
// Precedence key types (used for Ord, returned from .precedence_key)
// ---------------------------------------------------------------------------

/// The key used for SemVer *precedence* comparison (ignores build metadata).
/// Mirrors Python's `_cmp_precedence_key` (base.py:123).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PrecedenceKey {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// `[Max]` when there is no prerelease (so release > any prerelease).
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
/// For standard (non-partial) versions: minor and patch are always present.
/// For partial versions: minor and/or patch may be `None`.
///
/// `prerelease` and `build` are `Vec` of dot-split components.
/// For standard versions they are always `Some` (possibly empty).
/// For partial versions they may be `None` (not specified) or `Some([])` (trailing `-` or `+`).
#[derive(Debug, Clone)]
pub struct Version {
    pub major: u64,
    pub minor: Option<u64>,
    pub patch: Option<u64>,
    /// `None` = not specified (partial only).
    /// `Some([])` = trailing `-` with no identifiers (partial only, or standard with no prerelease).
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
            return Err(SemverError::InvalidVersion(s.to_owned()));
        }

        let re = if partial { partial_version_re() } else { version_re() };
        let caps = re.captures(s).ok_or_else(|| SemverError::InvalidVersion(s.to_owned()))?;

        // Group 1: major (always present)
        let major_s = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        // Group 2: minor (optional in partial)
        let minor_s = caps.get(2).map(|m| m.as_str());
        // Group 3: patch (optional in partial)
        let patch_s = caps.get(3).map(|m| m.as_str());
        // Group 4: prerelease (optional; None = absent, Some("") = trailing dash)
        let prerel_s = caps.get(4).map(|m| m.as_str());
        // Group 5: build (optional; None = absent, Some("") = trailing plus)
        let build_s = caps.get(5).map(|m| m.as_str());

        // Leading-zero check (base.py:329–334)
        if has_leading_zero(major_s) {
            return Err(SemverError::InvalidVersion(s.to_owned()));
        }
        if minor_s.map(has_leading_zero).unwrap_or(false) {
            return Err(SemverError::InvalidVersion(s.to_owned()));
        }
        if patch_s.map(has_leading_zero).unwrap_or(false) {
            return Err(SemverError::InvalidVersion(s.to_owned()));
        }

        let major = major_s.parse::<u64>().map_err(|_| SemverError::InvalidVersion(s.to_owned()))?;
        let minor = minor_s.map(|v| v.parse::<u64>().map_err(|_| SemverError::InvalidVersion(s.to_owned()))).transpose()?;
        let patch = patch_s.map(|v| v.parse::<u64>().map_err(|_| SemverError::InvalidVersion(s.to_owned()))).transpose()?;

        // Prerelease parsing (base.py:340–350)
        let prerelease = match prerel_s {
            None if partial && build_s.is_none() => None,  // No prerelease, no build, partial → None
            None => Some(vec![]),                            // No prerelease, standard → empty
            Some("") => Some(vec![]),                       // Trailing `-` → empty tuple
            Some(pr) => {
                let ids = parse_prerelease_identifiers(pr)
                    .map_err(|_| SemverError::InvalidVersion(s.to_owned()))?;
                Some(ids)
            }
        };

        // Build parsing (base.py:352–361)
        let build = match build_s {
            None if partial => None,   // No build, partial → None
            None => Some(vec![]),      // No build, standard → empty
            Some("") => Some(vec![]),  // Trailing `+` → empty
            Some(b) => {
                let ids = parse_build_identifiers(b)
                    .map_err(|_| SemverError::InvalidVersion(s.to_owned()))?;
                Some(ids)
            }
        };

        Ok(Self { major, minor, patch, prerelease, build, partial })
    }

    // -----------------------------------------------------------------------
    // Constructor from components (base.py:84–119)
    // -----------------------------------------------------------------------

    /// Construct from explicit components (mirrors `Version(major=M, minor=m, ...)`).
    ///
    /// For non-partial: prerelease and build default to `[]` if not provided.
    pub fn from_parts(
        major: u64,
        minor: u64,
        patch: u64,
        prerelease: Option<Vec<PreReleaseIdent>>,
        build: Option<Vec<BuildIdent>>,
    ) -> Result<Self, SemverError> {
        // Validate prerelease identifiers for leading zeros
        if let Some(ref prs) = prerelease {
            for id in prs {
                if let PreReleaseIdent::Numeric(n) = id {
                    // n is already parsed; no leading-zero risk
                    let _ = n;
                }
            }
        }
        Ok(Self {
            major,
            minor: Some(minor),
            patch: Some(patch),
            prerelease: Some(prerelease.unwrap_or_default()),
            build: Some(build.unwrap_or_default()),
            partial: false,
        })
    }

    // -----------------------------------------------------------------------
    // Validate (base.py module-level `validate` function)
    // -----------------------------------------------------------------------

    /// Returns `true` if `s` is a valid SemVer version string.
    pub fn is_valid(s: &str) -> bool {
        Self::parse(s).is_ok()
    }

    // -----------------------------------------------------------------------
    // Coerce (base.py:225–302)
    // -----------------------------------------------------------------------

    /// Coerce an arbitrary version string into a valid SemVer string.
    ///
    /// Rules:
    /// - Extract leading `N`, `N.M`, or `N.M.P` component.
    /// - Fill missing minor/patch with `.0` (unless `partial=true`).
    /// - Strip leading zeros from numeric components.
    /// - Any trailing content after the numeric part is cleaned and appended as
    ///   prerelease and/or build metadata.
    pub fn coerce(s: &str, partial: bool) -> Result<Self, SemverError> {
        let re = coerce_base_re();
        let m = re.find(s).ok_or_else(|| SemverError::InvalidCoerce(s.to_owned()))?;
        let end = m.end();

        let mut version = s[..end].to_owned();

        if !partial {
            // Fill missing minor/patch
            while version.matches('.').count() < 2 {
                version.push_str(".0");
            }
        }

        // Strip leading zeros from each component (base.py:262–266)
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

        // Process trailing rest (base.py:271–300)
        let rest_raw = &s[end..];
        // Replace non-semver chars with `-` (base.py:274)
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

        // base.py:295: build = build.replace('+', '.')
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
    ///
    /// If this version has a prerelease and minor == patch == 0, returns the same
    /// M.0.0 without the prerelease (promotes the prerelease to release).
    pub fn next_major(&self) -> Self {
        let (minor, patch) = (self.minor.unwrap_or(0), self.patch.unwrap_or(0));
        let prerelease = self.prerelease.as_deref().unwrap_or(&[]);
        if !prerelease.is_empty() && minor == 0 && patch == 0 {
            Self::from_parts(self.major, 0, 0, None, None).unwrap()
        } else {
            Self::from_parts(self.major + 1, 0, 0, None, None).unwrap()
        }
    }

    /// Return the next minor version (strips prerelease/build).
    pub fn next_minor(&self) -> Self {
        let patch = self.patch.unwrap_or(0);
        let prerelease = self.prerelease.as_deref().unwrap_or(&[]);
        let minor = self.minor.unwrap_or(0);
        if !prerelease.is_empty() && patch == 0 {
            Self::from_parts(self.major, minor, 0, None, None).unwrap()
        } else {
            Self::from_parts(self.major, minor + 1, 0, None, None).unwrap()
        }
    }

    /// Return the next patch version (strips prerelease/build).
    pub fn next_patch(&self) -> Self {
        let minor = self.minor.unwrap_or(0);
        let patch = self.patch.unwrap_or(0);
        let prerelease = self.prerelease.as_deref().unwrap_or(&[]);
        if !prerelease.is_empty() {
            Self::from_parts(self.major, minor, patch, None, None).unwrap()
        } else {
            Self::from_parts(self.major, minor, patch + 1, None, None).unwrap()
        }
    }

    /// Return this version truncated to patch level (strips prerelease and build).
    ///
    /// Used by `NpmSpec` to compute the non-prerelease range boundary.
    pub fn truncate(&self) -> Self {
        Self::from_parts(
            self.major,
            self.minor.unwrap_or(0),
            self.patch.unwrap_or(0),
            None,
            None,
        ).unwrap()
    }

    // -----------------------------------------------------------------------
    // Precedence keys (base.py:424–463)
    // -----------------------------------------------------------------------

    /// Build the precedence key used for `PartialOrd`/`Ord` comparison.
    ///
    /// **Build metadata is excluded** (SemVer §11: build has no precedence).
    /// When there is no prerelease, we use `[Max]` so that releases sort AFTER
    /// pre-releases (base.py:436–438 — the `MaxIdentifier` sentinel).
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
    /// Used for stable sorting (`sorted(versions, key=lambda v: v.precedence_key)` in Python).
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
}

// ---------------------------------------------------------------------------
// PartialEq / Eq — includes build metadata (base.py:477–491)
// ---------------------------------------------------------------------------
//
// DECISION D06: `PartialEq` includes build metadata (all 5 fields).
// `PartialOrd`/`Ord` excludes build (cmp_precedence_key).
// This means `v1 == v2` can be false while `v1.cmp(&v2) == Equal`.
// We document this divergence from Rust's standard convention.

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.major == other.major
            && self.minor == other.minor
            && self.patch == other.patch
            && self.prerelease.as_deref().unwrap_or(&[]) == other.prerelease.as_deref().unwrap_or(&[])
            && self.build.as_deref().unwrap_or(&[]) == other.build.as_deref().unwrap_or(&[])
    }
}

impl Eq for Version {}

// ---------------------------------------------------------------------------
// Hash — mirrors base.py:419–422
// ---------------------------------------------------------------------------

impl Hash for Version {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.major.hash(state);
        self.minor.hash(state);
        self.patch.hash(state);
        // Use empty slice when None (partial version with unspecified prerelease)
        self.prerelease.as_deref().unwrap_or(&[]).hash(state);
        self.build.as_deref().unwrap_or(&[]).hash(state);
    }
}

// PartialEq on PreReleaseIdent is needed for Hash consistency
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
// PartialOrd / Ord — uses cmp_precedence_key, excludes build
// ---------------------------------------------------------------------------

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.cmp_precedence_key().cmp(&other.cmp_precedence_key())
    }
}

// ---------------------------------------------------------------------------
// Display / Debug (base.py:400–417)
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

        // Prerelease: show if non-empty, or if partial with empty prerelease and no build
        let prerelease = self.prerelease.as_deref().unwrap_or(&[]);
        if !prerelease.is_empty() {
            let s = prerelease.iter().map(|id| match id {
                PreReleaseIdent::Numeric(n) => n.to_string(),
                PreReleaseIdent::Alpha(s) => s.clone(),
                PreReleaseIdent::Max => String::new(),
            }).collect::<Vec<_>>().join(".");
            write!(f, "-{}", s)?;
        } else if self.partial && self.prerelease == Some(vec![]) && self.build.is_none() {
            // Trailing `-` with no identifiers and no build: print `1.0.0-`
            write!(f, "-")?;
        }

        // Build
        let build = self.build.as_deref().unwrap_or(&[]);
        if !build.is_empty() {
            let s = build.iter().map(|id| id.as_str()).collect::<Vec<_>>().join(".");
            write!(f, "+{}", s)?;
        } else if self.partial && self.build == Some(vec![]) {
            // Trailing `+` with no identifiers
            write!(f, "+")?;
        }

        Ok(())
    }
}

impl fmt::Display for PreReleaseIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PreReleaseIdent::Numeric(n) => write!(f, "{}", n),
            PreReleaseIdent::Alpha(s) => write!(f, "{}", s),
            PreReleaseIdent::Max => write!(f, ""),
        }
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Returns `true` if `s` is a non-zero all-digit string with a leading `0`.
/// Mirrors Python's `_has_leading_zero` (base.py:10–14).
fn has_leading_zero(s: &str) -> bool {
    s.len() > 1 && s.starts_with('0') && s.bytes().all(|b| b.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// Module-level functions (mirrors base.py::validate, compare)
// ---------------------------------------------------------------------------

/// Returns `true` if `s` is a valid SemVer version string.
/// Mirrors `base.validate(version_string)` (base.py).
pub fn validate(s: &str) -> bool {
    Version::is_valid(s)
}

/// Compare two version strings by SemVer precedence.
///
/// Returns `-1`, `0`, or `1` (like Python `cmp()`).
/// Returns `NotImplemented` (as `None`) when the two versions differ only in build metadata
/// and are therefore unordered.
///
/// Mirrors `base.compare(v1, v2)`.
pub fn compare(v1: &str, v2: &str) -> Result<Option<i32>, SemverError> {
    let a = Version::parse(v1)?;
    let b = Version::parse(v2)?;
    // If they are equal by precedence but not by full equality (build differs)
    if a.cmp_precedence_key() == b.cmp_precedence_key() && a != b {
        return Ok(None); // NotImplemented in Python
    }
    use std::cmp::Ordering::*;
    Ok(Some(match a.cmp(&b) {
        Less => -1,
        Equal => 0,
        Greater => 1,
    }))
}
