//! Error types for semanticversion-rs.
//!
//! ## Error message format (GROUND TRUTH from Python base.py)
//!
//! The original library raises `ValueError` with single-quoted strings:
//!   `ValueError("Invalid version string: 'garbage'")`
//!   `ValueError("Invalid leading zero in major: '01.2.3'")`
//!   `ValueError("Invalid leading zero in minor: '0.01.2'")`
//!   `ValueError("Invalid leading zero in patch: '0.1.02'")`
//!   `ValueError("Invalid empty version string: ''")`
//!   `ValueError("Invalid empty identifier '' in 'bad..id'")`
//!
//! In Rust, `{:?}` gives double-quoted strings (`"garbage"`) — WRONG for PyO3.
//! We store the formatted string directly so the PyO3 layer can pass it verbatim to PyValueError.
//!
//! Variants carry the complete, Python-formatted message string.

use thiserror::Error;

/// All errors produced by this crate.
///
/// Every variant maps to a `ValueError` in the PyO3 layer.
/// The message string is already formatted in Python's style (single-quoted).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SemverError {
    /// A version string failed to parse.
    /// Message mirrors Python: `Invalid version string: 'VERSION'`
    #[error("{0}")]
    InvalidVersion(String),

    /// A spec string failed to parse.
    /// Message mirrors Python's various ValueError messages.
    #[error("{0}")]
    InvalidSpec(String),

    /// `Version::coerce` could not find a leading numeric component.
    /// Message mirrors Python: `Version string lacks a numerical component: 'INPUT'`
    #[error("{0}")]
    InvalidCoerce(String),
}

impl SemverError {
    /// Build an `InvalidVersion` with Python-style single-quoted message.
    pub fn invalid_version(s: &str) -> Self {
        SemverError::InvalidVersion(format!("Invalid version string: '{}'", s))
    }

    /// Build an `InvalidVersion` for empty input.
    pub fn empty_version(s: &str) -> Self {
        SemverError::InvalidVersion(format!("Invalid empty version string: '{}'", s))
    }

    /// Build an `InvalidVersion` for leading zero in major.
    pub fn leading_zero_major(s: &str) -> Self {
        SemverError::InvalidVersion(format!("Invalid leading zero in major: '{}'", s))
    }

    /// Build an `InvalidVersion` for leading zero in minor.
    pub fn leading_zero_minor(s: &str) -> Self {
        SemverError::InvalidVersion(format!("Invalid leading zero in minor: '{}'", s))
    }

    /// Build an `InvalidVersion` for leading zero in patch.
    pub fn leading_zero_patch(s: &str) -> Self {
        SemverError::InvalidVersion(format!("Invalid leading zero in patch: '{}'", s))
    }

    /// Build an `InvalidVersion` for empty identifier in a prerelease/build tuple.
    pub fn empty_identifier(ident: &str, context: &str) -> Self {
        SemverError::InvalidVersion(format!("Invalid empty identifier '{}' in '{}'", ident, context))
    }

    /// Build an `InvalidVersion` for leading zero in a prerelease numeric identifier.
    pub fn leading_zero_identifier(ident: &str) -> Self {
        SemverError::InvalidVersion(format!("Invalid leading zero in identifier '{}'", ident))
    }

    /// Build an `InvalidCoerce` error.
    pub fn invalid_coerce(s: &str) -> Self {
        SemverError::InvalidCoerce(format!("Version string lacks a numerical component: '{}'", s))
    }

    /// Build an `InvalidSpec` error with a free-form message.
    pub fn invalid_spec(msg: impl Into<String>) -> Self {
        SemverError::InvalidSpec(msg.into())
    }
}
