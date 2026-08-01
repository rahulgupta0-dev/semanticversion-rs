//! Specification Clause primitives and matching logic.
//!
//! Mirrors `base.py::Range`, `Clause`, `Always`, `Never`, `AllOf`, `AnyOf` @ commit `2cbbee3`.
//!
//! ## Key concepts
//! - `Operator`: `==`, `!=`, `<`, `<=`, `>`, `>=`.
//! - `PrereleasePolicy`: `Natural` (default), `Always` (bound had `-`), `SamePatch` (npm).
//! - `BuildPolicy`: `Implicit` (default), `Strict` (target had build or spec had `+`).
//! - `Range`: A single `op target` comparison node.
//! - `Clause`: An AST of `Always`, `Never`, `Range`, `AllOf`, `AnyOf`.

use std::fmt;
use std::ops::{BitAnd, BitOr};

use crate::error::SemverError;
use crate::version::Version;

// ---------------------------------------------------------------------------
// Operator enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operator {
    Eq,  // == or = or empty
    Neq, // !=
    Lt,  // <
    Lte, // <=
    Gt,  // >
    Gte, // >=
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operator::Eq => write!(f, "=="),
            Operator::Neq => write!(f, "!="),
            Operator::Lt => write!(f, "<"),
            Operator::Lte => write!(f, "<="),
            Operator::Gt => write!(f, ">"),
            Operator::Gte => write!(f, ">="),
        }
    }
}

// ---------------------------------------------------------------------------
// PrereleasePolicy enum (base.py:941–945)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrereleasePolicy {
    /// Default: `<1.2.3` does not match `1.2.2-alpha` or `1.2.3-alpha`.
    Natural,
    /// Triggered by trailing `-` on bound (e.g. `<1.2.3-` or `!=1.2.3-`):
    /// `<1.2.3-` matches `1.2.3-alpha` and `1.2.2-alpha`.
    Always,
    /// Used by NpmSpec: prerelease of the SAME M.m.p patch is allowed; other patches excluded.
    SamePatch,
}

// ---------------------------------------------------------------------------
// BuildPolicy enum (base.py:948–950)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildPolicy {
    /// Default: `1.2.3` matches `1.2.3+build42` (build metadata ignored).
    Implicit,
    /// Strict: target had build metadata or spec had `+` suffix (`==1.2.3+build42` or `==1.2.3+`).
    Strict,
}

// ---------------------------------------------------------------------------
// Range struct (base.py:932–1041)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Range {
    pub operator: Operator,
    pub target: Version,
    pub prerelease_policy: PrereleasePolicy,
    pub build_policy: BuildPolicy,
}

impl Range {
    pub fn new(
        operator: Operator,
        target: Version,
        prerelease_policy: PrereleasePolicy,
        build_policy: BuildPolicy,
    ) -> Result<Self, SemverError> {
        let has_build = target.build.as_deref().map_or(false, |b| !b.is_empty());
        if has_build && operator != Operator::Eq && operator != Operator::Neq {
            return Err(SemverError::invalid_spec(format!(
                "Invalid range {}{}: build numbers have no ordering.",
                operator, target
            )));
        }

        let effective_build_policy = if has_build {
            BuildPolicy::Strict
        } else {
            build_policy
        };

        Ok(Self {
            operator,
            target,
            prerelease_policy,
            build_policy: effective_build_policy,
        })
    }

