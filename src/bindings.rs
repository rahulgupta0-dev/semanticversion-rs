//! PyO3 bindings exposing the Rust `semantic_version` core as a Python
//! extension module.
//!
//! The exposed surface mirrors `python-semanticversion`'s `base.py` module:
//! `Version`, `SimpleSpec`, `LegacySpec` (registered both as `Spec` and
//! `LegacySpec`), `NpmSpec`, `SpecItem`, `Clause` plus the module-level
//! functions `compare`, `match` and `validate`.
//!
//! The same classes are registered on the submodule `semantic_version.base`
use pyo3::types::PyType;

use std::hash::{Hash, Hasher};

use pyo3::basic::CompareOp;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PyList, PyTuple};

use crate::error::SemverError;
use crate::identifiers::{BuildIdent, PreReleaseIdent};
use crate::npm_spec::NpmSpec as RustNpmSpec;
use crate::simple_spec::SimpleSpec as RustSimpleSpec;
use crate::version::Version as RustVersion;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn to_pyvalue_error(err: SemverError) -> PyErr {
    PyValueError::new_err(err.to_string())
}

/// Hash of a `Version` over its RAW fields (prerelease/build `Option`
/// included), mirroring Python's `hash((major, minor, patch, prerelease,
/// build))` where `None != ()`.
fn version_raw_hash(v: &RustVersion) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.major.hash(&mut h);
    v.minor.hash(&mut h);
    v.patch.hash(&mut h);
    v.prerelease.hash(&mut h);
    v.build.hash(&mut h);
    h.finish()
}

fn clause_hash(c: &crate::clause::Clause) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn mix(seed: u64, value: u64) -> u64 {
        let mut h = DefaultHasher::new();
        seed.hash(&mut h);
        value.hash(&mut h);
        h.finish()
    }

    match c {
        crate::clause::Clause::Always => mix(1, 0),
        crate::clause::Clause::Never => mix(2, 0),
        crate::clause::Clause::Range(r) => {
            let mut h = DefaultHasher::new();
            3u64.hash(&mut h);
            r.hash(&mut h);
            h.finish()
        }
        // base.py stores AllOf/AnyOf children in a `frozenset`: hashing is
        // order-insensitive and duplicates collapse.
        crate::clause::Clause::AllOf(children) | crate::clause::Clause::AnyOf(children) => {
            let seed = if matches!(c, crate::clause::Clause::AllOf(_)) { 4 } else { 5 };
            let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();
            for child in children {
                seen.insert(clause_hash(child));
            }
            let mut acc = 0u64;
            for h in seen {
                acc ^= h.wrapping_mul(0x9e37_79b9_7f4a_7c15);
            }
            mix(seed, acc)
        }
    }
}

/// Python-exact clause equality. `AllOf`/`AnyOf` compare as frozensets
/// (order-insensitive, deduplicated) — mirrors base.py:770-771 and 833-834.
fn clause_eq_python(a: &crate::clause::Clause, b: &crate::clause::Clause) -> bool {
    use crate::clause::Clause as C;

    fn set_eq(a: &[crate::clause::Clause], b: &[crate::clause::Clause]) -> bool {
        a.iter().all(|ca| b.iter().any(|cb| clause_eq_python(ca, cb)))
            && b.iter().all(|cb| a.iter().any(|ca| clause_eq_python(ca, cb)))
    }

    match (a, b) {
        (C::Always, C::Always) => true,
        (C::Never, C::Never) => true,
        (C::Range(ra), C::Range(rb)) => {
            ra.operator == rb.operator
                && ra.target == rb.target
                && ra.prerelease_policy == rb.prerelease_policy
                && ra.build_policy == rb.build_policy
        }
        (C::AllOf(aa), C::AllOf(bb)) => set_eq(aa, bb),
        (C::AnyOf(aa), C::AnyOf(bb)) => set_eq(aa, bb),
        _ => false,
    }
}

/// `'15'` -> `Numeric(15)`, anything else -> `Alpha`.
fn string_to_prerelease_ident(s: &str) -> PreReleaseIdent {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
        PreReleaseIdent::Numeric(s.parse::<u64>().unwrap_or(0))
    } else {
        PreReleaseIdent::Alpha(s.to_owned())
    }
}

/// Validate prerelease/build identifiers like `Version._validate_identifiers`.
fn validate_identifiers(ids: &[String], allow_leading_zeroes: bool) -> PyResult<()> {
    for item in ids {
        if item.is_empty() {
            return Err(PyValueError::new_err(format!(
                "Invalid empty identifier {:?} in {:?}",
                item,
                ids.join(".")
            )));
        }
        if !allow_leading_zeroes
            && item.len() > 1
            && item.starts_with('0')
            && item.chars().all(|c| c.is_ascii_digit())
        {
            return Err(PyValueError::new_err(format!(
                "Invalid leading zero in identifier {:?}",
                item
            )));
        }
    }
    Ok(())
}

/// Build a new instance of `ty` from a display string, the way Python does
/// when constructing `self.__class__(str(self))` — this keeps subclasses.
fn construct_via_type(
    py: Python<'_>,
    ty: &Bound<'_, PyType>,
    display: &str,
    partial: bool,
) -> PyResult<PyObject> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("partial", partial)?;
    Ok(ty.call((display,), Some(&kwargs))?.unbind())
}

/// Tuple of per-identifier precedence keys for a prerelease list.
/// `Max` -> `(2,)`, numeric -> `(0, n)`, alpha -> `(1, s)`.
fn prerelease_key_parts(py: Python<'_>, ids: &[PreReleaseIdent]) -> PyResult<PyObject> {
    let mut parts: Vec<Py<PyAny>> = Vec::with_capacity(ids.len());
    for id in ids {
        let elem: Py<PyAny> = match id {
            PreReleaseIdent::Max => PyTuple::new(py, [2_u64])?.into_any().unbind(),
            PreReleaseIdent::Numeric(n) => PyTuple::new(py, [(0_u64, *n)])?.into_any().unbind(),
            PreReleaseIdent::Alpha(s) => {
                PyTuple::new(py, [(1_u64, s.clone())])?.into_any().unbind()
            }
        };
        parts.push(elem);
    }
    Ok(PyTuple::new(py, parts)?.into_any().unbind())
}

