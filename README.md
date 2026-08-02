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
  <a href="https://github.com/cnzakii/h11r/blob/main/crates/h11r-python/pyproject.toml"><img src="https://img.shields.io/badge/Python-3.11%20to%203.14-3776AB?logo=python&amp;logoColor=white" alt="Python 3.11–3.14"></a>
  <a href="https://github.com/cnzakii/h11r/blob/main/Cargo.toml"><img src="https://img.shields.io/badge/Rust-1.88%2B-000000?logo=rust&amp;logoColor=white" alt="Rust 1.88+"></a>
  <a href="https://github.com/cnzakii/h11r/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
</p>

`h11r` is a low-level HTTP/1.1 protocol engine with a transport-independent
Rust core and a typed Python API. Applications own their network I/O: send
methods produce wire bytes, while received peer bytes become immutable Python
events. The library enforces message framing, connection state, and protocol
errors while your code keeps control of transport, runtime, and application
policy.

Use it to build HTTP/1.1 clients, servers, proxies, protocol adapters, or test
tools. It is not a ready-made HTTP client that opens connections and sends
requests for you.

> `h11r` is currently alpha software. Its public API may change during alpha
> development.

## Why h11r

- **Low protocol-layer overhead.** The checked-in benchmarks show higher
  throughput than `h11` across five equivalent HTTP/1.1 workloads.
  [See the results.](#performance)
- **A Python-native, typed API.** Work with immutable event objects, Python
  exceptions, and precise type information in editors and type checkers.
- **Your transport and runtime.** The library does not choose sockets,
  TLS, concurrency, cancellation, or back-pressure policy.
- **More control when you need it.** Start with ordinary request and response
  bytes, then opt into streaming bodies, buffer-preserving writes, pipelining,
  or protocol handoff.

## Quick Start

For Python, add `h11r` to a uv-managed project:

```console
uv add h11r
```

Or install it with pip:

```console
pip install h11r
```

For Rust, add the protocol core and use its [docs.rs API][rust-api]:

```console
cargo add h11r
```

Create a client-role connection, serialize one bodyless request, and parse a
complete response:

```python
import h11r

client = h11r.Connection(h11r.Role.CLIENT)
request_bytes = client.send_request(
    "GET",
    "/hello",
    [("Host", "example.test")],
)
request_bytes += client.end_of_message()

client.receive_data(b"HTTP/1.1 200 OK\r\nContent-Length: 17\r\n\r\nHello from h11r!\n")

status_code = None
response_body = bytearray()

while True:
    event = client.next_event()

    match event:
        case h11r.Response(status_code=code):
            status_code = code
        case h11r.Data(data=chunk):
            response_body.extend(chunk)
        case h11r.EndOfMessage():
            break

print(status_code, bytes(response_body))
```

Output:

```text
200 b'Hello from h11r!\n'
```

In a real client, your transport writes `request_bytes` and supplies each read
to `receive_data()`. The [first tutorial][first-client-guide] keeps the same
client-side flow and makes that boundary visible before adding a socket.

## Learn and Integrate

- Start with the [first client tutorial][first-client-guide], which serializes a
  request and parses a simulated response.
- Continue with the [client/server round trip][round-trip-guide] to move bytes
  through a real local stream and reuse the connection.
- Read the [protocol model][protocol-model] before connecting `h11r` to your own
  [transport adapter][integration-guide].
- Use [advanced patterns][advanced-guide] only when you need streaming,
  buffer-preserving writes, pipelining, or protocol handoff.

Use the runnable examples when you need a specific integration:

| Goal | Example |
| --- | --- |
| Process a body incrementally and validate trailers | [`streaming_body.py`][streaming-example] |
| Preserve response order for pipelined requests | [`pipelining.py`][pipelining-example] |
| Pass a transport-owned file region through to `sendfile()` | [`zero_copy_body.py`][zero-copy-example] |
| Hand a successful WebSocket Upgrade to `wsproto` | [`websocket_upgrade.py`][upgrade-example] |
| Build a complete teaching server with `asyncio` streams | [`asyncio_server.py`][asyncio-example] |

## Performance

![Python benchmark comparing h11r and h11][benchmark-chart]

The chart compares protocol-layer throughput for `h11r` and `h11 0.16.0`
across five equivalent Python HTTP/1.1 workloads that reuse their connections.
Each workload uses public APIs and includes protocol state transitions. It does
not include socket, TLS, or asynchronous runtime overhead. Higher is faster.

The results were produced by the [`pyperf` benchmark][benchmark-script]. The
[raw pyperf result][benchmark-results] used to render the chart is included for
reproduction and inspection. The measurement environment is recorded in the
chart. Results will vary with hardware and Python version.

## Scope and compatibility

Each `Connection` maintains one endpoint's view of an HTTP/1.1 connection.
Higher-level clients, servers, proxies, and test tools decide how to schedule
I/O and apply application policy.

It supports client and server roles, HTTP/1.0 peers, `Content-Length` and
chunked framing, keep-alive cycles, informational responses, trailers, and
protocol handoff after Upgrade. Header and trailer section byte size and field
count have independent limits; application body limits remain the caller's
responsibility. Local API misuse is reported separately from remote protocol
errors.

The Python package supports GIL-enabled CPython 3.11 through 3.14 and
free-threaded CPython 3.14t. CI also exercises CPython 3.15 and 3.15t while
they are prereleases. Independent `Connection` instances may run
in parallel. Operations on one connection still have protocol order and must
be serialized by its caller.

## Relationship to h11

`h11r` is an independent Sans-I/O HTTP/1.1 protocol engine with a Rust core
and a typed Python API. It performs no network I/O and does not depend on the
Python [`h11`](https://github.com/python-hyper/h11) package at runtime. `h11r`
and `h11` occupy the same protocol-engine layer, but their public APIs differ,
so switching between them requires adapting application integration code.

The Rust core uses [`httparse`](https://github.com/seanmonstar/httparse) for
request and response heads and trailer fields. `h11r` implements framing,
buffering and resource limits, wire serialization, and its public Rust and
Python APIs.

Interoperability tests verify representative `h11r`-client/`h11`-server and
`h11`-client/`h11r`-server exchanges at the HTTP wire boundary. They provide
evidence for those exchanges, not an API-compatibility or exhaustive
wire-compatibility guarantee. `h11` remains a mature pure-Python library with
its own established API and ecosystem.

## Contributing

See [CONTRIBUTING.md][contributing-guide] for development and release guidance.

## License

MIT

[benchmark-chart]: https://raw.githubusercontent.com/cnzakii/h11r/main/docs/assets/python-benchmark.svg
[benchmark-results]: https://github.com/cnzakii/h11r/blob/main/docs/assets/python-benchmark.json
[benchmark-script]: https://github.com/cnzakii/h11r/blob/main/crates/h11r-python/benchmarks/compare_h11.py
[first-client-guide]: https://h11r.readthedocs.io/en/stable/getting-started/
[round-trip-guide]: https://h11r.readthedocs.io/en/stable/round-trip/
[streaming-example]: https://github.com/cnzakii/h11r/blob/main/examples/python/streaming_body.py
[pipelining-example]: https://github.com/cnzakii/h11r/blob/main/examples/python/pipelining.py
[zero-copy-example]: https://github.com/cnzakii/h11r/blob/main/examples/python/zero_copy_body.py
[upgrade-example]: https://github.com/cnzakii/h11r/blob/main/examples/python/websocket_upgrade.py
[asyncio-example]: https://github.com/cnzakii/h11r/blob/main/examples/python/asyncio_server.py
[protocol-model]: https://h11r.readthedocs.io/en/stable/concepts/
[integration-guide]: https://h11r.readthedocs.io/en/stable/integration/
[advanced-guide]: https://h11r.readthedocs.io/en/stable/advanced/
[contributing-guide]: https://github.com/cnzakii/h11r/blob/main/CONTRIBUTING.md
[rust-api]: https://docs.rs/h11r
