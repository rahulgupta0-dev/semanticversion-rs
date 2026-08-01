//! SimpleSpec parser — "simple" spec syntax.
//!
//! Mirrors `base.py::SimpleSpec` @ commit `2cbbee3`.
//!
//! ## Grammar (from ground truth probe 2026-08-01)
//!
//! ```text
//! spec     = block (',' block)*         -> AllOf
//! block    = op? version suffix?
//! op       = '==' | '=' | '' | '!=' | '<' | '<=' | '>' | '>=' | '~=' | '^' | '~'
//! version  = NUM '.' NUM '.' NUM ['-']? [prerel]? ['+']? [build]?
//! suffix   = '-' (prerelease_policy=Always) | '+' (build_policy=Strict)
//! wildcard = '*' only (-> Range('>=', 0.0.0))
//! ```
//!
//! **CRITICAL:** SimpleSpec does NOT support `x`/`X` wildcards like `1.x` or `1.2.x`.
//! Those are INVALID and raise `ValueError: Invalid simple block '1.x'`.
//! Only `*` is valid as a standalone wildcard.
//!
//! ## Operator expansions (emit upper bound first)
//!
//! - `~=M.m`   → `AllOf(<M+1.0.0, >=M.m.0)`
//! - `~=M.m.p` → `AllOf(<M.m+1.0, >=M.m.p)`
//! - `^M.m.p`  → `AllOf(<M+1.0.0, >=M.m.p)`  (if M>0)
//! - `^0.m.p`  → `AllOf(<0.m+1.0, >=0.m.p)`  (if M=0, m>0)
//! - `^0.0.p`  → `AllOf(<0.0.p+1, >=0.0.p)`  (if M=0, m=0)
//! - `~M.m.p`  → `AllOf(<M.m+1.0, >=M.m.p)`
//! - `~M.m`    → `AllOf(<M.m+1.0, >=M.m.0)`

use std::fmt;

use regex::Regex;

use crate::clause::{BuildPolicy, Clause, Operator, PrereleasePolicy, Range};
use crate::error::SemverError;
use crate::version::Version;

// ---------------------------------------------------------------------------
// Regex: NAIVE_SPEC block (base.py:1054–1062)
// ---------------------------------------------------------------------------

fn naive_spec_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^(?P<op>==|=|!=|<=|>=|<|>|~=|\^|~)?(?P<major>[0-9]+)(?:\.(?P<minor>[0-9]+)(?:\.(?P<patch>[0-9]+))?)?(?:-(?P<prerel>[a-zA-Z0-9.-]*))?(?:\+(?P<build>[a-zA-Z0-9.-]*))?$"
        ).expect("naive_spec regex is valid")
    })
}

// ---------------------------------------------------------------------------
// SimpleSpec struct
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleSpec {
    pub clause: Clause,
}

impl SimpleSpec {
    /// Parse a SimpleSpec string.
    ///
    /// # Errors
    ///
    /// Returns `SemverError::InvalidSpec` for:
    /// - Invalid syntax
    /// - `x`/`X` wildcards (not supported by SimpleSpec)
    /// - Space-separated blocks (use `,` for AND)
    /// - Build metadata on non-EQ/NEQ operators
    pub fn parse(s: &str) -> Result<Self, SemverError> {
        if s.trim().is_empty() {
            return Err(SemverError::invalid_spec("Invalid simple spec: empty string"));
        }

        // Special case: '*' wildcard
        if s == "*" {
            let range = Range::new(
                Operator::Gte,
                Version::parse("0.0.0")?,
                PrereleasePolicy::Natural,
                BuildPolicy::Implicit,
            )?;
            return Ok(Self { clause: Clause::Range(range) });
        }

        // Reject space-separated blocks (only comma is valid separator)
        if s.contains(' ') {
            return Err(SemverError::invalid_spec(&format!("Invalid simple block '{}'", s)));
        }

        // Split on comma (AND)
        let blocks: Vec<&str> = s.split(',').collect();
        let mut clauses: Vec<Clause> = Vec::new();

        for block in blocks {
            let clause = Self::parse_block(block)?;
            clauses.push(clause);
        }

        let combined = clauses.into_iter().reduce(|acc, c| acc & c).unwrap_or(Clause::Always);
        Ok(Self { clause: combined.simplify() })
    }

