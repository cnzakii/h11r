from __future__ import annotations

import copy
import pickle
from collections.abc import Sequence

import h11r
import pytest


@pytest.mark.parametrize(
    ("method", "target"),
    [
        pytest.param("GET", "/items?q=1", id="origin-form"),
        pytest.param("GET", "https://example.test/items", id="absolute-form"),
        pytest.param("CONNECT", "example.test:443", id="authority-form"),
        pytest.param("OPTIONS", "*", id="asterisk-form"),
    ],
)
def test_request_target_forms_cross_the_python_boundary(
    method: str, target: str
) -> None:
    # RFC 9112 Section 3.2 defines the four request-target forms. This verifies
    # that the Python/Rust boundary preserves each form as bytes.
    # https://www.rfc-editor.org/rfc/rfc9112.html#section-3.2
    connection = h11r.Connection(h11r.Role.CLIENT)
    wire = connection.send_request(method, target, [("Host", "example.test")])
    assert wire.startswith(f"{method} {target} HTTP/1.1\r\n".encode())

    server = h11r.Connection(h11r.Role.SERVER)
    server.receive_data(wire)
    request = server.next_event()
    assert isinstance(request, h11r.Request)
    assert request.target == target.encode()


@pytest.mark.parametrize("target", ["", "/bad target", b"/bad\x7f", b"/bad\xff"])
def test_request_target_rejects_bytes_outside_its_field_boundary(
    target: bytes | str,
) -> None:
    connection = h11r.Connection(h11r.Role.CLIENT)
    with pytest.raises(h11r.LocalProtocolError):
        connection.send_request("GET", target, [("Host", "example.test")])


def test_response_octets_and_folded_fields_cross_the_python_boundary() -> None:
    # RFC 9112 Sections 4 and 5.2 permit obs-text in reason-phrase and require
    # a user agent to replace response obs-fold with SP.
    # https://www.rfc-editor.org/rfc/rfc9112.html#section-4
    # https://www.rfc-editor.org/rfc/rfc9112.html#section-5.2
    connection = h11r.Connection(h11r.Role.CLIENT)
    connection.send_request("GET", "/", [("Host", "example.test")])
    connection.end_of_message()
    connection.receive_data(b"HTTP/1.1 200 OK\xff\r\nX-Test: one\r\n two\r\n\r\n")

    response = connection.next_event()
    assert isinstance(response, h11r.Response)
    assert response.reason == b"OK\xff"
    assert response.headers == ((b"X-Test", b"one two"),)


def test_request_framing_errors_keep_the_rfc_status_across_python() -> None:
    # RFC 9112 Section 6.1 recommends 501 for an unsupported request transfer
    # coding. The Python exception must retain that Rust protocol diagnosis.
    # https://www.rfc-editor.org/rfc/rfc9112.html#section-6.1
    connection = h11r.Connection(h11r.Role.SERVER)
    connection.receive_data(
        b"POST / HTTP/1.1\r\nHost: example.test\r\n"
        b"Transfer-Encoding: gzip, chunked\r\n\r\n"
    )
    with pytest.raises(h11r.RemoteProtocolError) as raised:
        connection.next_event()
    assert raised.value.suggested_status_code == 501


def test_python_text_and_buffer_inputs_preserve_http_octets() -> None:
    # RFC 9110 Section 5.5 permits obs-text in a field value. Python str is an
    # ASCII convenience input; bytes remain the lossless protocol form.
    # https://www.rfc-editor.org/rfc/rfc9110.html#section-5.5
    connection = h11r.Connection(h11r.Role.CLIENT)
    wire = connection.send_request(
        b"GET",
        memoryview(b"/"),
        (("Host", "example.test"), (b"X-Octet", b"\xff")),
    )
    assert b"X-Octet: \xff\r\n" in wire

    invalid_text = h11r.Connection(h11r.Role.CLIENT)
    with pytest.raises(ValueError, match="ASCII"):
        invalid_text.send_request("GET", "/", (("Host", "é.example"),))

    invalid_pair = h11r.Connection(h11r.Role.CLIENT)
    with pytest.raises(TypeError, match="2-tuple"):
        invalid_pair.send_request("GET", "/", [["Host", "example.test"]])
    with pytest.raises(TypeError, match="name and value"):
        invalid_pair.send_request("GET", "/", [("Host", "example.test", "extra")])


