from __future__ import annotations

import ctypes
import gc
import sys

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
        b"POST / HTTP/1.1\r\nHost: example.test\r\nTransfer-Encoding: gzip\r\n\r\n"
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


def write_receive_buffer(
    connection: h11r.Connection, data: bytes, *, size: int | None = None
) -> None:
    with connection.receive_buffer(size or len(data)) as receive_buffer:
        with memoryview(receive_buffer) as view:
            view[: len(data)] = data
        receive_buffer.commit(len(data))


def test_receive_buffer_parses_fragmented_request_and_body() -> None:
    connection = h11r.Connection(h11r.Role.SERVER)
    fragments = [
        b"POST /upload HTTP/1.1\r\nHost: example.test\r\nContent-L",
        b"ength: 7\r\n\r\npay",
        b"load",
    ]

    for fragment in fragments:
        write_receive_buffer(connection, fragment, size=64)

    request = connection.next_event()
    assert isinstance(request, h11r.Request)
    assert request.target == b"/upload"
    body = bytearray()
    while True:
        event = connection.next_event()
        if isinstance(event, h11r.Data):
            body.extend(event.data)
        elif isinstance(event, h11r.EndOfMessage):
            break
        else:
            raise AssertionError(f"unexpected event: {event!r}")
    assert body == b"payload"


def test_receive_buffer_is_nonconstructible_and_final() -> None:
    with pytest.raises(TypeError):
        h11r.ReceiveBuffer()
    with pytest.raises(TypeError):
        type("ReceiveBufferSubclass", (h11r.ReceiveBuffer,), {})


def test_receive_buffer_zero_commit_records_eof() -> None:
    connection = h11r.Connection(h11r.Role.SERVER)
    lease = connection.receive_buffer(1).acquire()
    lease.commit(0)

    assert isinstance(connection.next_event(), h11r.ConnectionClosed)
    assert connection.trailing_data == (b"", True)


