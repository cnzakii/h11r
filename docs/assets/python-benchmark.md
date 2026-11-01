| Scenario | h11r (µs/op) | h11 (µs/op) | Relative throughput |
| --- | ---: | ---: | ---: |
| API GET · 10 fields | 1.21 | 27.03 | 22.4× |
| JSON POST · 1 KiB | 1.44 | 30.63 | 21.3× |
| Fragmented GET · 16 B reads | 3.27 | 46.23 | 14.1× |
| Chunked upload · 64 KiB | 6.49 | 83.97 | 12.9× |
| Upload · 1 MiB in 64 KiB reads | 3.96 | 127.04 | 32.1× |
| Download · 1 MiB in 64 KiB writes | 1.81 | 42.06 | 23.3× |
| Client/server round trip | 2.11 | 53.43 | 25.4× |

_Environment: h11r 0.3.1 · h11 0.16.0 · CPython 3.13.12 (64-bit) · Apple M4 · macOS-27.2-arm64-arm-64bit-Mach-O · 2026-11-01_
