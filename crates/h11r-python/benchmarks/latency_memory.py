"""Measure round-trip latency percentiles and memory per idle connection.

Latency follows gRPC's contentionless latency benchmark: one exchange in
flight at a time, reporting the median and the tail. Each client/server round
trip is timed on its own with the garbage collector enabled, so allocation and
collection pauses appear in the tail. Memory counts what a server holds for a
keep-alive connection between requests. Each measurement runs in a fresh
interpreter.
https://grpc.io/docs/guides/benchmarking/
"""

from __future__ import annotations

import argparse
import json
import math
import platform
import statistics
import subprocess
import sys
import time
from datetime import date
from pathlib import Path

import h11 as reference
import h11r as candidate
import psutil
from compare_h11 import (
    API_GET,
    API_RESPONSE_HEADERS,
    JSON_BODY,
    CandidateRoundTrip,
    H11RoundTrip,
    machine_name,
    platform_name,
)

IMPLEMENTATIONS = ("h11r", "h11")
WARMUP = 10_000
SAMPLES = 200_000
IDLE_CONNECTIONS = 20_000
# A single run's tail moves with scheduling noise; report the median of several.
LATENCY_RUNS = 5


def latency(implementation: str) -> dict[str, float]:
    workload = CandidateRoundTrip() if implementation == "h11r" else H11RoundTrip()
    for _ in range(WARMUP):
        workload()
    clock = time.perf_counter_ns
    samples = []
    for _ in range(SAMPLES):
        start = clock()
        workload()
        samples.append(clock() - start)
    samples.sort()
    return {
        "p50_us": percentile(samples, 50) / 1000,
        "p99_us": percentile(samples, 99) / 1000,
    }


def percentile(ordered: list[int], rank: int) -> int:
    """The nearest-rank percentile of already sorted samples."""
    return ordered[math.ceil(len(ordered) * rank / 100) - 1]


def idle_connection(implementation: str) -> object:
    """Returns a server connection that finished one exchange and awaits the next."""
    if implementation == "h11r":
        connection = candidate.Connection(candidate.Role.SERVER)
        connection.receive_data(API_GET)
        connection.next_event()
        connection.next_event()
        connection.send_response(200, API_RESPONSE_HEADERS, reason=b"OK")
        connection.send_data(JSON_BODY)
        connection.end_of_message()
    else:
        connection = reference.Connection(reference.SERVER)
        connection.receive_data(API_GET)
        connection.next_event()
        connection.next_event()
        connection.send(
            reference.Response(
                status_code=200, headers=API_RESPONSE_HEADERS, reason=b"OK"
            )
        )
        connection.send(reference.Data(data=JSON_BODY))
        connection.send(reference.EndOfMessage())
    connection.start_next_cycle()
    return connection


def memory(implementation: str) -> dict[str, float]:
    # Rust allocations bypass tracemalloc, so both libraries are measured by RSS.
    idle_connection(implementation)
    process = psutil.Process()
    before = process.memory_info().rss
    held = [idle_connection(implementation) for _ in range(IDLE_CONNECTIONS)]
    after = process.memory_info().rss
    assert len(held) == IDLE_CONNECTIONS
    return {"idle_connection_kib": (after - before) / IDLE_CONNECTIONS / 1024}


def measure(kind: str, implementation: str) -> dict[str, float]:
    result = subprocess.run(
        (sys.executable, __file__, "--worker", kind, implementation),
        capture_output=True,
        check=True,
        text=True,
    )
    return json.loads(result.stdout)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=False)
    parser.add_argument("--worker", nargs=2, metavar=("KIND", "IMPLEMENTATION"))
    args = parser.parse_args()
    if args.worker:
        kind, implementation = args.worker
        work = latency if kind == "latency" else memory
        print(json.dumps(work(implementation)))
        return

    results = {}
    for implementation in IMPLEMENTATIONS:
        runs = [measure("latency", implementation) for _ in range(LATENCY_RUNS)]
        results[implementation] = {
            key: statistics.median(run[key] for run in runs) for key in runs[0]
        } | measure("memory", implementation)
    document = {
        "metadata": {
            "h11r_version": candidate.__version__,
            "h11_version": reference.__version__,
            "python_version": platform.python_version(),
            "machine_name": machine_name(),
            "platform": platform_name(),
            "date": date.today().isoformat(),
            "samples": SAMPLES,
            "latency_runs": LATENCY_RUNS,
            "idle_connections": IDLE_CONNECTIONS,
        },
        "results": results,
    }
    text = json.dumps(document, indent=2) + "\n"
    if args.output:
        args.output.write_text(text, encoding="utf-8")
    else:
        print(text, end="")


if __name__ == "__main__":
    main()
