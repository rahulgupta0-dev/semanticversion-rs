//! Native Rust tests for npm_spec.rs — ground-truth parity.
//!
//! All test cases derived from Python probe in grammar.md "## Ground Truth (NpmSpec AST + prerelease gate)".
//! This file is the permanent record of verified behavior.

use semantic_version::clause::{BuildPolicy, Range};

/// Build a `Range` with the policies used throughout NpmSpec parsing.
fn rng(op: Operator, target: &str, policy: PrereleasePolicy) -> Clause {
    Clause::Range(Range::new(op, Version::parse(target).unwrap(), policy, BuildPolicy::Implicit).unwrap())
}
use semantic_version::clause::{Clause, Operator, PrereleasePolicy};
use semantic_version::npm_spec::NpmSpec;
use semantic_version::version::Version;

// ===========================================================================
// x-ranges / star / empty
// ===========================================================================

#[test]
fn test_x_range_patch() {
    // '1.2.x' -> AllOf(<1.3.0 SAMEPATCH, >=1.2.0 SAMEPATCH)
    let spec = NpmSpec::parse("1.2.x").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
            match (&clauses[0], &clauses[1]) {
                (Clause::Range(upper), Clause::Range(lower)) => {
                    assert_eq!(upper.operator, Operator::Lt);
                    assert_eq!(upper.target, Version::parse("1.3.0").unwrap());
                    assert_eq!(upper.prerelease_policy, PrereleasePolicy::SamePatch);
                    assert_eq!(lower.operator, Operator::Gte);
                    assert_eq!(lower.target, Version::parse("1.2.0").unwrap());
                    assert_eq!(lower.prerelease_policy, PrereleasePolicy::SamePatch);
                }
                _ => panic!("Unexpected clause types"),
            }
        }
        _ => panic!("Expected AllOf, got {:?}", spec.clause),
    }
}

#[test]
fn test_x_range_minor() {
    // '1.x' -> AllOf(<2.0.0 SAMEPATCH, >=1.0.0 SAMEPATCH)
    let spec = NpmSpec::parse("1.x").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
            match (&clauses[0], &clauses[1]) {
                (Clause::Range(upper), Clause::Range(lower)) => {
                    assert_eq!(upper.operator, Operator::Lt);
                    assert_eq!(upper.target, Version::parse("2.0.0").unwrap());
                    assert_eq!(lower.operator, Operator::Gte);
                    assert_eq!(lower.target, Version::parse("1.0.0").unwrap());
                }
                _ => panic!("Unexpected clause types"),
            }
        }
        _ => panic!("Expected AllOf, got {:?}", spec.clause),
    }
}

#[test]
fn test_star_wildcard() {
    // '*' -> AllOf(>=0.0.0 SAMEPATCH)
    let spec = NpmSpec::parse("*").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 1);
            match &clauses[0] {
                Clause::Range(r) => {
                    assert_eq!(r.operator, Operator::Gte);
                    assert_eq!(r.target, Version::parse("0.0.0").unwrap());
                    assert_eq!(r.prerelease_policy, PrereleasePolicy::SamePatch);
                }
                _ => panic!("Expected Range"),
            }
        }
        _ => panic!("Expected AllOf, got {:?}", spec.clause),
    }
}

#[test]
fn test_empty_string() {
    // '' -> AllOf(>=0.0.0 SAMEPATCH)
    let spec = NpmSpec::parse("").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 1);
        }
        _ => panic!("Expected AllOf"),
    }
}

#[test]
fn test_x_range_with_eq() {
    // '=1.2.x' -> AllOf(<1.3.0 SAMEPATCH, >=1.2.0 SAMEPATCH)
    let spec = NpmSpec::parse("=1.2.x").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
        }
        _ => panic!("Expected AllOf"),
    }
}

// ===========================================================================
// Hyphen ranges
// ===========================================================================

#[test]
fn test_hyphen_both_full() {
    // '1.2.3 - 2.3.4' -> AllOf(<=2.3.4 SAMEPATCH, >=1.2.3 SAMEPATCH)
    let spec = NpmSpec::parse("1.2.3 - 2.3.4").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
            match (&clauses[0], &clauses[1]) {
                (Clause::Range(upper), Clause::Range(lower)) => {
                    assert_eq!(upper.operator, Operator::Lte);
                    assert_eq!(upper.target, Version::parse("2.3.4").unwrap());
                    assert_eq!(lower.operator, Operator::Gte);
                    assert_eq!(lower.target, Version::parse("1.2.3").unwrap());
                }
                _ => panic!("Unexpected clause types"),
            }
        }
        _ => panic!("Expected AllOf"),
    }
}

