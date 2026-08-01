//! Native Rust tests for simple_spec.rs — ground-truth parity.
//!
//! All test cases derived from Python probe in grammar.md "## Ground Truth (SimpleSpec AST)".
//! This file is the permanent record of verified behavior.

use semantic_version::clause::{BuildPolicy, Clause, Operator, PrereleasePolicy, Range};
use semantic_version::simple_spec::SimpleSpec;
use semantic_version::version::Version;

// ===========================================================================
// AST structure tests (from ground truth probe)
// ===========================================================================

#[test]
fn test_spec_eq_forms() {
    // ==1.2.3, 1.2.3, =1.2.3 all produce identical Range('==', Version('1.2.3'))
    for spec_str in &["==1.2.3", "1.2.3", "=1.2.3"] {
        let spec = SimpleSpec::parse(spec_str).unwrap();
        match spec.clause {
            Clause::Range(r) => {
                assert_eq!(r.operator, Operator::Eq);
                assert_eq!(r.target, Version::parse("1.2.3").unwrap());
            }
            _ => panic!("Expected Range, got {:?}", spec.clause),
        }
    }
}

#[test]
fn test_spec_star_wildcard() {
    // '*' -> Range('>=', Version('0.0.0'))
    let spec = SimpleSpec::parse("*").unwrap();
    match spec.clause {
        Clause::Range(r) => {
            assert_eq!(r.operator, Operator::Gte);
            assert_eq!(r.target, Version::parse("0.0.0").unwrap());
        }
        _ => panic!("Expected Range, got {:?}", spec.clause),
    }
}

#[test]
fn test_spec_compatible_partial() {
    // ~=1.2 -> AllOf(<2.0.0, >=1.2.0)
    let spec = SimpleSpec::parse("~=1.2").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
            match (&clauses[0], &clauses[1]) {
                (Clause::Range(upper), Clause::Range(lower)) => {
                    assert_eq!(upper.operator, Operator::Lt);
                    assert_eq!(upper.target, Version::parse("2.0.0").unwrap());
                    assert_eq!(lower.operator, Operator::Gte);
                    assert_eq!(lower.target, Version::parse("1.2.0").unwrap());
                }
                _ => panic!("Unexpected clause order"),
            }
        }
        _ => panic!("Expected AllOf, got {:?}", spec.clause),
    }
}

#[test]
fn test_spec_compatible_full() {
    // ~=1.2.3 -> AllOf(<1.3.0, >=1.2.3)
    let spec = SimpleSpec::parse("~=1.2.3").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
            match (&clauses[0], &clauses[1]) {
                (Clause::Range(upper), Clause::Range(lower)) => {
                    assert_eq!(upper.operator, Operator::Lt);
                    assert_eq!(upper.target, Version::parse("1.3.0").unwrap());
                    assert_eq!(lower.operator, Operator::Gte);
                    assert_eq!(lower.target, Version::parse("1.2.3").unwrap());
                }
                _ => panic!("Unexpected clause order"),
            }
        }
        _ => panic!("Expected AllOf, got {:?}", spec.clause),
    }
}

#[test]
fn test_spec_caret() {
    // ^1.2.3 -> AllOf(<2.0.0, >=1.2.3)
    let spec = SimpleSpec::parse("^1.2.3").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
            match (&clauses[0], &clauses[1]) {
                (Clause::Range(upper), Clause::Range(lower)) => {
                    assert_eq!(upper.operator, Operator::Lt);
                    assert_eq!(upper.target, Version::parse("2.0.0").unwrap());
                    assert_eq!(lower.operator, Operator::Gte);
                    assert_eq!(lower.target, Version::parse("1.2.3").unwrap());
                }
                _ => panic!("Unexpected clause order"),
            }
        }
        _ => panic!("Expected AllOf, got {:?}", spec.clause),
    }
}

#[test]
fn test_spec_caret_zero_minor() {
    // ^0.2.3 -> AllOf(<0.3.0, >=0.2.3)
    let spec = SimpleSpec::parse("^0.2.3").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
            match (&clauses[0], &clauses[1]) {
                (Clause::Range(upper), Clause::Range(lower)) => {
                    assert_eq!(upper.operator, Operator::Lt);
                    assert_eq!(upper.target, Version::parse("0.3.0").unwrap());
                    assert_eq!(lower.operator, Operator::Gte);
                    assert_eq!(lower.target, Version::parse("0.2.3").unwrap());
                }
                _ => panic!("Unexpected clause order"),
            }
        }
        _ => panic!("Expected AllOf, got {:?}", spec.clause),
    }
}