/// Tuple of per-identifier precedence keys for a build list.
/// Numeric -> `(0, int(s))`, alpha -> `(1, s)`.
fn build_key_parts(py: Python<'_>, ids: &[BuildIdent]) -> PyResult<PyObject> {
    let mut parts: Vec<Py<PyAny>> = Vec::with_capacity(ids.len());
    for id in ids {
        let elem: Py<PyAny> = match id {
            BuildIdent::Numeric(s) => {
                PyTuple::new(py, [(0_u64, s.parse::<u64>().unwrap_or(0))])?
                    .into_any()
                    .unbind()
            }
            BuildIdent::Alpha(s) => PyTuple::new(py, [(1_u64, s.clone())])?.into_any().unbind(),
        };
        parts.push(elem);
    }
    Ok(PyTuple::new(py, parts)?.into_any().unbind())
}

// ---------------------------------------------------------------------------
// Version
// ---------------------------------------------------------------------------

#[pyclass(module = "semantic_version", subclass)]
pub struct Version {
    pub(crate) inner: RustVersion,
}

impl Version {
    pub(crate) fn from_rust(inner: RustVersion) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl Version {
    #[new]
    #[pyo3(signature = (version_string=None, major=None, minor=None, patch=None, prerelease=None, build=None, partial=false))]
    fn py_new(
        version_string: Option<&str>,
        major: Option<u64>,
        minor: Option<u64>,
        patch: Option<u64>,
        prerelease: Option<Vec<String>>,
        build: Option<Vec<String>>,
        partial: bool,
    ) -> PyResult<Self> {
        let has_text = version_string.is_some();
        let has_parts = major.is_some()
            || minor.is_some()
            || patch.is_some()
            || prerelease.is_some()
            || build.is_some();
        if has_text == has_parts {
            return Err(PyValueError::new_err(
                "Call either Version('1.2.3') or Version(major=1, ...).",
            ));
        }
        if let Some(text) = version_string {
            let inner = if partial {
                RustVersion::parse_partial(text)
            } else {
                RustVersion::parse(text)
            }
            .map_err(to_pyvalue_error)?;
            return Ok(Self { inner });
        }
        let major = major
            .ok_or_else(|| PyValueError::new_err("Invalid kwargs to Version(major=None, ...)"))?;
        if minor.is_none() || patch.is_none() {
            if !partial {
                return Err(PyValueError::new_err(
                    "Invalid kwargs to Version(major=..., minor=..., patch=..., ...)",
                ));
            }
        }
        // Mirrors base.py: `prerelease = tuple(prerelease or ())` and, when
        // not partial, `build = tuple(build or ())`.
        let prerelease_v = match prerelease {
            Some(ids) => {
                validate_identifiers(&ids, false)?;
                Some(ids.iter().map(|s| string_to_prerelease_ident(s)).collect())
            }
            None => {
                if partial {
                    None
                } else {
                    Some(vec![])
                }
            }
        };
        let build_v = match build {
            Some(ids) => {
                validate_identifiers(&ids, true)?;
                Some(ids.iter().map(|s| BuildIdent::parse(s)).collect())
            }
            None => {
                if partial {
                    None
                } else {
                    Some(vec![])
                }
            }
        };
        Ok(Self {
            inner: RustVersion {
                major,
                minor,
                patch,
                prerelease: prerelease_v,
                build: build_v,
                partial,
            },
        })
    }

    #[classmethod]
    #[pyo3(signature = (version_string, partial=false, coerce=false))]
    fn parse(
        cls: &Bound<'_, PyType>,
        version_string: &str,
        partial: bool,
        coerce: bool,
    ) -> PyResult<PyObject> {
        let inner = if coerce {
            RustVersion::coerce(version_string, partial)
        } else if partial {
            RustVersion::parse_partial(version_string)
        } else {
            RustVersion::parse(version_string)
        }
        .map_err(to_pyvalue_error)?;
        construct_via_type(cls.py(), cls, &inner.to_string(), partial)
    }

    #[classmethod]
    #[pyo3(signature = (version_string, partial=false))]
    fn coerce(
        cls: &Bound<'_, PyType>,
        version_string: &str,
        partial: bool,
    ) -> PyResult<PyObject> {
        let inner = RustVersion::coerce(version_string, partial).map_err(to_pyvalue_error)?;
        construct_via_type(cls.py(), cls, &inner.to_string(), partial)
    }

    #[getter]
    fn major(&self) -> u64 {
        self.inner.major
    }

    #[getter]
    fn minor(&self) -> Option<u64> {
        self.inner.minor
    }

    #[getter]
    fn patch(&self) -> Option<u64> {
        self.inner.patch
    }

    #[getter]
    fn partial(&self) -> bool {
        self.inner.partial
    }

    #[getter]
    fn prerelease(&self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        match &self.inner.prerelease {
            None => Ok(None),
            Some(ids) => {
                let strs: Vec<String> = ids.iter().map(|i| i.to_string()).collect();
                Ok(Some(PyTuple::new(py, strs)?.into_any().unbind()))
            }
        }
    }

    #[getter]
    fn build(&self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        match &self.inner.build {
            None => Ok(None),
            Some(ids) => {
                let strs: Vec<String> = ids.iter().map(|i| i.as_str().to_owned()).collect();
                Ok(Some(PyTuple::new(py, strs)?.into_any().unbind()))
            }
        }
    }

    #[getter]
    fn precedence_key(&self, py: Python<'_>) -> PyResult<PyObject> {
        let pre_key: Py<PyAny> = match self.inner.prerelease.as_deref() {
            Some(ids) if !ids.is_empty() => prerelease_key_parts(py, ids)?,
            _ => prerelease_key_parts(py, &[PreReleaseIdent::Max])?,
        };
        let build_ids: Vec<BuildIdent> = self.inner.build.clone().unwrap_or_default();
        let build_key = build_key_parts(py, &build_ids)?;
        let major: Py<PyAny> = self.inner.major.into_pyobject(py)?.into_any().unbind();
        let minor: Py<PyAny> = match self.inner.minor {
            Some(n) => n.into_pyobject(py)?.into_any().unbind(),
            None => py.None(),
        };
        let patch: Py<PyAny> = match self.inner.patch {
            Some(n) => n.into_pyobject(py)?.into_any().unbind(),
            None => py.None(),
        };
        Ok(PyTuple::new(py, [major, minor, patch, pre_key, build_key])?.into_any().unbind())
    }

    fn __str__(&self) -> String {
        self.inner.to_string()
    }

    fn __repr__(&self) -> String {
        if self.inner.partial {
            format!("Version('{}', partial=True)", self.inner)
        } else {
            format!("Version('{}')", self.inner)
        }
    }

    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        let prerelease: Py<PyAny> = match &self.inner.prerelease {
            Some(ids) => {
                let strs: Vec<String> = ids.iter().map(|i| i.to_string()).collect();
                PyTuple::new(py, strs)?.into_any().unbind()
            }
            None => py.None(),
        };
        let build: Py<PyAny> = match &self.inner.build {
            Some(ids) => {
                let strs: Vec<String> = ids.iter().map(|i| i.as_str().to_owned()).collect();
                PyTuple::new(py, strs)?.into_any().unbind()
            }
            None => py.None(),
        };
        let major: Py<PyAny> = self.inner.major.into_pyobject(py)?.into_any().unbind();
        let minor: Py<PyAny> = match self.inner.minor {
            Some(n) => n.into_pyobject(py)?.into_any().unbind(),
            None => py.None(),
        };
        let patch: Py<PyAny> = match self.inner.patch {
            Some(n) => n.into_pyobject(py)?.into_any().unbind(),
            None => py.None(),
        };
        let tuple = PyTuple::new(py, [major, minor, patch, prerelease, build])?;
        PyIterator::from_object(tuple.as_any())
    }

