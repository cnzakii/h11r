# Changelog

User-visible changes to h11r are recorded here.

## [Unreleased]

### Added

- A searchable Zensical documentation site with a guided learning path,
  task-oriented integration and advanced guides, generated Python API
  reference, and GitHub Pages deployment.

### Changed

- Python 3.11 is the minimum supported version; wheels now target the
  `abi3-py311` stable ABI.
- `receive_data()` and `send_data()` read `bytearray` and `memoryview`
  arguments through the buffer protocol without an intermediate copy, so
  `socket.recv_into()` loops with a reused buffer avoid a `bytes`
  allocation per read. Buffers must be C-contiguous with byte-sized items.
  Free-threaded builds still copy mutable buffers, because without the GIL
  another thread may change them mid-read; immutable `bytes` are never
  copied.
- Rust core: `Event::Data` borrows the connection's input buffer instead of
  owning a copy, so an event must be dropped before the connection is used
  again. The Python `Data` event still owns its `bytes`.

### Performance

- Body bytes now move from the transport to `Data.data` with two fewer
  copies. Receive-path overhead drops roughly 40% for 64 KiB chunks fed
  through a reused `recv_into()` buffer, and roughly 10% for 1 KiB bodies
  parsed by the Rust core.

## [0.1.1] - 2026-07-21

### Added

- Support for free-threaded CPython 3.14t, including version-specific wheels,
  with preview CI coverage for GIL-enabled and free-threaded CPython 3.15.
- Parallel operation across independent Python `Connection` instances;
  operations on one connection remain caller-serialized in protocol order.

## [0.1.0] - 2026-07-17

### Added

- Initial Rust core and Python package for Sans-I/O HTTP/1.1.
