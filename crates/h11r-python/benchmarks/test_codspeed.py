"""Track h11r's side of the h11 comparison workloads with CodSpeed."""

import pytest
from compare_h11 import (
    API_GET,
    API_RESPONSE,
    LARGE_BODY_SIZE,
    SCENARIOS,
    CandidateDownload,
    CandidateRoundTrip,
    CandidateServerCycle,
)


@pytest.mark.parametrize(
    "parts", [parts for _, parts in SCENARIOS], ids=[name for name, _ in SCENARIOS]
)
def test_server_cycle(benchmark, parts):
    _, count, _, _, response = benchmark(CandidateServerCycle(parts))

    assert count >= 2
    assert response == API_RESPONSE


def test_client_server_round_trip(benchmark):
    request, response, _, _ = benchmark(CandidateRoundTrip())

    assert (request, response) == (API_GET, API_RESPONSE)


def test_download(benchmark):
    wire = benchmark(CandidateDownload())

    assert sum(map(len, wire)) > LARGE_BODY_SIZE