#[test]
fn test_hyphen_both_partial() {
    // '1.2 - 2.3' -> AllOf(<2.4.0 SAMEPATCH, >=1.2.0 SAMEPATCH)
    let spec = NpmSpec::parse("1.2 - 2.3").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
            match (&clauses[0], &clauses[1]) {
                (Clause::Range(upper), Clause::Range(lower)) => {
                    assert_eq!(upper.operator, Operator::Lt);
                    assert_eq!(upper.target, Version::parse("2.4.0").unwrap());
                    assert_eq!(lower.operator, Operator::Gte);
                    assert_eq!(lower.target, Version::parse("1.2.0").unwrap());
                }
                _ => panic!("Unexpected clause types"),
            }
        }
        _ => panic!("Expected AllOf"),
    }
}

#[test]
fn test_hyphen_right_partial() {
    // '1.2.3 - 2.3' -> AllOf(<2.4.0 SAMEPATCH, >=1.2.3 SAMEPATCH)
    let spec = NpmSpec::parse("1.2.3 - 2.3").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
            match (&clauses[0], &clauses[1]) {
                (Clause::Range(upper), Clause::Range(lower)) => {
                    assert_eq!(upper.operator, Operator::Lt);
                    assert_eq!(upper.target, Version::parse("2.4.0").unwrap());
                    assert_eq!(lower.operator, Operator::Gte);
                    assert_eq!(lower.target, Version::parse("1.2.3").unwrap());
                }
                _ => panic!("Unexpected clause types"),
            }
        }
        _ => panic!("Expected AllOf"),
    }
}

#[test]
fn test_hyphen_left_partial() {
    // '1.2 - 2.3.4' -> AllOf(<=2.3.4 SAMEPATCH, >=1.2.0 SAMEPATCH)
    let spec = NpmSpec::parse("1.2 - 2.3.4").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
            match (&clauses[0], &clauses[1]) {
                (Clause::Range(upper), Clause::Range(lower)) => {
                    assert_eq!(upper.operator, Operator::Lte);
                    assert_eq!(upper.target, Version::parse("2.3.4").unwrap());
                    assert_eq!(lower.operator, Operator::Gte);
                    assert_eq!(lower.target, Version::parse("1.2.0").unwrap());
                }
                _ => panic!("Unexpected clause types"),
            }
        }
        _ => panic!("Expected AllOf"),
    }
}

// ===========================================================================
// Caret / tilde
// ===========================================================================

#[test]
fn test_caret_major_nonzero() {
    // '^1.2.3' -> AllOf(<2.0.0 SAMEPATCH, >=1.2.3 SAMEPATCH)
    let spec = NpmSpec::parse("^1.2.3").unwrap();
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
                _ => panic!("Unexpected clause types"),
            }
        }
        _ => panic!("Expected AllOf"),
    }
}

#[test]
fn test_caret_zero_minor() {
    // '^0.2.3' -> AllOf(<0.3.0 SAMEPATCH, >=0.2.3 SAMEPATCH)
    let spec = NpmSpec::parse("^0.2.3").unwrap();
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
                _ => panic!("Unexpected clause types"),
            }
        }
        _ => panic!("Expected AllOf"),
    }
}

#[test]
fn test_caret_zero_zero() {
    // '^0.0.3' -> AllOf(<0.0.4 SAMEPATCH, >=0.0.3 SAMEPATCH)
    let spec = NpmSpec::parse("^0.0.3").unwrap();
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
                _ => panic!("Unexpected clause types"),
            }
        }
        _ => panic!("Expected AllOf"),
    }
}

#[test]
fn test_tilde_full() {
    // '~1.2.3' -> AllOf(<1.3.0 SAMEPATCH, >=1.2.3 SAMEPATCH)
    let spec = NpmSpec::parse("~1.2.3").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
        }
        _ => panic!("Expected AllOf"),
    }
}

#[test]
fn test_tilde_partial() {
    // '~1.2' -> AllOf(<1.3.0 SAMEPATCH, >=1.2.0 SAMEPATCH)
    let spec = NpmSpec::parse("~1.2").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
        }
        _ => panic!("Expected AllOf"),
    }
}

#[test]
fn test_tilde_zero() {
    // '~0.2.3' -> AllOf(<0.3.0 SAMEPATCH, >=0.2.3 SAMEPATCH)
    let spec = NpmSpec::parse("~0.2.3").unwrap();
    match &spec.clause {
        Clause::AllOf(clauses) => {
            assert_eq!(clauses.len(), 2);
        }
        _ => panic!("Expected AllOf"),
    }
}

// ===========================================================================
// Prerelease OR-expansion
// ===========================================================================

