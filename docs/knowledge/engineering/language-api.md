---
title: Language and API conventions
description: Established Rust and Python naming, visibility, documentation, comment, and API conventions.
topics: [rust, python, pyo3, api, naming, documentation, comments, docstrings]
checked_at: 2026-07-14
---

# Language and API Conventions

This document records established Rust and Python API readability conventions,
distinguishing official convention from synthesis; it prescribes no layout or score.

## Rust: Official Convention

The Rust Style Guide defines the default style referenced by `rustfmt`; it does
not forbid projects from choosing a different style.
<https://doc.rust-lang.org/style-guide/#the-default-rust-style>

The Rust API Guidelines are recommendations, not language rules or a mandate;
they are largely authored by the Rust library team from ecosystem experience.
<https://rust-lang.github.io/api-guidelines/>

### Naming

- Types, traits, and enum variants use `UpperCamelCase`; functions, methods,
  modules, fields, and variables use `snake_case`; constants use `SCREAMING_SNAKE_CASE`.
  <https://doc.rust-lang.org/style-guide/advice.html#names>
- The API Guidelines use `as_` for free borrowed conversions, `to_` for work or
  owned results, and `into_` for ownership-consuming conversions.
  <https://rust-lang.github.io/api-guidelines/naming.html#ad-hoc-conversions-follow-as_-to_-into_-conventions-c-conv>
- Ordinary getters generally omit `get_`; a mutable counterpart commonly uses
  a `_mut` suffix. Collection iterator methods use `iter`, `iter_mut`, and
  `into_iter` when those concepts fit the collection.
  <https://rust-lang.github.io/api-guidelines/naming.html#getter-names-follow-rust-convention-c-getter>
- Consistent word order matters more than inventing a universal order; the API
  Guidelines compare names with similar standard-library functionality.
  <https://rust-lang.github.io/api-guidelines/naming.html#names-use-a-consistent-word-order-c-word-order>

### Public Surface and Modules

- Rust items are private by default, except associated items in a public trait
  and variants of a public enum. `pub(crate)`, `pub(super)`, and `pub(in path)`
  restrict visibility to explicit scopes.
  <https://doc.rust-lang.org/reference/visibility-and-privacy.html#visibility-and-privacy>
- A public item remains externally reachable only through reachable ancestor
  modules or a public re-export. Visibility therefore defines an API boundary.
  <https://doc.rust-lang.org/reference/visibility-and-privacy.html#r-vis.reexports>
- The Style Guide places imports and out-of-line module declarations before
  other items and says to avoid `#[path]` where possible. These are source
  layout conventions, not an architectural decomposition rule.
  <https://doc.rust-lang.org/style-guide/items.html#modules>
  <https://doc.rust-lang.org/style-guide/advice.html#modules>

### Documentation Contracts

- Rust distinguishes ordinary comments from documentation attributes. `//` and
  `/* ... */` are ordinary comments; `///` and `/** ... */` document the next
  item; `//!` and `/*! ... */` document the containing crate or module.
  <https://doc.rust-lang.org/reference/comments.html>
- rustdoc says public items should be documented. Crate and module docs use
  `//!`; item docs use `///`. The first paragraph should be a concise summary,
  followed by contract details and a realistic example when one helps callers.
  <https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html#getting-started>
- Public type documentation explains what a value represents and how it fits
  the API. Public functions and methods document caller-visible behavior rather
  than repeating types already present in the signature.
  <https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html#documenting-components>
- The API Guidelines call for `Errors`, `Panics`, and `Safety` sections when
  those conditions form part of a public function's contract. Public variants,
  fields, constants, and error types likewise need the meaning callers cannot
  obtain from their spelling alone.
  <https://rust-lang.github.io/api-guidelines/documentation.html#function-docs-include-error-panic-and-safety-considerations-c-failure>
- Rust documentation examples are compiled as doctests by default, subject to
  rustdoc's execution rules. This checks code, not completeness of the prose.
  <https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html>
  <https://doc.rust-lang.org/cargo/commands/cargo-test.html#documentation-tests>

### Internal Comments

