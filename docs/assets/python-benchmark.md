| Scenario | h11r (µs/op) | h11 (µs/op) | Relative throughput |
| --- | ---: | ---: | ---: |
| API GET · 10 fields | 1.34 | 27.13 | 20.3× |
| JSON POST · 1 KiB | 1.53 | 30.81 | 20.2× |
| Fragmented GET · 16 B reads | 3.29 | 46.74 | 14.2× |
| Chunked upload · 64 KiB | 6.36 | 85.22 | 13.4× |
| Client/server round trip | 2.32 | 52.87 | 22.8× |

_Environment: h11r 0.3.0 · h11 0.16.0 · CPython 3.13.12 (64-bit) · Apple M4 · macOS-27.2-arm64-arm-64bit-Mach-O · 2026-10-31_
