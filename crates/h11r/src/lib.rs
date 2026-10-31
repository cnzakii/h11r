#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

mod connection;
mod headers;
mod method;
mod state;
mod status;
mod wire;

pub use connection::Connection;
pub use headers::{Headers, HeadersIter};
pub use method::{InvalidMethod, Method};
pub use state::{Role, State};
pub use status::{InvalidStatusCode, StatusCode};

use std::{error::Error as StdError, fmt};

const DEFAULT_MAX_HEAD_BYTES: usize = 64 * 1024;
const DEFAULT_MAX_HEADER_COUNT: usize = 100;

/// HTTP protocol version carried by a request or response head.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Version {
    /// HTTP/1.0.
    Http10,
    /// HTTP/1.1.
    Http11,
}

impl Version {
    pub(crate) const fn wire(self) -> &'static [u8] {
        match self {
            Self::Http10 => b"1.0",
            Self::Http11 => b"1.1",
        }
    }
}

/// A request head.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request {
    /// Case-sensitive request method.
    pub method: Method,
    /// Request-target bytes from the HTTP request line.
    ///
    /// See [RFC 9112 Section 3.2](https://www.rfc-editor.org/rfc/rfc9112.html#section-3.2).
    pub target: Vec<u8>,
    /// Ordered header fields.
    pub headers: Headers,
    /// HTTP version.
    pub http_version: Version,
}

/// A final response head.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Response {
    /// Final status code.
    pub status: StatusCode,
    /// Raw reason-phrase bytes, including any legal SP, HTAB, or obs-text.
    pub reason: Vec<u8>,
    /// Ordered header fields.
    pub headers: Headers,
    /// HTTP version.
    pub http_version: Version,
}

/// A non-final `1xx` response head.
pub type InformationalResponse = Response;

/// A piece of message body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Data<'a> {
    /// Decoded body bytes, borrowed from the connection's input buffer.
    pub data: &'a [u8],
    /// Whether these bytes begin an HTTP chunk.
    pub chunk_start: bool,
    /// Whether these bytes end an HTTP chunk.
    pub chunk_end: bool,
}

/// Framing plus an unchanged caller-owned body object.
///
/// Write the prefix, body, and suffix in order without changing the body until
/// all three writes complete.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataParts<T> {
    /// Bytes to write before the body object.
    pub prefix: DataPrefix,
    /// The original body object.
    pub data: T,
    /// Bytes to write after the body object.
    pub suffix: &'static [u8],
}

/// Framing written before body data: a chunk-size line, or nothing.
///
/// It dereferences to its bytes and lives on the stack, so framing a chunk
/// does not allocate.
#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub struct DataPrefix {
    // Sixteen hex digits for any `u64` length plus CRLF.
    bytes: [u8; 18],
    len: u8,
}

impl DataPrefix {
    pub(crate) fn chunk_size(length: usize) -> Self {
        let digits = (usize::BITS - length.leading_zeros()).div_ceil(4).max(1) as usize;
        let mut bytes = [0; 18];
        for (at, byte) in bytes[..digits].iter_mut().enumerate() {
            *byte = b"0123456789abcdef"[(length >> (4 * (digits - 1 - at))) & 0xf];
        }
        bytes[digits..digits + 2].copy_from_slice(b"\r\n");
        Self {
            bytes,
            len: (digits + 2) as u8,
        }
    }
}

impl std::ops::Deref for DataPrefix {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

impl AsRef<[u8]> for DataPrefix {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl fmt::Debug for DataPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DataPrefix({:?})", self.escape_ascii().to_string())
    }
}

/// The end of one HTTP message, optionally carrying trailer fields.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EndOfMessage {
    /// Ordered trailer fields.
    pub trailers: Headers,
}

/// A semantic event decoded from peer bytes.
///
/// [`Event::Data`] borrows the connection's input buffer, so an event must be
/// dropped before the connection is used again.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event<'a> {
    /// A request head.
    Request(Request),
    /// A non-final response head.
    InformationalResponse(InformationalResponse),
    /// A final response head.
    Response(Response),
    /// Decoded body data.
    Data(Data<'a>),
    /// Message completion and optional trailers.
    EndOfMessage(EndOfMessage),
    /// Clean transport EOF.
    ConnectionClosed,
}

/// Result of polling [`Connection::next_event`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NextEvent<'a> {
    /// One decoded protocol event.
    Event(Event<'a>),
    /// More peer bytes are needed.
    NeedData,
    /// HTTP parsing is paused until the caller advances the cycle or protocol.
    Paused,
}

/// Independent resource limits applied to inbound field sections.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Maximum bytes through the terminating empty line of a head or trailers.
    /// This also bounds each chunk-size line and the total chunk-extension
    /// bytes of one message.
    max_head_bytes: usize,
    /// Maximum number of header or trailer fields.
    max_header_count: usize,
}

impl Limits {
    /// Creates non-zero field-section byte and field-count limits.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidLimits`] if any limit is zero.
    pub const fn new(head: usize, headers: usize) -> Result<Self, InvalidLimits> {
        if head == 0 || headers == 0 {
            Err(InvalidLimits)
        } else {
            Ok(Self {
                max_head_bytes: head,
                max_header_count: headers,
            })
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_head_bytes: DEFAULT_MAX_HEAD_BYTES,
            max_header_count: DEFAULT_MAX_HEADER_COUNT,
        }
    }
}

/// A zero inbound limit is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidLimits;

impl fmt::Display for InvalidLimits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HTTP limits must be non-zero")
    }
}

impl StdError for InvalidLimits {}

/// The caller attempted an operation forbidden by the local state or framing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalProtocolError(pub(crate) String);

impl fmt::Display for LocalProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl StdError for LocalProtocolError {}

/// The peer sent malformed HTTP or exceeded an inbound limit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteProtocolError {
    pub(crate) message: String,
    pub(crate) suggested_status_code: Option<u16>,
}

impl RemoteProtocolError {
    /// Status a server can use when rejecting this input.
    #[must_use]
    pub const fn suggested_status_code(&self) -> Option<u16> {
        self.suggested_status_code
    }
}

impl fmt::Display for RemoteProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl StdError for RemoteProtocolError {}
