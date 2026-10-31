<p align="center">
  <img src="https://github.com/cnzakii/h11r/raw/refs/heads/main/docs/site/assets/h11r.svg" width="144" height="144" alt="h11r logo">
</p>

<h1 align="center">h11r</h1>

<p align="center">
  <strong>A fast, typed <a href="https://sans-io.readthedocs.io/">Sans-I/O</a> HTTP/1.1 engine with a Rust core and Python API.</strong>
</p>

<p align="center">
  <a href="https://github.com/cnzakii/h11r/actions/workflows/ci.yml"><img src="https://github.com/cnzakii/h11r/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://codecov.io/gh/cnzakii/h11r"><img src="https://codecov.io/gh/cnzakii/h11r/graph/badge.svg" alt="codecov"></a>
  <a href="https://pypi.org/project/h11r/"><img src="https://img.shields.io/pypi/v/h11r.svg" alt="PyPI"></a>
  <a href="https://crates.io/crates/h11r"><img src="https://img.shields.io/crates/v/h11r.svg" alt="Crates.io"></a>
  <a href="https://docs.rs/h11r"><img src="https://docs.rs/h11r/badge.svg" alt="docs.rs"></a>
  <a href="https://github.com/cnzakii/h11r/blob/main/crates/h11r-python/pyproject.toml"><img src="https://img.shields.io/badge/Python-3.11%20to%203.15-3776AB?logo=python&amp;logoColor=white" alt="Python 3.11–3.15"></a>
  <a href="https://github.com/cnzakii/h11r/blob/main/Cargo.toml"><img src="https://img.shields.io/badge/Rust-1.88%2B-000000?logo=rust&amp;logoColor=white" alt="Rust 1.88+"></a>
  <a href="https://github.com/cnzakii/h11r/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
</p>

Build HTTP/1.1 clients, servers, proxies, and protocol tools on the transport
and runtime you already use. `h11r` turns received bytes into typed events and
your calls into bytes to send; your code decides how and when those bytes move.

> `h11r` is currently alpha software. Its public API may change before the first
> stable release.

## Why h11r

