//! Port tests — parsing slice.
//!
//! Mirrors `tests/original/test_parsing.py::ParsingTestCase` and
//! the parsing portions of `tests/original/test_base.py::VersionTestCase`.
//!
//! IMPORTANT: these are NATIVE Rust #[test]s — they do NOT import Python.
//! The original test files in tests/original/ are unmodified.

use semantic_version::version::{Version, validate, compare};
use semantic_version::identifiers::PreReleaseIdent;

// ---------------------------------------------------------------------------
// Mirror: test_parsing.py::ParsingTestCase::test_invalid
// ---------------------------------------------------------------------------

#[test]
fn parse_invalid_empty() {
    assert!(Version::parse("").is_err(), "empty string must be invalid");
}

#[test]
fn parse_invalid_major_only() {
    assert!(Version::parse("0").is_err());
}

#[test]
fn parse_invalid_major_minor_only() {
    assert!(Version::parse("0.1").is_err());
}

#[test]
fn parse_invalid_alpha_suffix() {
    assert!(Version::parse("0.1.4a").is_err());
}

#[test]
fn parse_invalid_four_components() {
    assert!(Version::parse("0.1.1.1").is_err());
}

#[test]
fn parse_invalid_prerelease_comma() {
    // "0.1.2-rc23,1" — comma in prerelease
    assert!(Version::parse("0.1.2-rc23,1").is_err());
}

// ---------------------------------------------------------------------------
// Mirror: test_parsing.py::ParsingTestCase::test_simple (round-trip)
// ---------------------------------------------------------------------------

#[test]
fn parse_valid_simple_round_trip() {
    let valids = [
        "0.1.1",
        "0.1.2-rc1",
        "0.1.2-rc1.3.4",
        "0.1.2+build42-12.2012-01-01.12h23",
        "0.1.2-rc1.3-14.15+build.2012-01-01.11h34",
    ];
    for s in &valids {
        let v = Version::parse(s).unwrap_or_else(|e| panic!("parse({:?}) failed: {:?}", s, e));
        assert_eq!(v.to_string(), *s, "round-trip failed for {:?}", s);
    }
}

// ---------------------------------------------------------------------------
// Mirror: test_parsing.py::ParsingTestCase::test_kwargs (component extraction)
// ---------------------------------------------------------------------------

#[test]
fn parse_valid_fields() {
    // (version_string, major, minor, patch, prerelease_len, build_len)
    let cases: &[(&str, u64, u64, u64, usize, usize)] = &[
        ("0.1.1", 0, 1, 1, 0, 0),
        ("0.1.2-rc1", 0, 1, 2, 1, 0),
        ("0.1.2-rc1.3.4", 0, 1, 2, 3, 0),
        ("0.1.2+build42-12.2012-01-01.12h23", 0, 1, 2, 0, 3),
        ("0.1.2-rc1.3-14.15+build.2012-01-01.11h34", 0, 1, 2, 3, 3),
    ];
    for (s, maj, min, pat, prel_len, build_len) in cases {
        let v = Version::parse(s).unwrap_or_else(|e| panic!("parse({:?}): {:?}", s, e));
        assert_eq!(v.major, *maj, "{}", s);
        assert_eq!(v.minor, Some(*min), "{}", s);
        assert_eq!(v.patch, Some(*pat), "{}", s);
        let pr = v.prerelease.as_deref().unwrap_or(&[]);
        assert_eq!(pr.len(), *prel_len, "{} prerelease len", s);
        let bd = v.build.as_deref().unwrap_or(&[]);
        assert_eq!(bd.len(), *build_len, "{} build len", s);
    }
}

// ---------------------------------------------------------------------------
// Mirror: test_base.py::VersionTestCase::test_parsing (full field check)
// ---------------------------------------------------------------------------

