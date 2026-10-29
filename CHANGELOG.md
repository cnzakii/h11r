# Changelog

User-visible changes to h11r are recorded here.

## [Unreleased]

### Added

- `Connection.keep_alive` (Rust `Connection::keep_alive()`) reports whether
  HTTP may continue on the connection after the current exchange, so an adapter
  no longer reconstructs that rule from both actor states.
- Python events can be constructed directly, have a readable `repr()`, and
  support positional `match` patterns such as `case h11r.Response(404)`.
- `h11r.Headers` holds header and trailer fields. It behaves like a tuple of
  `(name, value)` byte pairs and adds `get(name)` and `get_all(name)`, which
  match names case-insensitively. Received fields are converted to Python
  objects only when read, and a `Headers` passed to a sending method is used
  without converting its fields again.
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
- CI runs every fuzz target on each change.

### Changed

- Rust: header and trailer fields use the new `Headers` type instead of
  `Vec<(Vec<u8>, Vec<u8>)>`. All fields of a section share one buffer, so a
  section costs two allocations regardless of its size. Build one from pairs,
  such as `Headers::from([("Host", "example.test")])`, and pass
  `&Headers::new()` to `end_of_message()` when there are no trailers.
- Python: event `headers` and `trailers` are `h11r.Headers` instead of tuples.
  Iteration, indexing, `len()`, and comparison with a tuple of byte pairs work
  as before; use `tuple(event.headers)` where a real tuple is required.
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
  response sent once all content is read, before `EndOfMessage` is polled,
  keeps the connection.
- A response sent after a peer protocol error also carries
  `Connection: close`.
- Faster protocol processing: framing analysis no longer allocates, body data
  skips joint-state resolution, header sections are copied into one buffer,
  the Python binding reuses shared `bytes` and receive-status objects and reads
  header input without extra Python calls, and release builds use fat LTO. On
  an Apple M4 with CPython 3.13, the five `pyperf` workloads ran 1.26–1.81×
  faster than 0.2.2 (geometric mean 1.55×), and the Rust criterion benchmarks
  ran 19–42% faster.
- The Python benchmark now models a browser calling a JSON API: requests carry
  ten typical header fields and responses a JSON body. The README shows the
  recorded result.

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
  `Content-Length: 0` instead of chunked framing.

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
