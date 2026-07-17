---
title: Engineering quality evidence
description: Source-supported boundaries for code review, security, resources, and performance work.
topics: [quality, review, security, resources, performance, maintainability]
checked_at: 2026-07-13
---

# Engineering Quality Evidence

This document records source-supported boundaries for general code review,
security and resource reasoning, and performance work. It does not define a
project workflow, reviewer count, score, threshold, or mandatory checklist.

Language-specific naming, visibility, documentation, lint, and typing guidance
is already recorded in [Language and API conventions](language-api.md).
Protocol structure and resource ownership observations are recorded in
[Protocol architecture patterns](../protocol/architecture.md). Test evidence is
covered by [Test design](../testing/design.md) and
[Testing evidence](../testing/evidence.md).

## Published Code Review Guidance

Google's published engineering practices describe code review as a means of
improving overall code health over time. They explicitly balance that goal with
the need for developers to make progress and advise approval when a change is a
clear improvement, even if it is not perfect.
<https://google.github.io/eng-practices/review/reviewer/standard.html>

The same guidance states that technical facts and data take precedence over
opinion. When several designs are supported equally well, the author's
preference can stand. Purely optional polish should be distinguished from
required changes.
<https://google.github.io/eng-practices/review/reviewer/standard.html#principles>
<https://google.github.io/eng-practices/review/reviewer/comments.html#label-comment-severity>

Its review dimensions are design, functionality, complexity, tests, naming,
comments, style, and documentation. The complexity section specifically calls
out speculative generality and functionality that is not presently needed.
Tests are reviewed as maintained code and must be capable of failing when the
covered behavior is broken.
<https://google.github.io/eng-practices/review/reviewer/looking-for.html>

The guide asks reviewers to understand every human-written line within their
assigned scope, while allowing different parts to receive different scrutiny.
When a reviewer lacks the expertise for a specialized risk, another qualified
reviewer may cover that aspect; the scope reviewed should be made explicit.
<https://google.github.io/eng-practices/review/reviewer/looking-for.html#every-line>

## Official Risk-Based Security Guidance

NIST describes the Secure Software Development Framework as an outcome-based
basis for a risk-based approach, not a checklist. Organizations align and
prioritize practices with mission needs, risk tolerance, and resources. Its
notional implementation examples are neither required nor exhaustive.
<https://csrc.nist.gov/projects/ssdf>

CWE-400 describes uncontrolled resource consumption as failure to control the
allocation or retention of a limited resource. Its more specific descendants
cover causes such as allocation without limits, missing release, excessive
iteration, and asymmetric consumption. CWE warns that the broad class is often
misused when only the consequence, rather than the underlying mistake, is
known.
<https://cwe.mitre.org/data/definitions/400.html>

The Rust API Guidelines prefer static input enforcement when a suitable type
can exclude invalid values, followed by dynamic validation when the property
cannot reasonably be expressed in the type system. They also identify runtime
cost and new failure paths as trade-offs of dynamic validation.
<https://rust-lang.github.io/api-guidelines/dependability.html#functions-validate-their-arguments-c-validate>

The Rust Book distinguishes recoverable errors, represented by `Result`, from
unrecoverable conditions that indicate bugs and may panic. This distinction is
a language error-handling model; it does not by itself classify a particular
remote input or resource failure.
<https://doc.rust-lang.org/stable/book/ch09-00-error-handling.html>

## Performance Evidence Guidance

The Rust Performance Book describes representative workloads as the starting
point for benchmarking. Realistic inputs are preferred, while microbenchmarks
and stress tests can provide narrower evidence. It notes that metric choice
depends on the program and that no single method of summarizing multiple
workloads is always best.
<https://nnethercote.github.io/perf-book/benchmarking.html>

The same book recommends profiling to identify code that is hot enough to
matter before optimizing it. It also says performance-related build choices
should be validated by benchmarks because they trade runtime speed, memory,
binary size, compilation time, debuggability, and portability differently.
<https://nnethercote.github.io/perf-book/profiling.html>
<https://nnethercote.github.io/perf-book/build-configuration.html>

The `pyperf` documentation records benchmark metadata, supports repeated runs
and comparisons, detects some unstable results, and documents system tuning and
manual analysis needed for stable measurements. These controls reduce noise;
they do not decide whether a workload represents user behavior.
<https://pyperf.readthedocs.io/en/latest/>
<https://pyperf.readthedocs.io/en/latest/run_benchmark.html#how-to-get-reproducible-benchmark-results>

## Methodological Synthesis

The following conclusions are synthesis, not quotations, universal rules, or a
project policy:

- Quality review is an evidence-guided judgment about behavior and code health,
  not the number of checks, comments, reviewers, or tools used.
- Design, functionality, unnecessary complexity, and comprehensibility are
  broad review concerns. Language conventions, security, performance, and
  protocol correctness add narrower evidence when the changed boundary makes
  them relevant.
- A security measure needs an identified asset, trust or resource boundary,
  failure mode, and plausible triggering input. A generic possibility of harm
  does not determine a mitigation or prove that added code is beneficial.
- Resource limits should correspond to a resource the implementation actually
  allocates, retains, iterates over, or amplifies. The limit value and failure
  behavior remain context-dependent.
- A performance change needs a stated workload and metric. Profiling identifies
  where effort may matter; benchmarking compares outcomes. Neither justifies a
  claim outside the measured environment and workload.
- Style and static-analysis findings are bounded by the rules they encode. They
  do not establish architecture quality, protocol correctness, security, or
  performance.
- Review can separate demonstrated defects or contract violations from
  optional improvements and unresolved questions. This preserves room for
  multiple valid implementations without weakening evidence-backed boundaries.
