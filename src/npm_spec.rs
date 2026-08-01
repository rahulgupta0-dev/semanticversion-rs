//! NpmSpec parser — npm-style version ranges.
//!
//! Mirrors `base.py::NpmSpec` @ commit `2cbbee3`.
//!
//! ## Grammar (from ground truth probe 2026-08-01)
//!
//! ```text
//! expression = group ('||' group)*              -> AnyOf
//! group      = hyphen_range | simple_range
//! hyphen_range = version ' - ' version          -> AllOf(upper, lower) with partial rounding
//! simple_range = block (' ' block)*             -> AllOf (space = AND)
//! block      = 'v'? op? version_with_x
//! op         = '<' | '<=' | '>' | '>=' | '=' | '^' | '~'  (no != or ~=)
//! version_with_x = (NUM|x|X|*) ('.' (NUM|x|X|*) ('.' (NUM|x|X|*))?)?
//! ```
//!
//! **Key differences from SimpleSpec:**
//! - Supports `x`/`X` wildcards: `1.2.x` → `AllOf(<1.3.0, >=1.2.0)`
//! - Space is the AND separator (not comma)
//! - `||` is the OR separator
//! - Hyphen ranges with partial bounds
//! - ALL ranges use `PrereleasePolicy::SamePatch` by default
//! - Prerelease targets trigger AnyOf-tree expansion
//!
//! ## Prerelease OR-expansion
//!
//! When a target has prerelease (e.g., `>1.2.3-alpha.3`), npm generates:
//! ```text
//! AnyOf(
//!   AllOf(<1.2.4 ALWAYS, >1.2.3-alpha.3 SAMEPATCH),  // prerelease branch
//!   AllOf(>1.2.3 SAMEPATCH)                          // non-prerelease branch (truncated target)
//! )
//! ```
//!
//! The ALWAYS fence (`<1.2.4`) allows prereleases of `1.2.3` but excludes `1.2.4-x`.
//! The second branch allows non-prerelease versions `>1.2.3`.

use std::fmt;

use regex::Regex;

use crate::clause::{BuildPolicy, Clause, Operator, PrereleasePolicy, Range};
use crate::error::SemverError;
use crate::version::Version;

// ---------------------------------------------------------------------------
// Regex: NPM_SPEC_BLOCK (base.py:1269–1277)
// ---------------------------------------------------------------------------

fn npm_spec_block_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^v?(?P<op><=|>=|<|>|=|\^|~)?(?P<major>[0-9]+|[xX*])(?:\.(?P<minor>[0-9]+|[xX*])(?:\.(?P<patch>[0-9]+|[xX*]))?)?(?:-(?P<prerel>[a-zA-Z0-9.-]+))?(?:\+(?P<build>[a-zA-Z0-9.-]+))?$"
        ).expect("npm_spec_block regex is valid")
    })
}

// ---------------------------------------------------------------------------
// NpmSpec struct
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpmSpec {
    pub clause: Clause,
}

impl NpmSpec {
    /// Parse an NpmSpec string.
    pub fn parse(s: &str) -> Result<Self, SemverError> {
        let s = s.trim();
        
        // Special case: empty string or '*'
        if s.is_empty() || s == "*" {
            let range = Range::new(
                Operator::Gte,
                Version::parse("0.0.0")?,
                PrereleasePolicy::SamePatch,
                BuildPolicy::Implicit,
            )?;
            return Ok(Self { clause: Clause::AllOf(vec![Clause::Range(range)]) });
        }

        // Split on '||' (OR)
        let or_groups: Vec<&str> = s.split("||").map(|g| g.trim()).collect();
        let mut or_clauses: Vec<Clause> = Vec::new();

        for group in or_groups {
            let group_clause = Self::parse_group(group)?;
            or_clauses.push(group_clause);
        }

        let combined = if or_clauses.len() == 1 {
            or_clauses.into_iter().next().unwrap()
        } else {
            or_clauses.into_iter().reduce(|acc, c| acc | c).unwrap_or(Clause::Never)
        };

        Ok(Self { clause: combined.simplify() })
    }

