from __future__ import annotations

import asyncio
import runpy
import socket
from pathlib import Path
from threading import Thread

import h11r
import pytest

EXAMPLES = [
    ("first_client.py", "client received 200 with b'Hello from h11r!\\n'"),
    ("round_trip.py", "connection is ready for another request"),
    ("streaming_body.py", "streamed 36 bytes without collecting the body"),
    ("pipelining.py", "both pipelined responses were sent in request order"),
    (
        "zero_copy_body.py",
        "upload exchange is complete and the connection is reusable",
    ),
    ("websocket_upgrade.py", "client received WebSocket text: 'welcome'"),
]


@pytest.mark.parametrize(
    ("filename", "expected_output"),
    EXAMPLES,
    ids=[
        "first-client",
        "round-trip",
        "streaming-body",
        "pipelining",
        "zero-copy",
        "websocket-upgrade",
    ],
)
def test_python_example_runs(
    filename: str,
    expected_output: str,
    capsys: pytest.CaptureFixture[str],
) -> None:
    example = Path(__file__).parents[3] / "examples" / "python" / filename

    runpy.run_path(str(example), run_name="__main__")

    assert expected_output in capsys.readouterr().out


async def next_async_event(
    connection: h11r.Connection,
    reader: asyncio.StreamReader,
) -> object:
    while True:
        event = connection.next_event()
        if event is h11r.ReceiveStatus.NEED_DATA:
            connection.receive_data(await reader.read(64 * 1024))
            continue
        return event


async def receive_final_response(
    connection: h11r.Connection,
    reader: asyncio.StreamReader,
) -> tuple[h11r.Response, bytes]:
    response: h11r.Response | None = None
    body = bytearray()

    while True:
        event = await next_async_event(connection, reader)
        if isinstance(event, h11r.InformationalResponse):
            continue
        if isinstance(event, h11r.Response):
            response = event
        elif isinstance(event, h11r.Data):
            body.extend(event.data)
        elif isinstance(event, h11r.EndOfMessage):
            if response is None:
                raise RuntimeError("response ended before its Response event")
            return response, bytes(body)
        elif isinstance(event, h11r.ConnectionClosed):
            raise ConnectionError("server closed before finishing the response")
        elif event is h11r.ReceiveStatus.PAUSED:
            raise RuntimeError("client paused before the response completed")


def receive_socket_response(transport: socket.socket) -> bytes:
    response = bytearray()
    while True:
        chunk = transport.recv(64 * 1024)
        if not chunk:
            return bytes(response)
        response.extend(chunk)


def assert_http_response(response: bytes, status: int, body: bytes) -> None:
    head, separator, actual_body = response.partition(b"\r\n\r\n")
    assert separator == b"\r\n\r\n"
    assert head.startswith(f"HTTP/1.1 {status} ".encode())
    assert b"Connection: close\r\n" in head + b"\r\n"
    assert actual_body == body


def test_asyncio_server_runs_complete_connection_flow() -> None:
    example = Path(__file__).parents[3] / "examples" / "python" / "asyncio_server.py"
    namespace = runpy.run_path(str(example))
    handle_connection = namespace["handle_connection"]

    async def exercise_server() -> None:
        server = await asyncio.start_server(handle_connection, "127.0.0.1", 0)
        if not server.sockets:
            raise RuntimeError("asyncio did not create a listening socket")
        port = server.sockets[0].getsockname()[1]

        try:
            reader, writer = await asyncio.open_connection("127.0.0.1", port)
            client = h11r.Connection(h11r.Role.CLIENT)

            request = client.send_request(
                "POST",
                "/echo",
                [
                    ("Host", "example.test"),
                    ("Content-Length", "5"),
                    ("Expect", "100-continue"),
                ],
            )
            writer.write(request)
            await writer.drain()

            informational = await next_async_event(client, reader)
            assert isinstance(informational, h11r.InformationalResponse)
            assert informational.status_code == 100

            writer.write(client.send_data(b"hello") + client.end_of_message())
            await writer.drain()
            response, body = await receive_final_response(client, reader)
            assert response.status_code == 200
            assert body == b"hello"

            client.start_next_cycle()
            writer.write(
                client.send_request("GET", "/missing", [("Host", "example.test")])
                + client.end_of_message()
            )
            await writer.drain()
            response, body = await receive_final_response(client, reader)
            assert response.status_code == 404
            assert body == b"not found\n"

            client.start_next_cycle()
            writer.write(
                client.send_request("HEAD", "/", [("Host", "example.test")])
                + client.end_of_message()
            )
            await writer.drain()
            response, body = await receive_final_response(client, reader)
            assert response.status_code == 200
            assert body == b""
            assert (b"Content-Length", b"42") in response.headers

            client.start_next_cycle()
            writer.write(
                client.send_request("PUT", "/", [("Host", "example.test")])
                + client.end_of_message()
            )
            await writer.drain()
            response, body = await receive_final_response(client, reader)
            assert response.status_code == 405
            assert body == b"method not allowed\n"
            assert (b"Allow", b"GET, HEAD") in response.headers

            writer.close()
            await writer.wait_closed()

            bad_reader, bad_writer = await asyncio.open_connection("127.0.0.1", port)
            bad_writer.write(b"NOT HTTP\r\n\r\n")
            await bad_writer.drain()
            error_response = await bad_reader.read()
            assert error_response.startswith(b"HTTP/1.1 400 Bad Request\r\n")
            assert error_response.endswith(b"invalid HTTP request\n")
            bad_writer.close()
            await bad_writer.wait_closed()
        finally:
            server.close()
            await server.wait_closed()

    asyncio.run(asyncio.wait_for(exercise_server(), timeout=2))


