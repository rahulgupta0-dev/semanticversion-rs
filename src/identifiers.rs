//! Pre-release and build-metadata identifier types.
//!
//! ## Python mapping
//!
//! Python `base.py` defines three identifier classes (base.py:17–76):
//! - `MaxIdentifier`     — sentinel meaning "no prerelease" (sorts last)
//! - `NumericIdentifier` — an all-digit identifier, compared as integer
//! - `AlphaIdentifier`   — a mixed/alpha identifier, compared as ASCII bytes
//!
//! We unify them into a single `PreReleaseIdent` enum with the same ordering.
//!
//! ## SemVer 2.0.0 rules (also tested in test_spec.py::FormatTests::test_precedence)
//!
//! Prerelease identifier ordering (SemVer §11.4):
//!   - Identifiers consisting only of digits are compared numerically.
//!   - Identifiers with letters/hyphens are compared lexicographically (ASCII).
//!   - Numeric identifiers ALWAYS have lower precedence than alphanumeric ones.
//!   - A larger set of prerelease fields has higher precedence if all preceding
//!     fields are equal.
//!   - A version WITH prerelease has lower precedence than the same version WITHOUT.
//!
//! ## `MaxIdentifier` sentinel
//!
//! Python uses `MaxIdentifier()` as the *sole* entry in the prerelease tuple when
//! there IS no prerelease — so that e.g. `1.0.0` sorts AFTER `1.0.0-rc.1` even
//! though the prerelease tuple is shorter.  We mirror this with `PreReleaseIdent::Max`.
//!
//! ## Build-metadata identifiers
//!
//! Build metadata (§10) is ignored for version precedence (it's not included in
//! `_cmp_precedence_key`).  Build identifiers follow the same digit/alpha split
//! but leading zeros ARE allowed (§9 build allows any ASCII alphanumeric + hyphen).
//! We reuse `BuildIdent` which is structurally identical but allowed to have leading zeros.

/// A single dot-separated **pre-release** identifier.
///
/// Ordering: `Numeric(n) < Alpha(s) < Max` for all `n`, `s`.
/// `Numeric` values compared as `u64`; `Alpha` values compared as ASCII bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreReleaseIdent {
    /// An all-digit identifier (no leading zeros for versions < 3.0, allowed in test).
    Numeric(u64),
    /// A mixed/alpha identifier — stored as a `String` (guaranteed ASCII by caller).
    Alpha(String),
    /// Sentinel for "no prerelease field" — sorts after all real prerelease ids.
    Max,
}

impl PartialOrd for PreReleaseIdent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PreReleaseIdent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        use PreReleaseIdent::*;
        match (self, other) {
            // Max == Max
            (Max, Max) => Equal,
            // Max is always greatest
            (Max, _) => Greater,
            (_, Max) => Less,
            // Numeric < Alpha (SemVer §11.4.1 — numeric < non-numeric)
            (Numeric(_), Alpha(_)) => Less,
            (Alpha(_), Numeric(_)) => Greater,
            // Numeric vs Numeric: integer comparison
            (Numeric(a), Numeric(b)) => a.cmp(b),
            // Alpha vs Alpha: lexicographic ASCII byte comparison
            (Alpha(a), Alpha(b)) => a.as_bytes().cmp(b.as_bytes()),
        }
    }
}

/// A single dot-separated **build-metadata** identifier.
///
/// Build metadata is opaque — not used in version precedence comparisons, but
/// IS compared for equality (two versions with different build metadata are not equal).
/// Leading zeros are allowed in numeric build identifiers (§10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildIdent {
    /// A numeric build identifier (may have leading zeros in the original string).
    /// We keep it as a `String` to preserve the original representation.
    Numeric(String),
    /// An alphanumeric build identifier.
    Alpha(String),
}

impl BuildIdent {
    /// Parse a single build identifier string into a `BuildIdent`.
    /// Leading zeros are allowed (§10).
    pub fn parse(s: &str) -> Self {
        if s.bytes().all(|b| b.is_ascii_digit()) {
            BuildIdent::Numeric(s.to_owned())
        } else {
            BuildIdent::Alpha(s.to_owned())
        }
    }

    /// The original string representation of this identifier.
    pub fn as_str(&self) -> &str {
        match self {
            BuildIdent::Numeric(s) | BuildIdent::Alpha(s) => s.as_str(),
        }
    }
}

impl PartialOrd for BuildIdent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BuildIdent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Build metadata sort (used only by precedence_key / stable sort).
        // Same rules as prerelease but without the Max sentinel.
        use std::cmp::Ordering::*;
        use BuildIdent::*;
        match (self, other) {
            (Numeric(_), Alpha(_)) => Less,
            (Alpha(_), Numeric(_)) => Greater,
            (Numeric(a), Numeric(b)) => {
                // Compare numerically by parsing — leading zeros preserved in string
                // but actual numeric value is what matters for ordering.
                let av: u64 = a.parse().unwrap_or(0);
                let bv: u64 = b.parse().unwrap_or(0);
                av.cmp(&bv)
            }
            (Alpha(a), Alpha(b)) => a.as_bytes().cmp(b.as_bytes()),
        }
    }
}

