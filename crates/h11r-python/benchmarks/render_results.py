"""Render the public benchmark tables from the recorded results."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import pyperf

TABLE_START = "<!-- benchmark-table:start -->"
TABLE_END = "<!-- benchmark-table:end -->"
SCENARIOS = (
    ("api_get", "API GET · 10 fields"),
    ("json_post_1k", "JSON POST · 1 KiB"),
    ("fragmented_get_16b", "Fragmented GET · 16 B reads"),
    ("chunked_upload_64k", "Chunked upload · 64 KiB"),
    ("upload_1m", "Upload · 1 MiB in 64 KiB reads"),
    ("download_1m", "Download · 1 MiB in 64 KiB writes"),
    ("plaintext_pipelined_16", "TechEmpower Plaintext · 16 pipelined"),
    ("client_server_round_trip", "Client/server round trip"),
)


def load_results(path: Path) -> tuple[list[tuple[str, float, float]], str]:
    suite = pyperf.BenchmarkSuite.load(str(path))
    metadata = suite.get_metadata()
    means = {benchmark.get_name(): benchmark.mean() for benchmark in suite}
    rows = []
    for key, label in SCENARIOS:
        try:
            h11r = means[f"scenario/{key}/h11r"]
            h11 = means[f"scenario/{key}/h11-{metadata['h11_version']}"]
        except KeyError as error:
            raise ValueError(f"missing benchmark: {error.args[0]}") from None
        rows.append((label, h11r, h11))

    _, finished = suite.get_dates()
    versions = [
        f"h11r {metadata['h11r_version']}",
        f"h11 {metadata['h11_version']}",
        f"CPython {metadata['python_version']}",
    ]
    environment = [
        metadata.get("machine_name")
        or metadata.get("cpu_model_name")
        or f"{metadata.get('cpu_count', '?')} CPUs",
        metadata.get("platform"),
        finished.date().isoformat(),
    ]
    return rows, "\n".join(
        " · ".join(str(value) for value in line if value)
        for line in (versions, environment)
    )


def render_table(
    rows: list[tuple[str, float, float]], extra: dict, details: str
) -> str:
    lines = [
        "| Scenario | h11r (µs/op) | h11 (µs/op) | Relative throughput |",
        "| --- | ---: | ---: | ---: |",
    ]
    for label, h11r, h11 in rows:
        lines.append(
            f"| {label} | {h11r * 1e6:.2f} | {h11 * 1e6:.2f} | {h11 / h11r:.1f}× |"
        )
    if details:
        lines.extend(("", f"_Environment: {details.replace(chr(10), ' · ')}_"))
    h11r, h11 = extra["results"]["h11r"], extra["results"]["h11"]
    lines.extend(
        (
            "",
            "| Measurement | h11r | h11 | h11 ÷ h11r |",
            "| --- | ---: | ---: | ---: |",
        )
    )
    for key, label, unit in (
        ("p50_us", "Round-trip latency · P50", "µs"),
        ("p99_us", "Round-trip latency · P99", "µs"),
        ("idle_connection_kib", "Memory per idle connection", "KiB"),
    ):
        lines.append(
            f"| {label} | {h11r[key]:.2f} {unit} | {h11[key]:.2f} {unit} "
            f"| {h11[key] / h11r[key]:.1f}× |"
        )
    # The two result files can be refreshed separately, so each table names
    # the environment it was measured in.
    metadata = extra["metadata"]
    environment = (
        f"h11r {metadata['h11r_version']}",
        f"h11 {metadata['h11_version']}",
        f"CPython {metadata['python_version']}",
        metadata["machine_name"],
        metadata["platform"],
        metadata["date"],
    )
    lines.extend(("", f"_Environment: {' · '.join(environment)}_"))
    return "\n".join(lines) + "\n"


def replace_marked_table(document: str, table: str) -> str:
    """Return the document with the text between the table markers replaced."""
    before, start, rest = document.partition(TABLE_START)
    _, end, after = rest.partition(TABLE_END)
    if not start or not end:
        raise ValueError("benchmark table markers not found")
    return f"{before}{TABLE_START}\n{table}{TABLE_END}{after}"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("results", type=Path)
    parser.add_argument(
        "latency_memory", type=Path, help="the output of latency_memory.py"
    )
    parser.add_argument("--table", type=Path)
    parser.add_argument(
        "--readme",
        type=Path,
        help="replace the table between the README benchmark markers",
    )
    args = parser.parse_args()

    rows, details = load_results(args.results)
    extra = json.loads(args.latency_memory.read_text(encoding="utf-8"))
    table = render_table(rows, extra, details)
    if args.table:
        args.table.parent.mkdir(parents=True, exist_ok=True)
        args.table.write_text(table, encoding="utf-8")
    else:
        print(table, end="")
    if args.readme:
        args.readme.write_text(
            replace_marked_table(args.readme.read_text(encoding="utf-8"), table),
            encoding="utf-8",
        )


if __name__ == "__main__":
    main()
