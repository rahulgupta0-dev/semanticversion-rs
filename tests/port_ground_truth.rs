//! Ground truth executable specification test.
//!
//! Asserts exact parity between our Rust types and the Python `semantic_version` baseline behavior
//! verified against Python 3 in the reference venv.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use semantic_version::version::{Version, compare};
use semantic_version::error::SemverError;

fn hash_val<T: Hash>(val: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    val.hash(&mut hasher);
    hasher.finish()
}

#[test]
fn test_build_diff_truth_table() {
    let v1 = Version::parse("1.0.0+a").unwrap();
    let v2 = Version::parse("1.0.0+b").unwrap();

    // 1.0.0+a vs 1.0.0+b (differing builds)
    assert_eq!(v1 == v2, false, "eq is false for differing builds");
    assert_eq!(v1 != v2, true, "ne is true for differing builds");
    assert_eq!(v1.precedence_lt(&v2), false, "lt is false for same precedence key");
    assert_eq!(v1.precedence_le(&v2), true, "le is true for same precedence key");
    assert_eq!(v1.precedence_gt(&v2), false, "gt is false for same precedence key");
    assert_eq!(v1.precedence_ge(&v2), true, "ge is true for same precedence key");
    assert_ne!(hash_val(&v1), hash_val(&v2), "hash differs when builds differ");

    // 1.0.0+a vs 1.0.0+a (identical)
    let v1_again = Version::parse("1.0.0+a").unwrap();
    assert_eq!(v1 == v1_again, true, "eq is true for identical version");
    assert_eq!(v1.precedence_le(&v1_again), true);
    assert_eq!(v1.precedence_ge(&v1_again), true);
    assert_eq!(v1.precedence_lt(&v1_again), false);
    assert_eq!(v1.precedence_gt(&v1_again), false);
    assert_eq!(hash_val(&v1), hash_val(&v1_again), "hash equal for identical version");

    // 1.0.0 vs 1.0.0+build
    let v_no_build = Version::parse("1.0.0").unwrap();
    let v_with_build = Version::parse("1.0.0+build").unwrap();
    assert_eq!(v_no_build == v_with_build, false, "eq is false between release and release+build");
}

#[test]
fn test_display_and_repr() {
    let v = Version::parse("1.2.3-rc.1+b").unwrap();
    assert_eq!(v.to_string(), "1.2.3-rc.1+b");

    let v_simple = Version::parse("1.2.3").unwrap();
    let repr_string = format!("Version('{s}')", s = v_simple);
    assert_eq!(repr_string, "Version('1.2.3')");
}

#[test]
fn test_parse_partial_fields() {
    let v = Version::parse_partial("1.0").unwrap();
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, Some(0));
    assert_eq!(v.patch, None);
}

#[test]
fn test_single_quoted_error_messages() {
    let err = Version::parse("garbage").unwrap_err();
    assert_eq!(err.to_string(), "Invalid version string: 'garbage'");
    assert!(matches!(err, SemverError::InvalidVersion(_)));
}

#[test]
fn test_compare_function_ground_truth() {
    // Differing builds: compare() returns Ok(None) because precedence is equal but v1 != v2
    assert_eq!(compare("1.0.0+a", "1.0.0+b").unwrap(), None);
    // Identical versions: compare() returns Ok(Some(0))
    assert_eq!(compare("1.0.0", "1.0.0").unwrap(), Some(0));
    // Different precedence
    assert_eq!(compare("0.1.0", "0.1.1").unwrap(), Some(-1));
    assert_eq!(compare("0.1.1", "0.1.0").unwrap(), Some(1));
}
