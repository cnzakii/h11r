# h11r Repository Guidance

Read [CONTRIBUTING.md](CONTRIBUTING.md) first: its setup, checks, protocol
decision rules, code conventions, and test rules apply to every change. This
file adds what an agent needs beyond them.

## Product Boundary

- `h11r` is a Sans-I/O HTTP/1.1 protocol engine with a Rust core and a
  Python package.
- HTTP/1.1 is the whole product, not one module in a larger protocol suite. Do
  not add other protocol implementations or an umbrella-project hierarchy.
- Upgrade and handoff behavior belongs here only where HTTP/1.1 requires the
  connection boundary; ownership of the next protocol stays with the caller.
- Keep socket, TLS, async runtime, timeout, client, server, and
  connection-pool ownership outside the protocol core.

## Repository Map

- `crates/h11r/src`: the Rust core. `connection/` holds `Connection`, split
  into receiving (`receive.rs`) and sending (`send.rs`); `state.rs` is the
  client and server lifecycle state machine; `wire/` parses heads (through
  httparse), framing fields, chunked bodies, and request targets.
- `crates/h11r/tests`: `conformance.rs` (RFC requirements) and
  `protocol.rs` (engine behavior). `crates/h11r/benches` holds the Criterion
  benchmarks.
- `crates/h11r-python`: the PyO3 binding (`src/api`), the Python package and
  its stubs (`python/h11r/_core.pyi`), tests, and benchmarks:
  `compare_h11.py` and `latency_memory.py` by hand, whose results in
  `docs/assets` render the published tables, and `test_codspeed.py` on every
  pull request.
- `examples/python`: runnable examples, executed by `test_examples.py`.
- `fuzz`: cargo-fuzz targets for receiving, sending, fragmentation, and whole
  exchanges.
- `docs/site`: the public documentation site.
- `.github/workflows`: `ci.yml` runs the local gate, the Python and platform
  matrix, MSRV, fuzzing, coverage, and the documentation build;
  `codspeed.yml` tracks benchmarks; `release.yml` publishes tags. Actions are
  pinned to commit SHAs, which Dependabot updates monthly, except
  `dtolnay/rust-toolchain`: it has no release tags, so its pin is refreshed
  by hand to a current `master` commit.

## Working Rules

- Read the affected implementation, its callers, tests, exports, stubs, and
  configuration before editing. Update Python runtime exports, type stubs,
  and tests together when the Python API changes.
- Establish the requested outcome and its non-goals before adding files,
  options, or abstractions.
- Stop and ask before a change CONTRIBUTING.md says to discuss first: new
  public API, protocol behavior, or a substantial design change.
- For a protocol choice, apply the decision rules in CONTRIBUTING.md and
  present the RFC text and implementation comparison behind it.
- Report exactly which checks ran and their results. Never claim that a check
  passed without running it.
- Keep generated artifacts, build output, local environments, editor state,
  reports, and personal instructions out of version control.

Work is complete only after the relevant focused checks and `make check` pass,
followed by a read-only review of the final diff.
