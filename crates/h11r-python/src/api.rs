//! Python bindings for the HTTP/1.1 library.

use h11r as core;
use h11r::{Method, StatusCode};
use pyo3::PyTypeInfo;
use pyo3::buffer::{PyUntypedBuffer, ReadOnlyCell};
use pyo3::exceptions::{PyAttributeError, PyException, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{
    PyAny, PyBool, PyBytes, PyIterator, PyList, PyMemoryView, PyModule, PyString, PyTuple,
};

pyo3::create_exception!(
    h11r,
    ProtocolError,
    PyException,
    "Base HTTP/1 protocol error."
);
pyo3::create_exception!(
    h11r,
    LocalProtocolError,
    ProtocolError,
    "The caller attempted an invalid local HTTP operation."
);
pyo3::create_exception!(
    h11r,
    RemoteProtocolError,
    ProtocolError,
    "The peer sent invalid HTTP or exceeded an inbound limit.\n\n\
     Attributes:\n    \
         suggested_status_code (int | None): A suitable HTTP response status, \
         when one is known."
);

/// An HTTP actor role.
///
/// Attributes:
///     CLIENT (Role): The actor that sends requests and receives responses.
///     SERVER (Role): The actor that receives requests and sends responses.
#[pyclass(
    name = "Role",
    module = "h11r",
    eq,
    eq_int,
    rename_all = "SCREAMING_SNAKE_CASE",
    from_py_object
)]
#[derive(Clone, Debug, Eq, PartialEq)]
enum PyRole {
    Client,
    Server,
}

impl From<PyRole> for core::Role {
    fn from(value: PyRole) -> Self {
        match value {
            PyRole::Client => Self::Client,
            PyRole::Server => Self::Server,
        }
    }
}

/// An observable HTTP/1 actor lifecycle state.
///
/// Attributes:
///     IDLE (State): No message has started.
///     SEND_RESPONSE (State): The server can send informational or final responses.
///     SEND_BODY (State): The actor can send or receive body data and message end.
///     DONE (State): The actor completed its message for the current cycle.
///     MIGHT_SWITCH_PROTOCOL (State): The request ended while a switch decision is pending.
///     SWITCHED_PROTOCOL (State): HTTP processing ended after a successful switch.
///     MUST_CLOSE (State): The actor completed its message but reuse is forbidden.
///     CLOSED (State): The actor closed its transport side.
///     ERROR (State): A protocol error permanently poisoned the actor.
#[pyclass(
    name = "State",
    module = "h11r",
    eq,
    eq_int,
    rename_all = "SCREAMING_SNAKE_CASE",
    skip_from_py_object
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PyState {
    Idle,
    SendResponse,
    SendBody,
    Done,
    MightSwitchProtocol,
    SwitchedProtocol,
    MustClose,
    Closed,
    Error,
}

impl From<core::State> for PyState {
    fn from(value: core::State) -> Self {
        match value {
            core::State::Idle => Self::Idle,
            core::State::SendResponse => Self::SendResponse,
            core::State::SendBody => Self::SendBody,
            core::State::Done => Self::Done,
            core::State::MightSwitchProtocol => Self::MightSwitchProtocol,
            core::State::SwitchedProtocol => Self::SwitchedProtocol,
            core::State::MustClose => Self::MustClose,
            core::State::Closed => Self::Closed,
            core::State::Error => Self::Error,
        }
    }
}

/// A non-event result returned by `Connection.next_event()`.
///
/// Attributes:
///     NEED_DATA (ReceiveStatus): Supply more peer bytes before polling again.
///     PAUSED (ReceiveStatus): Complete the active cycle or hand off a switched protocol before
///         polling HTTP again.
#[pyclass(
    name = "ReceiveStatus",
    module = "h11r",
    eq,
    eq_int,
    rename_all = "SCREAMING_SNAKE_CASE",
    skip_from_py_object
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PyReceiveStatus {
    NeedData,
    Paused,
}

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
struct PyHeaders {
    fields: core::Headers,
    /// The `(name, value)` pairs, built on first use.
    items: PyOnceLock<Py<PyTuple>>,
}