The following is methodological synthesis rather than a Rust language rule:

- Use item documentation for a generated documentation audience and ordinary
  comments for implementation reasoning. Private items need `///` only when
  private rustdoc is intentionally part of the maintenance workflow.
- Put durable reasoning beside the smallest code region it governs: protocol
  edge cases, state invariants, failure atomicity, security or resource bounds,
  and non-obvious ownership or allocation constraints.
- Do not narrate syntax, names, file layout, patch history, or comparison with a
  reference implementation. If the code cannot express the idea clearly,
  simplify the code before adding explanatory prose.
- Bind a specification link to the exact rule it supports. A module-level
  bibliography is not a substitute for a local contract or a focused test.

### Errors, Enums, and Constants

- The API Guidelines recommend meaningful public error types that implement
  `std::error::Error` and `Display`, rather than using `()` as an error type.
  <https://rust-lang.github.io/api-guidelines/interoperability.html#error-types-are-meaningful-and-well-behaved-c-good-err>
- The failure contract belongs beside the fallible function or trait method,
  including implementation-permitted failures of trait methods.
  <https://rust-lang.github.io/api-guidelines/documentation.html#function-docs-include-error-panic-and-safety-considerations-c-failure>
- Enum variants use `UpperCamelCase`; constants and immutable statics use
  `SCREAMING_SNAKE_CASE`. Public enum variants are public by language rule.
  <https://doc.rust-lang.org/style-guide/advice.html#names>
  <https://doc.rust-lang.org/reference/visibility-and-privacy.html#visibility-and-privacy>

### Lints

- Clippy groups lints by intent. Style is explicitly opinionated, and pedantic
  lints may intentionally produce false positives.
  <https://doc.rust-lang.org/clippy/lints.html#clippys-lints>
- Clippy advises against enabling the whole `restriction` group because some
  restrictions are unsuitable or contradictory; individual lints can be
  selected and locally allowed with justification.
  <https://doc.rust-lang.org/clippy/lints.html#restriction>

## Python: Official Convention

PEP 8 gives consistency within a project or module priority when strict
adherence would reduce readability or conflict with established code.
<https://peps.python.org/pep-0008/#a-foolish-consistency-is-the-hobgoblin-of-little-minds>

### Naming

- Modules and packages use short lowercase names; module underscores may aid
  readability, while package underscores are discouraged. Classes use
  `CapWords`.
  <https://peps.python.org/pep-0008/#package-and-module-names>
  <https://peps.python.org/pep-0008/#class-names>
- Functions, methods, and variables use lowercase words separated by
  underscores. Module-level constants use uppercase words separated by
  underscores.
  <https://peps.python.org/pep-0008/#function-and-variable-names>
  <https://peps.python.org/pep-0008/#constants>
- Error exception classes use the class convention and normally end in
  `Error` when they represent errors. A trailing underscore is preferred over
  misspelling a name that conflicts with a keyword.
  <https://peps.python.org/pep-0008/#exception-names>
  <https://peps.python.org/pep-0008/#function-and-method-arguments>

### Public Surface and Modules

- PEP 8 treats documented interfaces as public unless explicitly marked
  provisional or internal. Internal names use one leading underscore, and
  modules can enumerate their public API with `__all__`.
  <https://peps.python.org/pep-0008/#public-and-internal-interfaces>
- Imported names are implementation details unless documented or re-exported.
  The typing spec also defines private symbols, re-exports, and `__all__`.
  <https://typing.python.org/en/latest/spec/distributing.html#library-interface-public-and-private-symbols>
- PEP 8 recommends absolute imports for their readability and diagnostics, but
  accepts explicit relative imports when absolute spelling would be needlessly
  verbose. Wildcard imports should generally be avoided because they obscure
  the namespace.
  <https://peps.python.org/pep-0008/#imports>

### Documentation Contracts

- PEP 257 defines docstrings for public modules, functions, classes, and
  methods. A one-line docstring states an effect rather than repeating the
  callable signature; a multi-line docstring has a summary, blank line, and
  elaboration.
  <https://peps.python.org/pep-0257/#one-line-docstrings>
  <https://peps.python.org/pep-0257/#multi-line-docstrings>
