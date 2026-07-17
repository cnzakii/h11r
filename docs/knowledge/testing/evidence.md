---
title: Testing evidence
description: Official test organization and the distinct evidence produced by major test classes.
topics: [testing, unit-tests, integration, conformance, interoperability, fuzzing]
checked_at: 2026-07-12
---

# Testing Evidence

## Official Test Organization

The Rust Book distinguishes unit tests from integration tests:

- unit tests are focused, can exercise private interfaces, and conventionally
  live beside Rust source under `#[cfg(test)]`;
- integration tests are separate crates under `tests/` and exercise the public
  interface as external code would.

Cargo's `cargo test` compiles and runs unit, integration, and documentation
tests. Documentation tests are extracted and executed by rustdoc.

pytest documents both in-package and separate test layouts. Its current good
practices recommend `--import-mode=importlib` for new projects and explain that
testing an installed package detects import and packaging mistakes that a
source-tree import can hide.

Sources:

- <https://doc.rust-lang.org/book/ch11-03-test-organization.html>
- <https://doc.rust-lang.org/cargo/commands/cargo-test.html>
- <https://docs.pytest.org/en/stable/explanation/goodpractices.html>

## Evidence Classes

The following taxonomy is a methodological synthesis of official tooling
contracts and common protocol-library practice.

| Evidence class | Establishes | Does not establish |
| --- | --- | --- |
| Unit | A focused internal invariant for supplied cases | Public API usability or peer compatibility |
| Public integration | Behavior visible through a public API and package boundary | Full protocol conformance |
| RFC-derived conformance | Behavior for explicitly mapped normative requirements | Compatibility with every deployed peer |
| External conformance suite | Results for the suite's implemented cases and target role | Requirements omitted by the suite or unsupported roles |
| Differential | Agreement or disagreement with a named reference for supplied inputs | That the reference is correct or normative |
| Interoperability | Successful behavior with named real peers and configurations | Universal compatibility or malformed-input correctness |
| Fuzz | Absence of discovered failures during a stated engine, corpus, target, and budget | Proof that no failure exists |
| Benchmark | Measurements for a stated workload and environment | Correctness, security, or performance on another environment |
| Typing/stub check | Static surface consistency for the checked examples or runtime inventory | Runtime semantic correctness |
| Package-content test | Contents and importability of the built artifact | Protocol behavior |

External tools are role-specific: a result establishes only the implemented
cases, tested role, adapter, and configuration. Fuzzing guidance is maintained
by the [Rust Fuzz Book](https://rust-fuzz.github.io/book/cargo-fuzz.html).

## Independent Test Oracles

An oracle is the source used to decide the expected result. Detailed oracle,
traceability, model, property, differential, fuzz, and reproducibility material
is recorded in [`design.md`](design.md).

## Weak Evidence Patterns

The following patterns are methodological synthesis and provide little
independent behavioral evidence:

- asserting a constant against the same constant;
- restating implementation branches in test code without an external oracle;
- checking only that a helper script ran, while ignoring its result;
- treating skipped external tooling as a successful conformance run;
- using mocks to prove behavior that depends on a real parser, package, peer,
  clock, socket, or artifact boundary;
- treating test count or line coverage as proof that important protocol states
  were selected;
- comparing benchmark means without environment, distribution, warm-up, sample
  count, or uncertainty information.
