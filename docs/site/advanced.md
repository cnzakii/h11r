---
description: Use h11r for streaming bodies, passthrough writes, pipelining, and protocol handoff.
---

# Advanced usage

Start with ordinary send methods, process received events in order, and advance
each completed connection cycle. The features on this page solve narrower
transport and protocol-boundary problems; use one only when your application
has the matching need.

| Need | h11r feature |
| --- | --- |
| Process a body without collecting it | `Data` events and incremental `send_data()` calls |
| Let a transport fill recycled receive storage | `receive_buffer(size)` |
| Collect one bounded body contiguously | `collect_body(max_bytes=...)` |
| Pass a transport-owned body object through `h11r` | `send_data_parts()` |
| Accept queued requests without reordering responses | `PAUSED` and `start_next_cycle()` |
| Continue with WebSocket, CONNECT, or another selected protocol | `trailing_data` |

The linked programs are complete runnable examples in the repository. To run
them, clone the repository once and install its locked environment:

```console
git clone https://github.com/cnzakii/h11r.git
cd h11r
uv sync --locked
```

## Stream bodies incrementally

Send each application chunk as it becomes available instead of joining the
whole body first. On receive, process every `Data` event and finish at
`EndOfMessage`; network reads and HTTP body chunks do not have a one-to-one
relationship.

Trailers arrive on `EndOfMessage`, so checksums and other trailing metadata can
be validated after the final body chunk without buffering the entire body.

Run:

```console
uv run python examples/python/streaming_body.py
```

Success ends with `streamed 36 bytes without collecting the body`.

[Read `streaming_body.py` ↗](https://github.com/cnzakii/h11r/blob/{{ git.commit }}/examples/python/streaming_body.py)

## Recycle receive storage

When a transport supports caller-provided storage, acquire a connection-owned
receive lease, fill it, and commit only the initialized prefix:

```python
with connection.receive_buffer(64 * 1024) as receive_buffer:
    received = transport.recv_into(receive_buffer)
    receive_buffer.commit(received)
```

A zero-byte commit records EOF, just as `receive_data(b"")` does. If the
transport operation fails or connection loss abandons a pending lease, abort
it so the connection reservation is released. Do not retain a `memoryview` or
other buffer export across `commit()`.

The synchronous teaching adapter applies this pattern to `socket.recv_into()`;
the buffered asyncio adapter maps lease acquisition, commit, EOF, and abort to
the corresponding `asyncio.BufferedProtocol` callbacks. Both collect one
bounded request body, answer once, and close.

[Read `receive_buffer_server.py` ↗](https://github.com/cnzakii/h11r/blob/{{ git.commit }}/examples/python/receive_buffer_server.py) ·
[Read `asyncio_buffered_server.py` ↗](https://github.com/cnzakii/h11r/blob/{{ git.commit }}/examples/python/asyncio_buffered_server.py)

## Collect one bounded body

After `next_event()` returns a `Request` or final `Response`, opt into
contiguous collection before polling another event:

```python
collector = connection.collect_body(max_bytes=1024 * 1024)

while True:
    result = collector.next()
    if result is h11r.ReceiveStatus.NEED_DATA:
        connection.receive_data(read())
        continue
    body = result
    break

process(body.data, body.trailers)
```

The collector consumes `Data` events internally and moves trailers from
`EndOfMessage` into `CollectedBody.trailers`. `body.data` is a read-only,
byte-sized, C-contiguous `memoryview` over private storage. It remains valid
independently of the connection; call `bytes(body.data)` only when the
application needs a Python `bytes` copy.

`max_bytes` is required. Exceeding it raises `BodyTooLarge` with `max_bytes`
and `observed_bytes` attributes. Stop receiving and send an error response or
close the connection after a late abort or collection failure. Applications
that can process incrementally should keep using `Data` events so memory use
does not scale with the complete body.

The complete asyncio streams example exposes each request head, sends
`100 Continue` when needed, and then uses this collector with a one-megabyte
application limit.

[Read `asyncio_server.py` ↗](https://github.com/cnzakii/h11r/blob/{{ git.commit }}/examples/python/asyncio_server.py)

## Pass through a transport-owned body

`send_data()` returns one convenient `bytes` object containing the body and any
required framing. `send_data_parts()` instead determines framing from the
body's byte length and returns the original object separately:

```python
region = FileRegion(file, offset=offset, nbytes=length)
prefix, original_region, suffix = connection.send_data_parts(region)

write_all(prefix)
send_file_region(original_region)
write_all(suffix)
```

Contiguous buffers use their full `nbytes`; other objects expose an integer
`nbytes` property declaring the exact number of bytes they represent. This
lets a transport-owned file-region proxy pass through unchanged for
`socket.sendfile()` without giving `h11r` ownership of the file or its
transmission. Keep `send_data()` as the simpler default for small bodies.

[Inspect the 64 KiB body benchmark ↗](https://github.com/cnzakii/h11r/blob/{{ git.commit }}/crates/h11r/benches/h11r.rs)

Write the prefix, exactly the declared number of body bytes, and the suffix in
order. Resume any partial body write before sending the suffix; if that is
impossible, discard the connection because `h11r` has already accounted for
the declared bytes. Actual kernel zero-copy depends on the transport and
operating system. Here, `write_all()` and `send_file_region()` are
complete-write operations supplied by the
[transport adapter](integration.md#write-to-the-transport).

Run:

```console
uv run python examples/python/zero_copy_body.py
```

Success ends with `upload exchange is complete and the connection is reusable`.

[Read `zero_copy_body.py` ↗](https://github.com/cnzakii/h11r/blob/{{ git.commit }}/examples/python/zero_copy_body.py)

## Handle pipelined requests

A peer can send another request before receiving the current response. `h11r`
keeps the later request buffered and returns `PAUSED` until the current
response is complete. Call `start_next_cycle()` only then; the next request
becomes visible without allowing responses to be reordered.

Run:

```console
uv run python examples/python/pipelining.py
```

Success ends with `both pipelined responses were sent in request order`.

[Read `pipelining.py` ↗](https://github.com/cnzakii/h11r/blob/{{ git.commit }}/examples/python/pipelining.py)

## Protocol handoff

After a successful Upgrade or CONNECT switch, HTTP processing stops at an
exact byte boundary. Read `trailing_data` and pass any retained bytes to the
selected protocol before performing another transport read. Subsequent bytes
belong to that protocol, not to `h11r`.

The example below validates the essential WebSocket request fields, performs
the HTTP Upgrade, and transfers an already received WebSocket frame to
`wsproto`. It demonstrates ownership of the byte boundary; a production
integration must also apply its origin and authentication policy, timeouts,
and connection lifecycle.

Run:

```console
uv run python examples/python/websocket_upgrade.py
```

Success ends with `client received WebSocket text: 'welcome'`.

[Read `websocket_upgrade.py` ↗](https://github.com/cnzakii/h11r/blob/{{ git.commit }}/examples/python/websocket_upgrade.py)

For exact method signatures and failure conditions, use the
[Python API reference](api.md).
