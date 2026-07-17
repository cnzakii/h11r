---
title: Continuous integration and dependency automation
description: Official contracts and observed practice for CI gates, version matrices, reproducible dependency use, and automated updates.
topics: [ci, github-actions, dependencies, dependabot, cargo, uv, rust, python]
checked_at: 2026-07-15
---

# Continuous Integration and Dependency Automation

This document records reusable facts about continuous integration and automated
dependency updates. It does not choose a workflow, schedule, version matrix, or
merge policy for a particular repository.

## GitHub Actions Contracts

GitHub Actions workflows select their triggering events with `on`. A workflow
can run for `push`, `pull_request`, or both, with optional branch and path
filters. A job matrix creates one job for each selected combination, and
`fail-fast` controls whether one failing matrix job cancels the others.
Workflow-level `concurrency` can cancel an older in-progress run for the same
workflow and ref when a newer run starts.

GitHub's secure-use guidance recommends granting `GITHUB_TOKEN` only the
permissions required by the workflow. For a read-only test workflow, repository
contents do not require write permission. The same guidance warns against
checking out untrusted pull-request code in privileged `pull_request_target` or
`workflow_run` contexts.

GitHub states that a full-length commit SHA is the only immutable way to select
an action revision. Tags are more convenient but can move. Dependabot supports
the `github-actions` ecosystem and can update action references.

Sources:

- <https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax>
- <https://docs.github.com/en/actions/reference/security/secure-use>

## Reproducible Python and Rust Checks

Cargo's `--locked` option rejects an operation that would change dependency
resolution. The exact Rust test, formatting, lint, target, feature, and workspace
selection remains part of the evidence; the relevant commands are documented in
[Test tool operations and evidence](../testing/tools.md).

uv officially recommends `astral-sh/setup-uv` for GitHub Actions. Its
`python-version` input can override a repository's normal Python pin for a
matrix. `uv sync --locked` verifies that `uv.lock` is current, while `--frozen`
uses the existing lockfile without checking whether project metadata changed.
This distinction matters when a CI result is intended to prove that manifests
and the committed lockfile agree.

In a workspace, command working directory and project selection determine which
`pyproject.toml` and environment uv discovers. A native extension build must
therefore target the actual buildable package, not merely a non-buildable
workspace root.

Sources:

- <https://doc.rust-lang.org/cargo/commands/cargo-test.html>
- <https://docs.astral.sh/uv/guides/integration/github/>
- <https://docs.astral.sh/uv/concepts/projects/sync/>
- <https://docs.astral.sh/uv/reference/cli/>

## What Different Schedules Establish

The following table is **methodological synthesis**. The cadence is a project
choice; no cited tool defines a universal schedule.

| Execution class | Suitable evidence | Important boundary |
| --- | --- | --- |
| Change gate | deterministic formatting, lint, typing, build/install, unit, integration, and package tests | should fail for an actionable defect in the proposed change |
| Compatibility matrix | declared runtime, toolchain, or operating-system support | every matrix axis multiplies cost and must correspond to a supported surface or identified risk |
| Bounded fuzz run | no configured oracle failed for the recorded target, corpus, sanitizer, and budget | a clean bounded run is not proof that no failing input exists |
| Benchmark run | measurements for a recorded workload, build, machine, and environment | shared-runner timing is not automatically a stable regression threshold |
| Scheduled dependency update | the selected ecosystem was checked and an update PR passed its configured gates | automation does not review release notes, compatibility, or product semantics |

Compiling fuzz and benchmark harnesses can detect harness drift without claiming
that fuzz exploration or performance measurement occurred. Long-running fuzzing
and benchmark comparisons produce different evidence from deterministic tests;
their operational contracts are recorded in
[Test tool operations and evidence](../testing/tools.md) and their evidentiary
limits in [Testing evidence](../testing/evidence.md).

## Dependabot Contracts

Dependabot version updates are configured by `.github/dependabot.yml` with
configuration `version: 2`. Each update entry names a supported package
ecosystem, a manifest directory or directory set, and a schedule. GitHub's
current supported-ecosystem reference includes `cargo`, `uv`, and
`github-actions`.

By default, Dependabot opens separate version-update pull requests. `groups`
can combine matching dependencies within an ecosystem, and
`multi-ecosystem-groups` can combine supported ecosystems. Group rules without
`applies-to` apply to version updates by default.

Dependabot security updates and version updates are distinct features. Security
updates depend on the dependency graph, alerts, and repository security
settings. Grouped security updates require separate rules with
`applies-to: security-updates`; a normal version-update group does not also
group security updates.

Sources:

- <https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference>
- <https://docs.github.com/en/code-security/reference/supply-chain-security/supported-ecosystems-and-repositories>
- <https://docs.github.com/en/code-security/concepts/supply-chain-security/dependabot-version-updates>
- <https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/secure-your-dependencies/configure-security-updates>

## Observed Practice in Mature Projects

The following are pinned observations, not requirements:

- `pydantic-core` at commit
  [`383eb95`](https://github.com/pydantic/pydantic-core/tree/383eb95a19433754c0cecf7025b50c26b6d97a36)
  separates lint, supported-Python, operating-system, minimum-Rust, coverage,
  integration, and benchmark jobs. Its
  [Makefile](https://github.com/pydantic/pydantic-core/blob/383eb95a19433754c0cecf7025b50c26b6d97a36/Makefile)
  also provides a local aggregate target over build, lint, and tests.
- `orjson` at commit
  [`705515d`](https://github.com/ijl/orjson/tree/705515d77b28429d0b7c30c3d781abe52e8a1e5a)
  has separate lint and unusual-version workflows; the latter builds a wheel,
  installs it, and then tests selected minimum and forward Python versions.
- `h11` at commit
  [`62c5068`](https://github.com/python-hyper/h11/blob/62c5068c971579d61fa1b55373390e12f25fd856/.github/workflows/ci.yml)
  runs its test environment across its selected CPython and PyPy matrix for
  pushes and pull requests to its primary branch.

These projects select different matrices and job boundaries. Their common use
of automated checks does not make any one matrix, action set, or workflow split
universally appropriate.

## Methodological Synthesis

- A local aggregate command and CI should invoke the same underlying checks
  where practical; the wrapper is a convenience, while the commands and fresh
  results are the evidence.
- A compatibility matrix should be derived from the declared support contract
  and identified platform risks. Testing every available runtime is not an end
  in itself.
- Deterministic change gates, compatibility checks, fuzzing, benchmarks, and
  dependency automation answer different questions and should not be flattened
  into one pass/fail claim.
- Automated dependency pull requests need the same product gates and human
  compatibility review as other dependency changes.
- CI should not acquire speculative jobs for product surfaces or evidence
  classes the repository does not yet own.
