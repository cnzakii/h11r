# Changelog

User-visible changes to h11r are recorded here.

## [Unreleased]

## [0.3.0] - 2026-10-31

### Added

- Lenient parsing: `Connection(role, lenient=True)` (Rust:
  `Connection::new(role, limits).lenient()`) accepts received response heads
  that break HTTP syntax in ways mainstream clients tolerate. Invalid field
  lines, such as a name containing `/`, are dropped; whitespace before a
  field's colon and between status-line elements is accepted. A connection
  that drops a field line is not reused. Requests are never parsed leniently,
  and the default stays exactly as strict as RFC 9110 and RFC 9112.
- `Connection.keep_alive` (Rust `Connection::keep_alive()`) reports whether
  HTTP may continue on the connection after the current exchange, so an adapter
  no longer reconstructs that rule from both actor states.
- Python events can be constructed directly, have a readable `repr()`, and
  support positional `match` patterns such as `case h11r.Response(404)`.
- `h11r.Headers` holds header and trailer fields. It behaves like a tuple of
  `(name, value)` byte pairs and adds `get(name)` and `get_all(name)`, which
  match names case-insensitively. It is a `collections.abc.Sequence` with
  `index()` and `count()`. Received fields are converted to Python objects
  only when read, and a `Headers` passed to a sending method is used without
  converting its fields again.
- `send_request()` and `send_response()` accept `body=` to send a complete
  message in one call (Rust: `send_request_with_body()` and
  `send_response_with_body()`). When no framing field is present,
  `Content-Length` is added for a body the message may carry, and a rejected
  body leaves the connection unchanged. A successful `CONNECT` response with
  an empty body returns only its head, since the tunnel starts there.
- Rust `Method` constants for registered methods, such as `Method::GET`.
  Parsing a registered method no longer allocates.
- The Rust crate has its own README with a tested client/server example, used
  as the docs.rs crate documentation.

### Changed

- Rust: header and trailer fields use the new `Headers` type instead of
  `Vec<(Vec<u8>, Vec<u8>)>`. All fields of a section share one buffer, so a
  section costs two allocations regardless of its size. Build one from pairs,
  such as `Headers::from([("Host", "example.test")])`, and pass
  `Headers::new()` to `end_of_message()` when there are no trailers. The
  `Header` alias for `(Vec<u8>, Vec<u8>)` is removed.
- Rust: the send methods take their message by value, as
  `send_request(Request)`, `send_informational_response(InformationalResponse)`,
  `send_response(Response)`, and `end_of_message(Headers)`, matching the
  complete-message methods. Fields added to a head, such as
  `Transfer-Encoding: chunked`, no longer copy the caller's fields.
- Python: event `headers` and `trailers` are `h11r.Headers` instead of tuples.
  Iteration, indexing, `len()`, and comparison with a tuple of byte pairs work
  as before; use `tuple(event.headers)` where a real tuple is required.
- Python: `State`, `Role`, and `ReceiveStatus` members are hashable, so they
  work in sets and as dictionary keys, and no longer compare equal to
  integers.
- Python: a status code outside 100–599 raises `ValueError` however large the
  integer; very large values previously raised `OverflowError`.
  `RemoteProtocolError.suggested_status_code` is `None` on an instance raised
  outside the connection.
- A client-role `start_next_cycle()` now fails, and the connection must close,
  when peer bytes follow a complete response. RFC 9112 Section 6.3 forbids
  processing such bytes as a separate response; previously they became the
  response to the next request.
- Request targets must use the form their method allows: authority-form with
  a port from 1 to 65535 only for `CONNECT`, `*` only for `OPTIONS`, and
  origin-form or absolute-form otherwise, never with a fragment. Senders raise `LocalProtocolError` and
  receivers reject the request with status 400.
- `max_head_bytes` also bounds the total chunk-extension bytes of one message.
  Exceeding it raises `RemoteProtocolError` with status 400.
- A final response sent while request content remains unread now carries
  `Connection: close`, and the connection closes after the exchange. A
  response sent once the remaining `Content-Length` content has arrived, even
  if not yet polled, keeps the connection.
- HTTP/1.0 connections persist when the message carries
  `Connection: keep-alive`, as RFC 9112 Section 9.3 allows and mainstream
  servers do. A response to such a client gets `Connection: keep-alive` when
  it has a known length, and `Connection: close` otherwise. A proxy, which
  must not honor the option on requests, sends `Connection: close` itself.
- A received response whose final transfer coding is not chunked, such as
  `Transfer-Encoding: gzip`, is read until the server closes, as RFC 9112
  Section 6.3 requires, instead of raising `RemoteProtocolError`. Codings
  before a final `chunked` are left in the body for the caller to decode.
- A request whose final transfer coding is not chunked suggests status 400,
  as RFC 9112 Section 6.3 requires, instead of 501; 501 remains for an
  unsupported coding before a final `chunked`. A request line longer than
  `max_head_bytes` suggests 414 instead of 431.
- A response sent after a peer protocol error also carries
  `Connection: close`.
- Rust: `DataParts.prefix` is a `DataPrefix`, which dereferences to the
  chunk-size line, and `DataParts.suffix` is `&'static [u8]`, so framing a
  chunk no longer allocates. Python `send_data()` and `body=` sends copy the
  body once instead of twice.
- `Transfer-Encoding` is parsed as RFC 9110 Section 10.1.4 defines it, so a
  comma inside a quoted parameter no longer splits a coding, and empty list
  elements are ignored as Section 5.6.1.2 requires. A malformed coding, or a
  registered coding such as `chunked` or `gzip` with parameters, is rejected
  with status 400.
