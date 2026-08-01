# DECISIONS.md Skeleton

This file lives in the PORT REPO at kickoff. Pre-populate the categories; fill details during porting.

---

# Architectural Decisions — semanticversion-rs

Port Mortem 2026 | Track D (Python → Rust) | Source: `rbarrois/python-semanticversion` @ `2cbbee3`

Each entry: **Python behavior → Rust choice → Rationale → Tradeoff → Test impact**

---

## D00 [DONE] — Django Integration: Out of Scope per R6

**Python:** `semantic_version/django_fields.py` (107 lines) provides `VersionField` and `SpecField` as Django ORM model field types. `tests/test_django.py` (16 tests across `DjangoFieldTestCase`, `FieldMigrationTests`, `FullMigrateTests`, `DbInteractingTestCase`) exercises these fields. All 16 skip automatically on a clean clone with reason `"Django not installed"` — this is the baseline behavior, not a test failure.  
**Rust:** Not ported. Django is a Python-only web framework; its ORM has no Rust equivalent in scope for this hackathon.  
**Rationale:** R6 explicitly prohibits linking to the Python runtime. Porting Django fields to Rust would require a Rust ORM (Diesel, SeaORM), a complete reimplementation of Django's field protocol, and Python runtime linkage for the test runner — all out of scope. The 16 skips in our PyO3 build mirror the baseline exactly because Django is not installed in the test venv; zero special exclusion logic needed.  
**Tradeoff:** Honest parity denominator = 54 (non-Django tests only). The 16 Django tests skip in both original and port — identical behavior. Documented in `tests/original/SCOPE.md`.  
**Test impact:** `pytest tests/original/test_django.py -rs` → `16 skipped, reason: "Django not installed"` in BOTH original and Rust-backed runs. Parity = 54/54 target.

---

## D01 [PLANNED] — Integer Representation: Python arbitrary-precision int → bounded Rust `u64`

**Python:** `major`, `minor`, `patch` are Python `int` — arbitrary precision, no overflow.  
**Rust:** `u64` (max 18446744073709551615). SemVer 2.0 spec requires non-negative integers; no real version exceeds `u64`.  
**Rationale:** `u64` is idiomatic, safe, fast, and sufficient for any real-world SemVer version. Arbitrary precision would require `BigUint` (external dep) with no practical benefit.  
**Tradeoff:** Theoretically rejects versions with major > 2^64, which Python would accept. Acceptable: documented.  
**Test impact:** Zero — all test versions fit in `u32` let alone `u64`.

---

## D02 [PLANNED] — Optional Fields (partial versions) → `Option<u64>`

**Python:** `minor`, `patch` can be `None` when `partial=True` (deprecated in 2.x, removed in 3.0).  
**Rust:** Keep `Option<u64>` for minor/patch internally in a `PartialVersion` helper only used during spec parsing. `Version` struct always has `(u64, u64, u64)`.  
**Rationale:** `partial=True` is deprecated and only used internally by `SpecItem`/`LegacySpec`. We implement just enough to pass tests that use the deprecated `Spec` class.  
**Tradeoff:** Slight complexity to support deprecated API.  
**Test impact:** Required for `test_base.py::PartialVersionTestCase` and `SpecItem` tests.

---

## D03 [PLANNED] — Python exceptions → `Result<T, SemverError>` (thiserror)

**Python:** Invalid input raises `ValueError` with a descriptive message.  
**Rust:** `pub enum SemverError` via `thiserror`, with variants `InvalidVersion(String)`, `InvalidSpec(String)`, `InvalidRange(String)`.  
**Rationale:** `thiserror` is the idiomatic Rust error library; `Result` is the standard error propagation mechanism. No panics in library code.  
**Tradeoff:** Callers must handle `Result`; more verbose call sites. Worth it for safety and idiomaticity.  
**Test impact:** All test assertions on `ValueError` map to `Err(SemverError::...)` in Rust.

---

## D04 [PLANNED] — String Parsing: regex vs nom vs hand-rolled

