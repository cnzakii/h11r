---
title: Test tool operations and evidence
description: Operational facts and durable outputs for common Python and Rust test tools.
topics: [testing, pytest, cargo, hypothesis, fuzzing, coverage]
checked_at: 2026-07-13
---

# Test Tool Operations and Evidence

This document records operational facts for a small set of Python and Rust test
tools. It does not select tools for a particular repository, define execution
schedules, or set acceptance thresholds. The meaning and limits of the evidence
are covered by [Testing evidence](evidence.md) and
[Test design](design.md).

## Tool Boundaries

The following table is **methodological synthesis**.

| Tool | Primary role | Durable evidence |
| --- | --- | --- |
| `cargo test` orchestration | Select and launch Rust unit, integration, and documentation tests | command, selected targets/features, pass/fail output |
| pytest | Python test discovery and execution | command, collected node IDs, pass/fail/skip output |
| Hypothesis | generated examples and action sequences with shrinking | shrunk failure, explicit regression input, settings/profile |
| cargo-fuzz with libFuzzer | coverage-guided mutation of a target | target, corpus, crash artifact, engine options, completed budget |
| Criterion.rs | statistical Rust microbenchmarks | raw samples, estimates, environment, named baseline |
| pyperf | controlled Python benchmarks | JSON benchmark suite, metadata, comparison output |
| cargo-llvm-cov | Rust coverage collection and export | LCOV, JSON, Cobertura, text, or HTML report |
| coverage.py | Python coverage collection, combination, and export | data file plus XML, JSON, LCOV, text, or HTML report |
| Codecov | storage, merging, and presentation of uploaded reports | commit-associated uploads and flag/component views |

A runner, generator, fuzzer, benchmark harness, coverage collector, and report
host solve different problems. Passing one does not substitute for evidence from
the others.

## Rust Tests with Cargo

Cargo documents `cargo test` as compiling and running unit, integration, and
documentation tests. A filter before `--` selects test names; arguments after
`--` go to the Rust test binary. Cargo runs multiple test targets serially, while
the standard test harness runs tests within a target in parallel by default.

```bash
cargo test
cargo test name_filter
cargo test --test integration_target -- module::test_name
cargo test -- --no-capture
cargo test --no-run
```

`--workspace`, `--package`, target selectors, and feature selectors determine
what is built and tested. `--locked` prevents Cargo from changing dependency
resolution during the run. A workspace root can select only its configured
default members, and ignored tests do not run unless selected.
`--no-fail-fast` continues with later test executables after one executable
fails. `cargo test --benches` checks benchmark targets as tests but does not
perform their normal measurement and analysis.

The following is **methodological synthesis**: the exact command, selected
workspace members, features, targets, and ignored-test policy are part of the
scope of a recorded result.

Source: <https://doc.rust-lang.org/cargo/commands/cargo-test.html>.

## Python Tests with pytest

pytest discovers test functions and methods from configured paths or supplied
node IDs. Its good-practices guide describes both in-package and separate test
layouts, recommends `--import-mode=importlib` for new projects, and explains
that running tests against an installed package can expose import and packaging
errors hidden by source-tree imports.

```bash
python -m pytest
python -m pytest tests/path/test_module.py
python -m pytest tests/path/test_module.py::test_name
python -m pytest -k expression
```

Parametrization varies explicit cases under one test function; fixtures manage
setup and teardown.

The following is **methodological synthesis**: neither mechanism supplies an
independent expected result. The command, package installation mode, selected
node IDs, environment, and skip/xfail reasons determine the scope of a result.

`.pytest_cache` stores node IDs, prior failures, and plugin cache data; it is not
a complete execution report. Exit code 5 means no tests were collected, while
the default non-strict `XPASS` behavior does not fail the suite.

The following is **methodological synthesis**: persistent evidence records
collection and outcome details, not only exit code 0.

Sources:

- <https://docs.pytest.org/en/stable/explanation/goodpractices.html>
- <https://docs.pytest.org/en/stable/how-to/usage.html>
- <https://docs.pytest.org/en/stable/how-to/parametrize.html>
- <https://docs.pytest.org/en/stable/how-to/cache.html>
- <https://docs.pytest.org/en/stable/reference/exit-codes.html>
- <https://docs.pytest.org/en/stable/how-to/skipping.html>

## Generated and Stateful Tests with Hypothesis

