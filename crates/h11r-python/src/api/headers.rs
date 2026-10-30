//! The `Headers` field sequence.

use super::*;
use h11r as core;
use pyo3::exceptions::PyTypeError;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyAny, PyBool, PyBytes, PyIterator, PyList, PyString, PyTuple};

/// Ordered header or trailer fields that behave like a tuple of
/// `(name, value)` byte pairs.
///
/// Parsed fields stay in one Rust buffer until Python reads them: `get()` and
/// `get_all()` look up names without building the pairs, and iteration,
/// indexing, and comparison build them once. Construct one as
/// `Headers(fields=None)` from `(name, value)` pairs to reuse fields across
/// sends without converting them again; it compares equal to a tuple of the
/// same byte pairs.
#[pyclass(
    name = "Headers",
    module = "h11r",
    frozen,
    sequence,
    skip_from_py_object
)]
pub(super) struct PyHeaders {
    fields: core::Headers,
    /// The `(name, value)` pairs, built on first use.
    items: PyOnceLock<Py<PyTuple>>,
}

impl PyHeaders {
    pub(super) fn new_py(py: Python<'_>, fields: core::Headers) -> PyResult<Py<Self>> {
        Py::new(
            py,
            Self {
                fields,
                items: PyOnceLock::new(),
            },
        )
    }

    fn items<'py>(&self, py: Python<'py>) -> PyResult<&Bound<'py, PyTuple>> {
        if let Some(items) = self.items.get(py) {
            return Ok(items.bind(py));
        }
        // Built outside the cell: allocation can run Python code, such as a
        // finalizer, that reads these fields again, and re-entering an
        // initializing cell deadlocks. Concurrent first reads build twice.
        let items = py_headers(py, &self.fields)?.unbind();
        Ok(self.items.get_or_init(py, || items).bind(py))
    }
}

#[pymethods]
impl PyHeaders {
    #[new]
    #[pyo3(signature = (fields = None))]
    fn new(fields: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Ok(Self {
            fields: extract_headers(fields)?,
            items: PyOnceLock::new(),
        })
    }

    /// Return the first value of the field named `name`, or `default`.
    ///
    /// Field names match case-insensitively.
    ///
    /// Args:
    ///     name (str | bytes | bytearray | memoryview): The field name.
    ///     default (object): The result when no field matches.
    ///
    /// Returns:
    ///     value (bytes | object): The first matching value, or `default`.
    #[pyo3(signature = (name, default = None))]
    fn get(
        &self,
        py: Python<'_>,
        name: &Bound<'_, PyAny>,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        with_field_name(name, |name| match self.fields.get_all(name).next() {
            Some(value) => PyBytes::new(py, value).into_any().unbind(),
            None => default.unwrap_or_else(|| py.None()),
        })
    }

    /// Return the values of every field named `name`, in order.
    ///
    /// Field names match case-insensitively.
    ///
    /// Args:
    ///     name (str | bytes | bytearray | memoryview): The field name.
    ///
    /// Returns:
    ///     values (tuple[bytes, ...]): The matching values.
    fn get_all<'py>(
        &self,
        py: Python<'py>,
        name: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyTuple>> {
        with_field_name(name, |name| {
            PyTuple::new(
                py,
                self.fields
                    .get_all(name)
                    .map(|value| PyBytes::new(py, value))
                    .collect::<Vec<_>>(),
            )
        })?
    }

    fn __len__(&self) -> usize {
        self.fields.len()
    }

    /// Return the index of the first `(name, value)` pair equal to `pair`,
    /// searching `[start:stop]` as `tuple.index()` does.
    ///
    /// Raises:
    ///     ValueError: If no pair is equal to `pair`.
    #[pyo3(signature = (pair, *bounds))]
    fn index(
        &self,
        py: Python<'_>,
        pair: &Bound<'_, PyAny>,
        bounds: &Bound<'_, PyTuple>,
    ) -> PyResult<Py<PyAny>> {
        let args = PyTuple::new(py, [pair])?
            .as_sequence()
            .concat(bounds.as_sequence())?;
        self.items(py)?
            .as_any()
            .getattr("index")?
            .call1(args.to_tuple()?)
            .map(Bound::unbind)
    }

    /// Return how many `(name, value)` pairs are equal to `pair`.
    fn count(&self, py: Python<'_>, pair: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        self.items(py)?
            .call_method1("count", (pair,))
            .map(Bound::unbind)
    }

    fn __getitem__<'py>(
        &self,
        py: Python<'py>,
        index: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.items(py)?.as_any().get_item(index)
    }

    fn __iter__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyIterator>> {
        self.items(py)?.try_iter()
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let equal = if let Ok(other) = other.cast::<Self>() {
            self.fields == other.get().fields
        } else if other.is_instance_of::<PyTuple>() {
            self.items(py)?.eq(other)?
        } else {
            return Ok(py.NotImplemented());
        };
        Ok(PyBool::new(py, equal).to_owned().into_any().unbind())
    }

    // Hashes like the equal tuple of pairs.
    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        self.items(py)?.hash()
    }

    // Supports `copy` and `pickle` as the tuple of pairs did.
    fn __reduce__<'py>(
        slf: &Bound<'py, Self>,
        py: Python<'py>,
    ) -> PyResult<(Bound<'py, PyAny>, (Bound<'py, PyTuple>,))> {
        Ok((slf.get_type().into_any(), (slf.get().items(py)?.clone(),)))
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let pairs = PyList::new(py, self.items(py)?)?;
        Ok(format!("Headers({})", pairs.repr()?))
    }
}

