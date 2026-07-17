# Changelog

User-visible changes to h11r are recorded here.

## [Unreleased]

### Added

- Support for GIL-enabled CPython 3.15 and free-threaded CPython 3.14t and
  3.15t, including version-specific 3.14t and stable-ABI 3.15t wheels.
- Parallel operation across independent Python `Connection` instances;
  operations on one connection remain caller-serialized in protocol order.

## [0.1.0] - 2026-07-17

### Added

- Initial Rust core and Python package for Sans-I/O HTTP/1.1.
