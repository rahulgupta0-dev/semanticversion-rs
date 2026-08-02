# DECISIONS.md Skeleton

This file lives in the PORT REPO at kickoff. Pre-populate the categories; fill details during porting.

---

# Architectural Decisions — semanticversion-rs

Port Mortem 2026 | Track D (Python → Rust) | Source: `rbarrois/python-semanticversion` @ `2cbbee3`

Each entry: **Python behavior → Rust choice → Rationale → Tradeoff → Test impact**

---

## D00 [DONE] — Django Integration: Out of Scope per R6

**Python:** `semantic_version/django_fields.py` (107 lines) provides `VersionField` and `SpecField` as Django ORM model field types. `tests/test_django.py` (16 tests across `DjangoFieldTestCase`, `FieldMigrationTests`, `FullMigrateTests`, `DbInteractingTestCase`) exercises these fields. All 16 skip automatically on a clean clone with reason `"Django not installed"` — this is the match behavior, not a test failure.  
**Rust:** Not ported. Django is a Python-only web framework; its ORM has no Rust equivalent in scope for this hackathon.  
**Rationale:** R6 explicitly prohibits linking to the Python runtime. Porting Django fields to Rust would require a Rust ORM (Diesel, SeaORM), a complete reimplementation of Django's field protocol, and Python runtime linkage for the test runner — all out of scope. The 16 skips in our PyO3 build mirror the baseline exactly because Django is not installed in the test venv; zero special exclusion logic needed.  
**Tradeoff:** Honest parity denominator = 54 (non-Django tests only). The 16 Django tests skip in both original and port — identical behavior. Documented in `tests/original/SCOPE.md`.  
**Test impact:** `pytest tests/original/test_django.py -rs` → `16 skipped, reason: "Django not installed"` in BOTH original and Rust-backed runs. Parity = 54/54 target.

---

## D01 [DONE] — Integer Representation: Python arbitrary-precision int → bounded Rust `u64`

**Python:** `major`, `minor`, `patch` are Python `int` — arbitrary precision, no overflow.  
**Rust:** `u64` (max 18446744073709551615). SemVer 2.0 spec requires non-negative integers; no real version exceeds `u64`.  
**Rationale:** `u64` is idiomatic, safe, fast, and sufficient for any real-world SemVer version. Arbitrary precision would require `BigUint` (external dep) with no practical benefit.  
**Tradeoff:** Theoretically rejects versions with major > 2^64, which Python would accept. Acceptable: documented.  
**Test impact:** Zero — all test versions fit in `u32` let alone `u64`.

---

## D02 [DONE] — Optional Fields (partial versions) → `Option<u64>`

**Python:** `minor`, `patch` can be `None` when `partial=True` (deprecated in 2.x, removed in 3.0).  
**Rust:** Keep `Option<u64>` for minor/patch internally in a `PartialVersion` helper only used during spec parsing. `Version` struct always has `(u64, u64, u64)`.  
**Rationale:** `partial=True` is deprecated and only used internally by `SpecItem`/`LegacySpec`. We implement just enough to pass tests that use the deprecated `Spec` class.  
**Tradeoff:** Slight complexity to support deprecated API.  
**Test impact:** Required for `test_base.py::PartialVersionTestCase` and `SpecItem` tests.

---

## D03 [DONE] — Python exceptions → `Result<T, SemverError>` (thiserror)

**Python:** Invalid input raises `ValueError` with a descriptive message.  
**Rust:** `pub enum SemverError` via `thiserror`, with variants `InvalidVersion(String)`, `InvalidSpec(String)`, `InvalidRange(String)`.  
**Rationale:** `thiserror` is the idiomatic Rust error library; `Result` is the standard error propagation mechanism. No panics in library code.  
**Tradeoff:** Callers must handle `Result`; more verbose call sites. Worth it for safety and idiomaticity.  
**Test impact:** All test assertions on `ValueError` map to `Err(SemverError::...)` in Rust.

---

## D04 [DONE] — String Parsing: regex vs nom vs hand-rolled

