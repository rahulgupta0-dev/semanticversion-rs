//! Crash-fuzz target for the `semantic_version` Rust core.
//!
//! Feeds arbitrary bytes into the full parser + matching surface of the port:
//!   * `Version::parse` / `Version::parse_partial` (full + partial versions)
//!   * `SimpleSpec::parse` (python-semanticversion simple spec grammar)
//!   * `NpmSpec::parse` (npm range grammar)
//!   * on success: bump/truncate helpers, precedence comparisons, and
//!     `match_version()` / clause `matches()` against a fixed probe set,
//!     plus `compare()` on raw candidate pairs.
//!
//! The target must NEVER panic on attacker-controlled input: every call is
//! fallible, and all bump/expansion arithmetic in the core is overflow-safe
//! (`saturating_add`), so `u64::MAX` components cannot trigger arithmetic
//! panics (see DECISIONS.md D18).

#![no_main]

use libfuzzer_sys::fuzz_target;

use semantic_version::npm_spec::NpmSpec;
use semantic_version::simple_spec::SimpleSpec;
use semantic_version::version::Version;
use semantic_version::{Clause, Range};

/// Fixed probe versions used to exercise matching/comparison.
const PROBES: [&str; 4] = ["0.0.0", "1.2.3", "1.2.3-rc.1", "2.0.0+build.7"];

fn probe_versions() -> Vec<Version> {
    PROBES.iter().filter_map(|s| Version::parse(s).ok()).collect()
}

/// Exercise every fallible/hot path that takes a version.
fn exercise_version(v: &Version, probes: &[Version]) {
    let _ = v.next_major();
    let _ = v.next_minor();
    let _ = v.next_patch();
    let _ = v.truncate_to_prerelease();
    let _ = v.truncate_to_patch();
    for p in probes {
        let _ = v.precedence_gt(p);
        let _ = v.precedence_lt(p);
        let _ = v.precedence_ge(p);
        let _ = v.precedence_le(p);
        let _ = v == p;
    }
}

/// Walk a clause tree and exercise every `Range` target as a derived version.
fn exercise_clause(clause: &Clause, probes: &[Version]) {
    match clause {
        Clause::Always | Clause::Never => {}
        Clause::Range(r) => {
            // Derived versions: the range target + policy fields are public.
            let _ = r.operator;
            let _ = r.prerelease_policy;
            let _ = r.build_policy;
            exercise_version(&r.target, probes);
            for p in probes {
                let _ = r.matches(p);
            }
        }
        Clause::AllOf(children) | Clause::AnyOf(children) => {
            for c in children {
                exercise_clause(c, probes);
            }
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    let candidates: Vec<&str> = text
        .split(|c: char| matches!(c, ',' | ' ' | '\n' | '\t' | '|'))
        .filter(|s| !s.is_empty())
        .collect();
    let probes = probe_versions();

    for cand in &candidates {
        if let Ok(v) = Version::parse(cand) {
            exercise_version(&v, &probes);
        }
        if let Ok(v) = Version::parse_partial(cand) {
            let _ = v.next_major();
            for p in &probes {
                let _ = v.precedence_gt(p);
            }
        }
        if let Ok(spec) = SimpleSpec::parse(cand) {
            for p in &probes {
                let _ = spec.match_version(p);
            }
            exercise_clause(&spec.clause, &probes);
        }
        if let Ok(spec) = NpmSpec::parse(cand) {
            for p in &probes {
                let _ = spec.match_version(p);
            }
            exercise_clause(&spec.clause, &probes);
        }
        for other in &candidates {
            let _ = semantic_version::version::compare(cand, other);
            let _ = semantic_version::validate(cand);
        }
    }
});
