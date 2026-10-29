# h11r

A Sans-I/O HTTP/1.1 protocol engine. `h11r` turns HTTP messages into wire
bytes and peer bytes into events, enforcing message framing and connection
state. It never touches a socket: your code owns the transport, runtime,
timeouts, and application policy.

This crate is the Rust core of the [`h11r` Python package][python-docs]. It
supports client and server roles, HTTP/1.0 peers, `Content-Length` and chunked
framing, keep-alive, informational responses, trailers, pipelining, and
protocol handoff after Upgrade or CONNECT.

> `h11r` is alpha software. Its public API may change.

## Example

One `Connection` represents one endpoint. Send methods return bytes for your
transport to write; bytes your transport reads go to `receive_data`, and
`next_event` turns them into events.

```rust
use h11r::{
    Connection, Event, Headers, Limits, Method, NextEvent, Request, Response, Role, StatusCode,
    Version,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = Connection::new(Role::Client, Limits::default());
    let mut server = Connection::new(Role::Server, Limits::default());

    // The client serializes a request head and its (empty) body.
    let mut wire = client.send_request(&Request {
        method: Method::GET,
        target: b"/hello".to_vec(),
        headers: Headers::from([("Host", "example.test")]),
        http_version: Version::Http11,
    })?;
    wire.extend(client.end_of_message(&Headers::new())?);

    // The server parses the bytes its transport read.
    server.receive_data(&wire)?;
    let NextEvent::Event(Event::Request(request)) = server.next_event()? else {
        panic!("expected a request head");
    };
    assert_eq!(request.target, b"/hello");
    assert!(matches!(
        server.next_event()?,
        NextEvent::Event(Event::EndOfMessage(_))
    ));

    // A complete response in one call; `Content-Length` is added for the body.
    let wire = server.send_response_with_body(
        Response {
            status: StatusCode::from_u16(200)?,
            reason: b"OK".to_vec(),
            headers: Headers::from([("Content-Type", "text/plain")]),
            http_version: Version::Http11,
        },
        b"hello",
    )?;

    // A body can arrive as any number of `Data` events.
    client.receive_data(&wire)?;
    let mut body = Vec::new();
    loop {
        match client.next_event()? {
            NextEvent::Event(Event::Response(response)) => {
                assert_eq!(response.status.as_u16(), 200);
            }
            NextEvent::Event(Event::Data(data)) => body.extend_from_slice(data.data),
            NextEvent::Event(Event::EndOfMessage(_)) => break,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!(body, b"hello");

    // Both endpoints may carry another exchange on the same connection.
    assert!(client.keep_alive() && server.keep_alive());
    client.start_next_cycle()?;
    server.start_next_cycle()?;
    Ok(())
}
```

## Driving a connection

- `next_event` returns `NextEvent::NeedData` when the next event needs more
  bytes: read from the transport, then call `receive_data`. Pass an empty slice
  at transport EOF so a clean close is distinguished from a truncated message.
- `NextEvent::Paused` means more bytes cannot help: finish the current exchange
  and call `start_next_cycle`, or hand `trailing_data` to the protocol selected
  by Upgrade or CONNECT.
- Write every byte returned by a send method, in order, before the next send.
  `send_request_with_body` and `send_response_with_body` return a complete
  message; otherwise follow a head with `send_data` and `end_of_message`.
- `Headers` keeps fields in order with their original casing; build it from
  pairs such as `Headers::from([("Host", "example.test")])`.
- `RemoteProtocolError` reports invalid peer input. A server can answer with
  `suggested_status_code` and then close. `LocalProtocolError` reports an
  operation the current state or framing does not allow.
- When `keep_alive` is `false`, close the transport after the exchange.

`Limits` bound each inbound head or trailer section. Body size, timeouts, and
read back-pressure remain the caller's policy.

The [documentation site][python-docs] explains the protocol model in depth; its
examples use the Python API, which mirrors these Rust types.

[python-docs]: https://h11r.readthedocs.io/