def test_receive_data_accepts_reused_receive_buffer() -> None:
    # The socket.recv_into() pattern reads into one reused bytearray and hands
    # the filled prefix over as a memoryview, without a bytes object per read.
    request = b"GET / HTTP/1.1\r\nHost: example.test\r\n\r\n"
    buffer = bytearray(16)
    connection = h11r.Connection(h11r.Role.SERVER)
    for start in range(0, len(request), len(buffer)):
        chunk = request[start : start + len(buffer)]
        buffer[: len(chunk)] = chunk
        connection.receive_data(memoryview(buffer)[: len(chunk)])

    request_event = connection.next_event()
    assert isinstance(request_event, h11r.Request)
    assert request_event.headers == ((b"Host", b"example.test"),)


def test_strided_memoryview_inputs_keep_copying_compatibility() -> None:
    client = h11r.Connection(h11r.Role.CLIENT)
    wire = client.send_request(
        b"GET",
        memoryview(b"/_i_t_e_m_s")[::2],
        [(memoryview(b"H_o_s_t")[::2], b"example.test")],
    )

    interleaved = bytearray(len(wire) * 2)
    interleaved[::2] = wire
    server = h11r.Connection(h11r.Role.SERVER)
    server.receive_data(memoryview(interleaved)[::2])

    request = server.next_event()
    assert isinstance(request, h11r.Request)
    assert request.target == b"/items"
    assert request.headers == ((b"Host", b"example.test"),)


def test_signed_byte_memoryview_inputs_keep_copying_compatibility() -> None:
    client = h11r.Connection(h11r.Role.CLIENT)
    wire = client.send_request(
        memoryview(b"POST").cast("b"),
        b"/",
        [(b"Host", b"example.test"), (b"Content-Length", b"4")],
    )
    wire += client.send_data(memoryview(b"body").cast("b"))
    wire += client.end_of_message()

    server = h11r.Connection(h11r.Role.SERVER)
    server.receive_data(wire)
    assert isinstance(server.next_event(), h11r.Request)
    body = server.next_event()
    assert isinstance(body, h11r.Data)
    assert body.data == b"body"
    assert isinstance(server.next_event(), h11r.EndOfMessage)


def test_direct_api_and_receive_events() -> None:
    connection = h11r.Connection(h11r.Role.SERVER)
    connection.receive_data(b"GET / HTTP/1.1\r\nHost: example.test\r\n\r\n")

    request = connection.next_event()
    assert isinstance(request, h11r.Request)
    assert request.method == b"GET"
    assert request.target == b"/"
    assert request.headers == ((b"Host", b"example.test"),)
    assert connection.next_event() == h11r.EndOfMessage()
    assert connection.next_event() is h11r.ReceiveStatus.NEED_DATA

    assert connection.send_response(204, reason=b"No Content") == (
        b"HTTP/1.1 204 No Content\r\n\r\n"
    )
    assert connection.end_of_message() == b""
    assert connection.local_state is h11r.State.DONE
    assert connection.peer_state is h11r.State.DONE


