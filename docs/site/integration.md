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

## Apply read back-pressure

An adapter that pushes bytes in as they arrive, rather than reading only on
`NEED_DATA`, accepts them at the peer's pace. When the application consumes
body fragments more slowly than they arrive, that backlog grows to the whole
body. `buffered_bytes` reports it as a count, so an adapter can consult it
after every `receive_data()` without copying the bytes it is measuring:

```python
connection.receive_data(data)

if connection.buffered_bytes >= high_water:
    transport.pause_reading()
```

Resume once `next_event()` has brought the count back under a lower mark.
Parsing removes bytes from the count as events consume them, so an adapter
needs no accounting of its own. Choose both marks from the application's memory
budget: `h11r` imposes none, and `max_head_bytes` bounds only an inbound head
or trailer section.

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
| Build a teaching server with `asyncio` streams | [`asyncio_server.py` ↗](https://github.com/cnzakii/h11r/blob/{{ git.commit }}/examples/python/asyncio_server.py) |

Run any example from a repository checkout by replacing the filename in this
command:

```console
uv run python examples/python/round_trip.py
```

## Adapter checklist

Before treating an adapter as complete, confirm that it:

- creates one connection per transport endpoint;
- drains buffered events before another read;
- passes `b""` to `receive_data()` at EOF;
- handles every event and receive status possible for its role;
- writes all send results in order;
- applies application body, timeout, and concurrency limits;
- bounds its own receive backlog with `buffered_bytes` when it pushes bytes in
  rather than reading on `NEED_DATA`;
- calls `start_next_cycle()` only when reuse is legal;
- transfers `trailing_data` when HTTP hands off to another protocol;
- catches `RemoteProtocolError` separately from local API misuse.

For streaming bodies, buffer-preserving writes, pipelining, and protocol
handoff, continue to [Advanced usage](advanced.md).
