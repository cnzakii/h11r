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