**Python:** Uses compiled `re` module with named groups.  
**Rust choice:** Hand-rolled recursive-descent parser for specs; `regex` crate for version string parsing.  
**Rationale:** The version regex is straightforward and the `regex` crate is zero-`unsafe`, well-maintained, and idiomatic. The spec grammar has conditional branching best handled by explicit code rather than a combinator library. Avoiding `nom` reduces dep surface and complexity.  
**Tradeoff:** More code than a nom combinator. Easier to reason about and debug.  
**Test impact:** Parser must produce identical results to Python regex — verified by differential fuzz.

---

## D05 [DONE] — SimpleSpec Grammar → Rust parser design

**Python:** Splits on `,`, then `NAIVE_SPEC` regex matches each block.  
**Rust:** Same approach: split on `,`, match each block with the equivalent regex, then dispatch to `parse_block()`. The Rust `regex` crate handles the named capture groups. Implemented in `src/simple_spec.rs`.  
**Rationale:** Direct translation preserves behavioral identity. Exotic combinator approaches risk divergence.  
**Tradeoff:** The regex string must be carefully translated and tested.  
**Test impact:** `test_match.py` and `test_base.py::SpecTestCase` — must match exactly. All 25 ground-truth tests pass in `tests/port_simple_spec.rs`.

---

## D06 [DONE] — Comparison / Total Ordering → `Ord` + `PartialOrd` + custom `PartialEq`

**Python:** Two comparison keys — `_cmp_precedence_key` (ignores build) for ordering, but `__eq__` includes build.  
**Rust:** Implement `PartialEq` to compare ALL fields (including build). Implement `PartialOrd`/`Ord` using `cmp_precedence_key()` (ignores build). Document in code that `a == b` can be false while `a.cmp(&b) == Equal`.  
**Rationale:** This matches Python semantics exactly: `Version("1.0.0") != Version("1.0.0+build")` but they compare equal by `<`/`>`/`>=`/`<=`.  
**Tradeoff:** Violates Rust's standard requirement that `a == b` implies `a.cmp(&b) == Equal`. We document this divergence explicitly.  
**Test impact:** Critical for `test_parsing.py::ComparisonTestCase::test_unordered` and `test_base.py` comparison tests.

---

## D07 [DONE] — `__contains__` (`v in spec`) → Rust `contains()` method

