---
title: HTTP/1.1 semantic types
description: Source-based method, status, field, and message representation facts.
topics: [http1, api, types, semantics]
checked_at: 2026-07-12
---

# HTTP/1.1 Semantic Types

## Method

RFC 9110 defines a request method as a case-sensitive `token`. Standardized
methods conventionally use uppercase US-ASCII, and the namespace remains
extensible.
([RFC 9110 Section 9.1](https://www.rfc-editor.org/rfc/rfc9110.html#section-9.1),
[IANA HTTP Method Registry](https://www.iana.org/assignments/http-methods/)).

## Status Code

A valid HTTP status code is a three-digit integer from 100 through 599. Codes
are extensible; recipients understand an unrecognized code by its first-digit
class.
([RFC 9110 Section 15](https://www.rfc-editor.org/rfc/rfc9110.html#section-15),
[IANA HTTP Status Code Registry](https://www.iana.org/assignments/http-status-codes/)).

## Fields

Field names are case-insensitive `token` values. Field values permit visible
US-ASCII, horizontal tab where grammar allows it, and opaque `obs-text` octets;
CR, LF, and NUL are invalid inside a value.
([RFC 9110 Section 5.1](https://www.rfc-editor.org/rfc/rfc9110.html#section-5.1),
[Section 5.5](https://www.rfc-editor.org/rfc/rfc9110.html#section-5.5)).

Repeated field lines retain arrival order. General comma recombination is valid
only when the field definition permits it, and `Set-Cookie` is the documented
exception.
([RFC 9110 Section 5.2](https://www.rfc-editor.org/rfc/rfc9110.html#section-5.2),
[Section 5.3](https://www.rfc-editor.org/rfc/rfc9110.html#section-5.3)).

## HTTP/1.1 Carrier

A request line carries method, request target, and version. A status line
carries version, status code, and an optional reason phrase. Start lines, field
sections, and content follow the message structure and body-length rules in
RFC 9112.
([RFC 9112 Section 2.1](https://www.rfc-editor.org/rfc/rfc9112.html#section-2.1),
[Section 3](https://www.rfc-editor.org/rfc/rfc9112.html#section-3),
[Section 4](https://www.rfc-editor.org/rfc/rfc9112.html#section-4),
[Section 6](https://www.rfc-editor.org/rfc/rfc9112.html#section-6)).

## Observed Practice: h11 0.16.0

Request events store method, target, and version as bytes; response events
store numeric status, version bytes, and reason bytes. Headers are ordered
byte pairs, with raw name casing retained internally.
([event definitions](https://github.com/python-hyper/h11/blob/1c5b07581f058886c8bdd87adababd7d959dc7ca/h11/_events.py#L40-L160),
[header representation](https://github.com/python-hyper/h11/blob/1c5b07581f058886c8bdd87adababd7d959dc7ca/h11/_headers.py#L42-L124)).
