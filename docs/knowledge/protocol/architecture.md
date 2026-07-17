---
title: HTTP/1.1 Sans-I/O architecture
description: Source-supported ownership and event-flow patterns for an I/O-free HTTP/1.1 engine.
topics: [architecture, sans-io, http1, state-machines]
checked_at: 2026-07-12
---

# HTTP/1.1 Sans-I/O Architecture

## Official Guidance

- An I/O-free implementation contains no network I/O or asynchronous flow
  control; it exposes synchronous operations and does not block waiting for I/O.
  [Sans-I/O guidance](https://sans-io.readthedocs.io/how-to-sans-io.html#what-is-an-i-o-free-protocol-implementation)
- Callers pass received bytes into in-memory protocol state. Incomplete input
  may remain buffered inside the implementation.
  [Inputs and outputs](https://sans-io.readthedocs.io/how-to-sans-io.html#inputs-and-outputs)
- Events are semantic representations translated to and from wire bytes.
  [Events](https://sans-io.readthedocs.io/how-to-sans-io.html#events)
- Socket reads, writes, backpressure, and scheduling remain in an integration
  layer around the protocol object.
  [Integration](https://sans-io.readthedocs.io/how-to-sans-io.html#integrating-with-i-o)

## Observed Practice: h11 0.16.0

- `Connection` owns local and remote role states, state-dependent readers and
  writers, an incomplete-input buffer, and an input-size bound.
  [Connection state](https://github.com/python-hyper/h11/blob/1c5b07581f058886c8bdd87adababd7d959dc7ca/h11/_connection.py#L151-L205)
- `receive_data()` stores bytes; `next_event()` parses one semantic event and
  can report that more data is needed or that parsing is paused.
  [Receive path](https://github.com/python-hyper/h11/blob/1c5b07581f058886c8bdd87adababd7d959dc7ca/h11/_connection.py#L364-L474)
- `send()` accepts an event and returns serialized bytes directly.
  [Send path](https://github.com/python-hyper/h11/blob/1c5b07581f058886c8bdd87adababd7d959dc7ca/h11/_connection.py#L503-L585)
- Remote violations and invalid local sequencing have distinct error paths.
  This is an observed API boundary, not an RFC-defined exception model.
  [Error paths](https://github.com/python-hyper/h11/blob/1c5b07581f058886c8bdd87adababd7d959dc7ca/h11/_connection.py#L438-L500)

## Methodological Synthesis

- Transport ownership is the stable boundary: sockets and scheduling stay
  outside while protocol state consumes explicit input and returns output.
- Parsing progress, message framing, connection lifecycle, and application
  backpressure are separate concerns even when one `Connection` owns them.
- Resource ownership is visible through bounded input, caller-owned output,
  and explicit cycle or protocol-transition boundaries.
