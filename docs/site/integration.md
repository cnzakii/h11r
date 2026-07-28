---
description: Connect h11r to an existing synchronous or asynchronous byte transport.
---

# Build a transport adapter

Use this page when a client, server, proxy, or framework already owns a socket
or byte stream. Because `h11r` is Sans-I/O, it never reads from or writes to
that transport itself. The adapter between them has four jobs:

- read only when `h11r` returns `NEED_DATA`;
- pass transport EOF to `receive_data(b"")`;
- deliver protocol events to the application;
- write every byte returned by `h11r` in order.

If those rules are new, run the [complete round trip](round-trip.md) and read
[Core concepts](concepts.md) first.

The receive loop is the same for both roles. The message-dispatch and write
snippets below use the server's point of view; a client handles `Response`
events and calls `send_request()` under the same transport rules.

The snippets use placeholders supplied by your application:

| Name | What your application supplies |
| --- | --- |
| `connection` | One `h11r.Connection` for this transport endpoint |
| `read()` | A synchronous transport read that returns `bytes` and returns `b""` at EOF |
| `write_all(data)` | A write operation that preserves all bytes and their order |
| `process_body_chunk()` and `process_trailers()` | Your request-handling callbacks |
| `body` | Response bytes chosen by your application |

## Receive from a pull-style transport

For a synchronous stream, the receive loop can follow this shape:

```python
from collections.abc import Callable

import h11r


def next_event(
    connection: h11r.Connection,
    read: Callable[[], bytes],
) -> object:
    while True:
        event = connection.next_event()

        if event is h11r.ReceiveStatus.NEED_DATA:
            connection.receive_data(read())
            continue

        return event
```

The `read` callback must return `b""` at EOF. Handle `PAUSED` in the layer that
owns connection reuse or protocol handoff; do not turn it into another read.

An asynchronous adapter uses the same protocol loop and awaits only where this
example calls `read()`.

## Receive into recycled storage

Keep `receive_data()` as the direct path when a transport already returns
`bytes`. When a transport can fill caller-owned storage, `receive_buffer()`
avoids both that fresh Python `bytes` allocation and the safety copy required
for an arbitrary mutable exporter:

```python
import socket

import h11r


def receive_into(connection: h11r.Connection, transport: socket.socket) -> None:
    with connection.receive_buffer(64 * 1024) as receive_buffer:
        received = transport.recv_into(receive_buffer)
        receive_buffer.commit(received)
```

Creating the lease reserves the connection. Entering the context calls
`acquire()` and enables writable buffer exports. `commit(received)` passes the
initialized prefix to the protocol engine and returns the storage for reuse;
`commit(0)` records the same transport EOF as `receive_data(b"")`. If the read
raises or the block exits before commit, context cleanup calls `abort()`.

Do not retain a `memoryview` or another export after the transport operation.
`commit()` raises `BufferError` while an export remains. An abort with an
escaped export stays pending and keeps the connection reserved until the final
export is released. While reserved, state-changing connection methods raise
`RuntimeError`; state properties remain readable.

### Map `asyncio.BufferedProtocol` callbacks

An asyncio buffered protocol keeps the lease returned from `get_buffer()` and
commits it in `buffer_updated()`:

```python
import asyncio

import h11r


class Protocol(asyncio.BufferedProtocol):
    def __init__(self) -> None:
        self.connection = h11r.Connection(h11r.Role.SERVER)
        self.pending: h11r.ReceiveBuffer | None = None

    def get_buffer(self, sizehint: int) -> h11r.ReceiveBuffer:
        if self.pending is None:
            size = sizehint if sizehint > 0 else 64 * 1024
            self.pending = self.connection.receive_buffer(size).acquire()
        return self.pending

    def buffer_updated(self, nbytes: int) -> None:
        if self.pending is None:
            raise RuntimeError("buffer_updated without get_buffer")
        receive_buffer = self.pending
        self.pending = None
        receive_buffer.commit(nbytes)
        self.drain_http_events()

    def eof_received(self) -> None:
        if self.pending is None:
            self.pending = self.connection.receive_buffer(1).acquire()
        receive_buffer = self.pending
        self.pending = None
        receive_buffer.commit(0)
        self.drain_http_events()

    def connection_lost(self, exc: Exception | None) -> None:
        if self.pending is not None:
            self.pending.abort()
            self.pending = None
```

