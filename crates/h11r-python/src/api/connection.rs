//! The `Connection` class.

use super::*;
use h11r as core;
use pyo3::buffer::ReadOnlyCell;
use pyo3::exceptions::PyValueError;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyAny, PyBytes};

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
///     lenient=False,
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
/// By default the connection is as strict as RFC 9110 and RFC 9112 allow.
/// `lenient=True` additionally accepts received response heads that break
/// HTTP syntax in ways mainstream clients tolerate: invalid field lines are
/// dropped, and whitespace before a field's colon or between status-line
/// elements is accepted. A connection that drops a field line is not reused,
/// since the line may have framed the message. Requests are never parsed
/// leniently.
///
/// Raises:
///     ValueError: If any inbound limit is zero.
#[pyclass(name = "Connection", module = "h11r")]
#[derive(Debug)]
pub(super) struct PyConnection(core::Connection);

#[pymethods]
impl PyConnection {
    #[new]
    #[pyo3(signature = (role, *, max_head_bytes = 65536, max_header_count = 100, lenient = false))]
    fn new(
        role: PyRole,
        max_head_bytes: usize,
        max_header_count: usize,
        lenient: bool,
    ) -> PyResult<Self> {
        let limits = core::Limits::new(max_head_bytes, max_header_count)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        let connection = core::Connection::new(role.into(), limits);
        Ok(Self(if lenient {
            connection.lenient()
        } else {
            connection
        }))
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
            target: text_or_buffer(target)?,
            headers: extract_headers(Some(headers))?,
            http_version: extract_version(http_version)?,
        };
        match body {
            None => self
                .0
                .send_request(request)
                .map(|bytes| PyBytes::new(py, &bytes).unbind())
                .map_err(local_error),
            Some(body) => framed(py, body, |length| {
                self.0.send_request_with_body_framing(request, length)
            }),
        }
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
        status_code: &Bound<'_, PyAny>,
        headers: Option<&Bound<'_, PyAny>>,
        reason: Option<&Bound<'_, PyAny>>,
        http_version: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyBytes>> {
        let response = core::InformationalResponse {
            status: status(status_code)?,
            reason: optional_bytes(reason)?,
            headers: extract_headers(headers)?,
            http_version: extract_version(http_version)?,
        };
        self.0
            .send_informational_response(response)
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
        status_code: &Bound<'_, PyAny>,
        headers: Option<&Bound<'_, PyAny>>,
        body: Option<&Bound<'_, PyAny>>,
        reason: Option<&Bound<'_, PyAny>>,
        http_version: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyBytes>> {
        let response = core::Response {
            status: status(status_code)?,
            reason: optional_bytes(reason)?,
            headers: extract_headers(headers)?,
            http_version: extract_version(http_version)?,
        };
        match body {
            None => self
                .0
                .send_response(response)
                .map(|bytes| PyBytes::new(py, &bytes).unbind())
                .map_err(local_error),
            Some(body) => framed(py, body, |length| {
                self.0.send_response_with_body_framing(response, length)
            }),
        }
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
        framed(py, data, |length| self.0.send_data_framing(length))
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
            PyBytes::new(py, suffix).unbind(),
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
            .end_of_message(trailers)
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