    fn __hash__(&self) -> isize {
        version_raw_hash(&self.inner) as isize
    }

    fn __richcmp__(
        &self,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
        op: CompareOp,
    ) -> PyResult<PyObject> {
        let other = match other.extract::<PyRef<Version>>() {
            Ok(o) => o,
            Err(_) => return Ok(py.NotImplemented()),
        };
        let result = match op {
            // Equality is normalized: `prerelease=None` == `prerelease=()`
            // (mirrors base.py `(self.prerelease or ()) == ...`).
            CompareOp::Eq => self.inner == other.inner,
            // Inequality is a RAW field comparison: `None != ()`.
            CompareOp::Ne => {
                self.inner.major != other.inner.major
                    || self.inner.minor != other.inner.minor
                    || self.inner.patch != other.inner.patch
                    || self.inner.prerelease != other.inner.prerelease
                    || self.inner.build != other.inner.build
            }
            CompareOp::Lt => self.inner.precedence_lt(&other.inner),
            CompareOp::Le => self.inner.precedence_le(&other.inner),
            CompareOp::Gt => self.inner.precedence_gt(&other.inner),
            CompareOp::Ge => self.inner.precedence_ge(&other.inner),
        };
        Ok(u8::from(result).into_pyobject(py)?.unbind().into_any())
    }

    #[pyo3(signature = (level=None))]
    fn truncate(slf: &Bound<'_, Self>, level: Option<&str>) -> PyResult<PyObject> {
        let inner = slf.borrow().inner.clone();
        let level = level.unwrap_or("patch");
        // All branches mirror base.py's kwargs construction path:
        // `prerelease` is always normalized to `()` (unless explicitly
        // passed), `build` to `()` only when not partial.
        let pre = inner.prerelease.clone().unwrap_or_default();
        let build_ids = inner.build.clone().unwrap_or_default();
        let build_v = if inner.partial { None } else { Some(vec![]) };
        let truncated = match level {
            "build" => RustVersion {
                major: inner.major,
                minor: inner.minor,
                patch: inner.patch,
                prerelease: Some(pre),
                build: if inner.partial { None } else { Some(build_ids) },
                partial: inner.partial,
            },
            "prerelease" => RustVersion {
                major: inner.major,
                minor: inner.minor,
                patch: inner.patch,
                prerelease: Some(pre),
                build: build_v,
                partial: inner.partial,
            },
            "patch" => RustVersion {
                major: inner.major,
                minor: inner.minor,
                patch: inner.patch,
                prerelease: Some(vec![]),
                build: build_v,
                partial: inner.partial,
            },
            "minor" => RustVersion {
                major: inner.major,
                minor: inner.minor,
                patch: if inner.partial { None } else { Some(0) },
                prerelease: Some(vec![]),
                build: build_v,
                partial: inner.partial,
            },
            "major" => RustVersion {
                major: inner.major,
                minor: if inner.partial { None } else { Some(0) },
                patch: if inner.partial { None } else { Some(0) },
                prerelease: Some(vec![]),
                build: build_v,
                partial: inner.partial,
            },
            other => {
                return Err(PyValueError::new_err(format!(
                    "Invalid truncation level `{}`.",
                    other
                )));
            }
        };
        construct_via_type(
            slf.py(),
            &slf.get_type(),
            &truncated.to_string(),
            truncated.partial,
        )
    }

    fn next_major(slf: &Bound<'_, Self>) -> PyResult<PyObject> {
        let inner = slf.borrow().inner.clone();
        let bumped = inner.next_major();
        let result = RustVersion {
            major: bumped.major,
            minor: Some(0),
            patch: Some(0),
            prerelease: Some(vec![]),
            build: if inner.partial { None } else { Some(vec![]) },
            partial: inner.partial,
        };
        construct_via_type(slf.py(), &slf.get_type(), &result.to_string(), inner.partial)
    }

