# semanticversion-rs

> **Port Mortem 2026 — Track D (Python → Rust)**  
> A faithful Rust port of [`rbarrois/python-semanticversion`](https://github.com/rbarrois/python-semanticversion).

## Status

🚧 **In progress** — scaffold committed, modules being implemented one-by-one.

## What this is

`python-semanticversion` is a Python library implementing [Semantic Versioning 2.0.0](https://semver.org/) parsing, comparison, and range-matching — including npm-style specs (`NpmSpec`) and simple comma-separated specs (`SimpleSpec`).

This Rust port exposes an identical public API via a [PyO3](https://pyo3.rs/) extension module so that the original unmodified Python test suite (`pytest tests/original/`) runs against the Rust implementation after `maturin develop`.

## Build

```bash
# Development (installs into active venv)
pip install maturin
maturin develop

# Run original test suite against Rust
pytest tests/original/

# Native Rust tests
cargo test

# Differential fuzz (60s)
python fuzz/fuzz_driver.py --duration 60 --output fuzz/log.txt
```

## Architecture

| Rust module | Python equivalent | Status |
|---|---|---|
| `error.rs` | `ValueError` | PLANNED |
| `identifiers.rs` | `MaxIdentifier`, `NumericIdentifier`, `AlphaIdentifier` | PLANNED |
| `version.rs` | `Version` | PLANNED |
| `clause.rs` | `Clause`, `Range`, `AnyOf`, `AllOf` | PLANNED |
| `simple_spec.rs` | `SimpleSpec` | PLANNED |
| `npm_spec.rs` | `NpmSpec` | PLANNED |
| `src/lib.rs` (PyO3) | `semantic_version/__init__.py` | PLANNED |

## Source

- Source repo: https://github.com/rbarrois/python-semanticversion
- Source commit: `2cbbee3154d9011cee873ae3a020cd17c669f6df`
- Test suite hash: `5a4c71cee61257d91d04562a5df3d3eb66f6162e255796bef927d9653f67342c`
- License: BSD-2-Clause (same as original)