def test_continue_body_trailers_and_reuse_cross_the_python_boundary() -> None:
    # RFC 9110 Section 10.1.1 defines 100-continue, while RFC 9112 Section
    # 7.1.2 carries trailer fields at the end of a chunked message.
    # https://www.rfc-editor.org/rfc/rfc9110.html#section-10.1.1
    # https://www.rfc-editor.org/rfc/rfc9112.html#section-7.1.2
    client = h11r.Connection(h11r.Role.CLIENT)
    server = h11r.Connection(h11r.Role.SERVER)
    request_wire = client.send_request(
        "POST",
        "/upload",
        [
            ("Host", "example.test"),
            ("Transfer-Encoding", "chunked"),
            ("Expect", "100-continue"),
        ],
    )
    assert client.client_is_waiting_for_100_continue

    server.receive_data(request_wire)
    assert isinstance(server.next_event(), h11r.Request)
    assert server.peer_http_version == b"1.1"
    assert server.client_is_waiting_for_100_continue

    client.receive_data(server.send_informational_response(100, reason="Continue"))
    informational = client.next_event()
    assert isinstance(informational, h11r.InformationalResponse)
    assert informational.status_code == 100
    assert not client.client_is_waiting_for_100_continue
    assert not server.client_is_waiting_for_100_continue

    body_wire = client.send_data(bytearray(b"body"))
    body_wire += client.end_of_message([("Digest", "ok")])
    server.receive_data(body_wire)
    body = server.next_event()
    end = server.next_event()
    assert isinstance(body, h11r.Data) and body.data == b"body"
    assert isinstance(end, h11r.EndOfMessage)
    assert end.trailers == ((b"Digest", b"ok"),)

    client.receive_data(server.send_response(204) + server.end_of_message())
    assert isinstance(client.next_event(), h11r.Response)
    assert isinstance(client.next_event(), h11r.EndOfMessage)
    client.start_next_cycle()
    server.start_next_cycle()
    assert client.local_state is client.peer_state is h11r.State.IDLE
    assert server.local_state is server.peer_state is h11r.State.IDLE


def test_upgrade_pause_and_trailing_data_cross_the_python_boundary() -> None:
    # RFC 9110 Section 7.8 leaves bytes after a successful 101 to the selected
    # protocol rather than the HTTP parser.
    # https://www.rfc-editor.org/rfc/rfc9110.html#section-7.8
    client = h11r.Connection(h11r.Role.CLIENT)
    server = h11r.Connection(h11r.Role.SERVER)
    request_wire = client.send_request(
        "GET",
        "/chat",
        [
            ("Host", "example.test"),
            ("Connection", "upgrade"),
            ("Upgrade", "next-protocol"),
        ],
    )
    request_wire += client.end_of_message()
    server.receive_data(request_wire + b"client-protocol-data")
    assert isinstance(server.next_event(), h11r.Request)
    assert isinstance(server.next_event(), h11r.EndOfMessage)
    assert server.next_event() is h11r.ReceiveStatus.PAUSED
    assert server.trailing_data == (b"client-protocol-data", False)

    switch = server.send_informational_response(
        101,
        [("Connection", "upgrade"), ("Upgrade", "next-protocol")],
        reason="Switching Protocols",
    )
    client.receive_data(switch + b"server-protocol-data")
    informational = client.next_event()
    assert isinstance(informational, h11r.InformationalResponse)
    assert informational.status_code == 101
    assert client.local_state is client.peer_state is h11r.State.SWITCHED_PROTOCOL
    assert server.local_state is server.peer_state is h11r.State.SWITCHED_PROTOCOL
    assert client.trailing_data == (b"server-protocol-data", False)


def test_buffered_nbytes_crosses_the_python_boundary() -> None:
    # The backlog length rather than the bytes, so a caller applying
    # transport backpressure can consult it after every receive without
    # copying what it is measuring.
    server = h11r.Connection(h11r.Role.SERVER)
    assert server.buffered_nbytes == 0

    head = b"POST / HTTP/1.1\r\nHost: example.test\r\nContent-Length: 6\r\n\r\n"
    server.receive_data(head)
    assert server.buffered_nbytes == len(head)

    # Parsing the head consumes it; body that arrived with it stays buffered,
    # and more of it accumulates while the reader is elsewhere.
    server.receive_data(b"abc")
    assert isinstance(server.next_event(), h11r.Request)
    assert server.buffered_nbytes == 3
    server.receive_data(b"def")
    assert server.buffered_nbytes == 6

    body = server.next_event()
    assert isinstance(body, h11r.Data)
    assert bytes(body.data) == b"abcdef"
    assert server.buffered_nbytes == 0
    assert isinstance(server.next_event(), h11r.EndOfMessage)

    # A pipelined successor is backlog too, and the same bytes `trailing_data`
    # reports at the message boundary.
    server.receive_data(b"GET /next HTTP/1.1\r\n")
    assert server.buffered_nbytes == len(server.trailing_data[0]) == 20


