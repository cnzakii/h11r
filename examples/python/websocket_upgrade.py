"""Hand an upgraded HTTP/1.1 connection from h11r to wsproto.

h11r owns the HTTP request, the 101 response, and the exact byte where HTTP
ends. wsproto owns every WebSocket frame after that boundary. Fixed byte strings
stand in for transport reads and writes so the handoff remains the focus.

This example validates the essential WebSocket request fields. A production
server must also apply its origin and authentication policy, timeouts, and
connection shutdown.
"""

from __future__ import annotations

import base64
import binascii
import hashlib

import h11r
from wsproto.connection import Connection as WebSocketConnection
from wsproto.connection import ConnectionType
from wsproto.events import TextMessage

WEBSOCKET_GUID = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11"


def single_value(head: h11r.Request | h11r.InformationalResponse, name: str) -> bytes:
    """Return the value of a header field that must appear exactly once."""
    values = head.headers.get_all(name)
    if len(values) != 1:
        raise ValueError(f"expected exactly one {name} header")
    return values[0]


def has_token(
    head: h11r.Request | h11r.InformationalResponse, name: str, token: bytes
) -> bool:
    """Match a token in a comma-separated field across all of its lines."""
    return any(
        token in {part.strip().lower() for part in value.split(b",")}
        for value in head.headers.get_all(name)
    )


def websocket_accept(request: h11r.Request) -> bytes:
    """Validate the essential WebSocket request fields and create the accept."""
    if request.method != b"GET" or request.http_version != b"1.1":
        raise ValueError("WebSocket over HTTP/1.1 requires a GET request")

    version = single_value(request, "Sec-WebSocket-Version")
    key = single_value(request, "Sec-WebSocket-Key")
    offered_upgrade = has_token(request, "Connection", b"upgrade") and has_token(
        request, "Upgrade", b"websocket"
    )

    if not offered_upgrade:
        raise ValueError("request did not offer a WebSocket upgrade")
    if version != b"13":
        raise ValueError("only WebSocket version 13 is supported")

    try:
        decoded_key = base64.b64decode(key, validate=True)
    except binascii.Error as error:
        raise ValueError("Sec-WebSocket-Key is not valid base64") from error
    if len(decoded_key) != 16:
        raise ValueError("Sec-WebSocket-Key must decode to 16 bytes")

    return base64.b64encode(hashlib.sha1(key + WEBSOCKET_GUID).digest())


def next_supplied_event(connection: h11r.Connection) -> object:
    """Return the next event when the complete input is already supplied."""
    event = connection.next_event()
    if event is h11r.ReceiveStatus.NEED_DATA:
        raise RuntimeError("the example did not supply enough transport bytes")
    return event


def main() -> None:
    key = b"dGhlIHNhbXBsZSBub25jZQ=="
    client_http = h11r.Connection(h11r.Role.CLIENT)
    server_http = h11r.Connection(h11r.Role.SERVER)

    # The client produces HTTP request bytes; a real transport would write them.
    request_wire = client_http.send_request(
        "GET",
        "/chat",
        [
            ("Host", "example.test"),
            ("Connection", "Upgrade"),
            ("Upgrade", "websocket"),
            ("Sec-WebSocket-Key", key),
            ("Sec-WebSocket-Version", "13"),
        ],
    )
    request_wire += client_http.end_of_message()

    # The server receives those bytes and decides whether to accept the switch.
    server_http.receive_data(request_wire)
    request = next_supplied_event(server_http)
    message_end = next_supplied_event(server_http)
    boundary = next_supplied_event(server_http)

    if not isinstance(request, h11r.Request):
        raise RuntimeError(f"expected Request, got {request!r}")
    if not isinstance(message_end, h11r.EndOfMessage):
        raise RuntimeError(f"expected EndOfMessage, got {message_end!r}")
    if boundary is not h11r.ReceiveStatus.PAUSED:
        raise RuntimeError(f"expected Upgrade boundary, got {boundary!r}")

    accept = websocket_accept(request)
    print(f"server accepted Upgrade request for {request.target.decode()}")

    switch_wire = server_http.send_informational_response(
        101,
        [
            ("Connection", "Upgrade"),
            ("Upgrade", "websocket"),
            ("Sec-WebSocket-Accept", accept),
        ],
        reason="Switching Protocols",
    )

    # The first WebSocket frame can arrive in the same read as the HTTP 101.
    server_websocket = WebSocketConnection(ConnectionType.SERVER)
    welcome_frame = server_websocket.send(TextMessage(data="welcome"))
    client_http.receive_data(switch_wire + welcome_frame)

    switch = next_supplied_event(client_http)
    boundary = next_supplied_event(client_http)
    if not isinstance(switch, h11r.InformationalResponse):
        raise RuntimeError(f"expected 101 response, got {switch!r}")
    if switch.status_code != 101:
        raise RuntimeError(f"expected status 101, got {switch.status_code}")
    expected_accept = base64.b64encode(hashlib.sha1(key + WEBSOCKET_GUID).digest())
    if single_value(switch, "Sec-WebSocket-Accept") != expected_accept:
        raise ValueError("server returned an invalid Sec-WebSocket-Accept")
    if not has_token(switch, "Connection", b"upgrade") or not has_token(
        switch, "Upgrade", b"websocket"
    ):
        raise ValueError("server did not confirm the WebSocket upgrade")
    if boundary is not h11r.ReceiveStatus.PAUSED:
        raise RuntimeError(f"expected switch boundary, got {boundary!r}")

    # Give retained post-HTTP bytes to wsproto before another transport read.
    trailing_data, transport_closed = client_http.trailing_data
    if transport_closed:
        raise ConnectionError("transport closed at the Upgrade boundary")

    client_websocket = WebSocketConnection(
        ConnectionType.CLIENT,
        trailing_data=trailing_data,
    )
    events = list(client_websocket.events())
    if len(events) != 1 or not isinstance(events[0], TextMessage):
        raise RuntimeError(f"expected one WebSocket text event, got {events!r}")

    print(f"client received WebSocket text: {events[0].data!r}")


if __name__ == "__main__":
    main()