#[test]
fn test_prerelease_gt() {
    // '>1.2.3-alpha.3' -> AnyOf(AllOf(<1.2.4 ALWAYS, >1.2.3-alpha.3 SAMEPATCH), AllOf(>1.2.3 SAMEPATCH))
    let spec = NpmSpec::parse(">1.2.3-alpha.3").unwrap();
    match &spec.clause {
        Clause::AnyOf(branches) => {
            assert_eq!(branches.len(), 2);
            // First branch: prerelease fence
            match &branches[0] {
                Clause::AllOf(clauses) => {
                    assert_eq!(clauses.len(), 2);
                    match (&clauses[0], &clauses[1]) {
                        (Clause::Range(fence), Clause::Range(orig)) => {
                            assert_eq!(fence.operator, Operator::Lt);
                            assert_eq!(fence.target, Version::parse("1.2.4").unwrap());
                            assert_eq!(fence.prerelease_policy, PrereleasePolicy::Always);
                            assert_eq!(orig.operator, Operator::Gt);
                            assert_eq!(orig.target, Version::parse("1.2.3-alpha.3").unwrap());
                            assert_eq!(orig.prerelease_policy, PrereleasePolicy::SamePatch);
                        }
                        _ => panic!("Unexpected clause types"),
                    }
                }
                _ => panic!("Expected AllOf in first branch"),
            }
            // Second branch: truncated target wrapped in AllOf (Python's frozenset
            // semantics keeps AllOf wrappers even on single-element children).
            match &branches[1] {
                Clause::AllOf(clauses) => {
                    assert_eq!(clauses.len(), 1);
                    match &clauses[0] {
                        Clause::Range(r) => {
                            assert_eq!(r.operator, Operator::Gt);
                            assert_eq!(r.target, Version::parse("1.2.3").unwrap());
                            assert_eq!(r.prerelease_policy, PrereleasePolicy::SamePatch);
                        }
                        _ => panic!("Expected Range inside AllOf in second branch"),
                    }
                }
                _ => panic!("Expected AllOf in second branch, got {:?}", branches[1]),
            }
        }
        _ => panic!("Expected AnyOf, got {:?}", spec.clause),
    }
}
#[test]
fn test_prerelease_tilde() {
    // '~1.2.3-beta.2' -> AnyOf(AllOf(<1.2.4 ALWAYS, >=1.2.3-beta.2 SAMEPATCH), AllOf(<1.3.0 SAMEPATCH, >=1.2.3 SAMEPATCH))
    let spec = NpmSpec::parse("~1.2.3-beta.2").unwrap();
     match &spec.clause {
        Clause::AnyOf(branches) => {
            assert_eq!(branches.len(), 2);
        }
        _ => panic!("Expected AnyOf"),
    }
}

#[test]
fn test_prerelease_gte() {
    // '>=1.0.0-rc.1' -> AnyOf(AllOf(<1.0.1 ALWAYS, >=1.0.0-rc.1 SAMEPATCH), AllOf(>=1.0.0 SAMEPATCH))
    let spec = NpmSpec::parse(">=1.0.0-rc.1").unwrap();
    match &spec.clause {
        Clause::AnyOf(branches) => {
            assert_eq!(branches.len(), 2);
        }
        _ => panic!("Expected AnyOf"),
    }
}

#[test]
fn test_prerelease_hyphen() {
    // '1.2.3-rc.1 - 2.0.0' -> AnyOf(AllOf(<1.2.4 ALWAYS, >=1.2.3-rc.1 SAMEPATCH), AllOf(<=2.0.0 SAMEPATCH, >=1.2.3 SAMEPATCH))
    let spec = NpmSpec::parse("1.2.3-rc.1 - 2.0.0").unwrap();
    match &spec.clause {
        Clause::AnyOf(branches) => {
            assert_eq!(branches.len(), 2);
        }
        _ => panic!("Expected AnyOf"),
    }
}

// ===========================================================================
// Set-level prerelease GATE (match behavior)
// ===========================================================================

#[test]
fn test_match_prerelease_same_patch() {
    // NpmSpec('>=1.0.0-rc.1 <2.0.0').match(1.0.0-rc.5) -> True
    assert!(NpmSpec::parse(">=1.0.0-rc.1 <2.0.0").unwrap().match_version(&Version::parse("1.0.0-rc.5").unwrap()));
}

#[test]
fn test_match_prerelease_different_patch() {
    // NpmSpec('>=1.0.0-rc.1 <2.0.0').match(1.0.1-rc.5) -> False
    assert!(!NpmSpec::parse(">=1.0.0-rc.1 <2.0.0").unwrap().match_version(&Version::parse("1.0.1-rc.5").unwrap()));
}

#[test]
fn test_match_prerelease_release_version() {
    // NpmSpec('>=1.0.0-rc.1 <2.0.0').match(1.0.0) -> True
    assert!(NpmSpec::parse(">=1.0.0-rc.1 <2.0.0").unwrap().match_version(&Version::parse("1.0.0").unwrap()));
}