    /// Parse a group (hyphen range or space-separated blocks).
    fn parse_group(group: &str) -> Result<Clause, SemverError> {
        // Check for hyphen range (space-hyphen-space)
        if let Some(hyphen_pos) = group.find(" - ") {
            let left = group[..hyphen_pos].trim();
            let right = group[hyphen_pos + 3..].trim();
            return Self::parse_hyphen_range(left, right);
        }

        // Space-separated blocks (AND)
        let blocks: Vec<&str> = group.split_whitespace().collect();
        
        // Parse all blocks first
        let mut parsed_blocks: Vec<(Clause, bool)> = Vec::new(); // (clause, has_prerelease)
        let mut any_has_prerelease = false;
        
        for block in blocks {
            let (clause, has_prerelease) = Self::parse_block_with_flag(block)?;
            parsed_blocks.push((clause, has_prerelease));
            if has_prerelease {
                any_has_prerelease = true;
            }
        }
        
        // If no prerelease blocks, simple AND
        if !any_has_prerelease {
            let clauses: Vec<Clause> = parsed_blocks.into_iter().map(|(c, _)| c).collect();
            let combined = clauses.into_iter().reduce(|acc, c| acc & c).unwrap_or(Clause::Always);
            return Ok(combined.simplify());
        }
        
        // If we have prerelease blocks, we need to construct the AnyOf tree
        // The prerelease branch gets the prerelease-specific clauses + ALWAYS fence
        // The non-prerelease branch gets ALL clauses with truncated targets
        
        let mut prerelease_branch_clauses: Vec<Clause> = Vec::new();
        let mut non_prerelease_branch_clauses: Vec<Clause> = Vec::new();
        
        for (clause, has_prerelease) in parsed_blocks {
            if has_prerelease {
                // Extract the AnyOf and get both branches
                if let Clause::AnyOf(branches) = clause {
                    // First branch is the prerelease branch (has ALWAYS fence)
                    if let Some(Clause::AllOf(prerelease_parts)) = branches.first() {
                        prerelease_branch_clauses.extend(prerelease_parts.clone());
                    }
                    // Second branch is the non-prerelease branch with truncated target
                    if let Some(second) = branches.get(1) {
                        non_prerelease_branch_clauses.push(second.clone());
                    }
                }
            } else {
                // Non-prerelease comparators go ONLY in the non-prerelease branch
                // (base.py:1335-1336). Putting them in the prerelease branch would
                // wrongly exclude same-patch prereleases: e.g. `<2.0.0` (SAMEPATCH)
                // rejects `1.0.0-rc.5` because its patch != 2.0.0's patch.
                non_prerelease_branch_clauses.push(clause);
            }
        }

        // Fence branches must be exactly one AllOf per prerelease block.
        debug_assert!(
            prerelease_branch_clauses.len() >= 2 || any_has_prerelease == false,
            "expected at least the ALWAYS fence + original range in the prerelease branch"
        );

        let prerelease_branch = prerelease_branch_clauses.into_iter().reduce(|acc, c| acc & c).unwrap_or(Clause::Always);
        let non_prerelease_branch = non_prerelease_branch_clauses.into_iter().reduce(|acc, c| acc & c).unwrap_or(Clause::Always);

        Ok(Clause::AnyOf(vec![prerelease_branch.simplify(), non_prerelease_branch.simplify()]))
    }