**Python:** Uses compiled `re` module with named groups.  
**Rust choice:** Hand-rolled recursive-descent parser for specs; `regex` crate for version string parsing.  
**Rationale:** The version regex is straightforward and the `regex` crate is zero-`unsafe`, well-maintained, and idiomatic. The spec grammar has conditional branching best handled by explicit code rather than a combinator library. Avoiding `nom` reduces dep surface and complexity.  
**Tradeoff:** More code than a nom combinator. Easier to reason about and debug.  
**Test impact:** Parser must produce identical results to Python regex — verified by differential fuzz.

---

## D05 [PLANNED] — SimpleSpec Grammar → Rust parser design

**Python:** Splits on `,`, then `NAIVE_SPEC` regex matches each block.  
**Rust:** Same approach: split on `,`, match each block with the equivalent regex, then dispatch to `parse_block()`. The Rust `regex` crate handles the named capture groups.  
**Rationale:** Direct translation preserves behavioral identity. Exotic combinator approaches risk divergence.  
**Tradeoff:** The regex string must be carefully translated and tested.  
**Test impact:** `test_match.py` and `test_base.py::SpecTestCase` — must match exactly.

---

## D06 [PLANNED] — Comparison / Total Ordering → `Ord` + `PartialOrd` + custom `PartialEq`

**Python:** Two comparison keys — `_cmp_precedence_key` (ignores build) for ordering, but `__eq__` includes build.  
**Rust:** Implement `PartialEq` to compare ALL fields (including build). Implement `PartialOrd`/`Ord` using `cmp_precedence_key()` (ignores build). Document in code that `a == b` can be false while `a.cmp(&b) == Equal`.  
**Rationale:** This matches Python semantics exactly: `Version("1.0.0") != Version("1.0.0+build")` but they compare equal by `<`/`>`/`>=`/`<=`.  
**Tradeoff:** Violates Rust's standard requirement that `a == b` implies `a.cmp(&b) == Equal`. We document this divergence explicitly.  
**Test impact:** Critical for `test_parsing.py::ComparisonTestCase::test_unordered` and `test_base.py` comparison tests.

---

## D07 [PLANNED] — `__contains__` (`v in spec`) → Rust `contains()` method

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

## D09 [PLANNED] — Version Coercion / Partial Semantics

**Python:** `Version.coerce()` accepts lax strings like `"0.1"`, `"0.1.2.3+4"`. Uses regex to extract leading numeric part, fills zeros, handles extra components as build.  
**Rust:** Port `coerce()` directly. Use `regex` for extraction, string manipulation for the rest.  
**Rationale:** `coerce()` is tested in `test_base.py::VersionTestCase::test_coerce`; must match exactly.  
**Tradeoff:** Complex coercion logic; cover with differential fuzz.  
**Test impact:** `test_base.py::VersionTestCase::test_coerce` — must match Python output exactly.

---

## D10 [PLANNED] — Error Messages & Parity of Failure Modes

**Python:** Error messages include `%r` (Python repr) of the invalid string.  
**Rust:** Match error message format where tests check it (some tests use `assertRaises` without checking message). Where message IS checked, use `{:?}` (Rust debug repr) which produces similar `"..."` quoting.  
**Rationale:** Exact message parity is secondary to behavior parity. Tests that only check exception type (not message) are trivially satisfied.  
**Tradeoff:** Minor message format differences acceptable; documented.  
**Test impact:** Low — most tests use `assertRaises(ValueError)` without checking message text.

---

## D11 [PLANNED] — Prerelease / Build Metadata Ordering Rules

**Python:** Prerelease identifiers: numeric parts compared as integers, alpha parts as ASCII bytes, numeric < alpha < MaxIdentifier (sentinel for "no prerelease").  
**Rust:** Implement `PreReleaseIdent` enum: `Numeric(u64)`, `Alpha(Vec<u8>)`, `Max`. Implement `Ord` with the same rules. `Max` only appears in the precedence key of versions without prerelease (so they sort AFTER prerelease versions with same patch).  
**Rationale:** Direct translation of the Python sentinel-based approach. The `MaxIdentifier` trick is elegant and must be preserved exactly.  
**Tradeoff:** `Vec<u8>` for alpha identifiers may be overkill; could use `String` since all are ASCII. Using `Vec<u8>` mirrors Python's `.encode('ascii')` exactly.  
**Test impact:** `test_spec.py::FormatTests::test_precedence`, `test_parsing.py::ComparisonTestCase`.

