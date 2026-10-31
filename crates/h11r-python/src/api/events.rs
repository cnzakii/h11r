//! Event classes and their conversion from core events.

use super::*;
use h11r as core;
use h11r::Method;
use pyo3::exceptions::PyValueError;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyAny, PyBool, PyBytes};

/// A request head.
///
/// `Connection.next_event()` returns parsed requests. Construct one directly,
/// for example to compare it with a received event, as
/// `Request(method, target, headers, *, http_version=None)`.
///
/// Attributes:
///     method (bytes): Case-sensitive method bytes.
///     target (bytes): Request-target bytes from the HTTP request line. See RFC 9112
///         Section 3.2.
///     headers (Headers): Ordered header fields.
///     http_version (bytes): Peer HTTP version bytes.
#[pyclass(
    name = "Request",
    module = "h11r",
    frozen,
    eq,
    get_all,
    skip_from_py_object
)]
#[derive(Debug)]
pub(super) struct PyRequest {
    /// The case-sensitive request method as bytes.
    method: Py<PyBytes>,
    /// The request target as bytes.
    target: Py<PyBytes>,
    /// Ordered header fields.
    headers: Py<PyHeaders>,
    /// The peer HTTP version as bytes.
    http_version: Py<PyBytes>,
}

impl PyRequest {
    fn from_core(py: Python<'_>, value: core::Request) -> PyResult<Self> {
        Ok(Self {
            method: py_method(py, &value.method),
            target: PyBytes::new(py, &value.target).unbind(),
            headers: PyHeaders::new_py(py, value.headers)?,
            http_version: py_version(py, value.http_version).unbind(),
        })
    }
}

impl PartialEq for PyRequest {
    fn eq(&self, other: &Self) -> bool {
        bytes_eq(&self.method, &other.method)
            && bytes_eq(&self.target, &other.target)
            && headers_eq(&self.headers, &other.headers)
            && bytes_eq(&self.http_version, &other.http_version)
    }
}

impl Eq for PyRequest {}

#[pymethods]
impl PyRequest {
    #[new]
    #[pyo3(signature = (method, target, headers, *, http_version = None))]
    fn new(
        py: Python<'_>,
        method: &Bound<'_, PyAny>,
        target: &Bound<'_, PyAny>,
        headers: &Bound<'_, PyAny>,
        http_version: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Self::from_core(
            py,
            core::Request {
                method: extract_method(method)?,
                target: text_or_buffer(target)?,
                headers: extract_headers(Some(headers))?,
                http_version: extract_version(http_version)?,
            },
        )
    }

    #[classattr]
    fn __match_args__() -> (&'static str, &'static str, &'static str) {
        ("method", "target", "headers")
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        repr_fields(
            "Request",
            &[
                ("method", self.method.bind(py).as_any()),
                ("target", self.target.bind(py).as_any()),
                ("headers", self.headers.bind(py).as_any()),
                ("http_version", self.http_version.bind(py).as_any()),
            ],
        )
    }
}

/// The fields shared by informational and final responses.
#[pyclass(
    name = "_ResponseBase",
    module = "h11r",
    subclass,
    frozen,
    eq,
    get_all,
    skip_from_py_object
)]
#[derive(Debug)]
pub(super) struct PyResponseBase {
    /// The status code.
    status_code: u16,
    /// The reason phrase as bytes.
    reason: Py<PyBytes>,
    /// Ordered header fields.
    headers: Py<PyHeaders>,
    /// The peer HTTP version as bytes.
    http_version: Py<PyBytes>,
}

impl PyResponseBase {
    fn from_core(py: Python<'_>, value: core::Response) -> PyResult<Self> {
        Ok(Self {
            status_code: value.status.as_u16(),
            reason: PyBytes::new(py, &value.reason).unbind(),
            headers: PyHeaders::new_py(py, value.headers)?,
            http_version: py_version(py, value.http_version).unbind(),
        })
    }

