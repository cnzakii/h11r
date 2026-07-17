---
title: Dependency and license evidence
description: Source-supported methods for dependency scope, release contents, and license analysis.
topics: [dependencies, licenses, packaging, supply-chain]
checked_at: 2026-07-13
---

# Dependency and License Evidence

This document records source-supported methods for reasoning about dependency
scope, release contents, and licenses. It is not legal advice, a dependency
approval list, or a decision for a particular repository.

## Scope Precedes License Conclusions

The following scope model is **methodological synthesis**.

| Scope | Question answered |
| --- | --- |
| Runtime | Must an installed program import, link, or load it? |
| Build | Is it required to produce an artifact? |
| Test/development | Is it used only to create verification evidence? |
| CI automation | Is executable third-party code run by automation? |
| External service | Are reports or source data sent to a hosted provider? |
| Vendored or bundled | Are any of its bytes present in a distributed artifact? |
| Transitive | Is it introduced through another dependency? |

The same package can occupy more than one scope. A lockfile entry proves that a
resolver selected a package.

The following is **methodological synthesis**: a lockfile entry does not prove
that the package is imported at runtime or included in a wheel, sdist, crate,
archive, or container. Artifact inspection is required to answer the bundling
question.

ASF release policy states that each package's `LICENSE` and `NOTICE` files must
account for the package's exact contents and must not describe separately
downloaded dependencies that are not bundled.

Source: <https://www.apache.org/legal/release-policy.html#licensing-documentation>.

The following is **methodological synthesis**: under that artifact-content
model, a test tool separately downloaded by a developer or CI job has a
different distribution analysis from code copied into a release artifact.

## ASF License Categories

The Apache Software Foundation classifies third-party licenses as:

- Category A: may be included in ASF products;
- Category B: may be included under stated conditions and labeling rules;
- Category X: may not be included in ASF products.

MIT, Apache-2.0, and the University of Illinois/NCSA license are listed as
Category A. MPL-2.0 is listed under Category B weak-copyleft licenses; the ASF
policy permits appropriately labeled binary-form inclusion under its stated
conditions. These categories govern ASF products. They can be used as a review
reference elsewhere, but that use does not turn a non-ASF project into an ASF
project or replace the actual license terms.

Source: <https://www.apache.org/legal/resolved.html>.

## Expressions and Transitive Closure

An SPDX `OR` expression offers alternatives under the expression's terms; an
`AND` expression requires satisfying every joined license. Parentheses preserve
grouping. For example, `(MIT OR Apache-2.0) AND NCSA` contains one choice plus
an additional required license.

The following is **methodological synthesis**: review has at least three layers.

1. Registry and upstream metadata identify the declared license and source.
2. The resolved dependency graph identifies direct and transitive packages for
   the exact lock state and enabled features.
3. Built artifact inspection identifies which code, license texts, notices, and
   generated or vendored materials are actually distributed.

Metadata can be incomplete or wrong. Ambiguous metadata requires inspection of
the package's license files and source provenance rather than guessing from a
classifier, repository badge, or dependency name.

SPDX expression syntax: <https://spdx.github.io/spdx-spec/v2.3/SPDX-license-expressions/>.

## Test-Tool License Snapshot

The table records registry versions or immutable repository heads observed on
2026-07-13. Which version or head is current remains date-sensitive and requires
a new check when a dependency is introduced, updated, locked, or distributed.

| Package/tool | Observed version or repository | Declared license | ASF category of listed license(s) | Source |
| --- | --- | --- | --- | --- |
| pytest | 9.1.1 | MIT | A | <https://pypi.org/pypi/pytest/9.1.1/json> |
| Hypothesis | 6.156.6 | MPL-2.0 | B | <https://pypi.org/pypi/hypothesis/6.156.6/json> |
| pyperf | 2.10.0 | MIT | A | <https://pypi.org/pypi/pyperf/2.10.0/json> |
| coverage.py | 7.15.1 | Apache-2.0 | A | <https://pypi.org/pypi/coverage/7.15.1/json> |
| criterion | 0.8.2 | Apache-2.0 OR MIT | A | <https://crates.io/api/v1/crates/criterion/0.8.2> |
| cargo-fuzz | 0.13.2 | MIT OR Apache-2.0 | A | <https://crates.io/api/v1/crates/cargo-fuzz/0.13.2> |
| libfuzzer-sys | 0.4.13 | (MIT OR Apache-2.0) AND NCSA | A | <https://crates.io/api/v1/crates/libfuzzer-sys/0.4.13> |
| arbitrary | 1.4.2 | MIT OR Apache-2.0 | A | <https://crates.io/api/v1/crates/arbitrary/1.4.2> |
| cargo-llvm-cov | 0.8.7 | Apache-2.0 OR MIT | A | <https://crates.io/api/v1/crates/cargo-llvm-cov/0.8.7> |
| codecov-action | repository head `fb8b358` | MIT | A | <https://github.com/codecov/codecov-action/blob/fb8b3582c8e4def4969c97caa2f19720cb33a72f/LICENSE> |
| codecov-cli | repository head `152ecf8` | Apache-2.0 | A | <https://github.com/codecov/codecov-cli/blob/152ecf87f4002ea8756c154d00f4f180f34fb814/LICENSE> |

The following is **methodological synthesis**: Hypothesis's MPL-2.0
classification does not by itself say whether a release
artifact contains Hypothesis. When it is separately downloaded and used only to
run tests, it is not bundled merely because test source imports it or a lockfile
records it. Copying its source, embedding it, or distributing an environment or
container that contains it changes the artifact analysis.

The following is **methodological synthesis**: when a container image is
distributed, the image is a distribution artifact. Its operating-system
packages, language environment, transitive packages, copied configuration, and
tool binaries are not covered merely by the top-level tool repository's license.

## Hosted Services and CI Actions

The following section is **methodological synthesis**.

A hosted coverage service and its upload software are different dependencies.
The uploader's open-source license governs the uploader code. Service terms,
data retention, credentials, source/report disclosure, availability, and
privacy are separate operational and contractual questions.

CI actions are executable third-party code even when they do not enter release
artifacts. Their repository license is therefore only one part of review;
source provenance, immutable revision, permissions, secrets, network behavior,
and produced artifacts are separate supply-chain facts.

License compatibility is one input to evaluating use or distribution; it does
not replace compliance with the actual terms or analysis of other applicable
rights. It also does not establish package integrity, maintainer trust,
vulnerability status, reproducibility, or fitness for a testing role.