    /// Parse a hyphen range `A - B`.
    fn parse_hyphen_range(left: &str, right: &str) -> Result<Clause, SemverError> {
        // Parse left (always floor to full version)
        let left_parsed = Self::parse_version_basic(left)?;
        let left_has_prerelease = left_parsed.3.is_some();
        let left_version = Version::from_parts(
            left_parsed.0,
            left_parsed.1.unwrap_or(0),
            left_parsed.2.unwrap_or(0),
            left_parsed.3,
            left_parsed.4,
        );

        let lower = Range::new(
            Operator::Gte,
            left_version.clone(),
            PrereleasePolicy::SamePatch,
            BuildPolicy::Implicit,
        )?;

        // Parse right and determine if partial
        let right_parsed = Self::parse_version_basic(right)?;
        let (upper_op, upper_target) = if right_parsed.1.is_none() {
            // Major only (e.g., "2") -> <3.0.0
            (Operator::Lt, Version::from_parts(right_parsed.0 + 1, 0, 0, None, None))
        } else if right_parsed.2.is_none() {
            // Major.minor only (e.g., "2.3") -> <2.4.0
            (Operator::Lt, Version::from_parts(right_parsed.0, right_parsed.1.unwrap() + 1, 0, None, None))
        } else {
            // Full version -> inclusive
            let right_version = Version::from_parts(
                right_parsed.0,
                right_parsed.1.unwrap(),
                right_parsed.2.unwrap(),
                right_parsed.3,
                right_parsed.4,
            );
            (Operator::Lte, right_version)
        };

        let upper = Range::new(upper_op, upper_target, PrereleasePolicy::SamePatch, BuildPolicy::Implicit)?;

        // Check if left has prerelease -> OR-expansion
        if left_has_prerelease {
            return Self::expand_prerelease_or_hyphen(lower, upper, &left_version);
        }

        Ok(Clause::AllOf(vec![Clause::Range(upper), Clause::Range(lower)]))
    }

    /// Parse version into (major, minor, patch, prerel, build).
    fn parse_version_basic(s: &str) -> Result<(u64, Option<u64>, Option<u64>, Option<Vec<crate::identifiers::PreReleaseIdent>>, Option<Vec<crate::identifiers::BuildIdent>>), SemverError> {
        let re = npm_spec_block_re();
        let caps = re.captures(s).ok_or_else(|| {
            SemverError::invalid_spec(&format!("Invalid npm version '{}'", s))
        })?;

        let major_str = caps.name("major").map(|m| m.as_str()).unwrap_or("0");
        let minor_str = caps.name("minor").map(|m| m.as_str());
        let patch_str = caps.name("patch").map(|m| m.as_str());
        let prerel_str = caps.name("prerel").map(|m| m.as_str());

        let major: u64 = major_str.parse().map_err(|_| SemverError::invalid_spec("Invalid major"))?;
        let minor: Option<u64> = minor_str.and_then(|m| m.parse().ok());
        let patch: Option<u64> = patch_str.and_then(|p| p.parse().ok());

        let prerel = if let Some(pr) = prerel_str {
            if pr.is_empty() {
                Some(vec![])
            } else {
                let ids = crate::identifiers::parse_prerelease_identifiers(pr)
                    .map_err(|e| SemverError::invalid_spec(&e))?;
                Some(ids)
            }
        } else {
            None
        };

        Ok((major, minor, patch, prerel, None))
    }