    /// Builds the fields from constructor arguments, requiring a 1xx status
    /// when `informational` and a final status otherwise.
    fn new(
        py: Python<'_>,
        informational: bool,
        status_code: &Bound<'_, PyAny>,
        headers: Option<&Bound<'_, PyAny>>,
        reason: Option<&Bound<'_, PyAny>>,
        http_version: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let status = status(status_code)?;
        if status.is_informational() != informational {
            return Err(PyValueError::new_err(if informational {
                "InformationalResponse requires a 1xx status"
            } else {
                "Response requires a final, non-1xx status"
            }));
        }
        Self::from_core(
            py,
            core::Response {
                status,
                reason: optional_bytes(reason)?,
                headers: extract_headers(headers)?,
                http_version: extract_version(http_version)?,
            },
        )
    }
}

impl PartialEq for PyResponseBase {
    fn eq(&self, other: &Self) -> bool {
        self.status_code == other.status_code
            && bytes_eq(&self.reason, &other.reason)
            && headers_eq(&self.headers, &other.headers)
            && bytes_eq(&self.http_version, &other.http_version)
    }
}

impl Eq for PyResponseBase {}

#[pymethods]
impl PyResponseBase {
    #[classattr]
    fn __match_args__() -> (&'static str, &'static str) {
        ("status_code", "headers")
    }

    fn __repr__(slf: &Bound<'_, Self>) -> PyResult<String> {
        let py = slf.py();
        let fields = slf.get();
        repr_fields(
            slf.get_type().name()?.to_str()?,
            &[
                (
                    "status_code",
                    fields.status_code.into_pyobject(py)?.as_any(),
                ),
                ("reason", fields.reason.bind(py).as_any()),
                ("headers", fields.headers.bind(py).as_any()),
                ("http_version", fields.http_version.bind(py).as_any()),
            ],
        )
    }
}

/// A non-final `1xx` response head.
///
/// Construct one as
/// `InformationalResponse(status_code, headers=None, *, reason=None, http_version=None)`.
///
/// Attributes:
///     status_code (int): Informational status code.
///     reason (bytes): Reason-phrase bytes.
///     headers (Headers): Ordered header fields.
///     http_version (bytes): Peer HTTP version bytes.
#[pyclass(
    name = "InformationalResponse",
    module = "h11r",
    extends = PyResponseBase,
    frozen,
    skip_from_py_object
)]
#[derive(Debug)]
pub(super) struct PyInformationalResponse;

#[pymethods]
impl PyInformationalResponse {
    #[new]
    #[pyo3(signature = (status_code, headers = None, *, reason = None, http_version = None))]
    fn new(
        py: Python<'_>,
        status_code: &Bound<'_, PyAny>,
        headers: Option<&Bound<'_, PyAny>>,
        reason: Option<&Bound<'_, PyAny>>,
        http_version: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let base = PyResponseBase::new(py, true, status_code, headers, reason, http_version)?;
        Ok(PyClassInitializer::from(base).add_subclass(Self))
    }
}

/// A final response head.
///
/// Construct one as
/// `Response(status_code, headers=None, *, reason=None, http_version=None)`.
///
/// Attributes:
///     status_code (int): Final status code.
///     reason (bytes): Reason-phrase bytes.
///     headers (Headers): Ordered header fields.
///     http_version (bytes): Peer HTTP version bytes.
#[pyclass(
    name = "Response",
    module = "h11r",
    extends = PyResponseBase,
    frozen,
    skip_from_py_object
)]
#[derive(Debug)]
pub(super) struct PyResponse;

#[pymethods]
impl PyResponse {
    #[new]
    #[pyo3(signature = (status_code, headers = None, *, reason = None, http_version = None))]
    fn new(
        py: Python<'_>,
        status_code: &Bound<'_, PyAny>,
        headers: Option<&Bound<'_, PyAny>>,
        reason: Option<&Bound<'_, PyAny>>,
        http_version: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let base = PyResponseBase::new(py, false, status_code, headers, reason, http_version)?;
        Ok(PyClassInitializer::from(base).add_subclass(Self))
    }
}