pub(super) fn headers_eq(left: &Py<PyHeaders>, right: &Py<PyHeaders>) -> bool {
    left.get().fields == right.get().fields
}

pub(super) fn py_headers<'py>(
    py: Python<'py>,
    headers: &core::Headers,
) -> PyResult<Bound<'py, PyTuple>> {
    PyTuple::new(
        py,
        headers
            .iter()
            .map(|(name, value)| (PyBytes::new(py, name), PyBytes::new(py, value))),
    )
}

pub(super) fn extract_headers(value: Option<&Bound<'_, PyAny>>) -> PyResult<core::Headers> {
    let Some(value) = value else {
        return Ok(core::Headers::new());
    };
    if let Ok(headers) = value.cast::<PyHeaders>() {
        return Ok(headers.get().fields.clone());
    }
    // Sized for typical fields so a request's headers rarely regrow.
    let fields = value.len().unwrap_or(0);
    let mut headers = core::Headers::with_capacity(fields, fields * 64);
    // A plain loop: `collect()` would ask the Python iterator for a length
    // hint, which costs a Python-level call per header section.
    for item in value.try_iter()? {
        let item = item?;
        let pair = item
            .cast::<PyTuple>()
            .map_err(|_| PyTypeError::new_err("each header must be a 2-tuple"))?;
        if pair.len() != 2 {
            return Err(PyTypeError::new_err(
                "each header must contain name and value",
            ));
        }
        with_text_or_buffer(&*pair.get_borrowed_item(0)?, |name| {
            with_text_or_buffer(&*pair.get_borrowed_item(1)?, |value| {
                headers.push(name, value)
            })
        })??;
    }
    Ok(headers)
}

/// Runs `consume` over a field name to look up. Any `str` is accepted: unlike
/// a name being sent, a non-ASCII one simply matches no valid field.
pub(super) fn with_field_name<R>(
    name: &Bound<'_, PyAny>,
    consume: impl FnOnce(&[u8]) -> R,
) -> PyResult<R> {
    match name.cast::<PyString>() {
        // Lossy so that surrogates, which no valid name contains, also miss.
        Ok(text) => Ok(consume(text.to_string_lossy().as_bytes())),
        Err(_) => with_buffer(name, consume),
    }
}