def test_connection_close_event_crosses_the_python_boundary() -> None:
    # RFC 9112 Section 9.6 makes transport closure terminal.
    # https://www.rfc-editor.org/rfc/rfc9112.html#section-9.6
    connection = h11r.Connection(h11r.Role.SERVER)
    connection.receive_data(b"")
    assert isinstance(connection.next_event(), h11r.ConnectionClosed)
    assert connection.trailing_data == (b"", True)
    assert connection.local_state is h11r.State.MUST_CLOSE
    assert connection.peer_state is h11r.State.CLOSED
    connection.close()
    assert connection.local_state is connection.peer_state is h11r.State.CLOSED


def test_parser_accepted_line_endings_cross_the_python_boundary() -> None:
    # RFC 9112 Section 2.2 permits recipients to recognize LF and requires a
    # server to ignore at least one leading empty request line. The binding must
    # preserve the core parser's decision for every transport split.
    # https://www.rfc-editor.org/rfc/rfc9112.html#section-2.2
    wire = b"\n\nGET / HTTP/1.1\nHost: example.test\n\n"
    for split in range(len(wire) + 1):
        connection = h11r.Connection(h11r.Role.SERVER)
        if split:
            connection.receive_data(wire[:split])
        if split < len(wire):
            connection.receive_data(wire[split:])
        request = connection.next_event()
        assert isinstance(request, h11r.Request)
        assert request.headers == ((b"Host", b"example.test"),)

    client = h11r.Connection(h11r.Role.CLIENT)
    client.send_request("GET", "/", [("Host", "example.test")])
    client.end_of_message()
    client.receive_data(b"HTTP/1.1 204 All Good\nX-Test: one\n two\t\n\n")
    response = client.next_event()
    assert isinstance(response, h11r.Response)
    assert response.reason == b"All Good"
    assert response.headers == ((b"X-Test", b"one two"),)


def test_data_parts_preserve_python_buffer_identity_and_use_nbytes() -> None:
    connection = h11r.Connection(h11r.Role.CLIENT)
    connection.send_request(
        b"POST",
        b"/",
        [(b"Host", b"x"), (b"Transfer-Encoding", b"chunked")],
    )
    body = memoryview(b"12345678").cast("B", shape=[2, 4])
    prefix, original, suffix = connection.send_data_parts(body)
    assert len(body) == 2
    assert body.nbytes == 8
    assert prefix == b"8\r\n"
    assert original is body
    assert suffix == b"\r\n"


def test_data_parts_use_non_buffer_nbytes_and_preserve_identity() -> None:
    class FileRegion:
        @property
        def nbytes(self) -> int:
            return 12

    connection = h11r.Connection(h11r.Role.CLIENT)
    connection.send_request(
        b"POST",
        b"/",
        [(b"Host", b"x"), (b"Transfer-Encoding", b"chunked")],
    )
    region = FileRegion()
    prefix, original, suffix = connection.send_data_parts(region)
    assert prefix == b"c\r\n"
    assert original is region
    assert suffix == b"\r\n"


def test_data_parts_propagate_nbytes_lookup_errors_without_changing_state() -> None:
    class UnavailableRegion:
        @property
        def nbytes(self) -> int:
            raise RuntimeError("byte size unavailable")

    connection = h11r.Connection(h11r.Role.CLIENT)
    connection.send_request(
        b"POST",
        b"/",
        [(b"Host", b"x"), (b"Content-Length", b"1")],
    )

    with pytest.raises(RuntimeError, match="byte size unavailable"):
        connection.send_data_parts(UnavailableRegion())

    assert connection.send_data_parts(b"x") == (b"", b"x", b"")
    assert connection.end_of_message() == b""


