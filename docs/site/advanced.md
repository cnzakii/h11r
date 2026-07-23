---
description: Use h11r for streaming bodies, buffer-preserving writes, pipelining, and protocol handoff.
---

# Advanced usage

Start with ordinary send methods, process received events in order, and advance
each completed connection cycle. The features on this page solve narrower
transport and protocol-boundary problems; use one only when your application
has the matching need.

| Need | h11r feature |
| --- | --- |
| Process a body without collecting it | `Data` events and incremental `send_data()` calls |
| Avoid copying a large existing body buffer inside `h11r` | `send_data_parts()` |
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

## Preserve an existing body buffer

`send_data()` returns one convenient `bytes` object containing the body and any
required framing. For a large, contiguous body that already exists in memory,
`send_data_parts()` can keep that body object separate:

```python
prefix, original_body, suffix = connection.send_data_parts(body)

write_all(prefix)
write_all(original_body)
write_all(suffix)
```

This avoids one body copy inside `h11r`, which can materially reduce the
protocol-layer serialization cost for a large body. Keep `send_data()` as the
simpler default for small bodies and measure the complete write path on the
transport you actually use.

[Inspect the 64 KiB body benchmark ↗](https://github.com/cnzakii/h11r/blob/{{ git.commit }}/crates/h11r/benches/h11r.rs)

This is buffer preservation at the `h11r` boundary, not an end-to-end zero-copy
guarantee. Keep `original_body` alive and unchanged until all writes complete,
and preserve the returned order. Use this path only when the transport can
write or batch separate buffers without first combining them.
Here, `write_all()` is the complete-write operation supplied by your
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