---

## D12 [PLANNED] — Public API Naming: Python conventions → Rust conventions

**Python:** `snake_case` for methods/functions, `PascalCase` for classes.  
**Rust:** Same conventions (already aligned). `Version`, `SimpleSpec`, `NpmSpec`, `SemverError`. Methods: `parse()`, `match_version()`, `select()`, `filter()`, `contains()`, `validate()`, `compare()`.  
**Rationale:** Rust naming conventions match Python's for this domain. No snake_case→camelCase translation needed.  
**Tradeoff:** `match` is a Rust keyword — we use `match_version()` or `is_match()` instead of `match()`.  
**Test impact:** All test calls adapted for the keyword rename.

---

## D13 [PLANNED] — Django Fields: Excluded

**Python:** `django_fields.py` provides `VersionField`, `SpecField` as Django ORM field types.  
**Rust:** Not ported. Django is a Python-only web framework; no Rust equivalent in scope for this hackathon.  
**Rationale:** R6 prohibits Python runtime deps; porting Django fields to Rust would require a Rust ORM integration (e.g., Diesel), which is out of scope.  
**Test impact:** `test_django.py` (16 tests) excluded from parity %. Parity reported as `N_passing / (total - 16)`. Documented.

---

## D14 [PLANNED] — Test Execution: PyO3/maturin extension as primary strategy

**Python tests** (`tests/original/*.py`) do `from semantic_version import Version, NpmSpec` — they import a Python package, not a Rust binary.  
**Primary approach:** Build a **PyO3/maturin extension named `semantic_version`**. `maturin develop` installs it into the test venv so `import semantic_version` resolves to our Rust code. The original pytest suite runs **completely unmodified** — `pytest tests/original/` imports Rust. Map Rust errors to `pyo3::exceptions::PyValueError` so `pytest.raises(ValueError)` passes. Implement the full Python dunder surface: `__new__`, `__str__`, `__repr__`, `__eq__`, `__hash__`, `__lt__`, `__contains__`, `__iter__`.  
**Fallback (if PyO3 incomplete by hour 30–36):** Native Rust `#[test]` suite in `tests/port/` mirroring every Python test 1:1. The core library is identical — only the test driver changes. A finished native-test port with honest parity beats an unfinished PyO3 attempt.  
**Django self-resolution:** Under PyO3, `test_django.py` skips automatically (`"Django not installed"`) — exactly matching the baseline 16 skipped. No special exclusion needed; parity is naturally preserved.  
**Rationale:** The rules say "run the original test suite against your port" and the suite is hashed at kickoff — hashing only matters if the files actually execute. PyO3 is the rule-faithful, judge-impressive path. It also makes differential fuzzing trivial (both Python lib and Rust lib importable in one process).  
**Tradeoff:** PyO3 binding layer adds ~200–300 lines and a `maturin` build step. Manageable given the small API surface.  
**Test impact:** 54 Python tests + 586 subtests pass unmodified against Rust code. Zero test edits.

---

## D15 [PLANNED] — Monolithic `base.py` → Modular Rust crate structure

**Python:** The entire library lives in a single `semantic_version/base.py` (1,457 lines). All classes, parsers, and utilities are in one flat namespace.  
**Rust:** Split into focused modules: `src/version.rs` (Version struct + parse + ordering), `src/identifiers.rs` (PreReleaseIdent enum + Ord), `src/clause.rs` (Clause tree: AnyOf, AllOf, Range, Never, Always), `src/simple_spec.rs` (SimpleSpec parser), `src/npm_spec.rs` (NpmSpec parser), `src/error.rs` (SemverError), `src/lib.rs` (re-exports + PyO3 module definition).  
**Rationale:** Rust's borrow-checker naturally enforces module boundaries; split structure enables parallel development, targeted testing per module, and cleaner git history (one commit per module). The monolithic Python file has no internal abstraction barriers — splitting exposes the natural layering (identifiers → version → clause → spec).  
**Tradeoff:** Slightly more import paths; `pub use` re-exports in `lib.rs` maintain a flat public API identical to Python's.  
**Test impact:** All original tests import from the top-level `semantic_version` module — our PyO3 `lib.rs` re-exports preserve this surface exactly.
