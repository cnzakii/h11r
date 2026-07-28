from __future__ import annotations

import sys
import sysconfig
from concurrent.futures import ThreadPoolExecutor
from threading import Barrier

import h11r
import pytest

FREE_THREADED = bool(sysconfig.get_config_var("Py_GIL_DISABLED"))
WORKERS = 4
CYCLES = 100


@pytest.mark.skipif(not FREE_THREADED, reason="requires free-threaded CPython")
def test_import_leaves_gil_disabled() -> None:
    assert not sys._is_gil_enabled()


def test_independent_connections_do_not_share_state() -> None:
    barrier = Barrier(WORKERS)

    def exchange(worker: int) -> None:
        client = h11r.Connection(h11r.Role.CLIENT)
        server = h11r.Connection(h11r.Role.SERVER)
        barrier.wait()

        for cycle in range(CYCLES):
            identity = f"{worker}-{cycle}".encode()
            request_body = b"request-" + identity
            request_wire = client.send_request(
                "POST",
                b"/workers/" + identity,
                [
                    ("Host", "example.test"),
                    ("Content-Length", str(len(request_body))),
                ],
            )
            request_wire += client.send_data(request_body)
            request_wire += client.end_of_message()
            server.receive_data(request_wire)

            request = server.next_event()
            request_data = server.next_event()
            request_end = server.next_event()
            assert isinstance(request, h11r.Request)
            assert request.target == b"/workers/" + identity
            assert isinstance(request_data, h11r.Data)
            assert request_data.data == request_body
            assert isinstance(request_end, h11r.EndOfMessage)

            response_body = b"response-" + identity
            response_wire = server.send_response(
                200, [("Content-Length", str(len(response_body)))]
            )
            response_wire += server.send_data(response_body)
            response_wire += server.end_of_message()
            client.receive_data(response_wire)

            response = client.next_event()
            response_data = client.next_event()
            response_end = client.next_event()
            assert isinstance(response, h11r.Response)
            assert response.status_code == 200
            assert isinstance(response_data, h11r.Data)
            assert response_data.data == response_body
            assert isinstance(response_end, h11r.EndOfMessage)

            # HTTP operations on one Connection remain ordered by its caller.
            # This regression covers only module-global and cross-instance state.
            client.start_next_cycle()
            server.start_next_cycle()

    with ThreadPoolExecutor(max_workers=WORKERS) as executor:
        futures = [executor.submit(exchange, worker) for worker in range(WORKERS)]
        for future in futures:
            future.result()


@pytest.mark.skipif(not FREE_THREADED, reason="requires free-threaded CPython")
def test_receive_buffer_reservation_and_export_are_race_safe() -> None:
    connection = h11r.Connection(h11r.Role.SERVER)
    lease = connection.receive_buffer(64).acquire()
    exported = memoryview(lease)
    barrier = Barrier(3)

    def mutate_connection() -> type[BaseException]:
        barrier.wait()
        try:
            connection.receive_data(b"")
        except BaseException as error:
            return type(error)
        raise AssertionError("reserved connection mutation unexpectedly succeeded")

    def commit_exported_buffer() -> type[BaseException]:
        barrier.wait()
        try:
            lease.commit(0)
        except BaseException as error:
            return type(error)
        raise AssertionError("commit unexpectedly raced an active export")

    with ThreadPoolExecutor(max_workers=2) as executor:
        mutation = executor.submit(mutate_connection)
        commit = executor.submit(commit_exported_buffer)
        barrier.wait()
        assert mutation.result() is RuntimeError
        assert commit.result() is BufferError

    exported.release()
    lease.abort()
    connection.receive_data(b"")


@pytest.mark.skipif(not FREE_THREADED, reason="requires free-threaded CPython")
def test_body_collector_and_receive_data_are_race_safe() -> None:
    connection = h11r.Connection(h11r.Role.SERVER)
    connection.receive_data(b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 4\r\n\r\n")
    assert isinstance(connection.next_event(), h11r.Request)
    collector = connection.collect_body(max_bytes=4)
    barrier = Barrier(3)

    def receive() -> None:
        barrier.wait()
        connection.receive_data(b"body")

    def poll() -> object:
        barrier.wait()
        return collector.next()

    with ThreadPoolExecutor(max_workers=2) as executor:
        receiving = executor.submit(receive)
        polling = executor.submit(poll)
        barrier.wait()
        receiving.result()
        result = polling.result()

    if result is h11r.ReceiveStatus.NEED_DATA:
        result = collector.next()
    assert isinstance(result, h11r.CollectedBody)
    assert bytes(result.data) == b"body"
