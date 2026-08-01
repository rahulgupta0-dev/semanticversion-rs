//! Error types for semanticversion-rs.
//!
//! Designed to map cleanly to Python exceptions in the PyO3 binding layer:
//! - `SemverError::InvalidVersion` → `PyValueError`
//! - `SemverError::InvalidSpec`    → `PyValueError`
//! - `SemverError::InvalidCoerce`  → `PyValueError`
//!
//! All variants carry the offending string for diagnostics.

use thiserror::Error;

/// All errors produced by this crate.
///
/// The Python `semantic_version` library raises `ValueError` for all invalid
/// inputs.  Every variant here maps to a `ValueError` in the PyO3 layer.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SemverError {
    /// A version string failed to parse (e.g. `"1.2"`, `"01.2.3"`, `"1.2.3 "`)
    #[error("Invalid version string {0:?}")]
    InvalidVersion(String),

    /// A spec string failed to parse (e.g. `"!0.1"`, `"1.2.3+build<bad>"`)
    #[error("Invalid spec string {0:?}")]
    InvalidSpec(String),

    /// `Version::coerce` could not find a leading numeric component.
    #[error("Version string lacks a numerical component: {0:?}")]
    InvalidCoerce(String),
}
