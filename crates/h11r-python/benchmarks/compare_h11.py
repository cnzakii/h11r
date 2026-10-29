"""Compare steady-state HTTP/1 scenarios through the public Python APIs.\n\nEach scenario carries the header fields of a browser calling a JSON API, and\nthe server answers with typical response fields and a JSON body.\n"""

from __future__ import annotations

import argparse
import platform
import subprocess
import sys

import h11 as reference
import h11r as candidate
import pyperf

# Field sets modeled on a browser calling a JSON API and its server's reply.
API_REQUEST_HEADERS = (
    (b"Host", b"api.example.test"),
    (
        b"User-Agent",
        b"Mozilla/5.0 (Macintosh; Intel Mac OS X 14_5) AppleWebKit/605.1.15 "
        b"(KHTML, like Gecko) Version/17.5 Safari/605.1.15",
    ),
    (b"Accept", b"application/json, text/plain, */*"),
    (b"Accept-Language", b"en-US,en;q=0.9"),
    (b"Accept-Encoding", b"gzip, deflate, br"),
    (b"Referer", b"https://app.example.test/dashboard"),
    (b"Origin", b"https://app.example.test"),
    (b"Cookie", b"session=4f1c2b7e9a0d4c3b8e6f5a1d2c3b4a59; theme=dark"),
    (b"Sec-Fetch-Mode", b"cors"),
    (b"Connection", b"keep-alive"),
)
JSON_BODY = (
    b'{"items":['
    + b",".join(b'{"id":%d,"name":"item-%d"}' % (index, index) for index in range(20))
    + b"]}"
)
API_RESPONSE_HEADERS = (
    (b"Content-Type", b"application/json"),
    (b"Content-Length", b"%d" % len(JSON_BODY)),
    (b"Date", b"Wed, 29 Oct 2026 12:00:00 GMT"),
    (b"Server", b"example"),
    (b"Cache-Control", b"no-store"),
    (b"Vary", b"Origin, Accept-Encoding"),
    (b"Access-Control-Allow-Origin", b"https://app.example.test"),
)


def head(start_line: bytes, fields: tuple[tuple[bytes, bytes], ...]) -> bytes:
    lines = (start_line, *(name + b": " + value for name, value in fields))
    return b"\r\n".join(lines) + b"\r\n\r\n"


API_GET = head(b"GET /api/v1/items?page=2 HTTP/1.1", API_REQUEST_HEADERS)
POST_BODY = b'{"name":"' + b"x" * 1012 + b'"}'
JSON_POST = (
    head(
        b"POST /api/v1/items HTTP/1.1",
        (
            *API_REQUEST_HEADERS,
            (b"Content-Type", b"application/json"),
            (b"Content-Length", b"%d" % len(POST_BODY)),
        ),
    )
    + POST_BODY
)
FRAGMENTED_GET = tuple(
    API_GET[index : index + 16] for index in range(0, len(API_GET), 16)
)
CHUNK = b"x" * 4096
CHUNKED_UPLOAD = (
    head(
        b"POST /api/v1/uploads HTTP/1.1",
        (*API_REQUEST_HEADERS, (b"Transfer-Encoding", b"chunked")),
    ),
    *(b"1000\r\n" + CHUNK + b"\r\n" for _ in range(16)),
    b"0\r\nDigest: ok\r\n\r\n",
)
API_RESPONSE = head(b"HTTP/1.1 200 OK", API_RESPONSE_HEADERS) + JSON_BODY

SCENARIOS = (
    ("api_get", (API_GET,)),
    ("json_post_1k", (JSON_POST,)),
    ("fragmented_get_16b", FRAGMENTED_GET),
    ("chunked_upload_64k", CHUNKED_UPLOAD),
)


class CandidateServerCycle:
    __slots__ = ("connection", "parts")

    def __init__(self, parts: tuple[bytes, ...]) -> None:
        self.connection = candidate.Connection(candidate.Role.SERVER)
        self.parts = parts

    def __call__(self) -> tuple[bytes, int, int, object, bytes]:
        method = b""
        event_count = body_bytes = 0
        last_event: object = candidate.ReceiveStatus.NEED_DATA

        for part in self.parts:
            self.connection.receive_data(part)
            while True:
                event = self.connection.next_event()
                if (
                    event is candidate.ReceiveStatus.NEED_DATA
                    or event is candidate.ReceiveStatus.PAUSED
                ):
                    break
                event_count += 1
                last_event = event
                if isinstance(event, candidate.Request):
                    method = event.method
                elif isinstance(event, candidate.Data):
                    body_bytes += len(event.data)

        response = (
            self.connection.send_response(200, API_RESPONSE_HEADERS, reason=b"OK")
            + self.connection.send_data(JSON_BODY)
            + self.connection.end_of_message()
        )
        self.connection.start_next_cycle()
        return method, event_count, body_bytes, last_event, response


class H11ServerCycle:
    __slots__ = ("connection", "parts")

    def __init__(self, parts: tuple[bytes, ...]) -> None:
        self.connection = reference.Connection(reference.SERVER)
        self.parts = parts

    def __call__(self) -> tuple[bytes, int, int, object, bytes]:
        method = b""
        event_count = body_bytes = 0
        last_event: object = reference.NEED_DATA

        for part in self.parts:
            self.connection.receive_data(part)
            while True:
                event = self.connection.next_event()
                if event is reference.NEED_DATA or event is reference.PAUSED:
                    break
                event_count += 1
                last_event = event
                if isinstance(event, reference.Request):
                    method = event.method
                elif isinstance(event, reference.Data):
                    body_bytes += len(event.data)

        response = (
            self.connection.send(
                reference.Response(
                    status_code=200, headers=API_RESPONSE_HEADERS, reason=b"OK"
                )
            )
            + self.connection.send(reference.Data(data=JSON_BODY))
            + self.connection.send(reference.EndOfMessage())
        )
        self.connection.start_next_cycle()
        return method, event_count, body_bytes, last_event, response