impl PyHeaders {
    fn new_py(py: Python<'_>, fields: core::Headers) -> PyResult<Py<Self>> {
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
        with_text_or_buffer(name, |name| match self.fields.get_all(name).next() {
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
        with_text_or_buffer(name, |name| {
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
struct PyRequest {
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
                target: text_or_buffer(py, target)?,
                headers: extract_headers(Some(headers))?,
                http_version: extract_version(py, http_version)?,
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

#[derive(Debug)]
struct PyResponseValue {
    status_code: u16,
    reason: Py<PyBytes>,
    headers: Py<PyHeaders>,
    http_version: Py<PyBytes>,
}

impl PyResponseValue {
    fn from_core(py: Python<'_>, value: core::Response) -> PyResult<Self> {
        Ok(Self {
            status_code: value.status.as_u16(),
            reason: PyBytes::new(py, &value.reason).unbind(),
            headers: PyHeaders::new_py(py, value.headers)?,
            http_version: py_version(py, value.http_version).unbind(),
        })
    }
}

impl PyResponseValue {
    fn new(
        py: Python<'_>,
        status_code: u16,
        headers: Option<&Bound<'_, PyAny>>,
        reason: Option<&Bound<'_, PyAny>>,
        http_version: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        Self::from_core(
            py,
            core::Response {
                status: status(status_code)?,
                reason: optional_bytes(py, reason)?,
                headers: extract_headers(headers)?,
                http_version: extract_version(py, http_version)?,
            },
        )
    }

    fn repr(&self, py: Python<'_>, class: &str) -> PyResult<String> {
        repr_fields(
            class,
            &[
                ("status_code", self.status_code.into_pyobject(py)?.as_any()),
                ("reason", self.reason.bind(py).as_any()),
                ("headers", self.headers.bind(py).as_any()),
                ("http_version", self.http_version.bind(py).as_any()),
            ],
        )
    }
}

impl PartialEq for PyResponseValue {
    fn eq(&self, other: &Self) -> bool {
        self.status_code == other.status_code
            && bytes_eq(&self.reason, &other.reason)
            && headers_eq(&self.headers, &other.headers)
            && bytes_eq(&self.http_version, &other.http_version)
    }
}

impl Eq for PyResponseValue {}

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
    frozen,
    eq,
    skip_from_py_object
)]
#[derive(Debug)]
struct PyInformationalResponse(PyResponseValue);

impl PartialEq for PyInformationalResponse {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for PyInformationalResponse {}

#[pymethods]
impl PyInformationalResponse {
    #[new]
    #[pyo3(signature = (status_code, headers = None, *, reason = None, http_version = None))]
    fn new(
        py: Python<'_>,
        status_code: u16,
        headers: Option<&Bound<'_, PyAny>>,
        reason: Option<&Bound<'_, PyAny>>,
        http_version: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let value = PyResponseValue::new(py, status_code, headers, reason, http_version)?;
        if value.status_code >= 200 {
            return Err(PyValueError::new_err(
                "InformationalResponse requires a 1xx status",
            ));
        }
        Ok(Self(value))
    }

    #[classattr]
    fn __match_args__() -> (&'static str, &'static str) {
        ("status_code", "headers")
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        self.0.repr(py, "InformationalResponse")
    }

    /// The informational status code.
    #[getter]
    fn status_code(&self) -> u16 {
        self.0.status_code
    }
    /// The reason phrase as bytes.
    #[getter]
    fn reason(&self, py: Python<'_>) -> Py<PyBytes> {
        self.0.reason.clone_ref(py)
    }
    /// Ordered header fields.
    #[getter]
    fn headers(&self, py: Python<'_>) -> Py<PyHeaders> {
        self.0.headers.clone_ref(py)
    }
    /// The peer HTTP version as bytes.
    #[getter]
    fn http_version(&self, py: Python<'_>) -> Py<PyBytes> {
        self.0.http_version.clone_ref(py)
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
#[pyclass(name = "Response", module = "h11r", frozen, eq, skip_from_py_object)]
#[derive(Debug)]
struct PyResponse(PyResponseValue);

impl PartialEq for PyResponse {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for PyResponse {}

#[pymethods]
impl PyResponse {
    #[new]
    #[pyo3(signature = (status_code, headers = None, *, reason = None, http_version = None))]
    fn new(
        py: Python<'_>,
        status_code: u16,
        headers: Option<&Bound<'_, PyAny>>,
        reason: Option<&Bound<'_, PyAny>>,
        http_version: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let value = PyResponseValue::new(py, status_code, headers, reason, http_version)?;
        if value.status_code < 200 {
            return Err(PyValueError::new_err(
                "Response requires a final, non-1xx status",
            ));
        }
        Ok(Self(value))
    }

    #[classattr]
    fn __match_args__() -> (&'static str, &'static str) {
        ("status_code", "headers")
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        self.0.repr(py, "Response")
    }

    /// The final status code.
    #[getter]
    fn status_code(&self) -> u16 {
        self.0.status_code
    }
    /// The reason phrase as bytes.
    #[getter]
    fn reason(&self, py: Python<'_>) -> Py<PyBytes> {
        self.0.reason.clone_ref(py)
    }
    /// Ordered header fields.
    #[getter]
    fn headers(&self, py: Python<'_>) -> Py<PyHeaders> {
        self.0.headers.clone_ref(py)
    }
    /// The peer HTTP version as bytes.
    #[getter]
    fn http_version(&self, py: Python<'_>) -> Py<PyBytes> {
        self.0.http_version.clone_ref(py)
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
struct PyData {
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
        let data = with_buffer_bytes(data, |data| match data {
            BufferBytes::Borrowed(bytes) => PyBytes::new(py, bytes),
            BufferBytes::Cells(cells) => {
                PyBytes::new(py, &cells.iter().map(ReadOnlyCell::get).collect::<Vec<_>>())
            }
        })?;
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
struct PyEndOfMessage {
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
struct PyConnectionClosed;

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

/// A Sans-I/O HTTP/1 connection.
///
/// Supply transport bytes with `receive_data()`, poll semantic events with
/// `next_event()`, and write bytes returned by the send methods. Client and
/// server state are tracked together regardless of the local role.
///
/// Construct a connection as:
///
/// ```text
/// Connection(
///     role,
///     *,
///     max_head_bytes=65536,
///     max_header_count=100,
/// )
/// ```
///
/// `role` selects the local client or server endpoint. `max_head_bytes`
/// defaults to 65,536 and limits each inbound head or trailer section, each
/// chunk-size line, and the total chunk-extension bytes of one message.
/// `max_header_count` defaults to 100 and limits each inbound head or trailer
/// section's field count.
///
/// Attributes:
///     local_state (State): The local protocol actor's lifecycle state.
///     peer_state (State): The peer protocol actor's lifecycle state.
///     peer_http_version (bytes | None): The most recently parsed peer HTTP
///         version, if any.
///     keep_alive (bool): Whether HTTP may continue on this connection after the
///         current exchange. It becomes false once either actor requires
///         closure, closes, fails, or switches protocols.
///     client_is_waiting_for_100_continue (bool): Whether the client is waiting
///         for `100 Continue`.
///     trailing_data (tuple[bytes, bool]): Bytes retained beyond the HTTP
///         boundary and whether transport EOF was received.
///     buffered_nbytes (int): How many received bytes are held but not yet
///         parsed.
///
/// Raises:
///     ValueError: If any inbound limit is zero.
#[pyclass(name = "Connection", module = "h11r")]
#[derive(Debug)]
struct PyConnection(core::Connection);

#[pymethods]
impl PyConnection {
    #[new]
    #[pyo3(signature = (role, *, max_head_bytes = 65536, max_header_count = 100))]
    fn new(role: PyRole, max_head_bytes: usize, max_header_count: usize) -> PyResult<Self> {
        let limits = core::Limits::new(max_head_bytes, max_header_count)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Ok(Self(core::Connection::new(role.into(), limits)))
    }

    /// Append bytes received from the peer; empty bytes mark EOF.
    ///
    /// Args:
    ///     data (bytes | bytearray | memoryview): Bytes read from the transport.
    ///
    /// Raises:
    ///     TypeError: If `data` does not implement the buffer protocol.
    ///     LocalProtocolError: If non-empty data follows EOF.
    fn receive_data(&mut self, data: &Bound<'_, PyAny>) -> PyResult<()> {
        with_buffer_bytes(data, |data| match data {
            BufferBytes::Borrowed(bytes) => self.0.receive_data(bytes),
            BufferBytes::Cells(cells) => self
                .0
                .receive_data_iter(cells.iter().map(ReadOnlyCell::get)),
        })?
        .map_err(local_error)
    }

    /// Return the next peer event or a receive status.
    ///
    /// Returns:
    ///     event_or_status (Request | InformationalResponse | Response | Data | EndOfMessage | ConnectionClosed | ReceiveStatus):
    ///         A protocol event, `ReceiveStatus.NEED_DATA` when more transport
    ///         bytes are required, or `ReceiveStatus.PAUSED` at a cycle or
    ///         switch boundary.
    ///
    /// Raises:
    ///     RemoteProtocolError: If peer bytes violate HTTP syntax, framing,
    ///         configured limits, or protocol state.
    fn next_event(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self
            .0
            .next_event()
            .map_err(|error| remote_error(py, error))?
        {
            core::NextEvent::NeedData => {
                static NEED_DATA: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
                cached_member::<PyReceiveStatus>(py, &NEED_DATA, "NEED_DATA")
            }
            core::NextEvent::Paused => {
                static PAUSED: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
                cached_member::<PyReceiveStatus>(py, &PAUSED, "PAUSED")
            }
            core::NextEvent::Event(event) => event_to_py(py, event),
        }
    }

    /// Serialize a request head.
    ///
    /// Args:
    ///     method (str | bytes | bytearray | memoryview): Case-sensitive ASCII
    ///         HTTP method.
    ///     target (str | bytes | bytearray | memoryview): ASCII request target.
    ///         See RFC 9112 Section 3.2.
    ///     headers (Iterable): `(name, value)` pairs. Each item accepts ASCII
    ///         `str`, `bytes`, `bytearray`, or `memoryview`.
    ///     body (bytes | bytearray | memoryview | None): The complete request
    ///         content. When given, the returned bytes also contain the body
    ///         and the end of the message, and `Content-Length` is added if no
    ///         framing field is present and the body is non-empty or the
    ///         method is `POST`, `PUT`, or `PATCH`. Omit it to stream
    ///         the body with `send_data()` and `end_of_message()`.
    ///     http_version (str | bytes | bytearray | memoryview | None): `b"1.1"`
    ///         (default) or `b"1.0"`.
    ///
    /// Returns:
    ///     wire_bytes (bytes): Bytes for the caller to write to the transport.
    ///
    /// Raises:
    ///     TypeError: If a header is not a two-item tuple or an input is not
    ///         text or a buffer.
    ///     ValueError: If text is non-ASCII, the method is invalid, or the HTTP
    ///         version is unsupported.
    ///     LocalProtocolError: If the head, body framing, or current state is
    ///         invalid. The connection is unchanged after this error.
    #[pyo3(signature = (method, target, headers, *, body = None, http_version = None))]
    fn send_request(
        &mut self,
        py: Python<'_>,
        method: &Bound<'_, PyAny>,
        target: &Bound<'_, PyAny>,
        headers: &Bound<'_, PyAny>,
        body: Option<&Bound<'_, PyAny>>,
        http_version: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyBytes>> {
        let request = core::Request {
            method: extract_method(method)?,
            target: text_or_buffer(py, target)?,
            headers: extract_headers(Some(headers))?,
            http_version: extract_version(py, http_version)?,
        };
        match body {
            None => self.0.send_request(&request),
            Some(body) => with_buffer(body, |body| self.0.send_request_with_body(request, body))?,
        }
        .map(|bytes| PyBytes::new(py, &bytes).unbind())
        .map_err(local_error)
    }

    /// Serialize an informational response head.
    ///
    /// Args:
    ///     status_code (int): A status in the range 100 through 199.
    ///     headers (Iterable | None): Optional `(name, value)` pairs. Each item
    ///         accepts ASCII `str`, `bytes`, `bytearray`, or `memoryview`.
    ///     reason (str | bytes | bytearray | memoryview | None): Optional ASCII
    ///         reason phrase.
    ///     http_version (str | bytes | bytearray | memoryview | None): `b"1.1"`
    ///         (default) or `b"1.0"`.
    ///
    /// Returns:
    ///     wire_bytes (bytes): Bytes for the caller to write to the transport.
    ///
    /// Raises:
    ///     TypeError: If a header is not a two-item tuple or an input is not
    ///         text or a buffer.
    ///     ValueError: If the status, text, or HTTP version is invalid.
    ///     LocalProtocolError: If the response or current state is invalid.
    #[pyo3(signature = (status_code, headers = None, *, reason = None, http_version = None))]
    fn send_informational_response(
        &mut self,
        py: Python<'_>,
        status_code: u16,
        headers: Option<&Bound<'_, PyAny>>,
        reason: Option<&Bound<'_, PyAny>>,
        http_version: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyBytes>> {
        let response = core::InformationalResponse {
            status: status(status_code)?,
            reason: optional_bytes(py, reason)?,
            headers: extract_headers(headers)?,
            http_version: extract_version(py, http_version)?,
        };
        self.0
            .send_informational_response(&response)
            .map(|bytes| PyBytes::new(py, &bytes).unbind())
            .map_err(local_error)
    }

    /// Serialize a final response head, or a complete response with `body`.
    ///
    /// Args:
    ///     status_code (int): A status in the range 200 through 599.
    ///     headers (Iterable | None): Optional `(name, value)` pairs. Each item
    ///         accepts ASCII `str`, `bytes`, `bytearray`, or `memoryview`.
    ///     body (bytes | bytearray | memoryview | None): The complete response
    ///         content. When given, the returned bytes also contain the body
    ///         and the end of the message, and `Content-Length` is added if no
    ///         framing field is present and the response may carry content.
    ///         Omit it to stream the body with `send_data()` and
    ///         `end_of_message()`.
    ///     reason (str | bytes | bytearray | memoryview | None): Optional ASCII
    ///         reason phrase.
    ///     http_version (str | bytes | bytearray | memoryview | None): `b"1.1"`
    ///         (default) or `b"1.0"`.
    ///
    /// Returns:
    ///     wire_bytes (bytes): Bytes for the caller to write to the transport.
    ///
    /// Raises:
    ///     TypeError: If a header is not a two-item tuple or an input is not
    ///         text or a buffer.
    ///     ValueError: If the status, text, or HTTP version is invalid.
    ///     LocalProtocolError: If the response, body framing, or current state
    ///         is invalid. The connection is unchanged after this error.
    #[pyo3(signature = (status_code, headers = None, *, body = None, reason = None, http_version = None))]
    fn send_response(
        &mut self,
        py: Python<'_>,
        status_code: u16,
        headers: Option<&Bound<'_, PyAny>>,
        body: Option<&Bound<'_, PyAny>>,
        reason: Option<&Bound<'_, PyAny>>,
        http_version: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyBytes>> {
        let response = core::Response {
            status: status(status_code)?,
            reason: optional_bytes(py, reason)?,
            headers: extract_headers(headers)?,
            http_version: extract_version(py, http_version)?,
        };
        match body {
            None => self.0.send_response(&response),
            Some(body) => with_buffer(body, |body| self.0.send_response_with_body(response, body))?,
        }
        .map(|bytes| PyBytes::new(py, &bytes).unbind())
        .map_err(local_error)
    }

    /// Serialize body data into one bytes object.
    ///
    /// Args:
    ///     data (bytes | bytearray | memoryview): Body bytes.
    ///
    /// Returns:
    ///     wire_bytes (bytes): Framed bytes for the caller to write to the
    ///         transport.
    ///
    /// Raises:
    ///     TypeError: If `data` does not implement the buffer protocol.
    ///     LocalProtocolError: If body data is forbidden or violates framing.
    fn send_data(&mut self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
        let bytes = with_buffer_bytes(data, |data| match data {
            BufferBytes::Borrowed(bytes) => self.0.send_data(bytes),
            BufferBytes::Cells(cells) => {
                let (prefix, suffix) = self.0.send_data_framing(cells.len())?;
                let mut out = prefix;
                out.reserve(cells.len() + suffix.len());
                out.extend(cells.iter().map(ReadOnlyCell::get));
                out.extend_from_slice(&suffix);
                Ok(out)
            }
        })?
        .map_err(local_error)?;
        Ok(PyBytes::new(py, &bytes).unbind())
    }

    /// Return `(prefix, original_object, suffix)` without copying body bytes.
    ///
    /// Contiguous buffers use their byte length. Other objects must expose
    /// `nbytes` as the exact number of body bytes represented by the object.
    ///
    /// Write the prefix, exactly the determined number of body bytes, and the
    /// suffix in order. After a partial body write, resume until the body and
    /// suffix are complete or discard the connection.
    ///
    /// Args:
    ///     data (object): A contiguous buffer or an object with an integer
    ///         `nbytes` property.
    ///
    /// Returns:
    ///     parts (tuple[bytes, object, bytes]): Framing prefix, the identical
    ///         input object, and framing suffix.
    ///
    /// Raises:
    ///     TypeError: If `data` is neither a buffer nor byte-sized, or if
    ///         `nbytes` is not an integer.
    ///     ValueError: If the buffer is not contiguous.
    ///     OverflowError: If `nbytes` is negative or does not fit in the
    ///         platform's address size.
    ///     LocalProtocolError: If body data is forbidden or violates framing.
    fn send_data_parts(
        &mut self,
        py: Python<'_>,
        data: Py<PyAny>,
    ) -> PyResult<(Py<PyBytes>, Py<PyAny>, Py<PyBytes>)> {
        let length = body_nbytes(data.bind(py))?;
        let (prefix, suffix) = self.0.send_data_framing(length).map_err(local_error)?;
        Ok((
            PyBytes::new(py, &prefix).unbind(),
            data,
            PyBytes::new(py, &suffix).unbind(),
        ))
    }

    /// Finish the current message and return its terminating framing.
    ///
    /// Core fields that require header-time processing are rejected in
    /// trailers. Extension trailer fields remain the caller's responsibility.
    ///
    /// Args:
    ///     trailers (Iterable | None): Optional `(name, value)` pairs. Each item
    ///         accepts ASCII `str`, `bytes`, `bytearray`, or `memoryview`.
    ///
    /// Returns:
    ///     wire_bytes (bytes): Message terminator bytes for the caller to write
    ///         to the transport.
    ///
    /// Raises:
    ///     TypeError: If a trailer is not a two-item tuple or an input is not
    ///         text or a buffer.
    ///     ValueError: If trailer text is non-ASCII.
    ///     LocalProtocolError: If framing is incomplete or forbids trailers.
    #[pyo3(signature = (trailers = None))]
    fn end_of_message(
        &mut self,
        py: Python<'_>,
        trailers: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyBytes>> {
        let trailers = extract_headers(trailers)?;
        self.0
            .end_of_message(&trailers)
            .map(|bytes| PyBytes::new(py, &bytes).unbind())
            .map_err(local_error)
    }

    /// Reset a completed reusable exchange.
    ///
    /// Raises:
    ///     LocalProtocolError: If either actor is incomplete, reuse is disabled,
    ///         or a protocol switch is pending.
    fn start_next_cycle(&mut self) -> PyResult<()> {
        self.0.start_next_cycle().map_err(local_error)
    }

    /// Mark the local protocol actor as closed.
    ///
    /// Raises:
    ///     LocalProtocolError: If closing is invalid in the current local state.
    fn close(&mut self) -> PyResult<()> {
        self.0.close().map_err(local_error)
    }

    /// The local protocol actor's lifecycle state.
    #[getter]
    fn local_state(&self) -> PyState {
        self.0.local_state().into()
    }
    /// The peer protocol actor's lifecycle state.
    #[getter]
    fn peer_state(&self) -> PyState {
        self.0.peer_state().into()
    }
    /// The most recently parsed peer HTTP version, if any.
    #[getter]
    fn peer_http_version<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        self.0
            .peer_http_version()
            .map(|version| py_version(py, version))
    }
    /// Whether HTTP may continue on this connection after the current exchange.
    #[getter]
    fn keep_alive(&self) -> bool {
        self.0.keep_alive()
    }
    /// Whether the client is waiting for `100 Continue`.
    #[getter]
    fn client_is_waiting_for_100_continue(&self) -> bool {
        self.0.client_is_waiting_for_100_continue()
    }
    /// Buffered bytes beyond HTTP plus whether EOF was received.
    #[getter]
    fn trailing_data<'py>(&self, py: Python<'py>) -> (Bound<'py, PyBytes>, bool) {
        let (data, eof) = self.0.trailing_data();
        (PyBytes::new(py, data), eof)
    }
    /// How many received bytes are held but not yet parsed.
    #[getter]
    fn buffered_nbytes(&self) -> usize {
        self.0.trailing_data().0.len()
    }
}

fn event_to_py(py: Python<'_>, event: core::Event<'_>) -> PyResult<Py<PyAny>> {
    Ok(match event {
        core::Event::Request(value) => Py::new(py, PyRequest::from_core(py, value)?)?.into_any(),
        core::Event::InformationalResponse(value) => Py::new(
            py,
            PyInformationalResponse(PyResponseValue::from_core(py, value)?),
        )?
        .into_any(),
        core::Event::Response(value) => {
            Py::new(py, PyResponse(PyResponseValue::from_core(py, value)?))?.into_any()
        }
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
fn repr_fields(class: &str, fields: &[(&str, &Bound<'_, PyAny>)]) -> PyResult<String> {
    let fields = fields
        .iter()
        .map(|(name, value)| Ok(format!("{name}={}", value.repr()?)))
        .collect::<PyResult<Vec<_>>>()?;
    Ok(format!("{class}({})", fields.join(", ")))
}

fn bytes_eq(left: &Py<PyBytes>, right: &Py<PyBytes>) -> bool {
    Python::attach(|py| left.bind(py).as_bytes() == right.bind(py).as_bytes())
}

fn headers_eq(left: &Py<PyHeaders>, right: &Py<PyHeaders>) -> bool {
    left.get().fields == right.get().fields
}

fn py_headers<'py>(py: Python<'py>, headers: &core::Headers) -> PyResult<Bound<'py, PyTuple>> {
    PyTuple::new(
        py,
        headers
            .iter()
            .map(|(name, value)| (PyBytes::new(py, name), PyBytes::new(py, value))),
    )
}

fn extract_headers(value: Option<&Bound<'_, PyAny>>) -> PyResult<core::Headers> {
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

enum BufferBytes<'a> {
    Borrowed(&'a [u8]),
    Cells(&'a [ReadOnlyCell<u8>]),
}

/// Runs `consume` over a safe view of a buffer-protocol object.
///
/// Immutable `bytes` are borrowed directly. Contiguous, `u8`-compatible
/// buffers expose cells whose values must be read individually because another
/// thread may mutate them. Other buffers retain the previous copying behavior.
fn with_buffer_bytes<R>(
    value: &Bound<'_, PyAny>,
    consume: impl FnOnce(BufferBytes<'_>) -> R,
) -> PyResult<R> {
    if let Ok(bytes) = value.cast::<PyBytes>() {
        return Ok(consume(BufferBytes::Borrowed(bytes.as_bytes())));
    }
    let buffer = PyUntypedBuffer::get(value)?;
    if let Ok(buffer) = buffer.as_typed::<u8>()
        && let Some(cells) = buffer.as_slice(value.py())
    {
        return Ok(consume(BufferBytes::Cells(cells)));
    }
    drop(buffer);
    let view = PyMemoryView::from(value)?;
    let bytes = view.call_method0("tobytes")?.cast_into::<PyBytes>()?;
    Ok(consume(BufferBytes::Borrowed(bytes.as_bytes())))
}

fn contiguous_buffer_len(value: &Bound<'_, PyAny>) -> PyResult<usize> {
    if let Ok(bytes) = value.cast::<PyBytes>() {
        return Ok(bytes.as_bytes().len());
    }
    let view = PyMemoryView::from(value)?;
    if !view.getattr("contiguous")?.extract::<bool>()? {
        return Err(PyValueError::new_err(
            "send_data_parts requires a contiguous buffer",
        ));
    }
    view.getattr("nbytes")?.extract()
}

fn body_nbytes(value: &Bound<'_, PyAny>) -> PyResult<usize> {
    match contiguous_buffer_len(value) {
        Ok(length) => Ok(length),
        Err(buffer_error) if buffer_error.is_instance_of::<PyTypeError>(value.py()) => {
            match value.getattr("nbytes") {
                Ok(length) => length.extract(),
                Err(error) if error.is_instance_of::<PyAttributeError>(value.py()) => {
                    Err(buffer_error)
                }
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

fn text_or_buffer(_py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    with_text_or_buffer(value, <[u8]>::to_vec)
}

/// Runs `consume` over the bytes of an ASCII `str` or a buffer-protocol
/// object, borrowing them when Python allows it.
fn with_text_or_buffer<R>(
    value: &Bound<'_, PyAny>,
    consume: impl FnOnce(&[u8]) -> R,
) -> PyResult<R> {
    if let Ok(text) = value.cast::<PyString>() {
        let text = text.to_str()?;
        if !text.is_ascii() {
            return Err(PyValueError::new_err("HTTP str inputs must be ASCII"));
        }
        Ok(consume(text.as_bytes()))
    } else {
        with_buffer(value, consume)
    }
}

/// Runs `consume` over the bytes of a buffer-protocol object.
fn with_buffer<R>(value: &Bound<'_, PyAny>, consume: impl FnOnce(&[u8]) -> R) -> PyResult<R> {
    with_buffer_bytes(value, |data| match data {
        BufferBytes::Borrowed(bytes) => consume(bytes),
        BufferBytes::Cells(cells) => {
            consume(&cells.iter().map(ReadOnlyCell::get).collect::<Vec<_>>())
        }
    })
}

fn extract_method(value: &Bound<'_, PyAny>) -> PyResult<Method> {
    if let Ok(text) = value.cast::<PyString>() {
        let text = text.to_str()?;
        if !text.is_ascii() {
            return Err(PyValueError::new_err("HTTP str inputs must be ASCII"));
        }
        return Method::from_bytes(text.as_bytes())
            .map_err(|error| PyValueError::new_err(error.to_string()));
    }
    with_buffer_bytes(value, |data| match data {
        BufferBytes::Borrowed(bytes) => Method::from_bytes(bytes),
        BufferBytes::Cells(cells) => {
            let bytes: Vec<_> = cells.iter().map(ReadOnlyCell::get).collect();
            Method::from_bytes(&bytes)
        }
    })?
    .map_err(|error| PyValueError::new_err(error.to_string()))
}

fn optional_bytes(py: Python<'_>, value: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<u8>> {
    value.map_or_else(|| Ok(Vec::new()), |value| text_or_buffer(py, value))
}

fn extract_version(py: Python<'_>, value: Option<&Bound<'_, PyAny>>) -> PyResult<core::Version> {
    match value
        .map(|value| text_or_buffer(py, value))
        .transpose()?
        .as_deref()
    {
        None | Some(b"1.1") => Ok(core::Version::Http11),
        Some(b"1.0") => Ok(core::Version::Http10),
        _ => Err(PyValueError::new_err(
            "http_version must be b'1.0' or b'1.1'",
        )),
    }
}

/// Returns a shared `bytes` object for a registered method, or a new one.
fn py_method(py: Python<'_>, method: &Method) -> Py<PyBytes> {
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
fn py_version(py: Python<'_>, value: core::Version) -> Bound<'_, PyBytes> {
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

fn status(value: u16) -> PyResult<StatusCode> {
    StatusCode::from_u16(value).map_err(|error| PyValueError::new_err(error.to_string()))
}
fn local_error(error: core::LocalProtocolError) -> PyErr {
    LocalProtocolError::new_err(error.to_string())
}

fn remote_error(py: Python<'_>, error: core::RemoteProtocolError) -> PyErr {
    let status = error.suggested_status_code();
    let exception = RemoteProtocolError::new_err(error.to_string());
    if let Err(set_error) = exception.value(py).setattr("suggested_status_code", status) {
        return set_error;
    }
    exception
}

/// Returns a class attribute, looking it up once per process.
fn cached_member<T: PyTypeInfo>(
    py: Python<'_>,
    cell: &'static PyOnceLock<Py<PyAny>>,
    name: &str,
) -> PyResult<Py<PyAny>> {
    cell.get_or_try_init(py, || {
        Ok::<_, PyErr>(py.get_type::<T>().getattr(name)?.unbind())
    })
    .map(|member| member.clone_ref(py))
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyRole>()?;
    module.add_class::<PyState>()?;
    module.add_class::<PyReceiveStatus>()?;
    module.add_class::<PyHeaders>()?;
    module.add_class::<PyRequest>()?;
    module.add_class::<PyInformationalResponse>()?;
    module.add_class::<PyResponse>()?;
    module.add_class::<PyData>()?;
    module.add_class::<PyEndOfMessage>()?;
    module.add_class::<PyConnectionClosed>()?;
    module.add_class::<PyConnection>()?;
    module.add("ProtocolError", module.py().get_type::<ProtocolError>())?;
    module.add(
        "LocalProtocolError",
        module.py().get_type::<LocalProtocolError>(),
    )?;
    module.add(
        "RemoteProtocolError",
        module.py().get_type::<RemoteProtocolError>(),
    )?;
    Ok(())
}