    fn next_minor(slf: &Bound<'_, Self>) -> PyResult<PyObject> {
        let inner = slf.borrow().inner.clone();
        let bumped = inner.next_minor();
        let result = RustVersion {
            major: bumped.major,
            minor: Some(bumped.minor.unwrap_or(0)),
            patch: Some(0),
            prerelease: Some(vec![]),
            build: if inner.partial { None } else { Some(vec![]) },
            partial: inner.partial,
        };
        construct_via_type(slf.py(), &slf.get_type(), &result.to_string(), inner.partial)
    }

    fn next_patch(slf: &Bound<'_, Self>) -> PyResult<PyObject> {
        let inner = slf.borrow().inner.clone();
        let bumped = inner.next_patch();
        let result = RustVersion {
            major: bumped.major,
            minor: Some(bumped.minor.unwrap_or(0)),
            patch: Some(bumped.patch.unwrap_or(0)),
            prerelease: Some(vec![]),
            build: if inner.partial { None } else { Some(vec![]) },
            partial: inner.partial,
        };
        construct_via_type(slf.py(), &slf.get_type(), &result.to_string(), inner.partial)
    }
}

// ---------------------------------------------------------------------------
// SpecItem
// ---------------------------------------------------------------------------

#[pyclass(module = "semantic_version")]
pub struct SpecItem {
    kind: String,
    spec_display: String,
    spec_version: Option<Py<Version>>,
    clause: crate::clause::Clause,
}

impl SpecItem {
    /// Parse a single requirement like `'>=1.2.3'`, `'^1.2.3'` or `'*'`.
    fn parse_requirement(py: Python<'_>, requirement_string: &str) -> PyResult<Self> {
        if requirement_string.is_empty() {
            return Err(PyValueError::new_err(format!(
                "Invalid empty requirement specification: {:?}",
                requirement_string
            )));
        }
        if requirement_string == "*" {
            let clause = python_simple_parse("*").map_err(to_pyvalue_error)?;
            return Ok(SpecItem {
                kind: "*".to_owned(),
                spec_display: String::new(),
                spec_version: None,
                clause,
            });
        }
        // Mirrors base.py's `re_spec`: `^(<|<=||=|==|>=|>|!=|\^|~|~=)(\d.*)$`
        // plus the `KIND_ALIASES = {'=': '==', '': '=='}` mapping and the
        // bare-`'0.1.0'` / `'1'` digit fallback.
        // Mirrors base.py's `re_spec`: `^(<|<=||=|==|>=|>|!=|\^|~|~=)(\d.*)$`
        // plus the `KIND_ALIASES = {'=': '==', '': '=='}` mapping and the
        // bare-`'0.1.0'` / `'1'` digit fallback.
        const PREFIXES: &[(&str, &str)] = &[
            ("<=", "<="),
            ("==", "=="),
            (">=", ">="),
            ("!=", "!="),
            ("~=", "~="),
            ("<", "<"),
            (">", ">"),
            ("=", "=="),
            ("^", "^"),
        ];
        let mut kind: Option<&str> = None;
        let mut rest: &str = requirement_string;
        for (prefix, canonical) in PREFIXES {
            if let Some(r) = requirement_string.strip_prefix(prefix) {
                if r.starts_with(|c: char| c.is_ascii_digit()) {
                    kind = Some(canonical);
                    rest = r;
                    break;
                }
            }
        }
        if kind.is_none() {
            if let Some(r) = requirement_string.strip_prefix('~') {
                if r.starts_with(|c: char| c.is_ascii_digit()) {
                    kind = Some("~");
                    rest = r;
                }
            }
        }
        if kind.is_none() {
            if requirement_string.starts_with(|c: char| c.is_ascii_digit()) {
                kind = Some("==");
                rest = requirement_string;
            }
        }
        let kind = kind.ok_or_else(|| {
            PyValueError::new_err(format!(
                "Invalid requirement specification: {:?}",
                requirement_string
            ))
        })?;

        let spec = RustVersion::parse_partial(rest).map_err(to_pyvalue_error)?;
        if spec.build.is_some() && kind != "==" && kind != "!=" {
            return Err(PyValueError::new_err(format!(
                "Invalid requirement specification {:?}: build numbers have no ordering.",
                requirement_string
            )));
        }
        let clause = python_simple_parse(requirement_string).map_err(to_pyvalue_error)?;
        let spec_display = spec.to_string();
        let spec_version = Some(Py::new(py, Version { inner: spec })?);
        Ok(SpecItem {
            kind: kind.to_owned(),
            spec_display,
            spec_version,
            clause,
        })
    }
}