`@given` supplies generated values from strategies. Hypothesis searches for
failing examples and then shrinks generated failures. Rule-based state machines
generate sequences of user-defined actions; bundles pass generated values
between rules, preconditions restrict applicable actions, and invariants run
after steps.

```python
from hypothesis import given, strategies as st


@given(st.binary())
def test_property(data: bytes) -> None:
    assert property_holds(data)
```

Hypothesis installs a pytest plugin. Function-scoped pytest fixtures are not
reset between the generated examples of one `@given` test, so mutable fixture
state can couple examples. Stateful rules generally cannot receive values from
pytest parametrization or fixtures; strategies and state-machine initialization
provide those values instead.

Failing inputs are saved in an example database, whose default local storage is
under `.hypothesis/examples`. The database accelerates replay but is not a durable
correctness oracle: source changes or Hypothesis upgrades can invalidate its
keys or representation. `@example` makes a regression input run every time.
`@reproduce_failure` blobs are version-sensitive and are intended for temporary
reproduction, not permanent regressions.

The following is **methodological synthesis**: a useful failure record contains
the shrunk input or action sequence, the property or independent model,
Hypothesis version, settings/profile, and any external state needed to reproduce
it. Generated examples do not prove behavior outside the strategy domain or
beyond the explored sequence budget. Shrinking is a search for a simpler
failing case, not a proof of a globally minimal case or of the defect's root
cause.

Sources:

- <https://hypothesis.readthedocs.io/en/latest/tutorial/introduction.html>
- <https://hypothesis.readthedocs.io/en/latest/stateful.html>
- <https://hypothesis.readthedocs.io/en/latest/reference/integrations.html#pytest>
- <https://hypothesis.readthedocs.io/en/latest/tutorial/replaying-failures.html>

## Coverage-Guided Fuzzing with cargo-fuzz

`cargo-fuzz` uses libFuzzer plus LLVM sanitizer instrumentation. Its documented
setup requires a nightly Rust toolchain and a C++11 compiler. The supported
sanitizer environments include x86-64 Linux, x86-64 macOS, Apple-Silicon macOS,
and Windows with MSVC AddressSanitizer.

`cargo fuzz init` creates a separate fuzz package and initial target;
`cargo fuzz list` lists targets, and `cargo fuzz run <target>` repeatedly invokes
the selected target through libFuzzer. A byte-slice target accepts arbitrary
bytes. Structure-aware fuzzing uses `arbitrary` and `Arbitrary` to decode the
fuzzer input into typed values.

```bash
cargo fuzz init
cargo fuzz list
cargo fuzz add target_name
cargo fuzz run target_name
cargo fuzz run target_name fuzz/artifacts/target_name/crash-file
cargo fuzz tmin target_name fuzz/artifacts/target_name/crash-file
```

The evolving corpus normally lives under `fuzz/corpus/<target>`. Failure
artifacts normally live under `fuzz/artifacts/<target>` and contain concrete
inputs for replay and minimization.

`cargo fuzz coverage <target>` recompiles with Rust source-coverage
instrumentation and replays the current corpus without mutation. It produces
merged profile data that `llvm-cov` can render. This reveals code reached by the
current corpus and helps identify shallow targets or missing seeds; it does not
measure semantic correctness or guarantee that an uncovered path is reachable
from the target.

The following is **methodological synthesis**: the corpus is search state, while
a failure artifact is the principal replayable regression evidence. A target
also needs an observable oracle such as a panic, sanitizer report, invariant
violation, model disagreement, forbidden transition, or explicit resource-bound
violation. The target source, corpus revision, artifact, engine/toolchain
versions, options, sanitizers, elapsed or execution budget, and exit state bound
the claim. A clean run means no configured oracle failed during that run; it is
not proof that no failing input exists.

Sources:

- <https://rust-fuzz.github.io/book/cargo-fuzz/setup.html>
- <https://rust-fuzz.github.io/book/cargo-fuzz/tutorial.html>
- <https://rust-fuzz.github.io/book/cargo-fuzz/guide.html>
- <https://rust-fuzz.github.io/book/cargo-fuzz/structure-aware-fuzzing.html>
- <https://rust-fuzz.github.io/book/cargo-fuzz/coverage.html>
- <https://llvm.org/docs/LibFuzzer.html>

## Rust Benchmarks with Criterion.rs

Criterion.rs benchmark targets use Cargo's benchmark target mechanism with the
standard harness disabled. `cargo bench` performs measurement and analysis.
Named baselines can be saved and compared; `cargo test --benches` only verifies
that benchmark targets execute successfully.