#[test]
fn test_spec_caret_zero_zero_patch() {
    // ^0.0.3 -> AllOf(<0.0.4, >=0.0.3)
    let spec = SimpleSpec::parse("^0.0.3").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
            match (&clauses[0], &clauses[1]) {
                (Clause::Range(upper), Clause::Range(lower)) => {
                    assert_eq!(upper.operator, Operator::Lt);
                    assert_eq!(upper.target, Version::parse("0.0.4").unwrap());
                    assert_eq!(lower.operator, Operator::Gte);
                    assert_eq!(lower.target, Version::parse("0.0.3").unwrap());
                }
                _ => panic!("Unexpected clause order"),
            }
        }
        _ => panic!("Expected AllOf, got {:?}", spec.clause),
    }
}

#[test]
fn test_spec_tilde_full() {
    // ~1.2.3 -> AllOf(<1.3.0, >=1.2.3)
    let spec = SimpleSpec::parse("~1.2.3").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
            match (&clauses[0], &clauses[1]) {
                (Clause::Range(upper), Clause::Range(lower)) => {
                    assert_eq!(upper.operator, Operator::Lt);
                    assert_eq!(upper.target, Version::parse("1.3.0").unwrap());
                    assert_eq!(lower.operator, Operator::Gte);
                    assert_eq!(lower.target, Version::parse("1.2.3").unwrap());
                }
                _ => panic!("Unexpected clause order"),
            }
        }
        _ => panic!("Expected AllOf, got {:?}", spec.clause),
    }
}

#[test]
fn test_spec_tilde_partial() {
    // ~1.2 -> AllOf(<1.3.0, >=1.2.0)
    let spec = SimpleSpec::parse("~1.2").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
            match (&clauses[0], &clauses[1]) {
                (Clause::Range(upper), Clause::Range(lower)) => {
                    assert_eq!(upper.operator, Operator::Lt);
                    assert_eq!(upper.target, Version::parse("1.3.0").unwrap());
                    assert_eq!(lower.operator, Operator::Gte);
                    assert_eq!(lower.target, Version::parse("1.2.0").unwrap());
                }
                _ => panic!("Unexpected clause order"),
            }
        }
        _ => panic!("Expected AllOf, got {:?}", spec.clause),
    }
}

#[test]
fn test_spec_comma_and() {
    // >=1.0.0,<2.0.0 -> AllOf(<2.0.0, >=1.0.0)
    let spec = SimpleSpec::parse(">=1.0.0,<2.0.0").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
            // Order after simplify() may vary; check both are present
            let has_upper = clauses.iter().any(|c| matches!(c, Clause::Range(r) if r.operator == Operator::Lt && r.target == Version::parse("2.0.0").unwrap()));
            let has_lower = clauses.iter().any(|c| matches!(c, Clause::Range(r) if r.operator == Operator::Gte && r.target == Version::parse("1.0.0").unwrap()));
            assert!(has_upper, "Missing upper bound <2.0.0");
            assert!(has_lower, "Missing lower bound >=1.0.0");
        }
        _ => panic!("Expected AllOf, got {:?}", spec.clause),
    }
}

#[test]
fn test_spec_empty_prerel_suffix() {
    // ==1.2.3- -> Range('==', Version('1.2.3'))
    let spec = SimpleSpec::parse("==1.2.3-").unwrap();
    match spec.clause {
        Clause::Range(r) => {
            assert_eq!(r.operator, Operator::Eq);
            assert_eq!(r.target, Version::parse("1.2.3").unwrap());
            assert_eq!(r.prerelease_policy, PrereleasePolicy::Natural);
        }
        _ => panic!("Expected Range, got {:?}", spec.clause),
    }
}

#[test]
fn test_spec_empty_build_suffix() {
    // ==1.2.3+ -> Range('==', Version('1.2.3'), build_policy='strict')
    let spec = SimpleSpec::parse("==1.2.3+").unwrap();
    match spec.clause {
        Clause::Range(r) => {
            assert_eq!(r.operator, Operator::Eq);
            assert_eq!(r.target, Version::parse("1.2.3").unwrap());
            assert_eq!(r.build_policy, BuildPolicy::Strict);
        }
        _ => panic!("Expected Range, got {:?}", spec.clause),
    }
}

#[test]
fn test_spec_prerelease_always_suffix() {
    // <1.2.3- -> Range('<', Version('1.2.3'), prerelease_policy='always')
    let spec = SimpleSpec::parse("<1.2.3-").unwrap();
    match spec.clause {
        Clause::Range(r) => {
            assert_eq!(r.operator, Operator::Lt);
            assert_eq!(r.target, Version::parse("1.2.3").unwrap());
            assert_eq!(r.prerelease_policy, PrereleasePolicy::Always);
        }
        _ => panic!("Expected Range, got {:?}", spec.clause),
    }
}

