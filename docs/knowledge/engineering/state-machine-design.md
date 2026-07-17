---
title: State-machine design for protocol implementations
description: Source-based methods for deriving, representing, and testing protocol state machines.
topics: [state-machines, protocols, architecture, rust, testing]
checked_at: 2026-07-14
---

# State-Machine Design For Protocol Implementations

This document records reusable state-machine methods. It does not prescribe a
state model, implementation library, or public API for a particular project.

## Evidence Boundaries

- **Normative source** identifies protocol states, events, requirements, and
  error consequences defined by a protocol specification.
- **Official guidance** describes state-machine and language mechanisms without
  making them protocol requirements.
- **Observed practice** records the structure of a pinned implementation.
- **Methodological synthesis** combines those sources into a review method. It
  is not a formal proof or a universal implementation rule.

## Composite State Configurations

One logical state machine need not be represented by one flat state value.
SCXML defines a state configuration as the set of currently active states and
defines parallel regions whose children are simultaneously active. Parallel
regions process the same event independently in a defined serial order; they do
not imply threads or nondeterministic execution.
[W3C SCXML parallel states](https://www.w3.org/TR/scxml/#parallel)
[W3C SCXML legal state configurations](https://www.w3.org/TR/scxml/#LegalStateConfigurations)

The following is **methodological synthesis**: when protocol concerns evolve
mostly independently, representing the machine as a product of small state
regions avoids naming every reachable Cartesian-product combination. The full
configuration remains one machine if one transition boundary owns the regions,
their invariants, and their cross-region effects.

HTTP/1.1 has no complete normative connection automaton, but later updates can
add transition consequences. RFC 9931 requires a proxy server that rejects a
CONNECT request to close the connection unless it knows the client waited for
the response before forwarding tunnel bytes. A model derived only from RFC
9112 or an older implementation can therefore incorrectly permit reuse after
that response.
[RFC 9931 Section 8](https://www.rfc-editor.org/rfc/rfc9931.html#section-8)

## Event And Condition-Triggered Transitions

SCXML transitions can be triggered by events and guarded by conditions. It also
defines eventless transitions that become eligible when their condition is
true. A macrostep continues through enabled internal or eventless transitions
until the machine reaches a stable configuration before consuming the next
external event.
[W3C SCXML transitions](https://www.w3.org/TR/scxml/#transition)
[W3C SCXML eventless transitions](https://www.w3.org/TR/scxml/#EventDescriptors)
[W3C SCXML interpretation algorithm](https://www.w3.org/TR/scxml/#AlgorithmforSCXMLInterpretation)

The following is **methodological synthesis**: a protocol transition procedure
can therefore separate:

1. an external semantic event;
2. the direct transition caused by that event;
3. derived transitions enabled by the resulting joint configuration; and
4. the stable configuration and effects returned to the caller.

If multiple derived transitions can apply, their priority is part of the model.
Leaving priority to map iteration, call order, or duplicated caller logic makes
the transition system underspecified.

## Observed Practice: h11 0.16.0

h11 places all state and transition rules in one module and deliberately models
the abstract client and server rather than local and remote roles. Its one
`ConnectionState` owns client state, server state, a monotonic keep-alive flag,
and two pending protocol-switch dimensions.
[h11 state-machine rules](https://github.com/python-hyper/h11/blob/1c5b07581f058886c8bdd87adababd7d959dc7ca/h11/_state.py#L1-L90)
[h11 `ConnectionState`](https://github.com/python-hyper/h11/blob/1c5b07581f058886c8bdd87adababd7d959dc7ca/h11/_state.py#L249-L297)

h11 represents direct client/server event transitions and joint-state
transitions as tables. It repeatedly applies state-triggered rules until the
configuration stops changing. Protocol-switch completion has explicit priority
over the keep-alive close rule.
[h11 transition tables](https://github.com/python-hyper/h11/blob/1c5b07581f058886c8bdd87adababd7d959dc7ca/h11/_state.py#L196-L246)
[h11 stabilization](https://github.com/python-hyper/h11/blob/1c5b07581f058886c8bdd87adababd7d959dc7ca/h11/_state.py#L316-L354)

This is observed practice, not an HTTP/1.1 requirement. In particular, h11's
public sentinels, exact state names, error poisoning, and single-cycle flow
control remain implementation choices.

## Deriving A Protocol Machine

The following is **methodological synthesis**:

1. Inventory normative rules with their role, preconditions, input, required or
   permitted outcome, and error scope.
2. Define semantic inputs independently of parsing chunks or public method
   names. Local operations and decoded peer input should identify the same
   protocol actor and semantic event when they describe the same wire action.
3. Separate mostly independent state dimensions, then state the legal joint
   configurations and cross-region invariants.
4. Define direct transitions, derived transitions, conflict priority, terminal
   states, and explicit reset or reuse operations.
5. Keep wire-parser progress, message framing, connection lifecycle, and
   application flow control distinct unless one directly changes another's
   legal transitions.
6. Specify effects and mutation order. A rejected input must have a defined
   state consequence; successful state and output changes need a coherent
   commit boundary.
7. Check representative complete traces and every state/input pair. Record
   unsupported but protocol-permitted behavior as a bounded implementation
   choice rather than silently treating it as forbidden by the protocol.

This process yields an executable interpretation of the cited requirements. It
does not prove that the requirement inventory is complete or correctly read.

## Rust Representation And Checks

Rust enums provide named closed sets of variants, and `match` requires
exhaustive handling unless a wildcard is used.
[Rust enum reference](https://doc.rust-lang.org/reference/items/enumerations.html)
[Rust `match`](https://doc.rust-lang.org/std/keyword.match.html)

The following is **methodological synthesis**:

- private state enums and exhaustive matches make newly added states visible at
  transition sites; broad wildcard arms discard that compiler assistance;
- a struct of small enums can represent one composite machine without a generic
  state-machine framework;
- transition-matrix tests cover legal and illegal pairs, while trace tests cover
  cross-region sequences and stabilization;
- invariants should be checked after each completed external input, when the
  machine has reached its stable configuration;
- model-based and differential tests add evidence but do not replace an RFC
  oracle or prove completeness. See [test design](../testing/design.md).
