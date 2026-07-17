---
title: HTTP/1.1 protocol authority
description: Authority relationships among HTTP/1.1 RFCs, errata, registries, and implementation evidence.
topics: [protocol, http1, rfc, iana, errata]
checked_at: 2026-07-12
---

# HTTP/1.1 Protocol Authority

## Source Roles

**Normative source.** A standards-track RFC defines protocol requirements.
BCP 14 terms carry their normative meanings only when used as specified by
RFC 2119 and RFC 8174.

**Update and obsolescence metadata.** RFC Editor info pages identify documents
that update or obsolete another RFC. Reading a base document without its
updates can omit current requirements.

**Errata.** RFC Editor publishes errata and their status. Errata are distinct
from later RFCs that formally update a specification.

**Registry.** IANA registries provide assigned names, codepoints, references,
and registration policies. They do not replace the RFC text defining wire
behavior or state transitions.

**Implementation behavior.** A library can provide compatibility evidence or
show how an ambiguity is handled. It does not override normative protocol text.

Primary indexes:

- <https://www.rfc-editor.org/>
- <https://www.rfc-editor.org/errata.php>
- <https://www.iana.org/protocols>
- <https://www.rfc-editor.org/info/rfc8174>

## HTTP/1.1 Sources

| Topic | Current base documents | Relationship notes |
| --- | --- | --- |
| HTTP semantics | RFC 9110 | Defines methods, status codes, fields, and message semantics. |
| HTTP caching | RFC 9111 | Obsoletes RFC 7234. |
| HTTP/1.1 messaging | RFC 9112 and RFC 9931 | RFC 9112 replaces the messaging portions of RFC 7230. RFC 9931 adds optimistic-transition requirements. |

RFC 7230 remains a historical reference because older tools and
implementations cite it. It is not the current base messaging specification.

## Reading A Requirement

The following evidence chain is methodological synthesis:

1. Identify the current base RFC and every formal update.
2. Check RFC Editor errata and their status.
3. Check IANA for assigned extension points and current references.
4. Separate sender, recipient, client, server, intermediary, and connection
   requirements.
5. Separate normative requirements from examples and informative text.
6. Use conformance tools and implementation behavior as additional evidence,
   not as replacements for steps 1-5.