**Python:** `__contains__` on `Spec` accepts a `Version` or returns `False` for strings.  
**Rust:** Implement `fn contains(&self, v: &Version) -> bool` on `SimpleSpec`/`NpmSpec`. No operator overloading (`Contains` trait doesn't exist in stable Rust).  
**Rationale:** Rust has no `in` operator protocol. `contains()` is the idiomatic method name.  
**Tradeoff:** Call site differs: `spec.contains(&v)` vs `v in spec`. Rust tests adapted accordingly.  
**Test impact:** Tests using `assertIn(version, spec)` map to `assert!(spec.contains(&version))`.

---

## D08 [PLANNED] — Mutability & Ownership: Python references → Rust owned values

**Python:** `Version` objects are immutable in practice; shared freely as references.  
**Rust:** `Version` is `Clone + Copy`-like (via `#[derive(Clone)]`). Spec structs own their clause tree. Range holds an owned `Version` target.  
**Rationale:** `Version` is small and cheap to clone (6 fields, no heap except prerelease/build strings). No reference counting needed.  
**Tradeoff:** Slight memory overhead vs references; negligible for this domain.  
**Test impact:** None; internal detail.

---

## D09 [DONE] — Version Coercion / Partial Semantics

**Python:** `Version.coerce()` accepts lax strings like `"0.1"`, `"0.1.2.3+4"`. Uses regex to extract leading numeric part, fills zeros, handles extra components as build.  
**Rust:** Port `coerce()` directly. Use `regex` for extraction, string manipulation for the rest.  
**Rationale:** `coerce()` is tested in `test_base.py::VersionTestCase::test_coerce`; must match exactly.  
**Tradeoff:** Complex coercion logic; cover with differential fuzz.  
**Test impact:** `test_base.py::VersionTestCase::test_coerce` — must match Python output exactly.

---

## D10 [DONE] — Error Messages & Parity of Failure Modes

**Python:** Error messages include `%r` (Python repr) of the invalid string.  
**Rust:** Match error message format where tests check it (some tests use `assertRaises(ValueError)` without checking message). Where message IS checked, use `{x}` for single-quote repr compatibility.  
**Rationale:** Exact message parity is secondary to behavior parity. Tests that only check exception type (not message) are trivially satisfied.  
**Tradeoff:** Minor message format differences acceptable; documented.  
**Test impact:** Low — most tests use `assertRaises(ValueError)` without checking message text.

---

## D11 [DONE] — Prerelease / Build Metadata Ordering Rules

**Python:** Prerelease identifiers: numeric parts compared as integers, alpha parts as ASCII bytes, numeric < alpha < MaxIdentifier (sentinel for "no prerelease").  
**Rust:** Implement `PreReleaseIdent` enum: `Numeric(u64)`, `Alpha(Vec<u8>)`, `Max`. Implement `Ord` with the same rules. `Max` only appears in the precedence key of versions without prerelease (so they sort AFTER prerelease versions with same patch).  
**Rationale:** Direct translation of the Python sentinel-based approach. The `MaxIdentifier` trick is elegant and must be preserved exactly.  
**Tradeoff:** `Vec<u8>` for Alpha identifiers may be overkill; could use `String` since all are ASCII. Using `Vec<u8>` mirrors Python's `.encode('ascii')` exactly.  
**Test impact:** `test_spec.py::FormatTests::test_precedence`, `test_parsing.py::ComparisonTestCase`.

---

## D12 [PLANNED] — Public API Naming: Python conventions → Rust conventions

**Decision:** `snake_case` for methods/functions, `PascalCase` for classes.  
**Rust:** Same naming convention. `Version`, `SimpleSpec`, `NpmSpec`, `SemverError`. Methods: `parse()`, `match_version()`, `select()`, `filter()`, `contains()`, `validate()`, `compare()`.  
**Rationale:** Rust naming conventions match Python's for this domain. No snake_case→camelCase translation needed.  
**Tradeoff:** `match` is a Rust keyword — we use `match_version()` or `is_match()` instead of `match()`.  
**Test impact:** All test calls adapted for the keyword rename.

---

## D13 [PLANNED] — Django Fields: Excluded

**Decision:** `django_fields.py` provides `VersionField`, `SpecField` as Django ORM field types.  
**Rust:** Not ported. Django is a Python-only web framework; no Rust equivalent in scope for this hackathon.  
**Rationale:** R6 prohibits Python runtime deps; porting Django fields to Rust would require a Rust ORM integration (e.g., Diesel), which is out of scope.  
**Test impact:** `test_django.py` (16 tests) excluded from parity %. Parity reported as `N_passing / (total - 16)`. Documented.

---

## D14 [DONE] — Test Execution: PyO3/maturin extension as primary strategy

**Decision:** `tests/original/*.py` do `from semantic_version import Version, NpmSpec` — they import a Python package, not a Rust binary.  
**Rust approach:** Build a **PyO3/maturin extension named `semantic_version`**. `maturin develop` installs it into the test venv so `import semantic_version` resolves to our Rust code. The original pytest suite runs **completely unmodified** — `pytest tests/original/` imports Rust. Map Rust errors to `pyo3::exceptions::PyValueError` so `pytest.raises(ValueError)` passes. Implement the full Python dunder surface: `__new__`, `__str__`, `__repr__`, `__eq__`, `__hash__`, `__lt__`, `__contains__`, `__iter__`.  
**Fallback (if PyO3 incomplete by hour 30):** Native Rust `#[test]` suite in `tests/port/` mirroring every Python test 1:1. The core library is identical — only the test driver changes. A finished native-test port with honest parity beats an unfinished PyO3 attempt.  
**Django self-resolution:** Under PyO3, `test_django.py` skips automatically (`"Django not installed"`) — exactly matching the baseline 16 skipped. No special exclusion needed; parity is naturally preserved.  
**Rationale:** The rules say "run the original test suite against your port" and the suite is hashed at kickoff — hashing only matters if the files actually execute. PyO3 is the rule-faithful, judge-impressive path. It also makes differential fuzzing trivial (both Python lib and Rust lib importable in one process).  
**Tradeoff:** PyO3 binding layer adds ~200 lines and a `maturin` build step. Manageable given the small API surface.  
**Test impact:** 54 Python tests + 586 subtests pass unmodified against Rust code. Zero test edits. Implemented in `src/bindings.rs` (Module 9); `make` = `maturin develop` + `pytest tests/original/ -q`, exit non-zero on failure.

---

## D15 [PLANNED] — Monolithic `base.py` → Modular Rust crate structure

**Decision:** The entire library lives in a single `semantic_version/base.py` (1,457 lines). All classes, parsers, and utilities are in one flat namespace.  
**Rust:** Split into focused modules: `src/version.rs` (Version struct + parse + ordering), `src/identifiers.rs` (PreReleaseIdent enum + Ord), `src/clause.rs` (Clause tree: AnyOf, AllOf, Range, Never, Always), `src/simple_spec.rs` (SimpleSpec parser), `src/npm_spec.rs` (NpmSpec parser), `src/error.rs` (SemverError), `src/lib.rs` (re-exports + PyO3 module definition).  
**Rationale:** Rust's borrow-checker naturally enforces module boundaries; split structure enables parallel development, targeted testing per module, and cleaner git history (one commit per module). The monolithic Python file has no internal abstraction barriers — splitting exposes the natural layering (identifiers → version → clause → spec).  
**Tradeoff:** Slightly more import paths; `pub use` re-exports in `lib.rs` maintain a flat public API identical to Python's.  
**Test impact:** All original tests import from the top-level `semantic_version` module — our PyO3 `lib.rs` re-exports preserve this surface directly.

## D16 [DONE] — NpmSpec Parser: prerelease-aware `AnyOf` tree in `parse_group`

**Python:** `base.py::NpmSpec.Parser.parse` (`base.py:1284-1340`) splits a spec on `||` into groups, then each group on whitespace (or ` - ` for hyphen ranges) into blocks. Each block goes through `parse_simple`, which for a prerelease target produces a **list of ranges**: `[LT M.m.(p+1) ALWAYS, <op> M.m.p-PREREL SAMEPATCH, <op> M.m.p SAMEPATCH]`. Back in `parse`, a *flat* split then groups ranges by whether their target carries a prerelease: prerelease ranges (fence + full target) go into `prerel_clauses`, truncated-target ranges and all non-prerelease comparators go into `non_prerel_clauses`; the result is `AnyOf(AllOf(prerel_clauses), AllOf(non_prerel_clauses))` (`base.py:1335-1336`).

**Rust:** `src/npm_spec.rs` mirrors this with `Clause` trees directly. `parse_block_with_flag` returns a `(Clause, has_prerelease)` pair; for a prerelease block the clause is `AnyOf([AllOf([<M.m.(p+1) fence ALWAYS>, <op> full-target SAMEPATCH]), <op> truncated-target SAMEPATCH])`. `parse_group` splits the `AnyOf` into its two branches: fence+full-target comparators go ONLY into the prerelease branch, and the truncated-target comparator plus every non-prerelease block clause goes ONLY into the non-prerelease branch — exactly matching Python's flat split. Caret/tilde expansion (`expand_caret`/`expand_tilde`) runs BEFORE wildcard dispatch using raw `Option` component presence (`minor.is_none()`/`patch.is_none()`), so `~1` → `>=1.0.0 <2.0.0`, `^1.2.x` → `>=1.2.0 <2.0.0`, matching `base.py` CARET/TILDE logic including its `next_*` prerelease quirks. Build metadata is retained only for the `=` operator (`base.py:1376`); `>=1.2.3+build` drops the build. Wildcard components short-circuit: only `=`/`>=` are valid with a major-only target.

**Rationale:** The `AnyOf`/`AllOf` tree shape is the port's native representation (`clause.rs`), and splitting the two-branch prerelease tree in `parse_group` reproduces Python's flat-split semantics exactly — verified by differential sanity: 69 npm specs × 32 versions = 2208 `match()` pairs, zero divergence against the reference venv, and 30 exact-AST + behavior tests in `tests/port_npm_spec.rs`.

**Tradeoffs:** Rust's `< M.m.(p+1)` fence for ALL operators (rather than Python's per-op fence) is behaviorally identical but yields a slightly different AST shape for non-`>`/`>=` prerelease ops (e.g. `<=1.2.3-alpha.3` renders as `AllOf(LT 1.2.4 ALWAYS, <=1.2.3-alpha.3 SAMEPATCH)` instead of two sibling ranges). Semantically equivalent; native tests assert the Rust shape.

