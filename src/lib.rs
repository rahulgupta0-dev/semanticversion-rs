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
//! - `version`     — Version struct, parse, ordering     [DONE]
//! - `clause`      — Clause tree, Range, policies        [DONE]
//! - `simple_spec` — SimpleSpec parser                   [DONE]
//! - `npm_spec`    — NpmSpec parser                      [PLANNED]

pub mod clause;
pub mod error;
pub mod identifiers;
pub mod simple_spec;
pub mod version;

// Re-exports for a flat API
pub use clause::{BuildPolicy, Clause, Operator, PrereleasePolicy, Range};
pub use error::SemverError;
pub use identifiers::{BuildIdent, PreReleaseIdent};
pub use simple_spec::SimpleSpec;
pub use version::{compare, validate, Version};