def test_receive_buffer_server_runs_one_request_over_loopback() -> None:
    example = (
        Path(__file__).parents[3] / "examples" / "python" / "receive_buffer_server.py"
    )
    namespace = runpy.run_path(str(example))
    serve_once = namespace["serve_once"]
    max_request_body = namespace["MAX_REQUEST_BODY"]

    def exchange(request: bytes) -> bytes:
        with socket.create_server(("127.0.0.1", 0)) as listener:
            listener.settimeout(2)
            thread = Thread(target=serve_once, args=(listener,))
            thread.start()
            try:
                with socket.create_connection(
                    listener.getsockname(), timeout=2
                ) as client:
                    client.sendall(request)
                    return receive_socket_response(client)
            finally:
                thread.join(timeout=2)
                assert not thread.is_alive()

    success = exchange(b"GET / HTTP/1.1\r\nHost: example.test\r\n\r\n")
    assert_http_response(success, 200, b"hello from h11r\n")

    malformed = exchange(b"NOT HTTP\r\n\r\n")
    assert_http_response(malformed, 400, b"bad request\n")

    oversized = exchange(
        b"POST / HTTP/1.1\r\n"
        b"Host: example.test\r\n"
        b"Content-Length: "
        + str(max_request_body + 1).encode()
        + b"\r\n\r\n"
        + b"x" * (max_request_body + 1)
    )
    assert_http_response(oversized, 413, b"request body too large\n")

    with socket.create_server(("127.0.0.1", 0)) as listener:
        listener.settimeout(2)
        thread = Thread(target=serve_once, args=(listener,))
        thread.start()
        try:
            with socket.create_connection(listener.getsockname(), timeout=2) as client:
                client.sendall(
                    b"POST /missing HTTP/1.1\r\n"
                    b"Host: example.test\r\n"
                    b"Content-Length: 4\r\n"
                    b"Expect: 100-continue\r\n\r\n"
                )
                informational = client.recv(64 * 1024)
                assert informational.startswith(b"HTTP/1.1 100 Continue\r\n")
                client.sendall(b"bo")
                client.sendall(b"dy")
                response = receive_socket_response(client)
        finally:
            thread.join(timeout=2)
            assert not thread.is_alive()
    assert_http_response(response, 404, b"not found\n")


def test_asyncio_buffered_server_uses_callbacks_and_cleans_up() -> None:
    example = (
        Path(__file__).parents[3] / "examples" / "python" / "asyncio_buffered_server.py"
    )
    namespace = runpy.run_path(str(example))
    protocol_type = namespace["BufferedHTTPProtocol"]

    async def exercise_server() -> None:
        loop = asyncio.get_running_loop()
        protocols = []

        class ObservedProtocol(protocol_type):
            def __init__(self) -> None:
                super().__init__()
                self.get_buffer_calls = 0
                self.buffer_updated_calls = 0
                self.eof_calls = 0
                self.lost_calls = 0

            def get_buffer(self, sizehint: int) -> h11r.ReceiveBuffer:
                self.get_buffer_calls += 1
                return super().get_buffer(sizehint)

            def buffer_updated(self, nbytes: int) -> None:
                self.buffer_updated_calls += 1
                super().buffer_updated(nbytes)

            def eof_received(self) -> None:
                self.eof_calls += 1
                super().eof_received()

            def connection_lost(self, exc: Exception | None) -> None:
                self.lost_calls += 1
                super().connection_lost(exc)

        def protocol_factory() -> asyncio.BufferedProtocol:
            protocol = ObservedProtocol()
            protocols.append(protocol)
            return protocol

        server = await loop.create_server(protocol_factory, "127.0.0.1", 0)
        if not server.sockets:
            raise RuntimeError("asyncio did not create a listening socket")
        port = server.sockets[0].getsockname()[1]

        async def exchange(request: bytes) -> bytes:
            reader, writer = await asyncio.open_connection("127.0.0.1", port)
            writer.write(request)
            await writer.drain()
            response = await reader.read()
            writer.close()
            await writer.wait_closed()
            return response

        try:
            success = await exchange(b"GET / HTTP/1.1\r\nHost: example.test\r\n\r\n")
            assert_http_response(success, 200, b"hello from h11r\n")

            malformed = await exchange(b"NOT HTTP\r\n\r\n")
            assert_http_response(malformed, 400, b"bad request\n")

            reader, writer = await asyncio.open_connection("127.0.0.1", port)
            writer.write_eof()
            assert await reader.read() == b""
            writer.close()
            await writer.wait_closed()
            await asyncio.sleep(0)

            assert len(protocols) == 3
            for protocol in protocols:
                assert protocol.get_buffer_calls > 0
                assert protocol.lost_calls == 1
                assert protocol.pending is None
                assert protocol.transport is None
            for protocol in protocols[:2]:
                assert protocol.buffer_updated_calls > 0
            assert protocols[-1].eof_calls == 1
        finally:
            server.close()
            await server.wait_closed()

    asyncio.run(asyncio.wait_for(exercise_server(), timeout=2))


def test_websocket_upgrade_accepts_recombined_list_fields() -> None:
    example = Path(__file__).parents[3] / "examples" / "python" / "websocket_upgrade.py"
    websocket_accept = runpy.run_path(str(example))["websocket_accept"]
    server = h11r.Connection(h11r.Role.SERVER)
    server.receive_data(
        b"GET /chat HTTP/1.1\r\n"
        b"Host: example.test\r\n"
        b"Connection: keep-alive\r\n"
        b"Connection: Upgrade\r\n"
        b"Upgrade: example-protocol, websocket\r\n"
        b"Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n"
        b"Sec-WebSocket-Version: 13\r\n"
        b"\r\n"
    )

    request = server.next_event()
    assert isinstance(request, h11r.Request)
    assert websocket_accept(request) == b"s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