    /// Parse a single block (no commas).
    fn parse_block(block: &str) -> Result<Clause, SemverError> {
        let re = naive_spec_re();
        let caps = re.captures(block).ok_or_else(|| {
            SemverError::invalid_spec(&format!("Invalid simple block '{}'", block))
        })?;

        let op_str = caps.name("op").map(|m| m.as_str()).unwrap_or("");
        let major_str = caps.name("major").map(|m| m.as_str()).unwrap();
        let minor_str = caps.name("minor").map(|m| m.as_str());
        let patch_str = caps.name("patch").map(|m| m.as_str());
        let prerel_str = caps.name("prerel").map(|m| m.as_str());
        let build_str = caps.name("build").map(|m| m.as_str());

        // Build version string for parsing
        let minor = minor_str.unwrap_or("0");
        let patch = patch_str.unwrap_or("0");
        let mut version_str = format!("{}.{}.{}", major_str, minor, patch);

        let has_empty_prerel = prerel_str == Some("");
        let has_empty_build = build_str == Some("");

        if let Some(prerel) = prerel_str {
            if !prerel.is_empty() {
                version_str.push('-');
                version_str.push_str(prerel);
            }
        }

        if let Some(build) = build_str {
            if !build.is_empty() {
                version_str.push('+');
                version_str.push_str(build);
            }
        }

        let target = Version::parse(&version_str)?;

        // Determine operator
        let operator = match op_str {
            "==" | "=" | "" => Operator::Eq,
            "!=" => Operator::Neq,
            "<" => Operator::Lt,
            "<=" => Operator::Lte,
            ">" => Operator::Gt,
            ">=" => Operator::Gte,
            "~=" => {
                // Compatible release operator
                return Self::expand_compatible(&target, patch_str.is_none());
            }
            "^" => {
                // Caret operator
                return Self::expand_caret(&target);
            }
            "~" => {
                // Tilde operator
                return Self::expand_tilde(&target, patch_str.is_none());
            }
            _ => {
                return Err(SemverError::invalid_spec(&format!("Invalid simple block '{}'", block)));
            }
        };

        // Determine policies from suffixes
        let prerelease_policy = if has_empty_prerel && matches!(operator, Operator::Lt | Operator::Neq) {
            PrereleasePolicy::Always
        } else {
            PrereleasePolicy::Natural
        };

        let build_policy = if has_empty_build && operator == Operator::Eq {
            BuildPolicy::Strict
        } else if !target.build.as_deref().unwrap_or(&[]).is_empty() {
            BuildPolicy::Strict
        } else {
            BuildPolicy::Implicit
        };

        let range = Range::new(operator, target, prerelease_policy, build_policy)?;
        Ok(Clause::Range(range))
    }

    /// Expand `~=M.m` or `~=M.m.p` operator.
    fn expand_compatible(target: &Version, partial: bool) -> Result<Clause, SemverError> {
        let lower = Range::new(
            Operator::Gte,
            target.clone(),
            PrereleasePolicy::Natural,
            BuildPolicy::Implicit,
        )?;

        let upper = if partial {
            // ~=M.m -> <M+1.0.0
            let upper_target = Version::from_parts(target.major + 1, 0, 0, None, None);
            Range::new(Operator::Lt, upper_target, PrereleasePolicy::Natural, BuildPolicy::Implicit)?
        } else {
            // ~=M.m.p -> <M.m+1.0
            let upper_target = Version::from_parts(
                target.major,
                target.minor.unwrap_or(0) + 1,
                0,
                None,
                None,
            );
            Range::new(Operator::Lt, upper_target, PrereleasePolicy::Natural, BuildPolicy::Implicit)?
        };

        Ok(Clause::AllOf(vec![Clause::Range(upper), Clause::Range(lower)]))
    }

    /// Expand `^M.m.p` operator (caret).
    fn expand_caret(target: &Version) -> Result<Clause, SemverError> {
        let lower = Range::new(
            Operator::Gte,
            target.clone(),
            PrereleasePolicy::Natural,
            BuildPolicy::Implicit,
        )?;

        let upper_target = if target.major > 0 {
            // ^M.m.p -> <M+1.0.0
            Version::from_parts(target.major + 1, 0, 0, None, None)
        } else if target.minor.unwrap_or(0) > 0 {
            // ^0.m.p -> <0.m+1.0
            Version::from_parts(0, target.minor.unwrap_or(0) + 1, 0, None, None)
        } else {
            // ^0.0.p -> <0.0.p+1
            Version::from_parts(0, 0, target.patch.unwrap_or(0) + 1, None, None)
        };

        let upper = Range::new(Operator::Lt, upper_target, PrereleasePolicy::Natural, BuildPolicy::Implicit)?;
        Ok(Clause::AllOf(vec![Clause::Range(upper), Clause::Range(lower)]))
    }

    /// Expand `~M.m.p` or `~M.m` operator (tilde).
    fn expand_tilde(target: &Version, partial: bool) -> Result<Clause, SemverError> {
        let lower = Range::new(
            Operator::Gte,
            target.clone(),
            PrereleasePolicy::Natural,
            BuildPolicy::Implicit,
        )?;

        let upper_target = if partial {
            // ~M.m -> <M.m+1.0
            Version::from_parts(target.major, target.minor.unwrap_or(0) + 1, 0, None, None)
        } else {
            // ~M.m.p -> <M.m+1.0
            Version::from_parts(target.major, target.minor.unwrap_or(0) + 1, 0, None, None)
        };

        let upper = Range::new(Operator::Lt, upper_target, PrereleasePolicy::Natural, BuildPolicy::Implicit)?;
        Ok(Clause::AllOf(vec![Clause::Range(upper), Clause::Range(lower)]))
    }

    /// Check if a version matches this spec.
    pub fn match_version(&self, version: &Version) -> bool {
        self.clause.matches(version)
    }
}

impl fmt::Display for SimpleSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.clause {
            Clause::Always => write!(f, "*"),
            Clause::Never => write!(f, "NEVER"),
            Clause::Range(r) => write!(f, "{}", r),
            Clause::AllOf(clauses) => {
                let parts: Vec<String> = clauses.iter().map(|c| c.to_string()).collect();
                write!(f, "{}", parts.join(","))
            }
            Clause::AnyOf(clauses) => {
                let parts: Vec<String> = clauses.iter().map(|c| c.to_string()).collect();
                write!(f, "||{}", parts.join("||"))
            }
        }
    }
}