#[test]
fn parse_base_versions_fields() {

    let cases: &[(&str, u64, u64, u64, &[&str], &[&str])] = &[
        ("1.0.0-alpha",               1, 0, 0, &["alpha"],               &[]),
        ("1.0.0-alpha.1",             1, 0, 0, &["alpha", "1"],          &[]),
        ("1.0.0-beta.2",              1, 0, 0, &["beta", "2"],           &[]),
        ("1.0.0-beta.11",             1, 0, 0, &["beta", "11"],          &[]),
        ("1.0.0-rc.1",                1, 0, 0, &["rc", "1"],             &[]),
        ("1.0.0-rc.1+build.1",        1, 0, 0, &["rc", "1"],             &["build", "1"]),
        ("1.0.0",                     1, 0, 0, &[],                      &[]),
        ("1.0.0+0.3.7",              1, 0, 0, &[],                      &["0", "3", "7"]),
        ("1.3.7+build",              1, 3, 7, &[],                      &["build"]),
        ("1.3.7+build.2.b8f12d7",   1, 3, 7, &[],                      &["build", "2", "b8f12d7"]),
        ("1.3.7+build.11.e0f985a",   1, 3, 7, &[],                      &["build", "11", "e0f985a"]),
        ("1.1.1",                     1, 1, 1, &[],                      &[]),
        ("1.1.2",                     1, 1, 2, &[],                      &[]),
        ("1.1.3-rc4.5",              1, 1, 3, &["rc4", "5"],            &[]),
        ("1.1.3-rc42.3-14-15.24+build.2012-04-13.223",
                                      1, 1, 3, &["rc42", "3-14-15", "24"], &["build", "2012-04-13", "223"]),
        ("1.1.3+build.2012-04-13.HUY.alpha-12.1",
                                      1, 1, 3, &[],                      &["build", "2012-04-13", "HUY", "alpha-12", "1"]),
    ];

    for (s, maj, min, pat, prel, bld) in cases {
        let v = Version::parse(s).unwrap_or_else(|e| panic!("parse({:?}): {:?}", s, e));
        assert_eq!(v.major, *maj, "{s}");
        assert_eq!(v.minor, Some(*min), "{s}");
        assert_eq!(v.patch, Some(*pat), "{s}");

        // Check prerelease identifiers as strings
        let pr = v.prerelease.as_deref().unwrap_or(&[]);
        assert_eq!(pr.len(), prel.len(), "{s} prerelease count");
        for (got, expected) in pr.iter().zip(prel.iter()) {
            let got_str = match got {
                PreReleaseIdent::Numeric(n) => n.to_string(),
                PreReleaseIdent::Alpha(a) => a.clone(),
                PreReleaseIdent::Max => "<MAX>".into(),
            };
            assert_eq!(&got_str, expected, "{s} prerelease ident");
        }

        // Check build identifiers as strings
        let bd = v.build.as_deref().unwrap_or(&[]);
        assert_eq!(bd.len(), bld.len(), "{s} build count");
        for (got, expected) in bd.iter().zip(bld.iter()) {
            assert_eq!(got.as_str(), *expected, "{s} build ident");
        }
    }
}

// ---------------------------------------------------------------------------
// Mirror: test_base.py::VersionTestCase::test_str (Display / repr)
// ---------------------------------------------------------------------------

#[test]
fn version_display_round_trip() {
    let versions = [
        "1.0.0-alpha",
        "1.0.0-alpha.1",
        "1.0.0-beta.2",
        "1.0.0-beta.11",
        "1.0.0-rc.1",
        "1.0.0-rc.1+build.1",
        "1.0.0",
        "1.0.0+0.3.7",
        "1.3.7+build",
        "1.3.7+build.2.b8f12d7",
        "1.3.7+build.11.e0f985a",
        "1.1.1",
        "1.1.2",
        "1.1.3-rc4.5",
        "1.1.3-rc42.3-14-15.24+build.2012-04-13.223",
        "1.1.3+build.2012-04-13.HUY.alpha-12.1",
    ];
    for s in &versions {
        let v = Version::parse(s).unwrap_or_else(|e| panic!("parse({:?}): {:?}", s, e));
        assert_eq!(v.to_string(), *s, "Display round-trip");
    }
}

// ---------------------------------------------------------------------------
// Mirror: test_base.py::TopLevelTestCase::test_validate_valid
// ---------------------------------------------------------------------------

#[test]
fn validate_valid_strings() {
    let valids = [
        "1.0.0-alpha", "1.0.0-alpha.1", "1.0.0-beta.2", "1.0.0-beta.11",
        "1.0.0-rc.1", "1.0.0-rc.1+build.1", "1.0.0", "1.0.0+0.3.7",
        "1.3.7+build", "1.3.7+build.2.b8f12d7", "1.3.7+build.11.e0f985a",
        "1.1.1", "1.1.2", "1.1.3-rc4.5",
        "1.1.3-rc42.3-14-15.24+build.2012-04-13.223",
        "1.1.3+build.2012-04-13.HUY.alpha-12.1",
    ];
    for s in &valids {
        assert!(validate(s), "{:?} should be valid", s);
    }
}