    /// Parse a single block, returning (clause, has_prerelease).
    fn parse_block_with_flag(block: &str) -> Result<(Clause, bool), SemverError> {
        let re = npm_spec_block_re();
        let caps = re.captures(block).ok_or_else(|| {
            SemverError::invalid_spec(&format!("Invalid npm block '{}'", block))
        })?;

        let op_str = caps.name("op").map(|m| m.as_str()).unwrap_or("");
        let major_str = caps.name("major").map(|m| m.as_str()).unwrap_or("");
        let minor_str = caps.name("minor").map(|m| m.as_str());
        let patch_str = caps.name("patch").map(|m| m.as_str());
        let prerel_str = caps.name("prerel").map(|m| m.as_str());
        let build_str = caps.name("build").map(|m| m.as_str());

        // Wildcard components become None (base.py::EMPTY_VALUES = ['*', 'x', 'X', None]).
        let major = Self::component_value(major_str);
        let minor = minor_str.and_then(Self::component_value);
        let patch = patch_str.and_then(Self::component_value);

        // base.py:1398 — wildcards are incompatible with prerelease/build.
        if (major.is_none() || minor.is_none() || patch.is_none())
            && (prerel_str.is_some() || build_str.is_some())
        {
            return Err(SemverError::invalid_spec(&format!("Invalid NPM spec: '{}'", block)));
        }

        // base.py:1376-1378 — build is only kept for `=`; other ops drop it.
        let kept_build = if build_str.is_some() && op_str == "=" { build_str } else { None };

        let target = Version::from_parts(
            major.unwrap_or(0),
            minor.unwrap_or(0),
            patch.unwrap_or(0),
            match prerel_str {
                Some(pr) => Some(
                    crate::identifiers::parse_prerelease_identifiers(pr)
                        .map_err(SemverError::invalid_spec)?,
                ),
                None => None,
            },
            match kept_build {
                Some(b) => Some(
                    crate::identifiers::parse_build_identifiers(b)
                        .map_err(SemverError::invalid_spec)?,
                ),
                None => None,
            },
        );

        let has_prerelease = !target.prerelease.as_deref().unwrap_or(&[]).is_empty();

        // Caret/tilde must be handled BEFORE the wildcard component logic; they
        // expand partial/wildcard components internally (base.py::parse_simple
        // checks the prefix before any component-based branching).
        if op_str == "^" {
            let clause = Self::expand_caret(&target, minor.is_none(), patch.is_none())?;
            return Ok((clause, has_prerelease));
        }
        if op_str == "~" {
            let clause = Self::expand_tilde(&target, minor.is_none())?;
            return Ok((clause, has_prerelease));
        }

        let operator = match op_str {
            "=" | "" => Operator::Eq,
            "<" => Operator::Lt,
            "<=" => Operator::Lte,
            ">" => Operator::Gt,
            ">=" => Operator::Gte,
            _ => {
                return Err(SemverError::invalid_spec(&format!("Invalid npm block '{}'", block)));
            }
        };

        // Operator-specific wildcard handling (base.py::parse_simple).
        let clause = if major.is_none() {
            // `*`/`x`/`X` alone: only `=`/`>=` allowed (base.py:1380-1384).
            if operator != Operator::Eq && operator != Operator::Gte {
                return Err(SemverError::invalid_spec(&format!("Invalid expression '{}'", block)));
            }
            Self::simple_range(Operator::Gte, Version::from_parts(0, 0, 0, None, None))?
        } else if minor.is_none() {
            // `1.x` / `1.*`: `=` -> >=1.0.0 <2.0.0; `>` -> >=2.0.0; `<=` -> <2.0.0;
            // `>=`/`<` operate on the zero-filled target (base.py:1385-1396, 1422-1456).
            match operator {
                Operator::Eq => Self::bounded_range(
                    Operator::Gte,
                    Version::from_parts(major.unwrap(), 0, 0, None, None),
                    Operator::Lt,
                    Version::from_parts(major.unwrap() + 1, 0, 0, None, None),
                )?,
                Operator::Gt => Self::simple_range(
                    Operator::Gte,
                    Version::from_parts(major.unwrap() + 1, 0, 0, None, None),
                )?,
                Operator::Lte => Self::simple_range(
                    Operator::Lt,
                    Version::from_parts(major.unwrap() + 1, 0, 0, None, None),
                )?,
                Operator::Gte => Self::simple_range(Operator::Gte, target.clone())?,
                Operator::Lt => Self::simple_range(Operator::Lt, target.clone())?,
                _ => {
                    return Err(SemverError::invalid_spec(&format!("Invalid npm block '{}'", block)));
                }
            }
        } else if patch.is_none() {
            // `1.2.x` / `1.2.*`: `=` -> >=1.2.0 <1.3.0; `>` -> >=1.3.0; `<=` -> <1.3.0;
            // `>=`/`<` operate on the zero-filled target.
            match operator {
                Operator::Eq => Self::bounded_range(
                    Operator::Gte,
                    Version::from_parts(major.unwrap(), minor.unwrap(), 0, None, None),
                    Operator::Lt,
                    Version::from_parts(major.unwrap(), minor.unwrap() + 1, 0, None, None),
                )?,
                Operator::Gt => Self::simple_range(
                    Operator::Gte,
                    Version::from_parts(major.unwrap(), minor.unwrap() + 1, 0, None, None),
                )?,
                Operator::Lte => Self::simple_range(
                    Operator::Lt,
                    Version::from_parts(major.unwrap(), minor.unwrap() + 1, 0, None, None),
                )?,
                Operator::Gte => Self::simple_range(Operator::Gte, target.clone())?,
                Operator::Lt => Self::simple_range(Operator::Lt, target.clone())?,
                _ => {
                    return Err(SemverError::invalid_spec(&format!("Invalid npm block '{}'", block)));
                }
            }
        } else if has_prerelease {
            // Full version with prerelease: AnyOf OR-expansion.
            Self::expand_prerelease_or(operator, &target)?
        } else {
            Self::simple_range(operator, target)?
        };

        Ok((clause, has_prerelease))
    }

