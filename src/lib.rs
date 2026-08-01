//! # semanticversion-rs
//!
//! Rust port of [python-semanticversion](https://github.com/rbarrois/python-semanticversion)
//! — Port Mortem 2026, Track D (Python → Rust).
//!
//! Source commit: `2cbbee3154d9011cee873ae3a020cd17c669f6df`
//!
//! ## Modules
//! - `error`       — SemverError                        [DONE]
//! - `identifiers` — PreReleaseIdent + BuildIdent        [DONE]
//! - `version`     — Version struct, parse, ordering     [DONE — parsing]
//! - `clause`      — Clause tree, Range, policies        [PLANNED]
//! - `simple_spec` — SimpleSpec parser                   [PLANNED]
//! - `npm_spec`    — NpmSpec parser                      [PLANNED]

pub mod error;
pub mod identifiers;
pub mod version;

// Re-exports for a flat API (mirrors `from semantic_version import Version, validate`)
pub use error::SemverError;
pub use identifiers::{BuildIdent, PreReleaseIdent};
pub use version::{Version, validate, compare};
