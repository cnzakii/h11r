"""One-request synchronous server using ``recv_into()`` and ``ReceiveBuffer``.

Run it from the repository root, then make a request from another terminal::

    uv run python examples/python/receive_buffer_server.py
    curl -v http://127.0.0.1:8080/

The example deliberately sends ``Connection: close`` after one request.
Production servers also need timeouts, concurrency, logging, graceful
shutdown, application-specific routing, and usually TLS.
"""

from __future__ import annotations

import argparse
import socket
from http import HTTPStatus

import h11r

READ_SIZE = 64 * 1024
MAX_REQUEST_BODY = 1024 * 1024
OK_BODY = b"hello from h11r\n"
NOT_FOUND_BODY = b"not found\n"
BAD_REQUEST_BODY = b"bad request\n"
TOO_LARGE_BODY = b"request body too large\n"


def send_response(
    transport: socket.socket,
    connection: h11r.Connection,
    status: HTTPStatus,
    body: bytes,
) -> None:
    """Send one complete response and declare the adapter's close policy."""
    transport.sendall(
        connection.send_response(
            status,
            [
                ("Content-Length", str(len(body))),
                ("Content-Type", "text/plain; charset=utf-8"),
                ("Connection", "close"),
            ],
            reason=status.phrase,
        )
    )
    if body:
        transport.sendall(connection.send_data(body))
    transport.sendall(connection.end_of_message())


def receive_once(
    connection: h11r.Connection,
    transport: socket.socket,
) -> None:
    """Commit one ``recv_into()`` result, including a zero-byte EOF."""
    with connection.receive_buffer(READ_SIZE) as receive_buffer:
        received = transport.recv_into(receive_buffer)
        receive_buffer.commit(received)


def handle_client(transport: socket.socket) -> None:
    """Read and answer one request on an accepted socket."""
    connection = h11r.Connection(h11r.Role.SERVER)

    try:
        while True:
            event = connection.next_event()
            if event is h11r.ReceiveStatus.NEED_DATA:
                receive_once(connection, transport)
            elif isinstance(event, h11r.Request):
                request = event
                if connection.client_is_waiting_for_100_continue:
                    transport.sendall(
                        connection.send_informational_response(
                            100,
                            reason="Continue",
                        )
                    )

                collector = connection.collect_body(max_bytes=MAX_REQUEST_BODY)
                while True:
                    body = collector.next()
                    if body is h11r.ReceiveStatus.NEED_DATA:
                        receive_once(connection, transport)
                        continue
                    if not isinstance(body, h11r.CollectedBody):
                        raise RuntimeError(f"unexpected collector result: {body!r}")
                    break

                if request.method == b"GET" and request.target == b"/":
                    send_response(transport, connection, HTTPStatus.OK, OK_BODY)
                else:
                    send_response(
                        transport,
                        connection,
                        HTTPStatus.NOT_FOUND,
                        NOT_FOUND_BODY,
                    )
                return
            elif isinstance(event, h11r.ConnectionClosed):
                return
            elif event is h11r.ReceiveStatus.PAUSED:
                raise RuntimeError("HTTP parsing paused before request completion")
            else:
                raise RuntimeError(f"unexpected server event: {event!r}")
    except h11r.BodyTooLarge:
        send_response(
            transport,
            connection,
            HTTPStatus.REQUEST_ENTITY_TOO_LARGE,
            TOO_LARGE_BODY,
        )
    except h11r.RemoteProtocolError:
        send_response(
            transport,
            connection,
            HTTPStatus.BAD_REQUEST,
            BAD_REQUEST_BODY,
        )


def serve_once(listener: socket.socket) -> None:
    """Accept and serve exactly one connection, primarily for tests."""
    transport, _address = listener.accept()
    with transport:
        handle_client(transport)


def serve(host: str, port: int) -> None:
    with socket.create_server((host, port), reuse_port=False) as listener:
        print(f"serving on {listener.getsockname()}; press Ctrl+C to stop")
        while True:
            serve_once(listener)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Run the h11r ReceiveBuffer synchronous server."
    )
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8080)
    arguments = parser.parse_args()

    try:
        serve(arguments.host, arguments.port)
    except KeyboardInterrupt:
        print("server stopped")


if __name__ == "__main__":
    main()
