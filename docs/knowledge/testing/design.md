---
title: Test design for protocol libraries
description: Source-based principles for tests that produce reviewable protocol evidence.
topics: [testing, properties, conformance, interoperability, fuzzing, benchmarks]
checked_at: 2026-07-12
---

# Test Design for Protocol Libraries

This document describes how tests can produce reviewable evidence about a
protocol implementation. It does not define a release policy, execution
schedule, coverage threshold, or project-specific test plan.

## Source Basis

Protocol expectations come from more than one document. RFC 8174 defines how
uppercase `MUST`, `SHOULD`, and `MAY` are interpreted when an RFC invokes BCP 14
(<https://www.rfc-editor.org/rfc/rfc8174.html#section-2>). RFC Editor records
expose update and obsolescence relations; its Errata system publishes reported
corrections (<https://www.rfc-editor.org/errata.php>). IANA registries maintain
registered protocol values and references (<https://www.iana.org/protocols>).

The following is **methodological synthesis**: a test derived from an RFC has a
source context comprising document number, section, endpoint role, applicable
updates, relevant errata status, and any IANA registry snapshot used to
interpret a value.

Rust assertions compare observed values with expectations; `assert_eq!` and
`assert_ne!` print both values on failure
(<https://doc.rust-lang.org/book/ch11-01-writing-tests.html#testing-equality-with-assert_eq-and-assert_ne>).
pytest parametrization associates argument sets with a test and permits case IDs
(<https://docs.pytest.org/en/stable/how-to/parametrize.html#pytest-mark-parametrize-parametrizing-test-functions>).
The following is **methodological synthesis**: neither mechanism supplies the
expected behavior; that remains the oracle's job.

## Independent Oracles

The following is **methodological synthesis**.

An oracle is independent when expected and observed results do not pass through
the same decision path. Oracle sources include:

- an identified normative requirement and its defined role or preconditions;
- an independently written executable model;
- an algebraic or metamorphic relation that does not restate implementation;
- an independently produced wire fixture with documented provenance;
- a named peer or reference implementation, with version and configuration,
  when the question is behavioral agreement rather than normative correctness.

Independence is relative. Implementations may share a misreading, copied table,
dependency, fixture generator, or assumption. A reference proves agreement with
that reference, not with an RFC.

`decode(encode(x)) == x` can detect encoder-decoder disagreement, but both paths
can share the same non-standard representation.

## Requirement-to-Case Traceability

The following is **methodological synthesis**.

A trace entry can identify:

- source, section, requirement text, role, and preconditions;
- prior state and bytes, events, operation sequence, or generated domain;
- expected output, transition, error class, or absence of output;
- strict requirement, permitted alternative, or implementation choice;
- case ID, oracle type, source revision, errata and registry version, and fixture
  origin.

This mapping exposes omissions and duplication. It does not prove that every
requirement was found or interpreted correctly. `SHOULD` retains the RFC's
allowance for justified exceptions; a test cannot turn it into `MUST`.

One case may cover several requirements; one requirement may need several roles,
states, boundaries, and outcomes. Case counts are not completeness measures.

## Properties and Metamorphic Relations

Hypothesis generates from strategies and has a phase for shrinking failures
(<https://hypothesis.readthedocs.io/en/latest/tutorial/introduction.html#defining-a-simple-test>,
<https://hypothesis.readthedocs.io/en/latest/reference/api.html#hypothesis.Phase.shrink>).
A property can express canonicalization, idempotence, bounded consumption, or
relations between transformations.

The following is **methodological synthesis**: strength is bounded by the input
domain and oracle. Tautologies, dependent round trips, excessive filtering, and
mostly rejected inputs can pass with little evidence. Generation is search, not
exhaustive proof over an unbounded domain.

## State-Machine and Model-Based Tests

Hypothesis state machines generate action sequences, flow values through
bundles, and check invariants. Its example compares a system with a separate
in-memory model and emits a reduced failing sequence
(<https://hypothesis.readthedocs.io/en/latest/stateful.html#rule-based-state-machines>).

The following is **methodological synthesis**: a protocol model can track states,
resources, and outputs without duplicating codec internals. Preconditions define
legal actions; malformed-input actions cover rejection paths.

Omitted actions are not explored; a wrong model is a wrong oracle; bounded
sequences miss longer histories; abstraction can hide byte behavior.

## Differential Evidence

The following is **methodological synthesis**.

Differential testing sends equivalent inputs or sequences to named
implementations and compares normalized outputs, exposing disagreements.

Normalization can hide meaningful differences or expose permitted diagnostic
differences. Agreement is not conformance; disagreement does not identify the
correct participant. RFCs and registries adjudicate normative questions.

## Coverage-Guided Fuzzing

libFuzzer mutates a corpus and uses instrumentation feedback to seek coverage;
its target consumes one byte array per invocation
(<https://llvm.org/docs/LibFuzzer.html#fuzz-target>). `cargo fuzz` supplies the
Rust integration and preserves crash inputs as artifacts that can be rerun
(<https://rust-fuzz.github.io/book/cargo-fuzz/tutorial.html>,
<https://llvm.org/docs/LibFuzzer.html#options>).
Structure-aware fuzzing can pass shallow syntax checks and reach deeper logic
(<https://rust-fuzz.github.io/book/cargo-fuzz/structure-aware-fuzzing.html>).

The following is **methodological synthesis**: a useful target needs an observable
failure such as panic, sanitizer finding, invariant violation, model mismatch,
resource event, or forbidden transition. Non-crashing does not establish
semantics. Evidence is bounded by target, instrumentation, corpus, generator,
limits, engine version, and completed run.

## Regression Evidence

libFuzzer reruns supplied files without fuzzing as regression cases
(<https://llvm.org/docs/LibFuzzer.html#options>). Hypothesis automatically
replays failures from its example database, but its documentation warns that
database entries and `@reproduce_failure` blobs can be invalidated by source or
version changes; an explicit `@example` is the stable test form for an input
that must always run
(<https://hypothesis.readthedocs.io/en/latest/tutorial/replaying-failures.html#prefer-example-over-the-database-for-correctness>).

The following is **methodological synthesis**: a regression records a faithful
input, expected behavior, and protected defect or requirement. A crash artifact
may preserve accidental detail; one input does not establish its input class.

## Determinism and Reproducibility

Hypothesis requires visible draws and outcomes to replay deterministically;
randomness, external state, filesystem state, scheduling, and network timing
are documented sources of flakiness
(<https://hypothesis.readthedocs.io/en/latest/tutorial/flaky.html#common-sources-of-flakiness>).
libFuzzer reports its seed and accepts `-seed`, while also recording corpus and
failure artifacts
(<https://llvm.org/docs/LibFuzzer.html#output>).

The following is **methodological synthesis**: reproducible evidence identifies
input or sequence, implementation and tool versions, seed, target, configuration,
role, limits, and expected outcome. A seed may fail across versions, platforms,
schedules, clocks, or peers. Minimized input still requires its initial state.