    /// Check if `version` satisfies this Range constraint.
    /// Faithfully implements `base.py::Range.match` (lines 965–1012).
    pub fn matches(&self, version: &Version) -> bool {
        // If build_policy != Strict, truncate version to prerelease level (strip build)
        let check_version = if self.build_policy != BuildPolicy::Strict {
            version.truncate_to_prerelease()
        } else {
            version.clone()
        };

        let version_has_prerelease = check_version
            .prerelease
            .as_deref()
            .map_or(false, |p| !p.is_empty());

        if version_has_prerelease {
            let same_patch = self.target.truncate_to_patch() == check_version.truncate_to_patch();
            if self.prerelease_policy == PrereleasePolicy::SamePatch && !same_patch {
                return false;
            }
        }

        let target_has_prerelease = self
            .target
            .prerelease
            .as_deref()
            .map_or(false, |p| !p.is_empty());

        match self.operator {
            Operator::Eq => {
                if self.build_policy == BuildPolicy::Strict {
                    self.target.truncate_to_prerelease() == check_version.truncate_to_prerelease()
                        && check_version.build == self.target.build
                } else {
                    check_version == self.target
                }
            }
            Operator::Gt => check_version.precedence_gt(&self.target),
            Operator::Gte => check_version.precedence_ge(&self.target),
            Operator::Lt => {
                if version_has_prerelease
                    && self.prerelease_policy == PrereleasePolicy::Natural
                    && check_version.truncate_to_patch() == self.target.truncate_to_patch()
                    && !target_has_prerelease
                {
                    return false;
                }
                check_version.precedence_lt(&self.target)
            }
            Operator::Lte => check_version.precedence_le(&self.target),
            Operator::Neq => {
                if self.build_policy == BuildPolicy::Strict {
                    !(self.target.truncate_to_prerelease() == check_version.truncate_to_prerelease()
                        && check_version.build == self.target.build)
                } else {
                    if version_has_prerelease
                        && self.prerelease_policy == PrereleasePolicy::Natural
                        && check_version.truncate_to_patch() == self.target.truncate_to_patch()
                        && !target_has_prerelease
                    {
                        return false;
                    }
                    check_version != self.target
                }
            }
        }
    }
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.operator, self.target)
    }
}

// ---------------------------------------------------------------------------
// Clause AST enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Clause {
    Always,
    Never,
    Range(Range),
    AllOf(Vec<Clause>),
    AnyOf(Vec<Clause>),
}

impl Clause {
    pub fn matches(&self, version: &Version) -> bool {
        match self {
            Clause::Always => true,
            Clause::Never => false,
            Clause::Range(r) => r.matches(version),
            Clause::AllOf(clauses) => clauses.iter().all(|c| c.matches(version)),
            Clause::AnyOf(clauses) => clauses.iter().any(|c| c.matches(version)),
        }
    }

    /// Simplify clause tree (base.py:813–826).
    pub fn simplify(self) -> Self {
        match self {
            Clause::AllOf(clauses) => {
                let mut simplified_list = Vec::new();
                for c in clauses {
                    let sc = c.simplify();
                    match sc {
                        Clause::Always => continue,
                        Clause::Never => return Clause::Never,
                        Clause::AllOf(sub) => simplified_list.extend(sub),
                        other => simplified_list.push(other),
                    }
                }
                if simplified_list.is_empty() {
                    Clause::Always
                } else if simplified_list.len() == 1 {
                    simplified_list.pop().unwrap()
                } else {
                    Clause::AllOf(simplified_list)
                }
            }
            Clause::AnyOf(clauses) => {
                let mut simplified_list = Vec::new();
                for c in clauses {
                    let sc = c.simplify();
                    match sc {
                        Clause::Never => continue,
                        Clause::Always => return Clause::Always,
                        Clause::AnyOf(sub) => simplified_list.extend(sub),
                        other => simplified_list.push(other),
                    }
                }
                if simplified_list.is_empty() {
                    Clause::Never
                } else if simplified_list.len() == 1 {
                    simplified_list.pop().unwrap()
                } else {
                    Clause::AnyOf(simplified_list)
                }
            }
            other => other,
        }
    }
}

// ---------------------------------------------------------------------------
// BitAnd (&) and BitOr (|) for Clause composition (base.py:803, 836)
// ---------------------------------------------------------------------------

impl BitAnd for Clause {
    type Output = Clause;

    fn bitand(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Clause::Never, _) | (_, Clause::Never) => Clause::Never,
            (Clause::Always, c) | (c, Clause::Always) => c,
            (Clause::AllOf(mut a), Clause::AllOf(b)) => {
                a.extend(b);
                Clause::AllOf(a)
            }
            (Clause::AllOf(mut a), c) | (c, Clause::AllOf(mut a)) => {
                a.push(c);
                Clause::AllOf(a)
            }
            (c1, c2) => Clause::AllOf(vec![c1, c2]),
        }
    }
}

impl BitOr for Clause {
    type Output = Clause;

    fn bitor(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Clause::Always, _) | (_, Clause::Always) => Clause::Always,
            (Clause::Never, c) | (c, Clause::Never) => c,
            (Clause::AnyOf(mut a), Clause::AnyOf(b)) => {
                a.extend(b);
                Clause::AnyOf(a)
            }
            (Clause::AnyOf(mut a), c) | (c, Clause::AnyOf(mut a)) => {
                a.push(c);
                Clause::AnyOf(a)
            }
            (c1, c2) => Clause::AnyOf(vec![c1, c2]),
        }
    }
}
