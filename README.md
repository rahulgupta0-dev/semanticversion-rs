# semanticversion-rs — Rust port of python-semanticversion
> **Port Mortem 2026 · Track D (Python → Rust)** · solo build (1 human + 1 AI coding agent)

A complete, memory-safe Rust reimplementation of
[python-semanticversion](https://github.com/rbarrois/python-semanticversion): SemVer 2.0
parsing/comparison plus npm-style `SimpleSpec` / `NpmSpec` / `LegacySpec` range matching.

### Headline
The **original, unmodified pytest suite passes against the Rust build** via a PyO3/maturin
extension named `semantic_version` — **54 tests + 586 subtests green, zero test edits.**
The port is **100% safe Rust (zero `unsafe`)**, validated by a **24,500-pair differential fuzz +
2.5M crash-fuzz runs (0 divergence, 0 crashes)**, documented in a **20-entry decision log**, and
benchmarked at **up to 60× faster spec matching, ~11× faster parsing, 21% lower memory**.

### Verify it yourself (one command)
```
make        # builds the extension + runs the ORIGINAL unmodified pytest → 54 passed / 16 skipped
```
`make` = `maturin develop && pytest tests/original -q` (exits non-zero on any failure).
The 16 skips are `test_django.py` ("Django not installed") — identical to the original baseline.

### Architecture (all DONE)
| Module | Status | Role |
|---|---|---|
| `error` | ✅ | `SemverError` → `PyValueError` |
| `identifiers` | ✅ | prerelease / build identifier rules |
| `version` | ✅ | parse / display / coerce / ordering / partial |
| `clause` | ✅ | `Clause` AST + `Range` matching + empty-marker policies |
| `simple_spec` | ✅ | SimpleSpec grammar + AST emission |
| `npm_spec` | ✅ | x/hyphen/caret/tilde + prerelease OR-expansion |
| `bindings` (PyO3) | ✅ | the `semantic_version` extension (dunder surface, Spec/LegacySpec) |

### Proof layer
- **Original pytest unmodified:** 54 passed / 16 skipped / 586 subtests.
- **Differential fuzz:** 24,500 random pairs (Rust binding vs original Python) → **0 behavioral
  divergences**; **2,554,822 crash-fuzz runs → 0 panics** (`fuzz/log.txt`).
- **8 latent port bugs caught & fixed by the fuzzer** (e.g. 18 `u64` overflow-panic sites →
  `saturating_add`; `~*`/`^*` rejection; empty-prerelease `1.2.3-..`; `||` empty-group; AllOf-wrap
  shape; hyphen LTE fence). See `DECISIONS.md` D18.
- **Zero `unsafe`:** `grep -rn unsafe src/` → empty.
- **Decision log:** 20 entries (D00–D19) in `DECISIONS.md`.

### Benchmarks (`bench/`, see `bench/methodology.md`)
Aggregate **9×**, top **60×** (npm matching), **~11×** parsing, **21% lower RSS**. Honest note:
the PyO3 precedence-key path drags the aggregate (Python tuple overhead); native precedence runs at
~386 ns p50. Hackathon cloud VM, not bare-metal.

### Demo
[![Demo Video](https://img.shields.io/badge/▶_Demo-Google%20Drive-4285F9?style=for-the-badge&logo=google-drive)](https://drive.google.com/file/d/1cK8otmY8cCQ7QHl45r-EHrqiKHHUsTuJ/view?usp=drivesdk)

### Team
`in.rahul.dev` — **Rahul Gupta** (a.k.a. Pranav) · Discord `pranav_dev.` · GitHub
`rahulgupta0-dev` · **solo**.

### License
BSD-2-Clause — same as the original project; see `LICENSE`. This is a from-scratch Rust port.