#[test]
fn test_match_hyphen_prerelease_same_patch() {
    // NpmSpec('1.0.0-rc.1 - 2.0.0').match(1.0.0-rc.5) -> True
    assert!(NpmSpec::parse("1.0.0-rc.1 - 2.0.0").unwrap().match_version(&Version::parse("1.0.0-rc.5").unwrap()));
}

#[test]
fn test_match_tilde_prerelease_same_patch() {
    // NpmSpec('~1.2.3-beta.2').match(1.2.3-beta.5) -> True
    assert!(NpmSpec::parse("~1.2.3-beta.2").unwrap().match_version(&Version::parse("1.2.3-beta.5").unwrap()));
}

#[test]
fn test_match_tilde_prerelease_different_patch() {
    // NpmSpec('~1.2.3-beta.2').match(1.2.4-beta.1) -> False
    assert!(!NpmSpec::parse("~1.2.3-beta.2").unwrap().match_version(&Version::parse("1.2.4-beta.1").unwrap()));
}

#[test]
fn test_match_x_range() {
    // NpmSpec('1.2.x').match(1.2.5) -> True
    assert!(NpmSpec::parse("1.2.x").unwrap().match_version(&Version::parse("1.2.5").unwrap()));
}

#[test]
fn test_match_hyphen_inclusive() {
    // NpmSpec('1.2.3 - 2.3.4').match(2.3.4) -> True
    assert!(NpmSpec::parse("1.2.3 - 2.3.4").unwrap().match_version(&Version::parse("2.3.4").unwrap()));
}


// ===========================================================================
// EXACT AST — prerelease OR-expansion (grammar.md "## Ground Truth (NpmSpec AST)")
// ===========================================================================

#[test]
fn test_exact_ast_multi_block_prerelease() {
    // '>=1.0.0-rc.1 <2.0.0' ->
    //   AnyOf(AllOf(<1.0.1[Always], >=1.0.0-rc.1[SamePatch]),
    //         AllOf(>=1.0.0[SamePatch], <2.0.0[SamePatch]))
    let spec = NpmSpec::parse(">=1.0.0-rc.1 <2.0.0").unwrap();
    let expected = Clause::AnyOf(vec![
        Clause::AllOf(vec![
            rng(Operator::Lt, "1.0.1", PrereleasePolicy::Always),
            rng(Operator::Gte, "1.0.0-rc.1", PrereleasePolicy::SamePatch),
        ]),
        Clause::AllOf(vec![
            rng(Operator::Gte, "1.0.0", PrereleasePolicy::SamePatch),
            rng(Operator::Lt, "2.0.0", PrereleasePolicy::SamePatch),
        ]),
    ]);
    assert_eq!(spec.clause, expected);
}

#[test]
fn test_exact_ast_hyphen_prerelease() {
    // '1.0.0-rc.1 - 2.0.0' ->
    //   AnyOf(AllOf(<1.0.1[Always], >=1.0.0-rc.1[SamePatch]),
    //         AllOf(<=2.0.0[SamePatch], >=1.0.0[SamePatch]))
    let spec = NpmSpec::parse("1.0.0-rc.1 - 2.0.0").unwrap();
    let expected = Clause::AnyOf(vec![
        Clause::AllOf(vec![
            rng(Operator::Lt, "1.0.1", PrereleasePolicy::Always),
            rng(Operator::Gte, "1.0.0-rc.1", PrereleasePolicy::SamePatch),
        ]),
        Clause::AllOf(vec![
            rng(Operator::Lte, "2.0.0", PrereleasePolicy::SamePatch),
            rng(Operator::Gte, "1.0.0", PrereleasePolicy::SamePatch),
        ]),
    ]);
    assert_eq!(spec.clause, expected);
}

#[test]
fn test_exact_ast_tilde_prerelease() {
    // '~1.2.3-beta.2' ->
    //   AnyOf(AllOf(<1.2.4[Always], >=1.2.3-beta.2[SamePatch]),
    //         AllOf(<1.3.0[SamePatch], >=1.2.3[SamePatch]))
    let spec = NpmSpec::parse("~1.2.3-beta.2").unwrap();
    let expected = Clause::AnyOf(vec![
        Clause::AllOf(vec![
            rng(Operator::Lt, "1.2.4", PrereleasePolicy::Always),
            rng(Operator::Gte, "1.2.3-beta.2", PrereleasePolicy::SamePatch),
        ]),
        Clause::AllOf(vec![
            rng(Operator::Lt, "1.3.0", PrereleasePolicy::SamePatch),
            rng(Operator::Gte, "1.2.3", PrereleasePolicy::SamePatch),
        ]),
    ]);
    assert_eq!(spec.clause, expected);
}