#[pymethods]
#[allow(non_snake_case)]
impl SpecItem {
    #[classattr]
    fn KIND_ANY() -> &'static str {
        "*"
    }
    #[classattr]
    fn KIND_LT() -> &'static str {
        "<"
    }
    #[classattr]
    fn KIND_LTE() -> &'static str {
        "<="
    }
    #[classattr]
    fn KIND_EQUAL() -> &'static str {
        "=="
    }
    #[classattr]
    fn KIND_SHORTEQ() -> &'static str {
        "="
    }
    #[classattr]
    fn KIND_EMPTY() -> &'static str {
        ""
    }
    #[classattr]
    fn KIND_GTE() -> &'static str {
        ">="
    }
    #[classattr]
    fn KIND_GT() -> &'static str {
        ">"
    }
    #[classattr]
    fn KIND_NEQ() -> &'static str {
        "!="
    }
    #[classattr]
    fn KIND_CARET() -> &'static str {
        "^"
    }
    #[classattr]
    fn KIND_TILDE() -> &'static str {
        "~"
    }
    #[classattr]
    fn KIND_COMPATIBLE() -> &'static str {
        "~="
    }

    #[new]
    fn new(py: Python<'_>, requirement_string: &str) -> PyResult<Self> {
        Self::parse_requirement(py, requirement_string)
    }

    #[getter]
    fn kind(&self) -> String {
        self.kind.clone()
    }

    #[getter]
    fn spec(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(match &self.spec_version {
            Some(v) => v.clone_ref(py).into_any(),
            None => String::new().into_pyobject(py)?.into_any().unbind(),
        })
    }

    #[pyo3(name = "match")]
    fn match_(&self, version: &Version) -> bool {
        self.clause.matches(&version.inner)
    }

    fn __str__(&self) -> String {
        format!("{}{}", self.kind, self.spec_display)
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        match &self.spec_version {
            Some(v) => format!(
                "<SpecItem: {} Version('{}', partial=True)>",
                self.kind, v.borrow(py).inner
            ),
            None => format!("<SpecItem: {} ''>", self.kind),
        }
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyObject {
        let other = match other.extract::<PyRef<SpecItem>>() {
            Ok(o) => o,
            Err(_) => return py.NotImplemented(),
        };
        let spec_eq = match (&self.spec_version, &other.spec_version) {
            (Some(a), Some(b)) => a.borrow(py).inner == b.borrow(py).inner,
            (None, None) => true,
            _ => false,
        };
        u8::from(self.kind == other.kind && spec_eq)
            .into_pyobject(py)
            .unwrap()
            .unbind()
            .into_any()
    }

    fn __hash__(&self, py: Python<'_>) -> isize {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.kind.hash(&mut h);
        match &self.spec_version {
            Some(v) => version_raw_hash(&v.borrow(py).inner).hash(&mut h),
            None => String::new().hash(&mut h),
        }
        h.finish() as isize
    }
}

