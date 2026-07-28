"""Benchmark focused Python receive and full-body collection paths.

The 64 KiB boundary cases isolate Python-to-Rust input choices. A matching
socketpair pair includes ``recv()`` or ``recv_into()`` and completes one
request/response cycle per call. The collection cases compare streaming into a
``bytearray`` with ``BodyCollector``, including a 1 MiB body in 32 KiB
fragments.
"""

from __future__ import annotations

import queue
import socket
import subprocess
import sys
from collections.abc import Callable
from threading import Event, Thread

import h11r
import pyperf

CHUNK_SIZE = 64 * 1024
FRAGMENTED_BODY_SIZE = 1024 * 1024
FRAGMENTED_CHUNK_SIZE = 32 * 1024
CONTENT_LENGTH = 1 << 60
REQUEST_HEAD = (
    b"POST /upload HTTP/1.1\r\n"
    b"Host: example.test\r\n"
    b"Content-Length: " + str(CONTENT_LENGTH).encode() + b"\r\n\r\n"
)


def collection_head(body_size: int) -> bytes:
    return (
        b"POST /upload HTTP/1.1\r\n"
        b"Host: example.test\r\n"
        b"Content-Length: " + str(body_size).encode() + b"\r\n\r\n"
    )


def git_metadata() -> tuple[str, str]:
    try:
        revision = subprocess.run(
            ("git", "rev-parse", "HEAD"),
            capture_output=True,
            check=False,
            text=True,
        )
        status = subprocess.run(
            ("git", "status", "--short"),
            capture_output=True,
            check=False,
            text=True,
        )
    except OSError:
        return "unknown", "unknown"
    if revision.returncode != 0 or status.returncode != 0:
        return "unknown", "unknown"
    return revision.stdout.strip(), str(bool(status.stdout.strip())).lower()


def is_free_threaded() -> bool:
    is_gil_enabled = getattr(sys, "_is_gil_enabled", None)
    return is_gil_enabled is not None and not is_gil_enabled()


def server_connection() -> h11r.Connection:
    connection = h11r.Connection(h11r.Role.SERVER)
    connection.receive_data(REQUEST_HEAD)
    event = connection.next_event()
    if not isinstance(event, h11r.Request):
        raise AssertionError(f"expected Request, got {event!r}")
    return connection


def fresh_bytes_workload() -> Callable[[], None]:
    connection = server_connection()
    backing = bytearray(CHUNK_SIZE)

    def receive_fresh_bytes() -> None:
        data = bytes(backing)
        connection.receive_data(data)
        event = connection.next_event()
        if not isinstance(event, h11r.Data) or len(event.data) != CHUNK_SIZE:
            raise AssertionError(f"expected {CHUNK_SIZE}-byte Data, got {event!r}")

    return receive_fresh_bytes


def memoryview_workload() -> Callable[[], None]:
    connection = server_connection()
    data = memoryview(bytearray(CHUNK_SIZE))

    def receive_memoryview() -> None:
        connection.receive_data(data)
        event = connection.next_event()
        if not isinstance(event, h11r.Data) or len(event.data) != CHUNK_SIZE:
            raise AssertionError(f"expected {CHUNK_SIZE}-byte Data, got {event!r}")

    return receive_memoryview


def bytearray_workload() -> Callable[[], None]:
    connection = server_connection()
    data = bytearray(CHUNK_SIZE)

    def receive_bytearray() -> None:
        connection.receive_data(data)
        event = connection.next_event()
        if not isinstance(event, h11r.Data) or len(event.data) != CHUNK_SIZE:
            raise AssertionError(f"expected {CHUNK_SIZE}-byte Data, got {event!r}")

    return receive_bytearray


def receive_buffer_workload() -> Callable[[], None]:
    connection = server_connection()

    def receive_reused_lease() -> None:
        lease = connection.receive_buffer(CHUNK_SIZE).acquire()
        lease.commit(CHUNK_SIZE)
        event = connection.next_event()
        if not isinstance(event, h11r.Data) or len(event.data) != CHUNK_SIZE:
            raise AssertionError(f"expected {CHUNK_SIZE}-byte Data, got {event!r}")

    return receive_reused_lease


class SocketpairWorkload:
    """Parse equivalent 64 KiB requests read from a synchronized socketpair."""

    def __init__(self, *, receive_into: bool) -> None:
        self.receive_into = receive_into
        self.connection = h11r.Connection(h11r.Role.SERVER)
        self.request = collection_head(CHUNK_SIZE) + b"x" * CHUNK_SIZE
        self.reader, self.writer = socket.socketpair()
        self.requests: queue.SimpleQueue[bool | None] = queue.SimpleQueue()
        self.stopping = Event()
        self.feeder = Thread(target=self._feed, daemon=True)
        self.feeder.start()

    def _feed(self) -> None:
        try:
            while not self.stopping.is_set():
                requested = self.requests.get()
                if requested is None:
                    return
                self.writer.sendall(self.request)
        except OSError:
            if not self.stopping.is_set():
                raise

    def run(self) -> None:
        self.requests.put(True)
        request_seen = False
        body_size = 0

        while True:
            event = self.connection.next_event()
            if event is h11r.ReceiveStatus.NEED_DATA:
                if self.receive_into:
                    with self.connection.receive_buffer(CHUNK_SIZE) as receive_buffer:
                        received = self.reader.recv_into(receive_buffer)
                        receive_buffer.commit(received)
                else:
                    self.connection.receive_data(self.reader.recv(CHUNK_SIZE))
            elif isinstance(event, h11r.Request):
                request_seen = True
            elif isinstance(event, h11r.Data):
                body_size += len(event.data)
            elif isinstance(event, h11r.EndOfMessage):
                if not request_seen or body_size != CHUNK_SIZE:
                    raise AssertionError("socketpair paths parsed different requests")
                self.connection.send_response(204)
                self.connection.end_of_message()
                self.connection.start_next_cycle()
                return
            else:
                raise AssertionError(f"expected request event, got {event!r}")

    def close(self) -> None:
        self.stopping.set()
        self.requests.put(None)
        self.reader.close()
        self.writer.close()
        self.feeder.join(timeout=1)


