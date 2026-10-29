| Scenario | h11r (µs/op) | h11 (µs/op) | Relative throughput |
| --- | ---: | ---: | ---: |
| API GET · 10 fields | 1.59 | 27.54 | 17.3× |
| JSON POST · 1 KiB | 1.83 | 32.58 | 17.8× |
| Fragmented GET · 16 B reads | 3.39 | 49.17 | 14.5× |
| Chunked upload · 64 KiB | 7.28 | 90.48 | 12.4× |
| Client/server round trip | 2.77 | 53.83 | 19.5× |

_Environment: h11r 0.2.2 · h11 0.16.0 · CPython 3.13.12 (64-bit) · Apple M4 · macOS-27.2-arm64-arm-64bit-Mach-O · 2026-10-29_