@pytest.mark.parametrize(
    ("body", "error_type"),
    [
        pytest.param(object(), TypeError, id="missing"),
        pytest.param("héllo", TypeError, id="text"),
        pytest.param([b"aa", b"bb"], TypeError, id="list"),
        pytest.param({"a": 1}, TypeError, id="dict"),
        pytest.param(
            type("NonIntegerByteSize", (), {"nbytes": 1.5})(),
            TypeError,
            id="non-integer",
        ),
        pytest.param(
            type("NegativeByteSize", (), {"nbytes": -1})(),
            OverflowError,
            id="negative",
        ),
        pytest.param(
            type("OverflowingByteSize", (), {"nbytes": 1 << 128})(),
            OverflowError,
            id="platform-overflow",
        ),
    ],
)
def test_data_parts_reject_invalid_byte_sizes_without_changing_state(
    body: object, error_type: type[Exception]
) -> None:
    connection = h11r.Connection(h11r.Role.CLIENT)
    connection.send_request(
        b"POST",
        b"/",
        [(b"Host", b"x"), (b"Content-Length", b"1")],
    )

    with pytest.raises(error_type):
        connection.send_data_parts(body)  # type: ignore[arg-type]

    proxy = type("OneByteRegion", (), {"nbytes": 1})()
    prefix, unchanged_proxy, suffix = connection.send_data_parts(proxy)
    assert prefix == suffix == b""
    assert unchanged_proxy is proxy
    assert connection.end_of_message() == b""


def test_limits_and_remote_error_status() -> None:
    connection = h11r.Connection(h11r.Role.SERVER, max_header_count=1)
    connection.receive_data(b"GET / HTTP/1.1\r\nHost: x\r\nX: y\r\n\r\n")
    try:
        connection.next_event()
    except h11r.RemoteProtocolError as error:
        assert error.suggested_status_code == 431
    else:
        raise AssertionError("expected RemoteProtocolError")
    assert h11r.RemoteProtocolError("raised by a caller").suggested_status_code is None


def test_events_have_value_equality_and_parts_require_contiguous_data() -> None:
    wire = b"GET / HTTP/1.1\r\nHost: x\r\n\r\n"
    first = h11r.Connection(h11r.Role.SERVER)
    second = h11r.Connection(h11r.Role.SERVER)
    first.receive_data(wire)
    second.receive_data(wire)
    assert first.next_event() == second.next_event()

    client = h11r.Connection(h11r.Role.CLIENT)
    client.send_request(
        b"POST",
        b"/",
        [(b"Host", b"x"), (b"Transfer-Encoding", b"chunked")],
    )
    try:
        client.send_data_parts(memoryview(b"abcdef")[::2])
    except ValueError:
        pass
    else:
        raise AssertionError("non-contiguous buffers cannot pass through")
    assert client.send_data_parts(b"ok") == (b"2\r\n", b"ok", b"\r\n")


def test_event_properties_reuse_their_immutable_python_values() -> None:
    connection = h11r.Connection(h11r.Role.SERVER)
    connection.receive_data(
        b"POST /items HTTP/1.1\r\nHost: example.test\r\nContent-Length: 4\r\n\r\nbody"
    )

    request = connection.next_event()
    assert isinstance(request, h11r.Request)
    assert request.method is request.method
    assert request.target is request.target
    assert request.headers is request.headers

    body = connection.next_event()
    assert isinstance(body, h11r.Data)
    assert body.data is body.data