class CandidateRoundTrip:
    __slots__ = ("client", "server")

    def __init__(self) -> None:
        self.client = candidate.Connection(candidate.Role.CLIENT)
        self.server = candidate.Connection(candidate.Role.SERVER)

    def __call__(self) -> tuple[bytes, bytes, object, object]:
        request_wire = (
            self.client.send_request(
                b"GET", b"/api/v1/items?page=2", API_REQUEST_HEADERS
            )
            + self.client.end_of_message()
        )
        self.server.receive_data(request_wire)
        self.server.next_event()
        request_end = self.server.next_event()

        response_wire = (
            self.server.send_response(200, API_RESPONSE_HEADERS, reason=b"OK")
            + self.server.send_data(JSON_BODY)
            + self.server.end_of_message()
        )
        self.client.receive_data(response_wire)
        self.client.next_event()
        self.client.next_event()
        response_end = self.client.next_event()

        self.client.start_next_cycle()
        self.server.start_next_cycle()
        return request_wire, response_wire, request_end, response_end


class H11RoundTrip:
    __slots__ = ("client", "server")

    def __init__(self) -> None:
        self.client = reference.Connection(reference.CLIENT)
        self.server = reference.Connection(reference.SERVER)

    def __call__(self) -> tuple[bytes, bytes, object, object]:
        request_wire = self.client.send(
            reference.Request(
                method=b"GET",
                target=b"/api/v1/items?page=2",
                headers=API_REQUEST_HEADERS,
            )
        ) + self.client.send(reference.EndOfMessage())
        self.server.receive_data(request_wire)
        self.server.next_event()
        request_end = self.server.next_event()

        response_wire = (
            self.server.send(
                reference.Response(
                    status_code=200, headers=API_RESPONSE_HEADERS, reason=b"OK"
                )
            )
            + self.server.send(reference.Data(data=JSON_BODY))
            + self.server.send(reference.EndOfMessage())
        )
        self.client.receive_data(response_wire)
        self.client.next_event()
        self.client.next_event()
        response_end = self.client.next_event()

        self.client.start_next_cycle()
        self.server.start_next_cycle()
        return request_wire, response_wire, request_end, response_end


def trailers(event: object) -> tuple[tuple[bytes, bytes], ...]:
    if isinstance(event, candidate.EndOfMessage):
        fields = event.trailers
    else:
        assert isinstance(event, reference.EndOfMessage)
        fields = event.headers
    return tuple((name.lower(), value) for name, value in fields)


def check_workloads() -> None:
    expected = {
        "api_get": (b"GET", 2, 0, ()),
        "json_post_1k": (b"POST", 3, len(POST_BODY), ()),
        "fragmented_get_16b": (b"GET", 2, 0, ()),
        "chunked_upload_64k": (b"POST", 18, 64 * 1024, ((b"digest", b"ok"),)),
    }
    for name, parts in SCENARIOS:
        for workload in (CandidateServerCycle(parts), H11ServerCycle(parts)):
            for _ in range(2):
                method, count, size, end, response = workload()
                assert (method, count, size, trailers(end)) == expected[name]
                assert response == API_RESPONSE

    for workload in (CandidateRoundTrip(), H11RoundTrip()):
        for _ in range(2):
            request, response, request_end, response_end = workload()
            assert request == API_GET
            assert response == API_RESPONSE
            assert trailers(request_end) == trailers(response_end) == ()


def machine_name() -> str:
    if sys.platform == "darwin":
        result = subprocess.run(
            ("sysctl", "-n", "machdep.cpu.brand_string"),
            capture_output=True,
            check=False,
            text=True,
        )
        if result.returncode == 0 and result.stdout.strip():
            return result.stdout.strip()
    return platform.processor() or platform.machine()


def main() -> None:
    check_workloads()

    def add_cmdline_args(command: list[str], args: argparse.Namespace) -> None:
        scenario = args.scenario
        if scenario:
            command.extend(("--scenario", scenario))
        implementation = args.implementation
        if implementation:
            command.extend(("--implementation", implementation))

    runner = pyperf.Runner(
        metadata={
            "h11r_version": candidate.__version__,
            "h11_version": reference.__version__,
            "machine_name": machine_name(),
        },
        add_cmdline_args=add_cmdline_args,
    )
    choices = [name for name, _ in SCENARIOS] + ["client_server_round_trip"]
    runner.argparser.add_argument("--scenario", choices=choices)
    runner.argparser.add_argument("--implementation", choices=("h11r", "h11"))
    args = runner.parse_args()
    for name, parts in SCENARIOS:
        if args.scenario and args.scenario != name:
            continue
        if args.implementation in (None, "h11r"):
            runner.bench_func(f"scenario/{name}/h11r", CandidateServerCycle(parts))
        if args.implementation in (None, "h11"):
            runner.bench_func(f"scenario/{name}/h11-0.16.0", H11ServerCycle(parts))
    if args.scenario in (None, "client_server_round_trip"):
        if args.implementation in (None, "h11r"):
            runner.bench_func(
                "scenario/client_server_round_trip/h11r", CandidateRoundTrip()
            )
        if args.implementation in (None, "h11"):
            runner.bench_func(
                "scenario/client_server_round_trip/h11-0.16.0", H11RoundTrip()
            )


if __name__ == "__main__":
    main()