/// Parse a dot-separated pre-release string (e.g. `"rc1.3.14"`) into a `Vec<PreReleaseIdent>`.
///
/// Returns an error if any identifier is empty or has a leading zero in a numeric field.
///
/// Rules (SemVer §9 / base.py:366–375):
/// - Empty identifier (`""`) → error
/// - All-digit with leading zero and not `"0"` → error
/// - All-digit → `Numeric`
/// - Otherwise → `Alpha`
pub fn parse_prerelease_identifiers(s: &str) -> Result<Vec<PreReleaseIdent>, String> {
    if s.is_empty() {
        return Ok(vec![]);
    }
    s.split('.')
        .map(|part| {
            if part.is_empty() {
                return Err(format!("Empty identifier in prerelease {:?}", s));
            }
            if part.bytes().all(|b| b.is_ascii_digit()) {
                // All-digit: check for leading zeros
                if part.len() > 1 && part.starts_with('0') {
                    return Err(format!("Leading zero in prerelease identifier {:?}", part));
                }
                // Numeric overflow falls back to Alpha — Python's int is
                // arbitrary-precision, so a u64-overflowing all-digit string
                // is stored as a raw string identifier.
                if let Ok(n) = part.parse::<u64>() {
                    Ok(PreReleaseIdent::Numeric(n))
                } else {
                    Ok(PreReleaseIdent::Alpha(part.to_owned()))
                }
            } else {
                Ok(PreReleaseIdent::Alpha(part.to_owned()))
            }
        })
        .collect()
}

/// Parse a dot-separated build-metadata string into `Vec<BuildIdent>`.
///
/// Leading zeros ARE allowed in numeric build identifiers.
pub fn parse_build_identifiers(s: &str) -> Result<Vec<BuildIdent>, String> {
    if s.is_empty() {
        return Ok(vec![]);
    }
    s.split('.')
        .map(|part| {
            if part.is_empty() {
                return Err(format!("Empty identifier in build {:?}", s));
            }
            Ok(BuildIdent::parse(part))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering::*;

    #[test]
    fn test_prerelease_ident_ordering() {
        // Numeric < Alpha (SemVer §11.4.1)
        assert_eq!(PreReleaseIdent::Numeric(9).cmp(&PreReleaseIdent::Alpha("a".into())), Less);
        assert_eq!(PreReleaseIdent::Alpha("a".into()).cmp(&PreReleaseIdent::Numeric(9)), Greater);

        // Numeric < Max, Alpha < Max
        assert_eq!(PreReleaseIdent::Numeric(0).cmp(&PreReleaseIdent::Max), Less);
        assert_eq!(PreReleaseIdent::Alpha("z".into()).cmp(&PreReleaseIdent::Max), Less);
        assert_eq!(PreReleaseIdent::Max.cmp(&PreReleaseIdent::Max), Equal);

        // Numeric comparison by value
        assert_eq!(PreReleaseIdent::Numeric(2).cmp(&PreReleaseIdent::Numeric(11)), Less);
        assert_eq!(PreReleaseIdent::Numeric(11).cmp(&PreReleaseIdent::Numeric(2)), Greater);

        // Alpha comparison: ASCII lexicographic
        assert_eq!(
            PreReleaseIdent::Alpha("aa".into()).cmp(&PreReleaseIdent::Alpha("ab".into())),
            Less
        );
    }

    #[test]
    fn test_parse_prerelease_identifiers_valid() {
        let ids = parse_prerelease_identifiers("rc1.3.14").unwrap();
        assert_eq!(ids, vec![
            PreReleaseIdent::Alpha("rc1".into()),
            PreReleaseIdent::Numeric(3),
            PreReleaseIdent::Numeric(14),
        ]);
    }

    #[test]
    fn test_parse_prerelease_leading_zero_error() {
        assert!(parse_prerelease_identifiers("01").is_err());
        assert!(parse_prerelease_identifiers("a.00").is_err());
        // "0" alone is fine
        assert!(parse_prerelease_identifiers("0").is_ok());
    }

    #[test]
    fn test_parse_prerelease_empty_ident_error() {
        assert!(parse_prerelease_identifiers("a..b").is_err());
    }

    #[test]
    fn test_build_ident_leading_zeros_ok() {
        // Build metadata allows leading zeros
        let ids = parse_build_identifiers("01.0a.2012-01-01").unwrap();
        assert_eq!(ids[0], BuildIdent::Numeric("01".into()));
        assert_eq!(ids[1], BuildIdent::Alpha("0a".into()));
    }

    // Mirror: test_spec.py::FormatTests::test_precedence — identifier ordering chain
    // 1.0.0-alpha < 1.0.0-alpha.1 < 1.0.0-alpha.beta < 1.0.0-beta < ...
    #[test]
    fn test_prerelease_ordering_chain() {
        let alpha  = vec![PreReleaseIdent::Alpha("alpha".into())];
        let alpha1 = vec![PreReleaseIdent::Alpha("alpha".into()), PreReleaseIdent::Numeric(1)];
        let alpha_beta = vec![PreReleaseIdent::Alpha("alpha".into()), PreReleaseIdent::Alpha("beta".into())];
        let beta   = vec![PreReleaseIdent::Alpha("beta".into())];

        assert!(alpha < alpha1);       // longer set with equal prefix wins
        assert!(alpha1 < alpha_beta);  // Numeric(1) < Alpha("beta")
        assert!(alpha_beta < beta);    // "alpha" < "beta" lexicographically
    }
}