def test_events_can_be_constructed_and_compared_with_received_events() -> None:
    connection = h11r.Connection(h11r.Role.SERVER)
    connection.receive_data(
        b"POST /items HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n"
        b"3\r\nabc\r\n0\r\nDigest: ok\r\n\r\n"
    )
    assert connection.next_event() == h11r.Request(
        "POST", "/items", [("Host", "x"), ("Transfer-Encoding", "chunked")]
    )
    assert connection.next_event() == h11r.Data(
        b"abc", chunk_start=True, chunk_end=True
    )
    assert connection.next_event() == h11r.EndOfMessage([("Digest", "ok")])

    response = h11r.Response(204, reason="No Content")
    assert (response.status_code, response.reason, response.headers) == (
        204,
        b"No Content",
        (),
    )
    assert response.http_version == b"1.1"
    assert h11r.InformationalResponse(103).status_code == 103
    with pytest.raises(ValueError, match="1xx"):
        h11r.InformationalResponse(200)
    with pytest.raises(ValueError, match="non-1xx"):
        h11r.Response(100)
    with pytest.raises(ValueError):
        h11r.Request("BAD METHOD", "/", [])


def test_events_support_repr_and_positional_patterns() -> None:
    request = h11r.Request("GET", "/", [("Host", "x")])
    assert repr(request) == (
        "Request(method=b'GET', target=b'/', headers=Headers([(b'Host', b'x')]), "
        "http_version=b'1.1')"
    )
    assert repr(h11r.Response(200)) == (
        "Response(status_code=200, reason=b'', headers=Headers([]), http_version=b'1.1')"
    )
    assert repr(h11r.InformationalResponse(100)) == (
        "InformationalResponse(status_code=100, reason=b'', headers=Headers([]), "
        "http_version=b'1.1')"
    )
    assert h11r.InformationalResponse(100) == h11r.InformationalResponse(100)
    assert repr(h11r.Data(bytearray(b"x"))) == (
        "Data(data=b'x', chunk_start=False, chunk_end=False)"
    )
    assert repr(h11r.EndOfMessage()) == "EndOfMessage(trailers=Headers([]))"
    assert repr(h11r.ConnectionClosed()) == "ConnectionClosed()"

    match request:
        case h11r.Request(b"GET", target):
            assert target == b"/"
        case _:
            raise AssertionError("positional pattern did not match")
    match h11r.Response(404):
        case h11r.Response(404):
            pass
        case _:
            raise AssertionError("positional pattern did not match")


def test_headers_look_up_names_case_insensitively() -> None:
    request = h11r.Request(
        "GET", "/", [("Host", "x"), ("X-Tag", "one"), ("x-tag", "two")]
    )
    assert request.headers.get_all("x-TAG") == (b"one", b"two")
    assert request.headers.get(b"X-TAG") == b"one"
    assert request.headers.get("missing") is None
    assert request.headers.get("missing", b"") == b""
    assert request.headers.get_all("missing") == ()
    assert request.headers.get("é", b"") == b""
    assert request.headers.get_all("é") == ()
    assert request.headers.get("\udcff", b"") == b""
    assert h11r.Response(200, [("Etag", "v")]).headers.get("ETAG") == b"v"
    end = h11r.EndOfMessage([("Digest", "ok")])
    assert end.trailers.get_all("digest") == (b"ok",)


def test_headers_behave_like_a_tuple_of_byte_pairs() -> None:
    pairs = ((b"Host", b"x"), (b"Accept", b"*/*"))
    headers = h11r.Headers([("Host", "x"), (memoryview(b"Accept"), b"*/*")])

    assert headers == pairs and pairs == headers and headers != list(pairs)
    assert hash(headers) == hash(pairs)
    assert headers == h11r.Headers(pairs)
    assert (len(headers), headers[0], headers[-1], headers[1:]) == (
        2,
        pairs[0],
        pairs[1],
        pairs[1:],
    )
    assert list(headers) == list(pairs)
    assert (b"Host", b"x") in headers
    assert isinstance(headers, Sequence)
    assert (headers.index(pairs[1]), headers.count(pairs[0])) == (1, 1)
    assert headers.index(pairs[1], -(2**100), 2**100) == 1
    with pytest.raises(ValueError):
        headers.index((b"Host", b"y"))
    with pytest.raises(ValueError):
        headers.index(pairs[0], 1)
    assert not h11r.Headers()
    assert repr(headers) == "Headers([(b'Host', b'x'), (b'Accept', b'*/*')])"
    assert copy.deepcopy(headers) == pickle.loads(pickle.dumps(headers)) == pairs