- Function and method docstrings document applicable arguments, returns, side
  effects, exceptions, call restrictions, and keyword-interface status. Class
  docstrings identify public and subclass-facing interfaces where applicable.
  <https://peps.python.org/pep-0257/#multi-line-docstrings>
- The Google Python Style Guide requires docstrings for public, non-trivial, or
  non-obvious functions and methods. Its format uses a one-line summary and,
  when needed, `Args:`, `Returns:` or `Yields:`, and `Raises:` sections. Sections
  that add no information may be omitted.
  <https://google.github.io/styleguide/pyguide.html#383-functions-and-methods>
- Google-style class docstrings say what an instance represents and use an
  `Attributes:` section for public non-property attributes. Property docstrings
  describe the property as a value rather than as a getter call.
  <https://google.github.io/styleguide/pyguide.html#384-classes>
- Module docstrings describe the module's contents and use. Test-module
  docstrings are omitted when they would add only a redundant label.
  <https://google.github.io/styleguide/pyguide.html#382-modules>
- Block and inline comments are reserved for tricky or non-obvious reasoning;
  they do not describe operations already clear from Python syntax.
  <https://google.github.io/styleguide/pyguide.html#385-block-and-inline-comments>

### PyO3 Documentation Boundary

- PyO3 0.29.0 exposes Rust functions and methods annotated with `#[pyfunction]`
  or `#[pymethods]` as Python callables. It derives `__text_signature__` from
  their Rust signatures, and its official example shows a Rust `///` comment
  becoming the callable's Python `__doc__` value.
  <https://pyo3.rs/main/function/signature#making-the-function-signature-available-to-python>
- `#[pyclass]` exposes a Rust struct or enum as a Python class, while
  `#[pymethods]` defines its Python-visible methods and descriptors.
  <https://pyo3.rs/v0.29.0/class>

The boundary implies that a doc comment on a Python-visible PyO3 item is part
of the Python API, even though it is written in Rust source. Its vocabulary,
sections, exceptions, and examples therefore need to target Python callers.
Selecting Google style for that text is a project convention, not a PyO3
requirement. Rust-only implementation details remain ordinary Rust comments.

### Errors, Enums, Constants, and Typing

- PEP 8 recommends deriving ordinary application exceptions from `Exception`,
  designing hierarchies around distinctions callers need to catch, preserving
  causes through exception chaining, and catching specific exceptions where
  possible.
  <https://peps.python.org/pep-0008/#programming-recommendations>
- Python's `Enum` exposes member `name` and `value`; values may be of any type.
  The typing spec requires class-syntax support but makes function syntax
  optional for type checkers.
  <https://docs.python.org/3/library/enum.html#enum.Enum>
  <https://typing.python.org/en/latest/spec/enums.html#enum-definition>
- PEP 8's uppercase convention is stated for module-level constants. It does
  not establish a separate mandatory case convention for every `Enum` member.
  <https://peps.python.org/pep-0008/#constants>
- Python type hints support static analysis but are not mandatory runtime type
  checks; runtime enforcement requires separate machinery.
  <https://typing.python.org/en/latest/spec/type-system.html#non-goals>

## Methodological Synthesis

The following conclusions are synthesis, not quotations or universal rules:

- Consistent naming lowers vocabulary-switching cost, but a correctly cased
  name can still describe the wrong abstraction.
- Explicit public/private boundaries reduce accidental surface area, but they
  do not prove that the chosen boundary is coherent or stable.
- Documentation and doctests provide contract and example evidence; neither
  proves that all behavior, failure modes, or compatibility obligations are
  covered.
- Comment syntax identifies its audience, not its quality. A complete public
  contract can require several paragraphs, while an internal invariant may need
  only one local line; neither should exist merely to satisfy a comment count.
- Formatter, lint, and type-checker success proves only encoded rules; it does
  not measure architecture, coupling, change cost, or maintainability.
- Numeric complexity thresholds are policy choices. None of the sources above
  supplies a universal maintainability threshold for mixed Rust/Python code.
