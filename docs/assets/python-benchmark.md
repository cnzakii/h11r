| Scenario | h11r (µs/op) | h11 (µs/op) | Relative throughput |
| --- | ---: | ---: | ---: |
| API GET · 10 fields | 3.23 | 99.53 | 30.8× |
| JSON POST · 1 KiB | 3.82 | 111.95 | 29.3× |
| Fragmented GET · 16 B reads | 8.01 | 149.50 | 18.7× |
| Chunked upload · 64 KiB | 20.68 | 275.27 | 13.3× |
| Upload · 1 MiB in 64 KiB reads | 10.85 | 315.73 | 29.1× |
| Download · 1 MiB in 64 KiB writes | 4.68 | 141.76 | 30.3× |
| TechEmpower Plaintext · 16 pipelined | 30.31 | 1170.63 | 38.6× |
| Client/server round trip | 5.14 | 189.82 | 37.0× |

_Environment: h11r 0.4.0 · h11 0.16.0 · CPython 3.13.5 (64-bit) · Intel(R) Xeon(R) Platinum 8336C CPU @ 2.30GHz · Linux x86_64 · 2026-11-23_

| Measurement | h11r | h11 | h11 ÷ h11r |
| --- | ---: | ---: | ---: |
| Round-trip latency · P50 | 5.06 µs | 183.48 µs | 36.3× |
| Round-trip latency · P99 | 10.15 µs | 288.11 µs | 28.4× |
| Memory per idle connection | 0.75 KiB | 0.94 KiB | 1.2× |

_Environment: h11r 0.4.0 · h11 0.16.0 · CPython 3.13.5 · Intel(R) Xeon(R) Platinum 8336C CPU @ 2.30GHz · Linux x86_64 · 2026-11-23_