def test_headers_can_be_reused_across_sends() -> None:
    fields = h11r.Headers([("Server", "example")])
    for _ in range(2):
        server = h11r.Connection(h11r.Role.SERVER)
        server.receive_data(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
        server.next_event()
        server.next_event()
        assert (
            server.send_response(200, fields, body=b"ok")
            == b"HTTP/1.1 200 \r\nServer: example\r\nContent-Length: 2\r\n\r\nok"
        )
    assert fields == ((b"Server", b"example"),)


def test_keep_alive_reports_whether_reuse_remains_possible() -> None:
    server = h11r.Connection(h11r.Role.SERVER)
    server.receive_data(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
    assert isinstance(server.next_event(), h11r.Request)
    assert server.keep_alive

    closing = h11r.Connection(h11r.Role.SERVER)
    closing.receive_data(b"GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
    assert isinstance(closing.next_event(), h11r.Request)
    assert not closing.keep_alive


def test_client_refuses_reuse_when_unsolicited_bytes_follow_a_response() -> None:
    # RFC 9112 Section 6.3: a client must not process extra data after a
    # complete response as a separate response.
    # https://www.rfc-editor.org/rfc/rfc9112.html#section-6.3
    client = h11r.Connection(h11r.Role.CLIENT)
    client.send_request("GET", "/", [("Host", "x")])
    client.end_of_message()
    client.receive_data(b"HTTP/1.1 204 No Content\r\n\r\nHTTP/1.1 200 OK\r\n\r\n")
    assert isinstance(client.next_event(), h11r.Response)
    assert isinstance(client.next_event(), h11r.EndOfMessage)
    with pytest.raises(h11r.LocalProtocolError):
        client.start_next_cycle()
    assert not client.keep_alive


def test_body_keyword_sends_a_complete_message_in_one_call() -> None:
    client = h11r.Connection(h11r.Role.CLIENT)
    wire = client.send_request("POST", "/items", [("Host", "x")], body=b"hi")
    assert wire == b"POST /items HTTP/1.1\r\nHost: x\r\nContent-Length: 2\r\n\r\nhi"
    assert client.local_state is h11r.State.DONE

    server = h11r.Connection(h11r.Role.SERVER)
    server.receive_data(wire)
    assert isinstance(server.next_event(), h11r.Request)
    assert server.next_event() == h11r.Data(b"hi")
    assert isinstance(server.next_event(), h11r.EndOfMessage)
    assert server.send_response(200, body=memoryview(b"ok")) == (
        b"HTTP/1.1 200 \r\nContent-Length: 2\r\n\r\nok"
    )


def test_rejected_body_keyword_leaves_the_connection_unchanged() -> None:
    server = h11r.Connection(h11r.Role.SERVER)
    server.receive_data(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
    assert isinstance(server.next_event(), h11r.Request)
    assert isinstance(server.next_event(), h11r.EndOfMessage)
    with pytest.raises(h11r.LocalProtocolError):
        server.send_response(200, [("Content-Length", "5")], body=b"four")
    assert server.local_state is h11r.State.SEND_RESPONSE
    assert server.send_response(204, body=b"") == b"HTTP/1.1 204 \r\n\r\n"


def test_received_registered_methods_and_versions_are_exact_bytes() -> None:
    for method in ("TRACE", "PATCH", "PURGE"):
        server = h11r.Connection(h11r.Role.SERVER)
        server.receive_data(f"{method} / HTTP/1.0\r\n\r\n".encode())
        request = server.next_event()
        assert isinstance(request, h11r.Request)
        assert (request.method, request.http_version) == (method.encode(), b"1.0")


def test_a_successful_connect_response_with_body_keyword_opens_the_tunnel() -> None:
    server = h11r.Connection(h11r.Role.SERVER)
    server.receive_data(
        b"CONNECT example.test:443 HTTP/1.1\r\nHost: example.test:443\r\n\r\n"
    )
    assert isinstance(server.next_event(), h11r.Request)
    assert server.send_response(200, body=b"") == b"HTTP/1.1 200 \r\n\r\n"
    assert server.local_state is h11r.State.SWITCHED_PROTOCOL


def test_states_and_roles_are_hashable_constants() -> None:
    finished = {h11r.State.DONE, h11r.State.MUST_CLOSE}
    assert h11r.State.MUST_CLOSE in finished
    assert h11r.State.IDLE not in finished
    assert {h11r.Role.CLIENT: 1}[h11r.Role.CLIENT] == 1
    assert hash(h11r.ReceiveStatus.NEED_DATA) == hash(h11r.ReceiveStatus.NEED_DATA)
    assert h11r.State.IDLE != 0
    assert not hasattr(h11r, "Sequence")


@pytest.mark.parametrize("status_code", [-1, 99, 600, 70000, 2**70])
def test_out_of_range_status_codes_raise_value_error(status_code: int) -> None:
    with pytest.raises(ValueError, match="between 100 and 599"):
        h11r.Response(status_code)
    server = h11r.Connection(h11r.Role.SERVER)
    server.receive_data(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
    server.next_event()
    with pytest.raises(ValueError, match="between 100 and 599"):
        server.send_response(status_code)


def test_lenient_clients_drop_invalid_response_field_lines() -> None:
    wire = b"HTTP/1.1 200 OK\r\nget-ban/portal.php: x\r\nContent-Length: 0\r\n\r\n"
    for lenient in (False, True):
        client = h11r.Connection(h11r.Role.CLIENT, lenient=lenient)
        client.send_request("GET", "/", [("Host", "example.test")], body=b"")
        client.receive_data(wire)
        if not lenient:
            with pytest.raises(h11r.RemoteProtocolError):
                client.next_event()
            continue
        response = client.next_event()
        assert isinstance(response, h11r.Response)
        assert response.headers == ((b"Content-Length", b"0"),)
        assert isinstance(client.next_event(), h11r.EndOfMessage)
        assert not client.keep_alive


def test_every_state_version_and_status_input_crosses_the_python_boundary() -> None:
    client = h11r.Connection(h11r.Role.CLIENT)
    client.send_request(
        "GET",
        "/",
        [("Host", "x"), ("Connection", "upgrade"), ("Upgrade", "demo")],
        http_version="1.1",
    )
    assert client.local_state is h11r.State.SEND_BODY
    client.end_of_message()
    assert client.local_state is h11r.State.MIGHT_SWITCH_PROTOCOL

    server = h11r.Connection(h11r.Role.SERVER)
    server.receive_data(b"GET / HTTP/1.0\r\n\r\n")
    request = server.next_event()
    assert isinstance(request, h11r.Request)
    assert request.http_version == b"1.0"
    with pytest.raises(h11r.LocalProtocolError):
        server.send_response(200, http_version=b"1.0", body=b"", reason=b"\r")
    assert server.send_response(200, http_version=b"1.0", body=b"").startswith(
        b"HTTP/1.0 200 "
    )

    with pytest.raises(ValueError, match="http_version"):
        h11r.Response(200, http_version=b"2.0")
    with pytest.raises(TypeError):
        h11r.Response("200")  # type: ignore[arg-type]

    broken = h11r.Connection(h11r.Role.SERVER)
    broken.receive_data(b"NOT HTTP\r\n\r\n")
    with pytest.raises(h11r.RemoteProtocolError):
        broken.next_event()
    assert broken.peer_state is h11r.State.ERROR