- **Fast.** On realistic browser-to-API traffic, `h11r` completes HTTP/1.1
  exchanges 13–23× faster than the pure-Python [h11][h11] in every
  [measured scenario](#performance).
- **Strict about message boundaries.** Ambiguous framing such as
  `Transfer-Encoding` with `Content-Length` is rejected, request targets must
  match their method, and a client never treats unsolicited bytes as a
  response, so peers cannot be led to disagree about where a message ends.
- **Python-native and typed.** Immutable events with readable `repr()`,
  `match` support, Python exceptions, and precise stubs for editors and type
  checkers.
- **Fits any runtime.** `h11r` never opens sockets or starts an event loop, so
  the same code works with `asyncio`, threads, Trio, or in-memory tests.

## Quick look

A client and a server exchange one request in memory. A real program writes
each returned `bytes` object to its socket and passes the bytes it reads to
`receive_data()`.

```python
import h11r

client = h11r.Connection(h11r.Role.CLIENT)
server = h11r.Connection(h11r.Role.SERVER)

server.receive_data(
    client.send_request("GET", "/hello", [("Host", "example.test")], body=b"")
)
print(server.next_event())
print(server.next_event())

client.receive_data(
    server.send_response(200, [("Content-Type", "text/plain")], body=b"Hello!\n")
)
while not isinstance(event := client.next_event(), h11r.EndOfMessage):
    print(event)
print(client.keep_alive)
```

```text
Request(method=b'GET', target=b'/hello', headers=Headers([(b'Host', b'example.test')]), http_version=b'1.1')
EndOfMessage(trailers=Headers([]))
Response(status_code=200, reason=b'', headers=Headers([(b'Content-Type', b'text/plain'), (b'Content-Length', b'7')]), http_version=b'1.1')
Data(data=b'Hello!\n', chunk_start=False, chunk_end=False)
True
```

`body=` sends a complete message and adds `Content-Length`. To stream a body
instead, call `send_data()` for each piece and `end_of_message()` at the end.

## Installation

```console
python -m pip install h11r
```

Or add it to a [uv](https://docs.astral.sh/uv/) project:

```console
uv add h11r
```

`h11r` supports CPython 3.11 through 3.15, including free-threaded 3.14t and
3.15t. Wheels are published for Linux (glibc and musl), macOS, and Windows.

The protocol core is also a Rust crate with its own [API docs][rust-api]:

```console
cargo add h11r
```

## What h11r handles

Each `Connection` is one endpoint of an HTTP/1.1 connection, in the client or
server role: `Content-Length` and chunked bodies, keep-alive, pipelining,
informational responses, trailers, HTTP/1.0 peers, and protocol handoff after
Upgrade or CONNECT. Header sections have byte and field-count limits, and
local API misuse is reported separately from remote protocol errors.

Your application owns sockets, TLS, buffering, connection pooling, routing,
retries, timeouts, and body size limits. Calls are synchronous and never block
on I/O. Drive each `Connection` from one thread or task at a time; separate
connections can run in parallel, including on free-threaded CPython.

## Documentation

- [First client request][first-client-guide]: start here. The tutorials go on
  to a [client/server round trip][round-trip-guide] over a real stream.
- [Core concepts][protocol-model]: events, connection cycles, receive
  statuses, and error boundaries.
- [Transport integration][integration-guide]: connect a `Connection` to
  synchronous or asynchronous byte streams.
- [Advanced patterns][advanced-guide]: streaming, buffer-preserving writes,
  pipelining, and protocol handoff.
- [Runnable examples][examples]: complete programs, including an
  [`asyncio` server][asyncio-example] and a
  [WebSocket Upgrade handoff][upgrade-example] to `wsproto`.

## Performance

Each scenario runs complete exchanges through the public APIs of `h11r` and
`h11`, measured with `pyperf`. Requests carry the header fields of a browser
calling a JSON API, and the server answers with typical response fields and a
JSON body. Connections are reused, so the timer covers parsing, framing, state
transitions, event construction, and serialization.

<!-- benchmark-table:start -->
| Scenario | h11r (µs/op) | h11 (µs/op) | Relative throughput |
| --- | ---: | ---: | ---: |
| API GET · 10 fields | 1.34 | 27.13 | 20.3× |
| JSON POST · 1 KiB | 1.53 | 30.81 | 20.2× |
| Fragmented GET · 16 B reads | 3.29 | 46.74 | 14.2× |
| Chunked upload · 64 KiB | 6.36 | 85.22 | 13.4× |
| Client/server round trip | 2.32 | 52.87 | 22.8× |

_Environment: h11r 0.3.0 · h11 0.16.0 · CPython 3.13.12 (64-bit) · Apple M4 · macOS-27.2-arm64-arm-64bit-Mach-O · 2026-10-31_
<!-- benchmark-table:end -->

These are protocol-layer measurements. They exclude sockets, TLS, event-loop
scheduling, and application work, so end-to-end gains depend on how much of
your program's time HTTP/1.1 processing takes.

Inspect the [benchmark source][benchmark-script], [raw result][benchmark-results],
and [chart][benchmark-chart], or follow the
[reproduction steps](https://github.com/cnzakii/h11r/blob/main/CONTRIBUTING.md#performance-changes).

## Contributing

See [CONTRIBUTING.md][contributing-guide] for development, testing, and release
guidance.

## License

MIT

[benchmark-chart]: https://raw.githubusercontent.com/cnzakii/h11r/main/docs/assets/python-benchmark.svg
[benchmark-results]: https://github.com/cnzakii/h11r/blob/main/docs/assets/python-benchmark.json
[benchmark-script]: https://github.com/cnzakii/h11r/blob/main/crates/h11r-python/benchmarks/compare_h11.py
[first-client-guide]: https://h11r.readthedocs.io/en/stable/getting-started/
[round-trip-guide]: https://h11r.readthedocs.io/en/stable/round-trip/
[upgrade-example]: https://github.com/cnzakii/h11r/blob/main/examples/python/websocket_upgrade.py
[examples]: https://github.com/cnzakii/h11r/tree/main/examples/python
[h11]: https://h11.readthedocs.io/
[asyncio-example]: https://github.com/cnzakii/h11r/blob/main/examples/python/asyncio_server.py
[protocol-model]: https://h11r.readthedocs.io/en/stable/concepts/
[integration-guide]: https://h11r.readthedocs.io/en/stable/integration/
[advanced-guide]: https://h11r.readthedocs.io/en/stable/advanced/
[contributing-guide]: https://github.com/cnzakii/h11r/blob/main/CONTRIBUTING.md
[rust-api]: https://docs.rs/h11r