    /// Check if a component is a wildcard.
    fn is_wildcard(s: &str) -> bool {
        s == "*" || s == "x" || s == "X"
    }

    /// Parse a component string: wildcard -> `None`, otherwise the numeric value.
    fn component_value(s: &str) -> Option<u64> {
        if Self::is_wildcard(s) {
            None
        } else {
            s.parse().ok()
        }
    }

    /// Build a single Range clause (prerelease SAMEPATCH, build IMPLICIT).
    fn simple_range(op: Operator, target: Version) -> Result<Clause, SemverError> {
        Ok(Clause::Range(Range::new(
            op,
            target,
            PrereleasePolicy::SamePatch,
            BuildPolicy::Implicit,
        )?))
    }

    /// Build an AllOf of two Ranges, upper bound first (matches the ground-truth repr order).
    fn bounded_range(op1: Operator, t1: Version, op2: Operator, t2: Version) -> Result<Clause, SemverError> {
        Ok(Clause::AllOf(vec![
            Clause::Range(Range::new(
                op2,
                t2,
                PrereleasePolicy::SamePatch,
                BuildPolicy::Implicit,
            )?),
            Clause::Range(Range::new(
                op1,
                t1,
                PrereleasePolicy::SamePatch,
                BuildPolicy::Implicit,
            )?),
        ]))
    }

    /// Expand caret operator.
    fn expand_caret(target: &Version, minor_is_none: bool, patch_is_none: bool) -> Result<Clause, SemverError> {
        let lower = Range::new(
            Operator::Gte,
            target.clone(),
            PrereleasePolicy::SamePatch,
            BuildPolicy::Implicit,
        )?;

        // base.py::parse_simple CARET branch — checked in this exact order.
        // The upper bound derives from the TRUNCATED target (prerelease stripped),
        // so the next_* prerelease quirks never fire here.
        let truncated = target.truncate_to_patch();
        let upper_target = if target.major > 0 {
            // ^1.2.4 / ^1.x => <2.0.0
            truncated.next_major()
        } else if target.minor.unwrap_or(0) > 0 {
            // ^0.1.2 / ^0.2.x => <0.2.0 / <0.3.0
            truncated.next_minor()
        } else if minor_is_none {
            // ^0.x => <1.0.0
            truncated.next_major()
        } else if patch_is_none {
            // ^0.0.x / ^0.0 => <0.1.0
            truncated.next_minor()
        } else {
            // ^0.0.3 => <0.0.4
            truncated.next_patch()
        };

        let upper = Range::new(Operator::Lt, upper_target, PrereleasePolicy::SamePatch, BuildPolicy::Implicit)?;

        // If target has prerelease, apply OR-expansion
        if target.prerelease.as_deref().map_or(false, |p| !p.is_empty()) {
            return Self::expand_prerelease_or_compound(lower, upper, target);
        }

        Ok(Clause::AllOf(vec![Clause::Range(upper), Clause::Range(lower)]))
    }

    /// Expand tilde operator (base.py::parse_simple TILDE branch).
    fn expand_tilde(target: &Version, minor_is_none: bool) -> Result<Clause, SemverError> {
        let lower = Range::new(
            Operator::Gte,
            target.clone(),
            PrereleasePolicy::SamePatch,
            BuildPolicy::Implicit,
        )?;

        // base.py:1414-1420 — minor absent/wildcard => next_major, else next_minor.
        // Python calls these on the RAW target (no truncate), so the next_* quirks
        // for prerelease targets with patch==0 / minor==0&&patch==0 DO apply.
        let upper_target = if minor_is_none {
            // ~1.x / ~1 => <2.0.0
            target.next_major()
        } else {
            // ~1.2.x / ~1.2 / ~1.2.3 => <1.3.0
            target.next_minor()
        };

        let upper = Range::new(Operator::Lt, upper_target, PrereleasePolicy::SamePatch, BuildPolicy::Implicit)?;

        // If target has prerelease, apply OR-expansion
        if target.prerelease.as_deref().map_or(false, |p| !p.is_empty()) {
            return Self::expand_prerelease_or_compound(lower, upper, target);
        }

        Ok(Clause::AllOf(vec![Clause::Range(upper), Clause::Range(lower)]))
    }


