"""Benchmark Python receive-buffer inputs at the Python/Rust boundary."""

from __future__ import annotations

import subprocess
import sys
from collections.abc import Callable

import h11r
import pyperf

CHUNK_SIZE = 64 * 1024
CONTENT_LENGTH = 1 << 60
REQUEST_HEAD = (
    b"POST /upload HTTP/1.1\r\n"
    b"Host: example.test\r\n"
    b"Content-Length: " + str(CONTENT_LENGTH).encode() + b"\r\n\r\n"
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


if __name__ == "__main__":
    main()