/// Convert a matcher clause into the flat list of `SpecItem`s Python's
/// `LegacySpec.__iter__` would produce.
fn clause_to_spec_items(
    py: Python<'_>,
    clause: &crate::clause::Clause,
) -> PyResult<Vec<Py<SpecItem>>> {
    match clause {
        crate::clause::Clause::Always => Ok(vec![Py::new(py, SpecItem::parse_requirement(py, "*")?)?]),
        crate::clause::Clause::Never => {
            Ok(vec![Py::new(py, SpecItem::parse_requirement(py, "<0.0.0-")?)?])
        }
        crate::clause::Clause::Range(r) => Ok(vec![Py::new(
            py,
            SpecItem::parse_requirement(py, &format!("{}{}", r.operator, r.target))?,
        )?]),
        crate::clause::Clause::AllOf(children) | crate::clause::Clause::AnyOf(children) => {
            children.iter().try_fold(Vec::new(), |mut acc, c| {
                acc.extend(clause_to_spec_items(py, c)?);
                Ok(acc)
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Python-exact SimpleSpec parsing (base.py SimpleSpec.Parser, lines 1052-1214)
// ---------------------------------------------------------------------------
//
// The Rust core's `SimpleSpec::parse` handles only the restricted subset used
// by its native tests. Python additionally accepts `*` wildcard components and
// expands partial versions (`==0.1.*` -> `>=0.1.0 <0.2.0`, `!=1.x` ->
// `<1.0.0 || >=2.0.0`, ...). These helpers reproduce that behavior exactly so
// the Python-visible `Spec`/`SimpleSpec`/`SpecItem` match `base.py` clause
// for clause.

/// Python-exact SimpleSpec parsing (base.py SimpleSpec.Parser.parse).
/// Matches Python's `clause = Always(); for block in blocks: clause &= parse_block(block)`,
/// including the AllOf-flattening and frozenset-dedup behavior.
fn python_simple_parse(expression: &str) -> Result<crate::clause::Clause, SemverError> {
    if expression.trim().is_empty() {
        return Err(SemverError::invalid_spec("Invalid simple spec: empty string"));
    }
    let mut clause = crate::clause::Clause::Always;
    for block in expression.split(',') {
        clause = clause & python_parse_block(block)?;
    }
    // Python's chain only produces a bare clause after the FIRST block
    // (Always & X = X).  From the second block onwards, `&=` always wraps
    // in AllOf via Matcher.__and__ -> AllOf(self, other).  Even with 2
    // identical blocks, Python's frozenset dedupes but keeps the AllOf wrapper.
    if expression.contains(',') {
        // Flatten nested AllOf and dedupe (frozenset semantics).
        let flat = flatten_allof_for_simple(clause);
        let mut seen: std::collections::HashSet<crate::clause::Clause> = std::collections::HashSet::new();
        let mut deduped: Vec<crate::clause::Clause> = Vec::new();
        for c in flat {
            if seen.insert(c.clone()) {
                deduped.push(c);
            }
        }
        Ok(crate::clause::Clause::AllOf(deduped))
    } else {
        // Single block: return bare (Always & X = X in Python).
        Ok(clause)
    }
}

/// Flatten AllOf children; other clauses pass through unchanged.
fn flatten_allof_for_simple(c: crate::clause::Clause) -> Vec<crate::clause::Clause> {
    if let crate::clause::Clause::AllOf(inner) = c {
        inner
    } else {
        vec![c]
    }
}

fn python_parse_block(block: &str) -> Result<crate::clause::Clause, SemverError> {
    use crate::clause::{BuildPolicy, Operator, PrereleasePolicy, Range};
    use crate::version::Version as V;
    use regex::Regex;
    use std::sync::OnceLock;

    // Mirrors base.py NAIVE_SPEC (1054-1062): op can be empty; `*` is a valid
    // component; prerel/build capture is present (possibly empty) or absent.
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r"^(?P<op><|<=||=|==|>=|>|!=|\^|~|~=)(?P<major>\*|0|[1-9][0-9]*)(?:\.(?P<minor>\*|0|[1-9][0-9]*)(?:\.(?P<patch>\*|0|[1-9][0-9]*))?)?(?:-(?P<prerel>[a-zA-Z0-9.-]*))?(?:\+(?P<build>[a-zA-Z0-9.-]*))?$",
        )
        .expect("naive spec regex is valid")
    });
    let caps = re.captures(block).ok_or_else(|| {
        SemverError::invalid_spec(&format!("Invalid simple block '{block}'"))
    })?;

    let op_str = caps.name("op").map(|m| m.as_str()).unwrap_or("");
    let major_t = caps.name("major").map(|m| m.as_str()).unwrap_or("");
    let minor_t = caps.name("minor").map(|m| m.as_str());
    let patch_t = caps.name("patch").map(|m| m.as_str());
    let prerel = caps.name("prerel").map(|m| m.as_str());
    let build = caps.name("build").map(|m| m.as_str());

    // base.py:1097 PREFIX_ALIASES: '=' -> '==', '' -> '=='
    let prefix = match op_str {
        "=" | "" => "==",
        other => other,
    };

    let empty = |t: Option<&str>| matches!(t, None | Some("*"));
    let major = if empty(Some(major_t)) { None } else { Some(major_t.parse::<u64>().map_err(|_| SemverError::invalid_spec(&format!("Invalid simple block '{block}'")))?) };
    let minor = if empty(minor_t) { None } else { Some(minor_t.unwrap().parse::<u64>().map_err(|_| SemverError::invalid_spec(&format!("Invalid simple block '{block}'")))?) };
    let patch = if empty(patch_t) { None } else { Some(patch_t.unwrap().parse::<u64>().map_err(|_| SemverError::invalid_spec(&format!("Invalid simple block '{block}'")))?) };
    // base.py:1103-1118 — kwargs-built full target.
    let target = if major.is_none() {
        V::from_parts(0, 0, 0, Some(vec![]), Some(vec![]))
    } else if minor.is_none() {
        V::from_parts(major.unwrap(), 0, 0, Some(vec![]), Some(vec![]))
    } else if patch.is_none() {
        V::from_parts(major.unwrap(), minor.unwrap(), 0, Some(vec![]), Some(vec![]))
    } else {
        let prerelease_idents: Vec<PreReleaseIdent> = prerel
            .filter(|p| !p.is_empty())
            .map(|p| {
                let parts: Result<Vec<PreReleaseIdent>, _> = p.split('.').map(|part| {
                    if part.is_empty() {
                        Err(SemverError::invalid_spec(&format!("Invalid empty identifier '' in {:?}", p)))
                    } else {
                        Ok(string_to_prerelease_ident(part))
                    }
                }).collect();
                parts
            })
            .transpose()?
            .unwrap_or_default();
        let build_idents: Vec<BuildIdent> = build
            .filter(|b| !b.is_empty())
            .map(|b| b.split('.').map(BuildIdent::parse).collect())
            .unwrap_or_default();
        V::from_parts(
            major.unwrap(),
            minor.unwrap(),
            patch.unwrap(),
            Some(prerelease_idents),
            Some(build_idents),
        )
    };

    // base.py:1120 — partial + (prerel or build) is invalid.
    if (major.is_none() || minor.is_none() || patch.is_none())
        && (prerel.map_or(false, |p| !p.is_empty()) || build.map_or(false, |b| !b.is_empty()))
    {
        return Err(SemverError::invalid_spec(&format!("Invalid simple spec '{block}'")));
    }
    // base.py:1123 — build only allowed on == and !=.
    if build.is_some() && prefix != "==" && prefix != "!=" {
        return Err(SemverError::invalid_spec(&format!("Invalid simple spec '{block}'")));
    }
    let natural = PrereleasePolicy::Natural;
    let implicit = BuildPolicy::Implicit;
    let r = |op: Operator, t: V, pp: PrereleasePolicy, bp: BuildPolicy| -> Result<crate::clause::Clause, SemverError> {
        Ok(crate::clause::Clause::Range(Range::new(op, t, pp, bp)?))
    };
    match prefix {
        // base.py:1126-1134
        "^" => {
            let high = if target.major > 0 {
                target.next_major()
            } else if target.minor.unwrap_or(0) > 0 {
                target.next_minor()
            } else {
                target.next_patch()
            };
            Ok(r(Operator::Gte, target, natural, implicit)?
                & r(Operator::Lt, high, natural, implicit)?)
        }
        // base.py:1136-1144
        "~" => {
            let high = if minor.is_none() {
                target.next_major()
            } else {
                target.next_minor()
            };
            Ok(r(Operator::Gte, target, natural, implicit)?
                & r(Operator::Lt, high, natural, implicit)?)
        }
        // base.py:1146-1154
        "~=" => {
            let high = if minor.is_none() || patch.is_none() {
                target.next_major()
            } else {
                target.next_minor()
            };
            Ok(r(Operator::Gte, target, natural, implicit)?
                & r(Operator::Lt, high, natural, implicit)?)
        }
        // base.py:1156-1166
        "==" => {
            if major.is_none() {
                r(Operator::Gte, target, natural, implicit)
            } else if minor.is_none() {
                Ok(r(Operator::Gte, target.clone(), natural, implicit)?
                    & r(Operator::Lt, target.next_major(), natural, implicit)?)
            } else if patch.is_none() {
                Ok(r(Operator::Gte, target.clone(), natural, implicit)?
                    & r(Operator::Lt, target.next_minor(), natural, implicit)?)
            } else if build == Some("") {
                r(Operator::Eq, target, natural, BuildPolicy::Strict)
            } else {
                r(Operator::Eq, target, natural, implicit)
            }
        }
        // base.py:1168-1183
        "!=" => {
            if minor.is_none() {
                Ok(r(Operator::Lt, target.clone(), natural, implicit)?
                    | r(Operator::Gte, target.next_major(), natural, implicit)?)
            } else if patch.is_none() {
                Ok(r(Operator::Lt, target.clone(), natural, implicit)?
                    | r(Operator::Gte, target.next_minor(), natural, implicit)?)
            } else if prerel == Some("") {
                r(Operator::Neq, target, PrereleasePolicy::Always, implicit)
            } else if build == Some("") {
                r(Operator::Neq, target, natural, BuildPolicy::Strict)
            } else {
                r(Operator::Neq, target, natural, implicit)
            }
        }
        // base.py:1185-1193
        ">" => {
            if minor.is_none() {
                r(Operator::Gte, target.next_major(), natural, implicit)
            } else if patch.is_none() {
                r(Operator::Gte, target.next_minor(), natural, implicit)
            } else {
                r(Operator::Gt, target, natural, implicit)
            }
        }
        // base.py:1195-1196
        ">=" => r(Operator::Gte, target, natural, implicit),
        // base.py:1198-1203
        "<" => {
            if prerel == Some("") {
                r(Operator::Lt, target, PrereleasePolicy::Always, implicit)
            } else {
                r(Operator::Lt, target, natural, implicit)
            }
        }
        // base.py:1205-1214
        _ => {
            if minor.is_none() {
                r(Operator::Lt, target.next_major(), natural, implicit)
            } else if patch.is_none() {
                r(Operator::Lt, target.next_minor(), natural, implicit)
            } else {
                r(Operator::Lte, target, natural, implicit)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Specs
// ---------------------------------------------------------------------------

/// Python's `LegacySpec` class. Registered both as `Spec` and `LegacySpec`.
#[pyclass(module = "semantic_version", name = "LegacySpec")]
pub struct Spec {
    inner: RustSimpleSpec,
    expression: String,
}

impl Spec {
    fn iter_items<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let list = PyList::empty(py);
        for item in clause_to_spec_items(py, &self.inner.clause)? {
            list.append(item)?;
        }
        Ok(list)
    }
}

#[pymethods]
impl Spec {
    #[new]
    #[pyo3(signature = (*expressions))]
    fn new(expressions: Vec<String>) -> PyResult<Self> {
        let expression = expressions.join(",");
        let clause = python_simple_parse(&expression).map_err(to_pyvalue_error)?;
        let inner = RustSimpleSpec { clause };
        Ok(Spec { inner, expression })
    }

    #[pyo3(name = "match")]
    fn match_(&self, version: &Version) -> bool {
        self.inner.match_version(&version.inner)
    }

    fn __contains__(&self, other: &Bound<'_, PyAny>) -> bool {
        match other.extract::<PyRef<Version>>() {
            Ok(v) => self.inner.match_version(&v.inner),
            Err(_) => false,
        }
    }

    fn filter(&self, py: Python<'_>, versions: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let list = PyList::empty(py);
        for item in versions.try_iter()? {
            let item = item?;
            if let Ok(v) = item.extract::<PyRef<Version>>() {
                if self.inner.match_version(&v.inner) {
                    list.append(item)?;
                }
            }
        }
        Ok(list.unbind().into_any())
    }

    fn select(&self, _py: Python<'_>, versions: &Bound<'_, PyAny>) -> PyResult<Option<Version>> {
        let mut best: Option<Version> = None;
        for item in versions.try_iter()? {
            let item = item?;
            if let Ok(v) = item.extract::<PyRef<Version>>() {
                let v = v.inner.clone();
                if !self.inner.match_version(&v) {
                    continue;
                }
                match &best {
                    None => best = Some(Version::from_rust(v)),
                    Some(cur) => {
                        if v.precedence_gt(&cur.inner) {
                            best = Some(Version::from_rust(v));
                        }
                    }
                }
            }
        }
        Ok(best)
    }

    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        let list = self.iter_items(py)?;
        PyIterator::from_object(list.as_any())
    }

    #[getter]
    fn specs(&self, py: Python<'_>) -> PyResult<PyObject> {
        Ok(self.iter_items(py)?.unbind().into_any())
    }

    #[getter]
    fn clause(&self) -> Clause {
        Clause {
            inner: self.inner.clause.clone(),
        }
    }

    fn __str__(&self) -> String {
        self.expression.clone()
    }

    fn __repr__(&self) -> String {
        format!("<LegacySpec: '{}'>", self.expression)
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyObject {
        match other.extract::<PyRef<Spec>>() {
            Ok(o) => u8::from(clause_eq_python(&self.inner.clause, &o.inner.clause))
                .into_pyobject(py)
                .unwrap()
                .unbind()
                .into_any(),
            Err(_) => py.NotImplemented(),
        }
    }

    fn __hash__(&self) -> isize {
        clause_hash(&self.inner.clause) as isize
    }
}

#[pyclass(module = "semantic_version")]
pub struct SimpleSpec {
    inner: RustSimpleSpec,
    expression: String,
}

#[pymethods]
impl SimpleSpec {
    #[new]
    fn new(expression: &str) -> PyResult<Self> {
        let clause = python_simple_parse(expression).map_err(to_pyvalue_error)?;
        let inner = RustSimpleSpec { clause };
        Ok(SimpleSpec {
            inner,
            expression: expression.to_owned(),
        })
    }

    #[pyo3(name = "match")]
    fn match_(&self, version: &Version) -> bool {
        self.inner.match_version(&version.inner)
    }

    fn __contains__(&self, other: &Bound<'_, PyAny>) -> bool {
        match other.extract::<PyRef<Version>>() {
            Ok(v) => self.inner.match_version(&v.inner),
            Err(_) => false,
        }
    }

    #[getter]
    fn clause(&self) -> Clause {
        Clause {
            inner: self.inner.clause.clone(),
        }
    }

    fn __str__(&self) -> String {
        self.expression.clone()
    }

    fn __repr__(&self) -> String {
        format!("<SimpleSpec: '{}'>", self.expression)
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyObject {
        match other.extract::<PyRef<SimpleSpec>>() {
            Ok(o) => u8::from(clause_eq_python(&self.inner.clause, &o.inner.clause))
                .into_pyobject(py)
                .unwrap()
                .unbind()
                .into_any(),
            Err(_) => py.NotImplemented(),
        }
    }

    fn __hash__(&self) -> isize {
        clause_hash(&self.inner.clause) as isize
    }
}

#[pyclass(module = "semantic_version")]
pub struct NpmSpec {
    inner: RustNpmSpec,
    expression: String,
}

#[pymethods]
impl NpmSpec {
    #[new]
    fn new(expression: &str) -> PyResult<Self> {
        let mut inner = RustNpmSpec::parse(expression).map_err(to_pyvalue_error)?;
        // base.py's npm Parser always builds group clauses via `AllOf(*...)`
        // (base.py:1337-1339), so a single-block group is `AllOf([Range])`,
        // never a bare `Range`. Normalize to that shape for clause equality.
        if let crate::clause::Clause::Range(r) = &inner.clause {
            inner.clause = crate::clause::Clause::AllOf(vec![crate::clause::Clause::Range(r.clone())]);
        }
        Ok(NpmSpec {
            inner,
            expression: expression.to_owned(),
        })
    }

    #[pyo3(name = "match")]
    fn match_(&self, version: &Version) -> bool {
        self.inner.match_version(&version.inner)
    }

    fn __contains__(&self, other: &Bound<'_, PyAny>) -> bool {
        match other.extract::<PyRef<Version>>() {
            Ok(v) => self.inner.match_version(&v.inner),
            Err(_) => false,
        }
    }

    #[getter]
    fn clause(&self) -> Clause {
        Clause {
            inner: self.inner.clause.clone(),
        }
    }

    fn __str__(&self) -> String {
        self.expression.clone()
    }

    fn __repr__(&self) -> String {
        format!("<NpmSpec: '{}'>", self.expression)
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyObject {
        match other.extract::<PyRef<NpmSpec>>() {
            Ok(o) => u8::from(clause_eq_python(&self.inner.clause, &o.inner.clause))
                .into_pyobject(py)
                .unwrap()
                .unbind()
                .into_any(),
            Err(_) => py.NotImplemented(),
        }
    }

    fn __hash__(&self) -> isize {
        clause_hash(&self.inner.clause) as isize
    }
}

// ---------------------------------------------------------------------------
// Clause
// ---------------------------------------------------------------------------

#[pyclass(module = "semantic_version")]
pub struct Clause {
    inner: crate::clause::Clause,
}

fn python_clause_repr(c: &crate::clause::Clause) -> String {
    use crate::clause::{BuildPolicy, PrereleasePolicy};
    match c {
        crate::clause::Clause::Always => "Always()".to_owned(),
        crate::clause::Clause::Never => "Never()".to_owned(),
        crate::clause::Clause::Range(r) => {
            let mut s = format!("Range('{}', Version('{}')", r.operator, r.target);
            if r.prerelease_policy != PrereleasePolicy::Natural {
                let v = match r.prerelease_policy {
                    PrereleasePolicy::SamePatch => "same-patch",
                    PrereleasePolicy::Always => "always",
                    PrereleasePolicy::Natural => unreachable!(),
                };
                s.push_str(&format!(", prerelease_policy='{}'", v));
            }
            if r.build_policy != BuildPolicy::Implicit {
                let v = match r.build_policy {
                    BuildPolicy::Strict => "strict",
                    BuildPolicy::Implicit => unreachable!(),
                };
                s.push_str(&format!(", build_policy='{}'", v));
            }
            s.push(')');
            s
        }
        crate::clause::Clause::AllOf(children) => {
            let mut inner: Vec<String> = children.iter().map(python_clause_repr).collect();
            inner.sort();
            format!("AllOf({})", inner.join(", "))
        }
        crate::clause::Clause::AnyOf(children) => {
            let mut inner: Vec<String> = children.iter().map(python_clause_repr).collect();
            inner.sort();
            format!("AnyOf({})", inner.join(", "))
        }
    }
}

#[pymethods]
impl Clause {
    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyObject {
        match other.extract::<PyRef<Clause>>() {
            Ok(o) => u8::from(clause_eq_python(&self.inner, &o.inner))
                .into_pyobject(py)
                .unwrap()
                .unbind()
                .into_any(),
            Err(_) => py.NotImplemented(),
        }
    }

    fn __hash__(&self) -> isize {
        clause_hash(&self.inner) as isize
    }

    fn __repr__(&self) -> String {
        python_clause_repr(&self.inner)
    }
}

// ---------------------------------------------------------------------------
// Module-level functions
// ---------------------------------------------------------------------------

#[pyfunction]
fn compare(py: Python<'_>, v1: &str, v2: &str) -> PyResult<PyObject> {
    match crate::version::compare(v1, v2).map_err(to_pyvalue_error)? {
        Some(n) => Ok(n.into_pyobject(py)?.into_any().unbind()),
        None => Ok(py.NotImplemented()),
    }
}

#[pyfunction(name = "match")]
fn py_match(spec: &str, version: &str) -> PyResult<bool> {
    let clause = python_simple_parse(spec).map_err(to_pyvalue_error)?;
    let version = RustVersion::parse(version).map_err(to_pyvalue_error)?;
    Ok(clause.matches(&version))
}

#[pyfunction]
fn validate(version_string: &str) -> bool {
    crate::version::validate(version_string)
}

// ---------------------------------------------------------------------------
// Module registration
// ---------------------------------------------------------------------------

fn fill_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Version>()?;
    m.add_class::<SimpleSpec>()?;
    m.add_class::<NpmSpec>()?;
    m.add_class::<SpecItem>()?;
    m.add_class::<Clause>()?;
    // The pyclass is named `LegacySpec`; Python also exposes it as `Spec`.
    m.add_class::<Spec>()?;
    let spec_cls = m.getattr("LegacySpec")?;
    m.add("Spec", spec_cls)?;
    m.add_function(wrap_pyfunction!(compare, m)?)?;
    m.add_function(wrap_pyfunction!(py_match, m)?)?;
    m.add_function(wrap_pyfunction!(validate, m)?)?;
    Ok(())
}

/// Register the `semantic_version` extension module and its `base` submodule.
pub fn register_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    fill_module(m)?;
    let py = m.py();
    let base = PyModule::new(py, "base")?;
    fill_module(&base)?;
    m.add_submodule(&base)?;
    // Make `from semantic_version import base` work: the submodule must be
    // present in `sys.modules` for import machinery.
    py.import("sys")?
        .getattr("modules")?
        .set_item("semantic_version.base", base)?;
    Ok(())
}