- Senders enforce the field rules of RFC 9110 and RFC 9112 for the fields
  `h11r` interprets, raising `LocalProtocolError` for: a request without Host,
  including HTTP/1.0; more than one `Content-Length` value; an empty element
  in `Connection`, `Upgrade`, `Expect`, `TE`, or `Transfer-Encoding`;
  `Upgrade` without `Connection: upgrade`; a `426` without `Upgrade`; `TE`
  naming `chunked` or without `Connection: TE`; and invalid framing fields on
  a `HEAD` or `304` response. A request's Host field is written first.
- A client that receives a response with no request outstanding raises
  `RemoteProtocolError`, as RFC 9112 Section 9.2 requires, instead of
  accepting it. `send_request()` raises `LocalProtocolError` when peer bytes
  arrived before it, so they can never become its response.
- A client stops reporting `keep_alive` once it receives EOF, and
  `start_next_cycle()` fails, since the server has closed. A server still
  drains requests buffered before the client's EOF.
- A chunk extension may have whitespace after a valueless name, as the RFC
  9112 Section 7.1.1 grammar allows.
- Faster protocol processing throughout parsing, framing, and the Python
  binding. The README benchmark now models a browser calling a JSON API and
  shows the recorded result.

### Fixed

- A head or trailer section that is invalid within `max_head_bytes` now fails
  with the same error under every transport segmentation. Previously, a
  segmented section could report 431 where the whole section reported 400.
  Invalid bytes are also rejected soon after they arrive instead of after
  `max_head_bytes` bytes have been buffered.
- Servers no longer send `1xx` responses to HTTP/1.0 clients, as RFC 9110
  Section 15.2 requires.
- Sending content in a `CONNECT` or `TRACE` request, or in a `205` response,
  raises `LocalProtocolError`, as RFC 9110 Sections 9.3.6, 9.3.8, and 15.3.6
  require. A `205` response without framing fields now carries
  `Content-Length: 0` instead of chunked framing. A server receiving such a
  request rejects it with status 400.
- A response that itself ends keep-alive now carries `Connection: close`:
  one sent before a request arrives, such as a `408` on an idle reused
  connection, and one that rejects `CONNECT`. Previously the peer could not
  tell that the connection would close.
- A response without content, such as one to `HEAD` or a `304`, that carries
  faulty framing fields (`Transfer-Encoding` in HTTP/1.0, or with
  `Content-Length`) now ends the connection, as RFC 9112 Sections 6.1 and 6.3
  require. Previously an HTTP/1.0 `Connection: keep-alive` response could
  keep it open.
- Empty lines followed by EOF between messages, such as a stray CRLF after a
  request body, are a clean close instead of a truncated-head error, as
  RFC 9112 Section 2.2 allows.
- Trailer parsing stops at the end of the message, so a whitespace-led line
  that follows it is no longer read as a folded trailer, which made the
  result depend on transport segmentation.
- A response to an HTTP/1.0 client that carries both `Transfer-Encoding` and
  `Content-Length` raises `LocalProtocolError`, as it does for an HTTP/1.1
  client, instead of being sent with both removed.

## [0.2.2] - 2026-10-15

### Added

- Support for CPython 3.15 and free-threaded 3.15t.
- Wheels for musl-based Linux distributions such as Alpine (`musllinux_1_2`
  on x86_64 and aarch64), which previously built from the source distribution.
- `abi3t` stable-ABI wheels for CPython 3.15 and later, which install on both
  GIL-enabled and free-threaded builds without compiling from source.

## [0.2.1] - 2026-08-22

### Added

- Python's `Connection.buffered_nbytes` reports how many received bytes are held
  but not yet parsed. `trailing_data` already exposed those bytes, but copied
  them on every access. Their byte length lets an adapter that pushes bytes in
  as they arrive apply read back-pressure without materializing the backlog.

## [0.2.0] - 2026-08-02

### Added

- A searchable Zensical documentation site with a guided learning path,
  task-oriented integration and advanced guides, generated Python API
  reference with typed stub signatures and PyO3 docstrings, and versioned
  Read the Docs deployment.
- Allow Python transport adapters to pass byte-sized body proxies through
  `Connection.send_data_parts(body)`. A proxy declares its exact byte length
  with `nbytes`, and h11r returns the identical object for the transport to
  write.

### Changed

- Python 3.11 is the minimum supported version; wheels now target the
  `abi3-py311` stable ABI.
- `receive_data()` and `send_data()` read C-contiguous, unsigned-byte
  `bytearray` and `memoryview` arguments through safe per-byte buffer reads
  directly into the Rust input or output allocation, so `socket.recv_into()`
  loops with a reused buffer avoid a `bytes` allocation per read. Other buffer
  layouts and formats retain the previous copying behavior. Immutable `bytes`
  are borrowed directly.
- Back-to-back `pyperf` runs of release builds measured 3.5%–14.7% higher
  throughput than 0.1.0 across five protocol-layer workloads. These results
  exclude socket, TLS, async runtime, and application overhead.
- Rust core: `Event::Data` borrows the connection's input buffer instead of
  owning a copy, so an event must be dropped before the connection is used
  again. The Python `Data` event still owns its `bytes`.

## [0.1.1] - 2026-07-21

### Added

- Support for free-threaded CPython 3.14t, including version-specific wheels,
  with preview CI coverage for GIL-enabled and free-threaded CPython 3.15.
- Parallel operation across independent Python `Connection` instances;
  operations on one connection remain caller-serialized in protocol order.

## [0.1.0] - 2026-07-17

### Added

- Initial Rust core and Python package for Sans-I/O HTTP/1.1.