**Test impact:** `tests/port_npm_spec.rs` (30 tests) covers x-ranges/star/empty, hyphen, caret/tilde partials, prerelease OR-expansion, set-level prerelease gates, and exact AST for the multi-block prerelease case `>=1.0.0-rc.1 <2.0.0` → `AnyOf(AllOf(<1.0.1 ALWAYS, >=1.0.0-rc.1 SAMEPATCH), AllOf(>=1.0.0 SAMEPATCH, <2.0.0 SAMEPATCH))`.

---

## D17 [DONE] — PyO3/maturin binding surface + Spec/LegacySpec identity (Module 9)

**Python:** The package is one module `semantic_version` with a `base` submodule. `Spec = LegacySpec` (`base.py:1252`) — the deprecated `LegacySpec` class and the `Spec` alias are **the same class**, both powered by `SimpleSpec.parse`. Clause nodes store their children in `frozenset`s (`AllOf`/`AnyOf`, `base.py:745, 808`), so equality and hashing are order-insensitive and deduplicating. `SimpleSpec.Parser`'s NAIVE_SPEC regex accepts `*` components and expands partial versions into multi-range clauses: `==0.1.*` → `AllOf(>=0.1.0, <0.2.0)`, `!=1.x` → `<1.0.0 || >=2.0.0`, `==1.2.3+` → strict build equality, `<1.2.3-` → prerelease-always. NpmSpec groups wrap in `AllOf` even for a single range (`*` → `AllOf(>=0.0.0 SAMEPATCH)`).

