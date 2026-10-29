# Security Policy

## Supported Versions

Security fixes are provided for the latest released version of h11r. Reports
that affect the current `main` branch are also accepted, including issues not
yet present in a release. Older releases are not maintained separately.

## Scope

h11r is a protocol engine without network I/O. Report a vulnerability when
h11r accepts or emits bytes that let HTTP/1.1 implementations disagree about
message boundaries, such as request smuggling or response splitting; crashes,
panics, or loops on peer input; or exceeds a documented limit.

h11r rejects ambiguous message framing, bounds each head and trailer section
and the chunk-extension bytes of a message with its configured limits, and
refuses to reuse a client connection when unsolicited bytes follow a response.
Callers own body size limits, timeouts and slow-peer defenses, read
back-pressure, connection and concurrency limits, TLS, and application policy.

## Reporting a Vulnerability

Please report suspected vulnerabilities through
[GitHub private vulnerability reporting ↗](https://github.com/cnzakii/h11r/security/advisories/new).

Do not open a public issue or pull request for a suspected vulnerability.

Include the affected version, a minimal reproducer, and the potential impact.
Please allow reasonable time for investigation and a fix before public
disclosure.