Here, `drain_http_events()` is the adapter's event loop, not an h11r method.
`connection_lost()` aborts an abandoned lease because asyncio may close a
transport without calling `buffer_updated()` or `eof_received()` for the
pending buffer.

## Dispatch one complete message

Keep the message head separate from body fragments and finish only at
`EndOfMessage`:

```python
request = None

while True:
    event = next_event(connection, read)

    if isinstance(event, h11r.Request):
        request = event
    elif isinstance(event, h11r.Data):
        process_body_chunk(event.data)
    elif isinstance(event, h11r.EndOfMessage):
        if request is None:
            raise RuntimeError("message ended before its request head")
        process_trailers(event.trailers)
        break
    elif isinstance(event, h11r.ConnectionClosed):
        raise ConnectionError("peer closed before the message completed")
    elif event is h11r.ReceiveStatus.PAUSED:
        raise RuntimeError("HTTP processing paused before message completion")
```

The two `process_*` calls are the application callbacks named in the table
above; they are not functions provided by `h11r`.

### Opt into bounded full-body collection

When an application requires one contiguous body, expose the request head
first, handle `100 Continue` if needed, then create a collector:

```python
request = next_event(connection, read)
if not isinstance(request, h11r.Request):
    raise RuntimeError("expected a request head")

if connection.client_is_waiting_for_100_continue:
    write_all(connection.send_informational_response(100))

collector = connection.collect_body(max_bytes=1024 * 1024)
while True:
    body = collector.next()
    if body is h11r.ReceiveStatus.NEED_DATA:
        connection.receive_data(read())
        continue
    if not isinstance(body, h11r.CollectedBody):
        raise RuntimeError("unexpected collector result")
    break
```

This is opt-in: ordinary `next_event()` continues to expose streaming `Data`
and `EndOfMessage` events. While collection is active, feed input with either
`receive_data()` or `receive_buffer()`, and poll the collector rather than
`next_event()`.

## Write to the transport

Every sending method returns bytes for the transport. The `write_all()`
placeholder below must preserve the entire value and its order:

```python
write_all(
    connection.send_response(
        200,
        [("Content-Length", str(len(body)))],
        reason="OK",
    )
)
write_all(connection.send_data(body))
write_all(connection.end_of_message())
```

Implement `write_all()` with the transport's complete-write operation or a
loop around partial writes. For transports with back-pressure, wait according
to the transport's contract. Do not let another task mutate the same
connection between these ordered operations.

## Complete integration examples

| Goal | Example |
| --- | --- |
| Follow a client/server exchange over a local stream | [`round_trip.py` ↗](https://github.com/cnzakii/h11r/blob/{{ git.commit }}/examples/python/round_trip.py) |
| Build a collector-based teaching server with `asyncio` streams | [`asyncio_server.py` ↗](https://github.com/cnzakii/h11r/blob/{{ git.commit }}/examples/python/asyncio_server.py) |
| Use `recv_into()` with a receive lease | [`receive_buffer_server.py` ↗](https://github.com/cnzakii/h11r/blob/{{ git.commit }}/examples/python/receive_buffer_server.py) |
| Implement `asyncio.BufferedProtocol` callbacks | [`asyncio_buffered_server.py` ↗](https://github.com/cnzakii/h11r/blob/{{ git.commit }}/examples/python/asyncio_buffered_server.py) |

Run any example from a repository checkout by replacing the filename in this
command:

```console
uv run python examples/python/round_trip.py
```

## Adapter checklist

Before treating an adapter as complete, confirm that it:

- creates one connection per transport endpoint;
- drains buffered events before another read;
- passes EOF with `receive_data(b"")` or `ReceiveBuffer.commit(0)`, and aborts
  abandoned leases;
- handles every event and receive status possible for its role;
- writes all send results in order;
- applies application body, timeout, and concurrency limits;
- calls `start_next_cycle()` only when reuse is legal;
- transfers `trailing_data` when HTTP hands off to another protocol;
- catches `RemoteProtocolError` separately from local API misuse.

For streaming bodies, buffer-preserving writes, pipelining, and protocol
handoff, continue to [Advanced usage](advanced.md).