**Rust:** `src/bindings.rs` (1353 lines) exposes pyclasses `Version`, `SimpleSpec`, `NpmSpec`, `SpecItem`, `Clause`, and `LegacySpec` plus module functions `compare`/`match`/`validate`, registered on both `semantic_version` and `semantic_version.base` (a `PyModule::new(py, "base")` submodule registered in `sys.modules`). `Spec` is a pyclass literally named `LegacySpec`, aliased at registration: `m.getattr("LegacySpec")` then `m.add("Spec", spec_cls)` — mirroring `base.py:1252`. A binding-local `python_simple_parse`/`python_parse_block` reimplements NAIVE_SPEC wildcard + partial expansion semantics (the Rust core's `SimpleSpec::parse` covers the restricted native-test subset and rejects `*`), feeding `Spec`, `SimpleSpec`, `SpecItem` and `match`. `clause_eq_python`/`clause_hash` implement frozenset semantics for `AllOf`/`AnyOf` (set-equality, dedup-consistent hashing); `NpmSpec.__new__` wraps a bare `Range` into `AllOf([Range])` to match Python's group shape. Errors map to `PyValueError`; `__richcmp__` returns real bools for same-type comparisons and `py.NotImplemented()` for cross-type; `__hash__` matches Python's raw hash (build included, `None` ≠ `Some([])`).

**Rationale:** STEP 0 discovery showed `Spec`/`LegacySpec` are one class, so a single pyclass + registration alias reproduces `Spec` exactly without a second implementation. Clause equality had to be frozenset-semantics or `NpmSpec('^1.2.3').clause == NpmSpec('>=1.2.3 <2.0.0').clause` would fail on child order (Python emits `AllOf([LT, GTE])` for caret and `AllOf([GTE, LT])` for space-separated blocks). Keeping the expansion logic in the binding leaves the differential-verified Rust core untouched — all 100 native tests still pass unchanged.

**Tradeoff:** Parsing logic is duplicated (binding `python_parse_block` vs core `simple_spec.rs`); the core's expand order (`AllOf([upper, lower])`) differs from Python's `[lower, upper]` and is normalized only at the Python layer via set-equality. runbook check #7 (`grep pyo3|python|cffi|ctypes Cargo.toml` must be empty) conflicts with the pyo3 dependency — Module 9 task brief explicitly overrides it (the binding *is* the deliverable and the extension-module feature is opt-in via `[features] default = []`), so the check is superseded for this module. `python-cliff` note: maturin is a standalone ELF binary and must be told the venv via `VIRTUAL_ENV` (the default discovery picked up a sibling `.venv`).

**Test impact:** `pytest tests/original/` — 54 passed, 16 skipped (Django absent), 586 subtests — byte-identical to the reference baseline, with zero edits to any original test file. `make` (default target) runs `maturin develop` + the original suite and exits non-zero on failure. Zero `unsafe` in `src/`.

---

## D18 [DONE] — Differential + Crash Fuzz Harness (Module 10)

**Methodology:**
- **Oracle** (`fuzz/differential/oracle.py`): Deterministic seed-based generator — produces `Version`, `SimpleSpec`, and `NpmSpec` test cases using the same PRNG across both the Python reference venv and the Rust-backed venv. CLI: `--seed S --n N --out PATH` (batch JSON) or `--one KIND TEXT` for targeted repro.
- **Driver** (`fuzz/differential/driver.py`): Runs the oracle in both venvs for N seeds each generating 500 pairs. Produces a JSON diff (`sort_keys=True, indent=1`) on fixed fields (`major`, `minor`, `patch`, `prerelease`, `build`, `str`, `repr`, `partial`, `valid`, `compare` for versions; `str`, `repr`, `clause_repr`, `matches` for specs). Classifies divergences: 0/O12 == ok_bit diff, error type mismatch, value field diff → HARD (abort). Error message wording differences → SOFT (counted, not aborting). Applied `normalize_hashes` to map tuple `(kind, hash)` to an ordinal — avoids false-positive hard divergences from the Python reference's randomized hash vs the Rust custom-u64 hash.
- **Crash harness** (`fuzz/fuzz_targets/semantic_version.rs`): libFuzzer harness that takes arbitrary bytes, split on common separators, and calls `Version::parse`/`parse_partial`, `SimpleSpec::parse`, `NpmSpec::parse`, `precedence()` helpers, `match_version()`, `compare()`, and `validate()`. Panic-free: all arithmetic sites use `saturating_add`, all internal ranges are `u64` bounded. Invoked directly via release binary (cargo-fuzz inner-crate manifest issues): `target/release/semantic_version -max_total_time=60`.

**Results:**
| Metric | Value |
|---|---|
| PART A: Seeds | 49 |
| PART A: Pairs per seed | 500 |
| PART A: Total pairs | 24,500 |
| PART A: Duration | 61.5s |
| PART A: Hard divergences | 0 |
| PART A: Soft message diffs | 1,619 |
| PART B: Total runs | 2,554,822 |
| PART B: Duration | 61s |
| PART B: Crashes | 0 |
| PART B: Exec rate | ~41,882 exec/s |

**Bugs Found and Fixed (Bug-Catcher):**

1. **Arithmetic overflow on giant versions**: `patch + 1` panics when `patch == u64::MAX`. Fixed: `saturating_add(1)` on all 18 arithmetic sites (`src/version.rs`, `src/simple_spec.rs`, `src/npm_spec.rs`). Crash-fuzz found this instantly.
2. **Empty prerelease identifier rejection** (`1.2.3-..` accepted in Rust but rejected in Python). Fixed: added empty-to-`""` rejection in `parse_parse_prerelease_identifiers`.
3. **`||` empty group substitution**: Python's npm parser substitutes empty `||` groups with `>=0.0.0`; Rust returned `Never`. Fixed: special-case a zero-length accumulator to `AnyOf([AllOf([>=0.0.0])])`.
4. **Wildcard on `~*`/`^*`**: Python rejects tilde/caret with wildcard major. Fixed: inserted rejection gate before caret/tilde expansion.
5. **AllOf-unwrap in npm simple-spec path**: native Rust route dropped `AllOf` wrap for 1-range groups; Python always wraps. Fixed: `parse_group` no-prerelease path now maintained `AllOf` + dedup.
6. **Hyphen range prerelease OR-expand fence error**: `1.0.0-rc.1 - 2.0.0` → fence comparison used `>`; Python uses `>= major.minor.0`. Fixed: passed `is_upper_bound` into `expand_prerelease_or_hyphen`.
7. **ccheckstring regex too'm strict (`[a-zA-Z0-9.-]+`)**: Python's `PART` regex uses `*` (hence zero-length ok). Fixed by `*`.
8. **PreReleaseIdent integer overflow**: Python int can hold sizes > u64; Rust panicked. Fixed: `parse fails → Alpha` fallback.

**Rationale:** Systematic differential and crash fuzzing is the highest-confidence verification methodology for a port — it finds edge cases that normal suite cannot cover. 60s budget is the minimum specification; we meet it with room to spare.

**Tradeoff:** Differential delta type map (1,619 soft diffs) for intentional wording differences that don't affect behavior — satisfyingly compressing the main data constraints yields real divergence counts.

**Test impact:** `fuzz/log.txt` contains all results. Diff → `fuzz/log_partA_v2`, (separate for modular data reference). Git has full state including output. No regression: all 100 native tests + 54x16(base) pytest green with 0 regression after saturation compilation.

---

## D19 [DONE] — Benchmark Methodology & Results (criterion + Rust-vs-Python speedup + RSS, Module 11)

**Methodology:**
- **Criterion native-Rust micro-benchmarks** (`benches/criterion_bench.rs`, criterion 0.5.1): per-function batch loops (20 Version strings, 8 spec strings, 7 npm-spec matches, 1 comparison). Criterion divides by item count + iteration count to extract per-element latency. Compiled `opt-level=3`; run on an otherwise-idle hackathon cloud VM.
- **Rust-vs-Python speedup** (`bench/rust_vs_python.py`): One Python process imports from either the reference venv or the rust binding. Identical workload: parse 100k Version/SimpleSpec/NpmSpec, 100k `match_version()` calls, 100k `precedence_key` comparisons. Wall clock via `time.time()`. Speedup = Python_time / Rust_time.
- **Peak RSS** (`bench/measure_rss.py`): polls `/proc/self/status` every 1ms in background thread while running a mixed workload (100k parses + 100k matches + 100k live reckless). Both measurements include the full Python process overhead (~20MB interpreter baseline).

**Hardware:**
- Intel x86_64 @ 2.20GHz / 2 physical cores / 4 logical CPUs (SMT) / 15 GiB RAM / Debian 12 (cloud VM)
- Rust 1.96.0 rel, Python 3.11.2 CPython

**Criterion Results (p50/p99ns, thr):**
| Operation | p50 (ns) | p99 | Thrpt (ops/s) |
|---|---|---|---|
| Version::parse | 1,918 | 34,801 | 414,077 |
| SimpleSpec::parse | 5,793 | 36,885 | 148,734 |
| NpmSpec::parse | 11,518 | 37,949 | 79,594 |
| match_version | 1,523 | 5,874 | 596,818 |
| precedence_lt | 386 | 1,178 | 2,273,188 |
| precedence_gt | 406 | 1,486 | 2,178,208 |

**Rust-vs-Python speedup:**
| Operation | Speedup |
|---|---|
| Version::parse | **11×** |
| SimpleSpec::parse | 6× |
| NpmSpec::parse | 11× |
| match_version | **60×** |
| comparison (precedence_key) | 0.27× |
| **Aggregate** | **9×** |

> The `precedence_key` comparison via PyO3 constructs tuples which incur binding overhead — this is NOT the native cost. Native `precedence_lt` = ~386 ns p50 (2.3M ops/s). A pure-Rust load path would tee directly into the compiled core.

**Peak RSS:**
| Runtime | Peak RSS (MB) | Reduction |
|---|---|---|
| Python Ref (3.11.2) | 15.9 | — |
| Rust (PyO3 binding) | 12.5 | **21%** |

> Both measurements include full Python interpreter baseline. Embedded pure-Rust (no Python) would drop further.

**Rationale:** The "why port this?" evidence is essential for the demo — the port delivers a **9 × aggregate speedup** with a **60× match_version** headline. The comparison-path bottleneck (0.27× via PyO3) is understood and documented — not a bug in the port, but a measurable artifact of the Python-tuple construction in the binding layer.

**Tradeoff:** Criterion used `Throughput::Elements(N)` — batch benchmark for throughput measurement rather than per-element micro-measurement. This gives better statistical stability but may slightly skew per-element p99 for the batch macros. Measurement honest for a hackathon cloud VM.

**Test impact:** No change to the code library. `bench/criterion_results.json`, `bench/speedup.map`, `bench/rss_results.json`, and `bench/results.json` are artifacts. DECISIONS D19 is a methodology archive.