/// A decoded piece of message body.
///
/// `chunk_start` and `chunk_end` preserve HTTP chunk boundaries when the peer
/// used chunked transfer coding. Construct one as
/// `Data(data, *, chunk_start=False, chunk_end=False)`.
///
/// Attributes:
///     data (bytes): Decoded body bytes.
///     chunk_start (bool): Whether this data begins an HTTP chunk.
///     chunk_end (bool): Whether this data ends an HTTP chunk.
#[pyclass(
    name = "Data",
    module = "h11r",
    frozen,
    eq,
    get_all,
    skip_from_py_object
)]
#[derive(Debug)]
pub(super) struct PyData {
    /// The decoded body bytes.
    data: Py<PyBytes>,
    /// Whether this data begins an HTTP chunk.
    chunk_start: bool,
    /// Whether this data ends an HTTP chunk.
    chunk_end: bool,
}

impl PartialEq for PyData {
    fn eq(&self, other: &Self) -> bool {
        bytes_eq(&self.data, &other.data)
            && self.chunk_start == other.chunk_start
            && self.chunk_end == other.chunk_end
    }
}

impl Eq for PyData {}

#[pymethods]
impl PyData {
    #[new]
    #[pyo3(signature = (data, *, chunk_start = false, chunk_end = false))]
    fn new(
        py: Python<'_>,
        data: &Bound<'_, PyAny>,
        chunk_start: bool,
        chunk_end: bool,
    ) -> PyResult<Self> {
        let data = with_buffer(data, |data| PyBytes::new(py, data))?;
        Ok(Self {
            data: data.unbind(),
            chunk_start,
            chunk_end,
        })
    }

    #[classattr]
    fn __match_args__() -> (&'static str,) {
        ("data",)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        repr_fields(
            "Data",
            &[
                ("data", self.data.bind(py).as_any()),
                ("chunk_start", PyBool::new(py, self.chunk_start).as_any()),
                ("chunk_end", PyBool::new(py, self.chunk_end).as_any()),
            ],
        )
    }
}

/// The end of one HTTP message, optionally carrying trailer fields.
///
/// `EndOfMessage()` creates an event without trailers. Pass an iterable of
/// `(name, value)` pairs to construct one with trailers.
///
/// Attributes:
///     trailers (Headers): Ordered trailer fields.
#[pyclass(
    name = "EndOfMessage",
    module = "h11r",
    frozen,
    eq,
    get_all,
    skip_from_py_object
)]
#[derive(Debug)]
pub(super) struct PyEndOfMessage {
    /// Ordered trailer fields.
    trailers: Py<PyHeaders>,
}

impl PartialEq for PyEndOfMessage {
    fn eq(&self, other: &Self) -> bool {
        headers_eq(&self.trailers, &other.trailers)
    }
}

impl Eq for PyEndOfMessage {}

#[pymethods]
impl PyEndOfMessage {
    #[new]
    #[pyo3(signature = (trailers = None))]
    fn new(py: Python<'_>, trailers: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        Ok(Self {
            trailers: PyHeaders::new_py(py, extract_headers(trailers)?)?,
        })
    }

    #[classattr]
    fn __match_args__() -> (&'static str,) {
        ("trailers",)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        repr_fields(
            "EndOfMessage",
            &[("trailers", self.trailers.bind(py).as_any())],
        )
    }
}

/// Clean transport EOF from the peer.
#[pyclass(
    name = "ConnectionClosed",
    module = "h11r",
    frozen,
    eq,
    skip_from_py_object
)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct PyConnectionClosed;

#[pymethods]
impl PyConnectionClosed {
    #[new]
    fn new() -> Self {
        Self
    }

    fn __repr__(&self) -> &'static str {
        "ConnectionClosed()"
    }
}