def test_receive_buffer_context_abort_and_drop_release_connection() -> None:
    connection = h11r.Connection(h11r.Role.SERVER)

    with connection.receive_buffer(16) as lease:
        assert len(lease) == 16
    connection.receive_data(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
    assert isinstance(connection.next_event(), h11r.Request)

    other = h11r.Connection(h11r.Role.SERVER)
    lease = other.receive_buffer(16)
    del lease
    gc.collect()
    other.receive_data(b"")
    assert isinstance(other.next_event(), h11r.ConnectionClosed)


def test_receive_buffer_recycles_connection_scratch_storage() -> None:
    connection = h11r.Connection(h11r.Role.SERVER)

    first = connection.receive_buffer(128).acquire()
    first_item = ctypes.c_ubyte.from_buffer(first)
    first_address = ctypes.addressof(first_item)
    del first_item
    first.abort()

    second = connection.receive_buffer(64).acquire()
    second_item = ctypes.c_ubyte.from_buffer(second)
    second_address = ctypes.addressof(second_item)
    del second_item
    second.abort()

    assert second_address == first_address


@pytest.mark.parametrize("size", [0, -1])
def test_receive_buffer_rejects_nonpositive_size(size: int) -> None:
    connection = h11r.Connection(h11r.Role.SERVER)
    with pytest.raises(ValueError):
        connection.receive_buffer(size)


def test_receive_buffer_rejects_platform_overflow_and_allocation_failure() -> None:
    connection = h11r.Connection(h11r.Role.SERVER)

    with pytest.raises(OverflowError):
        connection.receive_buffer(1 << 128)
    with pytest.raises(MemoryError):
        connection.receive_buffer(sys.maxsize)

    connection.receive_data(b"")


def test_receive_buffer_rejects_invalid_lifecycle_transitions_and_counts() -> None:
    connection = h11r.Connection(h11r.Role.SERVER)
    lease = connection.receive_buffer(8)

    with pytest.raises(RuntimeError):
        memoryview(lease)
    with pytest.raises(RuntimeError):
        lease.commit(0)

    assert lease.acquire() is lease
    with pytest.raises(RuntimeError):
        lease.acquire()
    with pytest.raises(ValueError):
        lease.commit(-1)
    with pytest.raises(ValueError):
        lease.commit(9)
    with pytest.raises(OverflowError):
        lease.commit(1 << 128)

    lease.commit(0)
    with pytest.raises(RuntimeError):
        lease.commit(0)
    with pytest.raises(RuntimeError):
        lease.acquire()
    lease.abort()
    lease.abort()


def test_receive_buffer_export_blocks_commit_and_defers_abort() -> None:
    connection = h11r.Connection(h11r.Role.SERVER)
    lease = connection.receive_buffer(32).acquire()
    view = memoryview(lease)
    assert not view.readonly
    assert view.itemsize == 1
    assert view.c_contiguous

    with pytest.raises(BufferError):
        lease.commit(0)
    lease.abort()
    lease.abort()
    with pytest.raises(RuntimeError, match="reserved"):
        connection.receive_data(b"")

    view.release()
    connection.receive_data(b"")
    assert isinstance(connection.next_event(), h11r.ConnectionClosed)


def test_receive_buffer_escaped_context_export_releases_after_final_view() -> None:
    connection = h11r.Connection(h11r.Role.SERVER)

    with connection.receive_buffer(32) as lease:
        escaped = memoryview(lease)

    with pytest.raises(RuntimeError, match="reserved"):
        connection.next_event()
    escaped.release()
    assert connection.next_event() is h11r.ReceiveStatus.NEED_DATA


def test_receive_buffer_reservation_rejects_mutation_but_allows_state_reads() -> None:
    connection = h11r.Connection(h11r.Role.SERVER)
    lease = connection.receive_buffer(32)

    assert connection.local_state is h11r.State.IDLE
    assert connection.peer_state is h11r.State.IDLE
    assert connection.peer_http_version is None
    assert not connection.client_is_waiting_for_100_continue
    assert connection.trailing_data == (b"", False)

    operations = [
        lambda: connection.receive_buffer(1),
        lambda: connection.receive_data(b""),
        connection.next_event,
        lambda: connection.send_request("GET", "/", []),
        lambda: connection.send_informational_response(100),
        lambda: connection.send_response(200),
        lambda: connection.send_data(b""),
        lambda: connection.send_data_parts(b""),
        connection.end_of_message,
        connection.start_next_cycle,
        connection.close,
    ]
    for operation in operations:
        with pytest.raises(RuntimeError, match="reserved"):
            operation()

    lease.abort()
    assert connection.next_event() is h11r.ReceiveStatus.NEED_DATA


def start_collected_request(
    wire: bytes, *, max_bytes: int
) -> tuple[h11r.Connection, h11r.BodyCollector]:
    connection = h11r.Connection(h11r.Role.SERVER)
    connection.receive_data(wire)
    request = connection.next_event()
    assert isinstance(request, h11r.Request)
    return connection, connection.collect_body(max_bytes=max_bytes)


def test_body_collector_collects_fragmented_fixed_length_body() -> None:
    connection, collector = start_collected_request(
        b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 7\r\n\r\npay",
        max_bytes=7,
    )

    assert collector.next() is h11r.ReceiveStatus.NEED_DATA
    connection.receive_data(b"load")
    body = collector.next()

    assert isinstance(body, h11r.CollectedBody)
    assert bytes(body.data) == b"payload"
    assert body.trailers == ()
    assert body.data.readonly
    assert body.data.itemsize == 1
    assert body.data.format == "B"
    assert body.data.ndim == 1
    assert body.data.c_contiguous
    with pytest.raises(TypeError):
        body.data[0] = 0
    with pytest.raises(RuntimeError, match="finished"):
        collector.next()


def test_collected_body_owner_rejects_writable_buffer_requests() -> None:
    class PyBuffer(ctypes.Structure):
        _fields_ = [
            ("buf", ctypes.c_void_p),
            ("obj", ctypes.py_object),
            ("len", ctypes.c_ssize_t),
            ("itemsize", ctypes.c_ssize_t),
            ("readonly", ctypes.c_int),
            ("ndim", ctypes.c_int),
            ("format", ctypes.c_char_p),
            ("shape", ctypes.POINTER(ctypes.c_ssize_t)),
            ("strides", ctypes.POINTER(ctypes.c_ssize_t)),
            ("suboffsets", ctypes.POINTER(ctypes.c_ssize_t)),
            ("internal", ctypes.c_void_p),
        ]

    connection, collector = start_collected_request(
        b"GET / HTTP/1.1\r\nHost: x\r\n\r\n",
        max_bytes=0,
    )
    body = collector.next()
    assert isinstance(body, h11r.CollectedBody)

    get_buffer = ctypes.pythonapi.PyObject_GetBuffer
    get_buffer.argtypes = [
        ctypes.py_object,
        ctypes.POINTER(PyBuffer),
        ctypes.c_int,
    ]
    get_buffer.restype = ctypes.c_int
    requested = PyBuffer()
    with pytest.raises(BufferError, match="read-only"):
        get_buffer(body.data.obj, ctypes.byref(requested), 0x0001)


def test_body_collector_collects_chunked_body_and_trailers() -> None:
    connection, collector = start_collected_request(
        b"POST / HTTP/1.1\r\n"
        b"Host: x\r\n"
        b"Transfer-Encoding: chunked\r\n\r\n"
        b"3\r\none\r\n3\r\ntwo\r\n0\r\nDigest: ok\r\n\r\n",
        max_bytes=6,
    )

    body = collector.next()

    assert isinstance(body, h11r.CollectedBody)
    assert bytes(body.data) == b"onetwo"
    assert body.trailers == ((b"Digest", b"ok"),)
    assert connection.peer_state is h11r.State.DONE


def test_body_collector_handles_empty_and_already_buffered_bodies() -> None:
    empty_connection, empty_collector = start_collected_request(
        b"GET / HTTP/1.1\r\nHost: x\r\n\r\n",
        max_bytes=0,
    )
    empty = empty_collector.next()
    assert isinstance(empty, h11r.CollectedBody)
    assert bytes(empty.data) == b""
    assert empty_connection.peer_state is h11r.State.DONE

    connection, collector = start_collected_request(
        b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 4\r\n\r\nbody",
        max_bytes=4,
    )
    body = collector.next()
    assert isinstance(body, h11r.CollectedBody)
    assert bytes(body.data) == b"body"
    assert connection.peer_state is h11r.State.DONE


def test_body_collector_handles_close_delimited_response_eof() -> None:
    connection = h11r.Connection(h11r.Role.CLIENT)
    connection.send_request("GET", "/", [("Host", "x")])
    connection.end_of_message()
    connection.receive_data(b"HTTP/1.0 200 OK\r\n\r\nclose-delimited")
    assert isinstance(connection.next_event(), h11r.Response)
    collector = connection.collect_body(max_bytes=15)

    assert collector.next() is h11r.ReceiveStatus.NEED_DATA
    connection.receive_data(b"")
    body = collector.next()

    assert isinstance(body, h11r.CollectedBody)
    assert bytes(body.data) == b"close-delimited"
    assert connection.peer_state is h11r.State.MUST_CLOSE


def test_body_collector_starts_after_final_not_informational_response() -> None:
    connection = h11r.Connection(h11r.Role.CLIENT)
    connection.send_request("GET", "/", [("Host", "x")])
    connection.end_of_message()
    connection.receive_data(
        b"HTTP/1.1 103 Early Hints\r\n\r\n"
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok"
    )

    assert isinstance(connection.next_event(), h11r.InformationalResponse)
    with pytest.raises(RuntimeError, match="immediately"):
        connection.collect_body(max_bytes=2)

    assert isinstance(connection.next_event(), h11r.Response)
    body = connection.collect_body(max_bytes=2).next()
    assert isinstance(body, h11r.CollectedBody)
    assert bytes(body.data) == b"ok"


def test_collected_body_storage_outlives_connection_and_conversion_is_explicit() -> (
    None
):
    connection, collector = start_collected_request(
        b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 4\r\n\r\nbody",
        max_bytes=4,
    )
    body = collector.next()
    assert isinstance(body, h11r.CollectedBody)
    view = body.data

    del body
    del collector
    del connection
    gc.collect()

    copied = bytes(view)
    assert copied == b"body"
    assert isinstance(copied, bytes)
    assert not isinstance(view, bytes)


def test_body_collector_limit_error_has_observed_size_and_poisons_receive() -> None:
    connection, collector = start_collected_request(
        b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 4\r\n\r\nbody",
        max_bytes=3,
    )

    with pytest.raises(h11r.BodyTooLarge) as raised:
        collector.next()
    assert raised.value.max_bytes == 3
    assert raised.value.observed_bytes == 4

    receive_operations = [
        lambda: connection.receive_data(b""),
        lambda: connection.receive_buffer(1),
        connection.next_event,
        lambda: connection.collect_body(max_bytes=3),
        connection.start_next_cycle,
    ]
    for operation in receive_operations:
        with pytest.raises(RuntimeError, match="receive processing is unusable"):
            operation()

    response = connection.send_response(413, [("Content-Length", "0")])
    assert response.startswith(b"HTTP/1.1 413")
    assert connection.end_of_message() == b""
    connection.close()


@pytest.mark.parametrize("max_bytes", [-1, 1 << 128])
def test_body_collector_validates_max_bytes(max_bytes: int) -> None:
    connection = h11r.Connection(h11r.Role.SERVER)
    connection.receive_data(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
    assert isinstance(connection.next_event(), h11r.Request)

    with pytest.raises((ValueError, OverflowError)):
        connection.collect_body(max_bytes=max_bytes)

    collector = connection.collect_body(max_bytes=0)
    assert isinstance(collector.next(), h11r.CollectedBody)


def test_body_collector_requires_the_immediately_returned_message_head() -> None:
    connection = h11r.Connection(h11r.Role.SERVER)
    with pytest.raises(RuntimeError, match="immediately"):
        connection.collect_body(max_bytes=0)

    connection.receive_data(b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 1\r\n\r\n")
    assert isinstance(connection.next_event(), h11r.Request)
    assert connection.next_event() is h11r.ReceiveStatus.NEED_DATA
    with pytest.raises(RuntimeError, match="immediately"):
        connection.collect_body(max_bytes=1)

    connection.receive_data(b"x")
    assert isinstance(connection.next_event(), h11r.Data)
    with pytest.raises(RuntimeError, match="immediately"):
        connection.collect_body(max_bytes=1)


def test_body_collector_rejects_double_collection_and_next_event() -> None:
    connection, collector = start_collected_request(
        b"GET / HTTP/1.1\r\nHost: x\r\n\r\n",
        max_bytes=0,
    )

    with pytest.raises(RuntimeError, match="active body collector"):
        connection.next_event()
    with pytest.raises(RuntimeError, match="already has"):
        connection.collect_body(max_bytes=0)
    with pytest.raises(RuntimeError, match="active body collector"):
        connection.start_next_cycle()

    assert isinstance(collector.next(), h11r.CollectedBody)


def test_body_collector_safe_early_abort_restores_streaming() -> None:
    connection, collector = start_collected_request(
        b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 4\r\n\r\nbody",
        max_bytes=4,
    )

    collector.abort()
    data = connection.next_event()
    end = connection.next_event()

    assert isinstance(data, h11r.Data)
    assert data.data == b"body"
    assert isinstance(end, h11r.EndOfMessage)
    collector.abort()
    with pytest.raises(RuntimeError, match="aborted"):
        collector.next()


def test_body_collector_late_abort_poisons_only_receive_processing() -> None:
    connection, collector = start_collected_request(
        b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 4\r\n\r\nbo",
        max_bytes=4,
    )
    assert collector.next() is h11r.ReceiveStatus.NEED_DATA

    collector.abort()
    with pytest.raises(RuntimeError, match="receive processing is unusable"):
        connection.receive_data(b"dy")

    response = connection.send_response(400, [("Content-Length", "0")])
    assert response.startswith(b"HTTP/1.1 400")
    connection.end_of_message()
    connection.close()


def test_body_collector_drop_before_and_after_consumption() -> None:
    safe_connection, safe_collector = start_collected_request(
        b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 1\r\n\r\nx",
        max_bytes=1,
    )
    del safe_collector
    gc.collect()
    assert isinstance(safe_connection.next_event(), h11r.Data)

    poisoned_connection, poisoned_collector = start_collected_request(
        b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 2\r\n\r\nx",
        max_bytes=2,
    )
    assert poisoned_collector.next() is h11r.ReceiveStatus.NEED_DATA
    del poisoned_collector
    gc.collect()
    with pytest.raises(RuntimeError, match="receive processing is unusable"):
        poisoned_connection.receive_data(b"y")


def test_body_collector_allows_send_and_receive_buffer_operations() -> None:
    connection, collector = start_collected_request(
        b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 4\r\nExpect: 100-continue\r\n\r\n",
        max_bytes=4,
    )

    assert connection.send_informational_response(100).startswith(b"HTTP/1.1 100")
    assert connection.send_response(200, [("Content-Length", "2")]).startswith(
        b"HTTP/1.1 200"
    )
    assert connection.send_data(b"ok") == b"ok"
    assert connection.end_of_message() == b""

    write_receive_buffer(connection, b"body", size=8)
    body = collector.next()
    assert isinstance(body, h11r.CollectedBody)
    assert bytes(body.data) == b"body"
    connection.close()


def test_body_collector_preserves_pipelining_boundary() -> None:
    first = b"POST /one HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\n\r\none"
    second = b"GET /two HTTP/1.1\r\nHost: x\r\n\r\n"
    connection, collector = start_collected_request(first + second, max_bytes=3)

    body = collector.next()
    assert isinstance(body, h11r.CollectedBody)
    assert bytes(body.data) == b"one"
    assert connection.next_event() is h11r.ReceiveStatus.PAUSED

    connection.send_response(204)
    connection.end_of_message()
    connection.start_next_cycle()
    request = connection.next_event()
    assert isinstance(request, h11r.Request)
    assert request.target == b"/two"


def test_body_collector_preserves_upgrade_handoff_and_remote_errors() -> None:
    upgrade, collector = start_collected_request(
        b"GET / HTTP/1.1\r\n"
        b"Host: x\r\n"
        b"Connection: upgrade\r\n"
        b"Upgrade: example\r\n\r\n"
        b"next-protocol",
        max_bytes=0,
    )
    assert isinstance(collector.next(), h11r.CollectedBody)
    assert upgrade.next_event() is h11r.ReceiveStatus.PAUSED
    assert upgrade.trailing_data == (b"next-protocol", False)

    malformed, malformed_collector = start_collected_request(
        b"POST / HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\ninvalid\r\n",
        max_bytes=100,
    )
    with pytest.raises(h11r.RemoteProtocolError):
        malformed_collector.next()
    assert malformed.peer_state is h11r.State.ERROR


def test_body_types_are_nonconstructible_and_final() -> None:
    for body_type in (h11r.BodyCollector, h11r.CollectedBody):
        with pytest.raises(TypeError):
            body_type()
        with pytest.raises(TypeError):
            type(f"{body_type.__name__}Subclass", (body_type,), {})


def test_buffer_inputs_must_be_contiguous() -> None:
    connection = h11r.Connection(h11r.Role.SERVER)
    with pytest.raises(ValueError, match="C-contiguous"):
        connection.receive_data(memoryview(b"GET / HTTP/1.1\r\n\r\n")[::2])


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