// ---------------------------------------------------------------------------
// Mirror: test_base.py::TopLevelTestCase::test_validate_invalid
// ---------------------------------------------------------------------------

#[test]
fn validate_invalid_strings() {
    let invalids = [
        "1", "v1", "1.2.3.4", "1.2", "1.2a3", "1.2.3a4",
        "v12.34.5", "1.2.3+4+5",
    ];
    for s in &invalids {
        assert!(!validate(s), "{:?} should be invalid", s);
    }
}

// ---------------------------------------------------------------------------
// Mirror: test_base.py::TopLevelTestCase::test_compare
// ---------------------------------------------------------------------------

#[test]
fn compare_versions() {
    // (a, b, expected: Some(-1/0/1) or None for NotImplemented)
    let cases: &[(&str, &str, Option<i32>)] = &[
        ("0.1.0", "0.1.1", Some(-1)),
        ("0.1.1", "0.1.1", Some(0)),
        ("0.1.1", "0.1.0", Some(1)),
        ("0.1.0-alpha", "0.1.0", Some(-1)),
        ("0.1.0-alpha+2", "0.1.0-alpha", None),  // NotImplemented — build-only diff
    ];
    for (a, b, expected) in cases {
        let result = compare(a, b).unwrap_or_else(|e| panic!("compare({:?},{:?}): {:?}", a, b, e));
        assert_eq!(result, *expected, "compare({:?}, {:?})", a, b);
    }
}

// ---------------------------------------------------------------------------
// Mirror: test_spec.py::FormatTests::test_precedence (ordering chain)
// ---------------------------------------------------------------------------

