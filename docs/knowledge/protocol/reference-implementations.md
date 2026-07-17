---
title: HTTP/1.1 reference implementations
description: Version-pinned implementations used as observed-practice evidence.
topics: [implementations, http1, python, rust]
checked_at: 2026-07-12
---

# HTTP/1.1 Reference Implementations

| Project | Evidence role |
| --- | --- |
| [h11 0.16.0 source](https://github.com/python-hyper/h11/tree/1c5b07581f058886c8bdd87adababd7d959dc7ca) | Python Sans-I/O state, event, and compatibility behavior |
| [httparse 1.10.1](https://docs.rs/httparse/1.10.1/httparse/) | Rust request and response head parsing |

A mature implementation's permissive or strict behavior is compatibility
evidence, not protocol authority. Python and Rust APIs also reflect different
ownership and allocation models, so observed structures are not universal API
templates.
