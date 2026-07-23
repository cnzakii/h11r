---
description: A fast, typed Sans-I/O HTTP/1.1 library for Python.
---

<div class="home-intro" markdown>
<div class="home-intro__copy" markdown>

# h11r

<p class="home-intro__tagline">
A fast, typed Sans-I/O HTTP/1.1 library for Python.
</p>

<p class="home-intro__summary">
Turn HTTP messages into wire bytes and peer bytes back into typed events while
your application keeps control of networking, concurrency, and policy.
</p>

[Send your first request](getting-started.md){ .md-button .md-button--primary }
[View the Python API](api.md){ .md-button }

</div>

<img class="home-intro__mark" src="assets/h11r.svg" alt="h11r">
</div>

## Why h11r

- **Low protocol-layer overhead.** Keep parsing, framing, and connection state
  inexpensive across small requests, streaming bodies, and complete exchanges.
  [Inspect the performance evidence →](#performance-evidence)
- **Python-native and precisely typed.** Work with immutable events, Python
  exceptions, and exact annotations in editors and type checkers.
- **Your I/O stays yours.** Use synchronous sockets, `asyncio`, Trio, test
  streams, or another byte transport without changing the protocol code.
- **Control when you need it.** Start with ordinary request and response bytes,
  then opt into streaming, buffer-preserving writes, pipelining, or protocol
  handoff.

!!! warning "Alpha software"

    `h11r` is under active alpha development. Its public API may change.

## Start with one request

Install `h11r`, create a client-role connection, and ask it for the wire bytes
of a bodyless `GET` request:

```console
uv add h11r
```

```python
import h11r

client = h11r.Connection(h11r.Role.CLIENT)
request_bytes = client.send_request(
    "GET",
    "/hello",
    [("Host", "example.test")],
)
request_bytes += client.end_of_message()
```

Your transport writes `request_bytes` to the peer. Bytes read from the peer go
back to `h11r`, which turns them into response events.

[Continue the tutorial →](getting-started.md)

## Keep learning

- [**Send your first client request**](getting-started.md) — serialize one
  request and decode a simulated server response.
- [**Run a client/server round trip**](round-trip.md) — move bytes through a
  real local stream and reuse the connection.
- [**Understand the model**](concepts.md) — learn the event flow, connection
  cycles, receive statuses, and error boundaries.
- [**Connect your transport**](integration.md) — join a `Connection` to a
  synchronous or asynchronous byte stream.

## How h11r fits

On the wire, requests and responses are bytes. In application code, received
messages appear as HTTP events. `h11r` owns the framing and connection rules
between those representations; it does not move bytes across the network.

![The application calls h11r send methods and receives events while its transport moves the returned and received bytes](assets/sans-io-boundary.svg)

Create a `Role.CLIENT` connection for the client endpoint or a `Role.SERVER`
connection for the server endpoint. Call `send_request()`, `send_response()`,
`send_data()`, and `end_of_message()` to produce bytes for the transport. Pass
received bytes to `receive_data()`, then drain events with `next_event()`.

`h11r` deliberately does not own:

- sockets or TLS;
- an async runtime or concurrency model;
- timeouts, cancellation, or back-pressure policy;
- routing, redirects, cookies, or connection pools.

That boundary lets the surrounding program choose its transport and lifecycle
without reimplementing HTTP/1.1 framing and state.

## Performance evidence

The recorded `pyperf` run compares `h11r` with `h11` through their public
Python APIs. Each workload includes protocol state transitions and reuses its
connection. Lower time and higher relative throughput are better.

--8<-- "python-benchmark.md"

This is not an end-to-end server benchmark. It excludes socket, TLS, async
runtime, and application overhead, and its result does not predict every
workload or machine.

[Benchmark script ↗](https://github.com/cnzakii/h11r/blob/{{ git.commit }}/crates/h11r-python/benchmarks/compare_h11.py) ·
[Raw `pyperf` result ↗](https://github.com/cnzakii/h11r/blob/{{ git.commit }}/docs/assets/python-benchmark.json)

## Relationship to h11

`h11r` follows the Sans-I/O connection and event model established by
[`h11` ↗](https://github.com/python-hyper/h11), but it does not depend on `h11`
at runtime and is not a drop-in replacement. Its Python API keeps familiar
roles, events, and connection cycles while using dedicated send methods and
its own types.

The Rust core uses
[`httparse` ↗](https://github.com/seanmonstar/httparse) for request and response
heads and trailer fields. `h11r` implements framing, buffering and resource
limits, wire serialization, and its public Rust and Python APIs.

Interoperability tests exercise both `h11r`-client/`h11`-server and
`h11`-client/`h11r`-server exchanges at the HTTP wire boundary. `h11` remains
a mature pure-Python library with its own established API and ecosystem.

[Inspect the interoperability tests ↗](https://github.com/cnzakii/h11r/blob/{{ git.commit }}/crates/h11r-python/tests/test_interop.py)

Use the [Python API reference](api.md) for parameters, return values, behavior,
and exceptions.

The Rust core has a separate
[docs.rs API reference ↗](https://docs.rs/h11r).
