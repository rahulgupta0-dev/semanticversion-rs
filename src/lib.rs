//! # semanticversion-rs
//!
//! Rust port of [python-semanticversion](https://github.com/rbarrois/python-semanticversion)
//! — Port Mortem 2026, Track D (Python → Rust).
//!
//! Source commit: `2cbbee3154d9011cee873ae3a020cd17c669f6df`
//!
//! ## Modules (implemented one-by-one, committed individually)
//! - `error`       — SemverError (thiserror)             [PLANNED]
//! - `identifiers` — PreReleaseIdent enum + Ord           [PLANNED]
//! - `version`     — Version struct, parse, ordering      [PLANNED]
//! - `clause`      — Clause tree, Range, policies         [PLANNED]
//! - `simple_spec` — SimpleSpec parser                    [PLANNED]
//! - `npm_spec`    — NpmSpec parser                       [PLANNED]
//!
//! ## PyO3 binding
//! `lib.rs` exposes all public types via `#[pymodule]` so the ORIGINAL unmodified
//! `pytest tests/original/` runs against this Rust build after `maturin develop`.
