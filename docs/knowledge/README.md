---
title: Protocol engineering knowledge
description: Source-based reference material for protocol, testing, architecture, and Python/Rust engineering.
topics: [index]
---

# Protocol Engineering Knowledge

This collection is a source-based reference library for protocol engineering,
testing, architecture, language conventions, and mixed Python/Rust packaging.

It does not contain product code, project audits, implementation plans,
benchmark results, release decisions, or recommendations for a particular
repository.

## Evidence Labels

Documents distinguish five kinds of material:

- **Normative source:** applicable RFC and BCP requirements and their updates.
- **Authoritative registry:** current IANA names, codepoints, and registration
  metadata.
- **Official guidance:** documentation published by a language, tool, or
  framework owner.
- **Observed practice:** behavior or structure visible in an immutable source
  revision or exact versioned documentation. Observed practice is not a
  protocol requirement.
- **Methodological synthesis:** a conclusion derived across sources. It is not
  a quotation, standard, or universal industry rule.

## Contents

- Protocol: [authority](protocol/authority.md), [semantic types](protocol/semantics.md),
  [architecture](protocol/architecture.md), and
  [reference implementations](protocol/reference-implementations.md)
- Testing: [evidence](testing/evidence.md), [design](testing/design.md), and
  [tool operations](testing/tools.md)
- Engineering: [quality](engineering/quality.md),
  [language and API conventions](engineering/language-api.md),
  [dependency and license evidence](engineering/dependency-licenses.md), and
  [state-machine design](engineering/state-machine-design.md), and
  [continuous integration and dependency automation](engineering/continuous-integration.md)
- Python/Rust: [extension boundaries](python-rust/boundary.md) and
  [tooling](python-rust/tooling.md)

## Freshness

`checked_at` means every external factual claim in that document was reviewed
against its cited source on that date. It is a maintenance signal, not proof
that mutable sources are still current.

- Age alone does not make a versioned specification or pinned source observation
  stale. Drift depends on the source type and the claim being made.
- Stable specifications and pinned source revisions remain tied to the cited
  version. Check RFC status and errata before normative use.
- Read mutable registries and claims about current tool or implementation
  behavior from their official source when they matter to the answer.
- Report unavailable, superseded, materially changed, or conflicting evidence
  when discovered. Read-only detection does not change `checked_at`.
- Change `checked_at` only after reviewing the whole document. Replace or
  remove stale claims instead of preserving contradictory versions.
