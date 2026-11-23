| Scenario | h11r (µs/op) | h11 (µs/op) | Relative throughput |
| --- | ---: | ---: | ---: |
| API GET · 10 fields | 1.28 | 28.51 | 22.4× |
| JSON POST · 1 KiB | 1.47 | 32.12 | 21.8× |
| Fragmented GET · 16 B reads | 3.32 | 47.24 | 14.2× |
| Chunked upload · 64 KiB | 6.72 | 86.94 | 12.9× |
| Upload · 1 MiB in 64 KiB reads | 4.19 | 131.77 | 31.5× |
| Download · 1 MiB in 64 KiB writes | 1.86 | 42.62 | 22.9× |
| TechEmpower Plaintext · 16 pipelined | 10.64 | 293.50 | 27.6× |
| Client/server round trip | 2.13 | 54.13 | 25.4× |

_Environment: h11r 0.3.1 · h11 0.16.0 · CPython 3.13.12 (64-bit) · Apple M4 · macOS-27.2.1-arm64-arm-64bit-Mach-O · 2026-11-22_

| Measurement | h11r | h11 | h11 ÷ h11r |
| --- | ---: | ---: | ---: |
| Round-trip latency · P50 | 2.17 µs | 53.17 µs | 24.5× |
| Round-trip latency · P99 | 2.58 µs | 63.08 µs | 24.4× |
| Memory per idle connection | 0.78 KiB | 0.94 KiB | 1.2× |

_Environment: h11r 0.3.1 · h11 0.16.0 · CPython 3.13.12 · Apple M4 · macOS-27.2.1-arm64-arm-64bit-Mach-O · 2026-11-22_
