//! Native Rust tests for clause.rs — matching logic, flags, policies.
//!
//! Mirrors range-matching logic from `test_spec.py::SpecItemTestCase` and `test_match.py::MatchTestCase`.

use semantic_version::clause::{BuildPolicy, Clause, Operator, PrereleasePolicy, Range};
use semantic_version::version::Version;

fn make_range(op: Operator, target_str: &str, pre_pol: PrereleasePolicy, build_pol: BuildPolicy) -> Range {
    let target = Version::parse(target_str).unwrap();
    Range::new(op, target, pre_pol, build_pol).unwrap()
}

#[test]
fn test_range_equal_matching() {
    let r = make_range(Operator::Eq, "0.1.0", PrereleasePolicy::Natural, BuildPolicy::Implicit);
    assert!(r.matches(&Version::parse("0.1.0").unwrap()));
    assert!(r.matches(&Version::parse("0.1.0+build1").unwrap()), "Implicit build policy ignores build");
    assert!(!r.matches(&Version::parse("0.0.1").unwrap()));
    assert!(!r.matches(&Version::parse("0.1.0-rc1").unwrap()));
    assert!(!r.matches(&Version::parse("0.2.0").unwrap()));
}

#[test]
fn test_range_strict_build_matching() {
    // ==0.1.2+build3.14 (target has build -> auto BUILD_STRICT)
    let r = make_range(Operator::Eq, "0.1.2+build3.14", PrereleasePolicy::Natural, BuildPolicy::Implicit);
    assert_eq!(r.build_policy, BuildPolicy::Strict, "Target with build forces BUILD_STRICT");
    assert!(r.matches(&Version::parse("0.1.2+build3.14").unwrap()));
    assert!(!r.matches(&Version::parse("0.1.2-rc+build3.14").unwrap()));
    assert!(!r.matches(&Version::parse("0.1.2+build3.15").unwrap()));
}

#[test]
fn test_range_lt_prerelease_natural() {
    // <0.1.1 (PrereleasePolicy::Natural)
    let r = make_range(Operator::Lt, "0.1.1", PrereleasePolicy::Natural, BuildPolicy::Implicit);
    assert!(r.matches(&Version::parse("0.1.0").unwrap()));
    assert!(r.matches(&Version::parse("0.0.0").unwrap()));
    assert!(!r.matches(&Version::parse("0.1.1").unwrap()));
    assert!(!r.matches(&Version::parse("0.1.1-alpha").unwrap()), "<0.1.1 natural excludes 0.1.1-alpha");
}

#[test]
fn test_range_lt_prerelease_always() {
    // <0.1.1- (PrereleasePolicy::Always, bound had trailing '-')
    let r = make_range(Operator::Lt, "0.1.1", PrereleasePolicy::Always, BuildPolicy::Implicit);
    assert!(r.matches(&Version::parse("0.1.0").unwrap()));
    assert!(r.matches(&Version::parse("0.1.1-alpha").unwrap()), "<0.1.1- always matches 0.1.1-alpha");
    assert!(r.matches(&Version::parse("0.1.1-alpha+4").unwrap()));
    assert!(!r.matches(&Version::parse("0.1.1").unwrap()));
    assert!(!r.matches(&Version::parse("0.2.0").unwrap()));
}

#[test]
fn test_range_gte_prerelease() {
    // >=0.2.3-rc2
    let r = make_range(Operator::Gte, "0.2.3-rc2", PrereleasePolicy::Natural, BuildPolicy::Implicit);
    assert!(r.matches(&Version::parse("0.2.3-rc3").unwrap()));
    assert!(r.matches(&Version::parse("0.2.3").unwrap()));
    assert!(r.matches(&Version::parse("0.2.3+1").unwrap()));
    assert!(r.matches(&Version::parse("0.2.3-rc2").unwrap()));
    assert!(r.matches(&Version::parse("0.2.3-rc2+1").unwrap()));
    assert!(!r.matches(&Version::parse("0.2.3-rc1").unwrap()));
    assert!(!r.matches(&Version::parse("0.2.2").unwrap()));
}

#[test]
fn test_range_neq_strict_build() {
    // !=0.2.3-rc2+12
    let r = make_range(Operator::Neq, "0.2.3-rc2+12", PrereleasePolicy::Natural, BuildPolicy::Implicit);
    assert!(r.matches(&Version::parse("0.2.3-rc3").unwrap()));
    assert!(r.matches(&Version::parse("0.2.3").unwrap()));
    assert!(r.matches(&Version::parse("0.2.3-rc2+1").unwrap()));
    assert!(!r.matches(&Version::parse("0.2.3-rc2+12").unwrap()), "exact build match fails NEQ");
}

#[test]
fn test_invalid_range_ordering_with_build() {
    let target = Version::parse("1.2.3+build").unwrap();
    // <1.2.3+build is invalid: build numbers have no ordering
    assert!(Range::new(Operator::Lt, target, PrereleasePolicy::Natural, BuildPolicy::Implicit).is_err());
}

#[test]
fn test_clause_composition_and_simplification() {
    let r1 = Clause::Range(make_range(Operator::Gte, "1.0.0", PrereleasePolicy::Natural, BuildPolicy::Implicit));
    let r2 = Clause::Range(make_range(Operator::Lt, "2.0.0", PrereleasePolicy::Natural, BuildPolicy::Implicit));

    let combined = r1 & r2;
    assert!(combined.matches(&Version::parse("1.5.0").unwrap()));
    assert!(!combined.matches(&Version::parse("0.9.0").unwrap()));
    assert!(!combined.matches(&Version::parse("2.0.0").unwrap()));

    // Always & Range -> Range
    let always_and_range = Clause::Always & combined.clone();
    assert_eq!(always_and_range, combined);
}
