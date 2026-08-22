# Changelog

User-visible changes to h11r are recorded here.

## [Unreleased]

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