```bash
cargo bench
cargo bench -- --save-baseline reference
cargo bench -- --baseline reference
cargo test --benches
```

Criterion reports estimates and change comparisons from collected samples and
classifies outliers rather than automatically discarding them. Its timing-loop
guide describes how setup, destruction, and allocation can accidentally enter
the measured loop.

Criterion benchmark executables accept `--profile-time <seconds>`. In that
mode, the selected workload is repeated without Criterion's normal statistical
analysis or result persistence, allowing an external sampling profiler to
observe the workload without measuring Criterion's analysis code. Rust's
performance guidance lists Instruments on macOS, `perf` on Linux, `samply`,
flame graphs, instruction/cache simulators, and allocation profilers as
different observation tools.

The following is **methodological synthesis**: ordinary timing and external
sampling need no counters or tracing embedded in production code. The benchmark
harness supplies the repeatable workload; build-time line-table debug
information can make sampled stacks attributable to source. In-process
allocation counters or profiler hooks are justified only when the chosen metric
cannot be obtained externally, and should remain benchmark-only because their
own work can perturb the measurement.

The following is **methodological synthesis**: statistical significance does
not by itself establish practical importance. A comparison record includes the
benchmark source and input sizes, build profile/features, machine and toolchain
metadata, raw output, and exact baseline provenance. Setup, destruction, and
allocation belong outside the measured loop unless they are part of the
operation being measured.

Sources:

- <https://bheisler.github.io/criterion.rs/book/getting_started.html>
- <https://bheisler.github.io/criterion.rs/book/analysis.html>
- <https://bheisler.github.io/criterion.rs/book/user_guide/command_line_options.html>
- <https://bheisler.github.io/criterion.rs/book/user_guide/timing_loops.html>
- <https://bheisler.github.io/criterion.rs/book/user_guide/profiling.html>
- <https://nnethercote.github.io/perf-book/profiling.html>
- <https://doc.rust-lang.org/cargo/reference/profiles.html#debug>

## Python Benchmarks with pyperf

pyperf provides `Runner` for benchmark programs and `pyperf timeit` for small
expressions. It stores benchmark suites and environment metadata in JSON and
provides `pyperf compare_to` for statistical comparisons.

```bash
python -m pyperf timeit -o result.json 'expression()'
python -m pyperf metadata result.json
python -m pyperf compare_to reference.json candidate.json
```

pyperf documents worker processes, warmups, repeated values, system tuning, and
instability warnings.

The following is **methodological synthesis**: these controls address
measurement noise; they do not choose representative workloads or make results
portable across machines.

Sources:

- <https://pyperf.readthedocs.io/en/latest/>
- <https://pyperf.readthedocs.io/en/latest/run_benchmark.html>
- <https://pyperf.readthedocs.io/en/latest/cli.html>

## Coverage Collection and Aggregation

Rust and Python use different collection mechanisms. `cargo-llvm-cov` drives
Rust's LLVM source-based instrumentation. coverage.py traces Python execution.
Their reports can share interchange formats, but one collector cannot measure
both language runtimes.

```bash
cargo llvm-cov --workspace --lcov --output-path rust.lcov
coverage run -m pytest
coverage xml -o python.xml
```

`cargo-llvm-cov clean --workspace` removes instrumentation artifacts that can
contaminate a later run; its documentation notes that documentation tests are
not included by default. coverage.py's configured `source`, branch setting,
omissions, multiprocessing mode, and path mappings determine the denominator
and collected data. A file that was never imported may be absent when the
source scope is not configured.

Both collectors support combining data from multiple executions before final
reporting. Codecov accepts multiple uploaded coverage reports for a commit and
merges them; explicit files and flags identify upload groups and can expose
language or test-suite views. Assigning one complete report to several flags
does not separate its contents. Carryforward flags reuse prior flag data when
an upload is absent, so a carried-forward view is not evidence that the omitted
suite ran on the current commit.

The following is **methodological synthesis**: coverage identifies code observed
during instrumented execution. It does not identify missing requirements,
oracle quality, untested input classes, peer compatibility, or state-space
completeness. Separate collector reports and flags preserve those boundaries
better than a single unexplained percentage.

Sources:

- <https://github.com/taiki-e/cargo-llvm-cov>
- <https://coverage.readthedocs.io/en/latest/commands/index.html>
- <https://docs.codecov.com/docs/merging-reports>
- <https://docs.codecov.com/docs/flags>