#[test]
fn test_spec_with_prerelease_target() {
    // >=1.0.0-rc.1 -> Range('>=', Version('1.0.0-rc.1'))
    let spec = SimpleSpec::parse(">=1.0.0-rc.1").unwrap();
    match spec.clause {
        Clause::Range(r) => {
            assert_eq!(r.operator, Operator::Gte);
            assert_eq!(r.target, Version::parse("1.0.0-rc.1").unwrap());
        }
        _ => panic!("Expected Range, got {:?}", spec.clause),
    }
}

// ===========================================================================
// Invalid specs (must raise)
// ===========================================================================

#[test]
fn test_invalid_wildcard_x() {
    // 1.x is INVALID in SimpleSpec
    assert!(SimpleSpec::parse("1.x").is_err());
    assert!(SimpleSpec::parse("1.2.x").is_err());
    assert!(SimpleSpec::parse("!=1.2.x").is_err());
    assert!(SimpleSpec::parse("==1.x").is_err());
    assert!(SimpleSpec::parse("==1.2.x").is_err());
}

#[test]
fn test_invalid_space_separator() {
    // Space is NOT a valid separator
    assert!(SimpleSpec::parse(">1.0.0 <2.0.0").is_err());
}

#[test]
fn test_invalid_build_on_ordering_op() {
    // >=1.0.0+build is INVALID (build on non-EQ/NEQ)
    assert!(SimpleSpec::parse(">=1.0.0+build").is_err());
    assert!(SimpleSpec::parse("<1.0.0+build").is_err());
}

// ===========================================================================
// Match behavior tests (from ground truth probe)
// ===========================================================================

#[test]
fn test_match_compatible_partial() {
    assert!(SimpleSpec::parse("~=1.2").unwrap().match_version(&Version::parse("1.5.0").unwrap()));
    assert!(!SimpleSpec::parse("~=1.2").unwrap().match_version(&Version::parse("2.0.0").unwrap()));
}

#[test]
fn test_match_compatible_full() {
    assert!(SimpleSpec::parse("~=1.2.3").unwrap().match_version(&Version::parse("1.2.9").unwrap()));
    assert!(!SimpleSpec::parse("~=1.2.3").unwrap().match_version(&Version::parse("1.3.0").unwrap()));
}

#[test]
fn test_match_caret() {
    assert!(SimpleSpec::parse("^1.2.3").unwrap().match_version(&Version::parse("1.9.0").unwrap()));
    assert!(!SimpleSpec::parse("^1.2.3").unwrap().match_version(&Version::parse("2.0.0").unwrap()));
}

#[test]
fn test_match_comma_and() {
    assert!(SimpleSpec::parse(">=1.0.0,<2.0.0").unwrap().match_version(&Version::parse("1.5.0").unwrap()));
}

#[test]
fn test_match_prerelease_always() {
    assert!(SimpleSpec::parse("<1.2.3-").unwrap().match_version(&Version::parse("1.2.3-alpha").unwrap()));
}

#[test]
fn test_match_cross_patch_prerelease() {
    // SimpleSpec allows cross-patch prerelease (PRERELEASE_NATURAL only excludes same-patch)
    assert!(SimpleSpec::parse(">=1.0.0").unwrap().match_version(&Version::parse("2.0.0-alpha").unwrap()));
    assert!(SimpleSpec::parse(">1.0.0").unwrap().match_version(&Version::parse("2.0.0-alpha").unwrap()));
    assert!(SimpleSpec::parse("<2.0.0").unwrap().match_version(&Version::parse("1.5.0-beta").unwrap()));
    assert!(SimpleSpec::parse("<=2.0.0").unwrap().match_version(&Version::parse("1.5.0-beta").unwrap()));
    assert!(SimpleSpec::parse(">1.0.0").unwrap().match_version(&Version::parse("1.0.1-alpha").unwrap()));
}

#[test]
fn test_match_same_patch_prerelease_excluded() {
    // SimpleSpec excludes same-patch prerelease for NATURAL policy
    assert!(!SimpleSpec::parse(">=1.0.0").unwrap().match_version(&Version::parse("1.0.0-rc.1").unwrap()));
    assert!(!SimpleSpec::parse("<1.0.1").unwrap().match_version(&Version::parse("1.0.1-alpha").unwrap()));
}

#[test]
fn test_match_prerelease_target() {
    assert!(SimpleSpec::parse(">=1.0.0-rc.1").unwrap().match_version(&Version::parse("1.0.0-rc.2").unwrap()));
    assert!(SimpleSpec::parse(">=1.0.0-rc.1").unwrap().match_version(&Version::parse("1.0.1-rc.1").unwrap()));
    assert!(SimpleSpec::parse(">=1.0.0-rc.1").unwrap().match_version(&Version::parse("1.0.0").unwrap()));
}