pub(super) fn event_to_py(py: Python<'_>, event: core::Event<'_>) -> PyResult<Py<PyAny>> {
    Ok(match event {
        core::Event::Request(value) => Py::new(py, PyRequest::from_core(py, value)?)?.into_any(),
        core::Event::InformationalResponse(value) => Py::new(
            py,
            PyClassInitializer::from(PyResponseBase::from_core(py, value)?)
                .add_subclass(PyInformationalResponse),
        )?
        .into_any(),
        core::Event::Response(value) => Py::new(
            py,
            PyClassInitializer::from(PyResponseBase::from_core(py, value)?)
                .add_subclass(PyResponse),
        )?
        .into_any(),
        core::Event::Data(value) => Py::new(
            py,
            PyData {
                data: PyBytes::new(py, value.data).unbind(),
                chunk_start: value.chunk_start,
                chunk_end: value.chunk_end,
            },
        )?
        .into_any(),
        core::Event::EndOfMessage(value) => Py::new(
            py,
            PyEndOfMessage {
                trailers: PyHeaders::new_py(py, value.trailers)?,
            },
        )?
        .into_any(),
        core::Event::ConnectionClosed => Py::new(py, PyConnectionClosed)?.into_any(),
    })
}

/// Formats `Class(name=value, ...)` from each value's Python `repr()`.
pub(super) fn repr_fields(class: &str, fields: &[(&str, &Bound<'_, PyAny>)]) -> PyResult<String> {
    let fields = fields
        .iter()
        .map(|(name, value)| Ok(format!("{name}={}", value.repr()?)))
        .collect::<PyResult<Vec<_>>>()?;
    Ok(format!("{class}({})", fields.join(", ")))
}

pub(super) fn bytes_eq(left: &Py<PyBytes>, right: &Py<PyBytes>) -> bool {
    Python::attach(|py| left.bind(py).as_bytes() == right.bind(py).as_bytes())
}

pub(super) fn extract_method(value: &Bound<'_, PyAny>) -> PyResult<Method> {
    with_text_or_buffer(value, Method::from_bytes)?
        .map_err(|error| PyValueError::new_err(error.to_string()))
}

pub(super) fn optional_bytes(value: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<u8>> {
    value.map_or_else(|| Ok(Vec::new()), text_or_buffer)
}

pub(super) fn extract_version(value: Option<&Bound<'_, PyAny>>) -> PyResult<core::Version> {
    match value.map(text_or_buffer).transpose()?.as_deref() {
        None | Some(b"1.1") => Ok(core::Version::Http11),
        Some(b"1.0") => Ok(core::Version::Http10),
        _ => Err(PyValueError::new_err(
            "http_version must be b'1.0' or b'1.1'",
        )),
    }
}

/// Returns a shared `bytes` object for a registered method, or a new one.
pub(super) fn py_method(py: Python<'_>, method: &Method) -> Py<PyBytes> {
    static REGISTERED: [PyOnceLock<Py<PyBytes>>; 9] = [const { PyOnceLock::new() }; 9];
    let index = match method.as_bytes() {
        b"GET" => 0,
        b"HEAD" => 1,
        b"POST" => 2,
        b"PUT" => 3,
        b"DELETE" => 4,
        b"CONNECT" => 5,
        b"OPTIONS" => 6,
        b"TRACE" => 7,
        b"PATCH" => 8,
        _ => return PyBytes::new(py, method.as_bytes()).unbind(),
    };
    REGISTERED[index]
        .get_or_init(py, || PyBytes::new(py, method.as_bytes()).unbind())
        .clone_ref(py)
}

/// Returns a shared `bytes` object for each supported version.
pub(super) fn py_version(py: Python<'_>, value: core::Version) -> Bound<'_, PyBytes> {
    static HTTP10: PyOnceLock<Py<PyBytes>> = PyOnceLock::new();
    static HTTP11: PyOnceLock<Py<PyBytes>> = PyOnceLock::new();
    let (cell, wire): (&PyOnceLock<Py<PyBytes>>, &[u8]) = match value {
        core::Version::Http10 => (&HTTP10, b"1.0"),
        core::Version::Http11 => (&HTTP11, b"1.1"),
    };
    cell.get_or_init(py, || PyBytes::new(py, wire).unbind())
        .bind(py)
        .clone()
}