class CollectionWorkload:
    def __init__(
        self,
        body_size: int = CHUNK_SIZE,
        *,
        fragment_size: int | None = None,
    ) -> None:
        self.body_size = body_size
        self.connection = h11r.Connection(h11r.Role.SERVER)
        head = collection_head(body_size)
        if fragment_size is None:
            self.initial_data = head + b"x" * body_size
            self.fragments: tuple[bytes, ...] = ()
        else:
            if body_size % fragment_size:
                raise ValueError("body size must be divisible by fragment size")
            self.initial_data = head
            fragment = b"x" * fragment_size
            self.fragments = (fragment,) * (body_size // fragment_size)

    def start(self) -> None:
        self.connection.receive_data(self.initial_data)
        event = self.connection.next_event()
        if not isinstance(event, h11r.Request):
            raise AssertionError(f"expected Request, got {event!r}")

    def finish(self) -> None:
        self.connection.send_response(204)
        self.connection.end_of_message()
        self.connection.start_next_cycle()

    def streaming_bytearray(self) -> None:
        self.start()
        body = bytearray()
        inputs: tuple[bytes | None, ...] = self.fragments or (None,)
        for fragment in inputs:
            if fragment is not None:
                self.connection.receive_data(fragment)
            while True:
                event = self.connection.next_event()
                if isinstance(event, h11r.Data):
                    body.extend(event.data)
                elif event is h11r.ReceiveStatus.NEED_DATA:
                    break
                elif isinstance(event, h11r.EndOfMessage):
                    if len(body) != self.body_size:
                        raise AssertionError(f"expected {self.body_size} body bytes")
                    self.finish()
                    return
                else:
                    raise AssertionError(f"expected body event, got {event!r}")
        raise AssertionError("streaming body did not finish")

    def body_collector(self) -> None:
        self.start()
        collector = self.connection.collect_body(max_bytes=self.body_size)
        inputs: tuple[bytes | None, ...] = self.fragments or (None,)
        for fragment in inputs:
            if fragment is not None:
                self.connection.receive_data(fragment)
            result = collector.next()
            if result is h11r.ReceiveStatus.NEED_DATA:
                continue
            if not isinstance(result, h11r.CollectedBody):
                raise AssertionError(f"expected CollectedBody, got {result!r}")
            if len(result.data) != self.body_size:
                raise AssertionError(f"expected {self.body_size} body bytes")
            self.finish()
            return
        raise AssertionError("collected body did not finish")


def main() -> None:
    revision, dirty = git_metadata()
    runner = pyperf.Runner(
        metadata={
            "h11r_version": h11r.__version__,
            "git_revision": revision,
            "git_dirty": dirty,
            "chunk_size": CHUNK_SIZE,
            "free_threaded": str(is_free_threaded()).lower(),
        }
    )
    runner.bench_func("receive_data/fresh_bytes_64k", fresh_bytes_workload())
    runner.bench_func("receive_data/reused_memoryview_64k", memoryview_workload())
    runner.bench_func("receive_data/reused_bytearray_64k", bytearray_workload())
    runner.bench_func("receive_buffer/reused_lease_64k", receive_buffer_workload())

    socket_bytes = SocketpairWorkload(receive_into=False)
    try:
        runner.bench_func(
            "socketpair/recv_receive_data_64k",
            socket_bytes.run,
        )
    finally:
        socket_bytes.close()

    socket_buffer = SocketpairWorkload(receive_into=True)
    try:
        runner.bench_func(
            "socketpair/recv_into_receive_buffer_64k",
            socket_buffer.run,
        )
    finally:
        socket_buffer.close()

    streaming = CollectionWorkload()
    runner.bench_func(
        "body_collection/streaming_bytearray_64k",
        streaming.streaming_bytearray,
    )
    collecting = CollectionWorkload()
    runner.bench_func(
        "body_collection/body_collector_64k",
        collecting.body_collector,
    )

    fragmented_streaming = CollectionWorkload(
        FRAGMENTED_BODY_SIZE,
        fragment_size=FRAGMENTED_CHUNK_SIZE,
    )
    runner.bench_func(
        "body_collection/streaming_bytearray_1mib_32k_chunks",
        fragmented_streaming.streaming_bytearray,
    )
    fragmented_collecting = CollectionWorkload(
        FRAGMENTED_BODY_SIZE,
        fragment_size=FRAGMENTED_CHUNK_SIZE,
    )
    runner.bench_func(
        "body_collection/body_collector_1mib_32k_chunks",
        fragmented_collecting.body_collector,
    )


if __name__ == "__main__":
    main()