    /// Expand prerelease OR-tree for simple operators.
    fn expand_prerelease_or(op: Operator, target: &Version) -> Result<Clause, SemverError> {
        // Prerelease branch: fence + original range with prerelease target
        let fence_target = Version::from_parts(
            target.major,
            target.minor.unwrap_or(0),
            target.patch.unwrap_or(0) + 1,
            None,
            None,
        );
        let fence = Range::new(Operator::Lt, fence_target, PrereleasePolicy::Always, BuildPolicy::Implicit)?;
        let prerel_range = Range::new(op, target.clone(), PrereleasePolicy::SamePatch, BuildPolicy::Implicit)?;
        let prerel_branch = Clause::AllOf(vec![Clause::Range(fence), Clause::Range(prerel_range)]);

        // Non-prerelease branch: truncated target
        let truncated = target.truncate_to_patch();
        let non_prerel_range = Range::new(op, truncated, PrereleasePolicy::SamePatch, BuildPolicy::Implicit)?;
        let non_prerel_branch = Clause::Range(non_prerel_range);

        Ok(Clause::AnyOf(vec![prerel_branch, non_prerel_branch]))
    }

    /// Expand prerelease OR-tree for compound operators (caret/tilde).
    fn expand_prerelease_or_compound(lower: Range, upper: Range, target: &Version) -> Result<Clause, SemverError> {
        // Prerelease branch: fence + lower with prerelease target
        let fence_target = Version::from_parts(
            target.major,
            target.minor.unwrap_or(0),
            target.patch.unwrap_or(0) + 1,
            None,
            None,
        );
        let fence = Range::new(Operator::Lt, fence_target, PrereleasePolicy::Always, BuildPolicy::Implicit)?;
        let prerel_branch = Clause::AllOf(vec![Clause::Range(fence), Clause::Range(lower)]);

        // Non-prerelease branch: upper + lower with truncated target
        let truncated = target.truncate_to_patch();
        let non_prerel_lower = Range::new(Operator::Gte, truncated, PrereleasePolicy::SamePatch, BuildPolicy::Implicit)?;
        let non_prerel_branch = Clause::AllOf(vec![Clause::Range(upper), Clause::Range(non_prerel_lower)]);

        Ok(Clause::AnyOf(vec![prerel_branch, non_prerel_branch]))
    }

    /// Expand prerelease OR-tree for hyphen ranges.
    fn expand_prerelease_or_hyphen(lower: Range, upper: Range, target: &Version) -> Result<Clause, SemverError> {
        // Prerelease branch: fence + lower with prerelease target
        let fence_target = Version::from_parts(
            target.major,
            target.minor.unwrap_or(0),
            target.patch.unwrap_or(0) + 1,
            None,
            None,
        );
        let fence = Range::new(Operator::Lt, fence_target, PrereleasePolicy::Always, BuildPolicy::Implicit)?;
        let prerel_branch = Clause::AllOf(vec![Clause::Range(fence), Clause::Range(lower)]);

        // Non-prerelease branch: upper + lower with truncated target
        let truncated = target.truncate_to_patch();
        let non_prerel_lower = Range::new(Operator::Gte, truncated, PrereleasePolicy::SamePatch, BuildPolicy::Implicit)?;
        let non_prerel_branch = Clause::AllOf(vec![Clause::Range(upper), Clause::Range(non_prerel_lower)]);

        Ok(Clause::AnyOf(vec![prerel_branch, non_prerel_branch]))
    }

    /// Check if a version matches this spec.
    pub fn match_version(&self, version: &Version) -> bool {
        self.clause.matches(version)
    }
}

impl fmt::Display for NpmSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.clause)
    }
}