#[test]
fn precedence_ordering_chain() {
    // SemVer spec §11 chain: 1.0.0-alpha < 1.0.0-alpha.1 < 1.0.0-alpha.beta
    // < 1.0.0-beta < 1.0.0-beta.2 < 1.0.0-beta.11 < 1.0.0-rc.1 < 1.0.0
    let ordered = [
        "1.0.0-alpha",
        "1.0.0-alpha.1",
        "1.0.0-alpha.beta",
        "1.0.0-beta",
        "1.0.0-beta.2",
        "1.0.0-beta.11",
        "1.0.0-rc.1",
        "1.0.0",
    ];
    let parsed: Vec<Version> = ordered.iter().map(|s| Version::parse(s).unwrap()).collect();
    for i in 0..parsed.len() {
        for j in 0..parsed.len() {
            match i.cmp(&j) {
                std::cmp::Ordering::Less => {
                    assert!(parsed[i].precedence_lt(&parsed[j]),
                        "{} should be < {}", ordered[i], ordered[j]);
                }
                std::cmp::Ordering::Equal => {
                    assert!(parsed[i].cmp_precedence_key() == parsed[j].cmp_precedence_key());
                }
                std::cmp::Ordering::Greater => {
                    assert!(parsed[i].precedence_gt(&parsed[j]),
                        "{} should be > {}", ordered[i], ordered[j]);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Mirror: test_parsing.py::ComparisonTestCase::test_unordered
// Build-only differences: not < or >, but also != (not equal)
// ---------------------------------------------------------------------------

#[test]
fn build_only_diff_is_unordered() {
    // Ground truth (2026-08-01):
    //   1.0.0+a == 1.0.0+b  -> False (build IS in __eq__)
    //   1.0.0+a <  1.0.0+b  -> False (same precedence key)
    //   1.0.0+a <= 1.0.0+b  -> True  (same precedence key, <= is True)
    //   1.0.0+a >  1.0.0+b  -> False
    let groups: &[&[&str]] = &[
        &["1.0.0-rc.1", "1.0.0-rc.1+build.1"],
        &["1.0.0", "1.0.0+0.3.7"],
        &["1.3.7", "1.3.7+build", "1.3.7+build.2.b8f12d7", "1.3.7+build.11.e0f985a"],
    ];
    for group in groups {
        for i in 0..group.len() {
            for j in 0..group.len() {
                let vi = Version::parse(group[i]).unwrap();
                let vj = Version::parse(group[j]).unwrap();
                if i == j {
                    assert_eq!(vi, vj);
                } else {
                    assert_ne!(vi, vj, "{} == {}", group[i], group[j]);
                    // Same precedence — neither lt nor gt
                    assert!(!vi.precedence_lt(&vj), "{} should not be < {}", group[i], group[j]);
                    assert!(vi.precedence_le(&vj),  "{} should be <= {}", group[i], group[j]);
                    assert!(!vj.precedence_gt(&vi), "{} should not be > {}", group[j], group[i]);
                    assert!(vj.precedence_ge(&vi),  "{} should be >= {}", group[j], group[i]);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Mirror: test_base.py::VersionTestCase::test_hash
// ---------------------------------------------------------------------------

#[test]
fn hash_equal_versions_same() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(Version::parse("0.1.0").unwrap());
    set.insert(Version::parse("0.1.0").unwrap());
    assert_eq!(set.len(), 1, "two identical versions should hash to same");
}

// ---------------------------------------------------------------------------
// Mirror: test_base.py::VersionTestCase::test_bump_clean_versions (subset)
// ---------------------------------------------------------------------------

#[test]
fn next_major_clean() {
    let v = Version::parse("1.0.0+build").unwrap();
    let n = v.next_major();
    assert_eq!(n.major, 2);
    assert_eq!(n.minor, Some(0));
    assert_eq!(n.patch, Some(0));
    assert_eq!(n.prerelease.as_deref().unwrap(), &[]);
    assert_eq!(n.build.as_deref().unwrap(), &[]);
}

#[test]
fn next_minor_clean() {
    let v = Version::parse("1.0.0+build").unwrap();
    let n = v.next_minor();
    assert_eq!((n.major, n.minor, n.patch), (1, Some(1), Some(0)));
}

#[test]
fn next_patch_clean() {
    let v = Version::parse("1.0.0+build").unwrap();
    let n = v.next_patch();
    assert_eq!((n.major, n.minor, n.patch), (1, Some(0), Some(1)));
}

// Prerelease-has-prerelease bump edge cases (base.py:133–179)
#[test]
fn next_major_prerelease_at_major_boundary() {
    // 1.0.0-pre → next_major → 1.0.0 (promotes the prerelease, minor==patch==0)
    let v = Version::parse("1.0.0-pre+build").unwrap();
    let n = v.next_major();
    assert_eq!((n.major, n.minor, n.patch), (1, Some(0), Some(0)));
    assert_eq!(n.prerelease.as_deref().unwrap(), &[]);
}

#[test]
fn next_major_prerelease_non_zero_minor() {
    // 1.1.0-pre → next_major → 2.0.0
    let v = Version::parse("1.1.0-pre+build").unwrap();
    let n = v.next_major();
    assert_eq!((n.major, n.minor, n.patch), (2, Some(0), Some(0)));
}

#[test]
fn next_minor_prerelease_at_minor_boundary() {
    // 1.0.0-pre → next_minor → 1.0.0 (patch==0, promotes the prerelease)
    let v = Version::parse("1.0.0-pre+build").unwrap();
    let n = v.next_minor();
    assert_eq!((n.major, n.minor, n.patch), (1, Some(0), Some(0)));
}

#[test]
fn next_patch_prerelease() {
    // 1.0.0-pre → next_patch → 1.0.0 (promotes the prerelease)
    let v = Version::parse("1.0.0-pre+build").unwrap();
    let n = v.next_patch();
    assert_eq!((n.major, n.minor, n.patch), (1, Some(0), Some(0)));
}

// ---------------------------------------------------------------------------
// Mirror: test_base.py::VersionTestCase::test_truncate
// ---------------------------------------------------------------------------

#[test]
fn truncate_to_patch() {
    let v = Version::parse("3.2.1-pre+build").unwrap();
    let t = v.truncate();
    assert_eq!(t.to_string(), "3.2.1");
}

// ---------------------------------------------------------------------------
// Leading zero rejection (test_spec.py::FormatTests::test_major_minor_patch)
// ---------------------------------------------------------------------------

#[test]
fn leading_zeros_rejected() {
    assert!(Version::parse("1.2.01").is_err(), "leading zero in patch");
    assert!(Version::parse("1.02.1").is_err(), "leading zero in minor");
    assert!(Version::parse("01.2.1").is_err(), "leading zero in major");
    assert!(Version::parse("0.0.0").is_ok(),   "all-zero is valid");
}

// Leading zero in prerelease identifier (test_spec.py::FormatTests::test_prerelease)
#[test]
fn leading_zero_prerelease_rejected() {
    assert!(Version::parse("1.2.3-a0.01").is_err(), "leading zero in prerelease numeric");
    assert!(Version::parse("1.2.3-00").is_err(), "leading zero prerelease 00");
    assert!(Version::parse("1.2.3-0a.0.000zz").is_ok(), "mixed starts with 0 but alphanumeric");
}
