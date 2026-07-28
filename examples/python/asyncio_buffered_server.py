"""One-request ``asyncio.BufferedProtocol`` server using ``ReceiveBuffer``.

Run it from the repository root, then make a request from another terminal::

    uv run python examples/python/asyncio_buffered_server.py
    curl -v http://127.0.0.1:8080/

``get_buffer()`` acquires a receive lease and returns a writable
``memoryview``. ``buffer_updated()`` releases that view before commit, EOF
commits zero bytes, and connection loss releases and aborts an abandoned lease.
"""

from __future__ import annotations

import argparse
import asyncio
from http import HTTPStatus

import h11r

READ_SIZE = 64 * 1024
MAX_REQUEST_BODY = 1024 * 1024
OK_BODY = b"hello from h11r\n"
NOT_FOUND_BODY = b"not found\n"
BAD_REQUEST_BODY = b"bad request\n"
TOO_LARGE_BODY = b"request body too large\n"


class BufferedHTTPProtocol(asyncio.BufferedProtocol):
    """Join one h11r server connection to one asyncio byte transport."""

    def __init__(self) -> None:
        self.connection = h11r.Connection(h11r.Role.SERVER)
        self.transport: asyncio.Transport | None = None
        self.pending: h11r.ReceiveBuffer | None = None
        self.pending_view: memoryview | None = None
        self.request: h11r.Request | None = None
        self.collector: h11r.BodyCollector | None = None
        self.responded = False

    def connection_made(self, transport: asyncio.BaseTransport) -> None:
        if not isinstance(transport, asyncio.Transport):
            raise TypeError("BufferedHTTPProtocol requires a byte transport")
        self.transport = transport

    def get_buffer(self, sizehint: int) -> memoryview:
        if self.pending is None:
            size = sizehint if sizehint > 0 else READ_SIZE
            self.pending = self.connection.receive_buffer(size).acquire()
            self.pending_view = memoryview(self.pending)
        if self.pending_view is None:
            raise RuntimeError("active receive lease has no writable view")
        return self.pending_view

    def buffer_updated(self, nbytes: int) -> None:
        if self.pending is None or self.pending_view is None:
            raise RuntimeError("buffer_updated() called without get_buffer()")
        receive_buffer = self.pending
        self.pending_view.release()
        self.pending_view = None
        self.pending = None
        receive_buffer.commit(nbytes)
        self._drain_events()

    def eof_received(self) -> None:
        if self.pending is None:
            self.pending = self.connection.receive_buffer(1).acquire()
        elif self.pending_view is None:
            raise RuntimeError("active receive lease has no writable view")
        if self.pending_view is not None:
            self.pending_view.release()
            self.pending_view = None
        receive_buffer = self.pending
        self.pending = None
        receive_buffer.commit(0)
        self._drain_events()

    def connection_lost(self, exc: Exception | None) -> None:
        if self.pending_view is not None:
            self.pending_view.release()
            self.pending_view = None
        if self.pending is not None:
            self.pending.abort()
            self.pending = None
        self.transport = None

    def _drain_events(self) -> None:
        try:
            while not self.responded:
                if self.collector is not None:
                    body = self.collector.next()
                    if body is h11r.ReceiveStatus.NEED_DATA:
                        return
                    if not isinstance(body, h11r.CollectedBody):
                        raise RuntimeError(f"unexpected collector result: {body!r}")
                    if self.request is None:
                        raise RuntimeError("body completed before its Request event")
                    self.collector = None

                    if self.request.method == b"GET" and self.request.target == b"/":
                        self._respond(HTTPStatus.OK, OK_BODY)
                    else:
                        self._respond(HTTPStatus.NOT_FOUND, NOT_FOUND_BODY)
                    continue

                event = self.connection.next_event()
                if event is h11r.ReceiveStatus.NEED_DATA:
                    return
                if isinstance(event, h11r.Request):
                    self.request = event
                    if self.connection.client_is_waiting_for_100_continue:
                        self._write(
                            self.connection.send_informational_response(
                                100,
                                reason="Continue",
                            )
                        )
                    self.collector = self.connection.collect_body(
                        max_bytes=MAX_REQUEST_BODY
                    )
                elif isinstance(event, h11r.ConnectionClosed):
                    if self.transport is not None:
                        self.transport.close()
                    return
                elif event is h11r.ReceiveStatus.PAUSED:
                    raise RuntimeError("HTTP parsing paused before request completion")
                else:
                    raise RuntimeError(f"unexpected server event: {event!r}")
        except h11r.BodyTooLarge:
            self._respond(
                HTTPStatus.REQUEST_ENTITY_TOO_LARGE,
                TOO_LARGE_BODY,
            )
        except h11r.RemoteProtocolError:
            self._respond(HTTPStatus.BAD_REQUEST, BAD_REQUEST_BODY)

    def _write(self, data: bytes) -> None:
        if self.transport is not None:
            self.transport.write(data)

    def _respond(self, status: HTTPStatus, body: bytes) -> None:
        if self.transport is None:
            return
        response = self.connection.send_response(
            status,
            [
                ("Content-Length", str(len(body))),
                ("Content-Type", "text/plain; charset=utf-8"),
                ("Connection", "close"),
            ],
            reason=status.phrase,
        )
        response += self.connection.send_data(body)
        response += self.connection.end_of_message()
        self.responded = True
        self.transport.write(response)
        self.transport.close()


async def serve(host: str, port: int) -> None:
    loop = asyncio.get_running_loop()
    server = await loop.create_server(BufferedHTTPProtocol, host, port)
    addresses = ", ".join(str(sock.getsockname()) for sock in server.sockets or [])
    print(f"serving on {addresses}; press Ctrl+C to stop")
    async with server:
        await server.serve_forever()


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Run the h11r asyncio BufferedProtocol server."
    )
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8080)
    arguments = parser.parse_args()

    try:
        asyncio.run(serve(arguments.host, arguments.port))
    except KeyboardInterrupt:
        print("server stopped")


if __name__ == "__main__":
    main()
