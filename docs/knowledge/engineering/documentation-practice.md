---
title: Technical documentation practice
description: Source-supported information architecture, writing, examples, tone, and reference practices for software libraries.
topics: [documentation, information-architecture, tutorials, guides, reference, examples, style]
checked_at: 2026-07-23
---

# Technical Documentation Practice

This document records official guidance, version-pinned observed practice, and
methodological synthesis for public software-library documentation. It does not
prescribe a site map, generator, or publication plan for a particular project.

## Documentation Modes

Diátaxis distinguishes four kinds of documentation by the need they serve:

- a **tutorial** guides a learner through a successful learning experience;
- a **how-to guide** helps a competent user accomplish a real-world goal;
- **reference** supplies accurate, complete, neutral facts needed while working;
- **explanation** supplies context and helps the reader understand why.

The framework also warns against creating four empty top-level sections merely
to imitate the model. It recommends improving content in small increments and
letting structure follow the actual material.
<https://diataxis.fr/start-here/>
<https://diataxis.fr/how-to-use-diataxis/>

The four modes describe authorial intent rather than mandatory navigation
labels. Mixing them without transitions creates predictable problems: extended
background interrupts a tutorial, a sequence of instructions makes reference
hard to scan, and an API inventory does not teach a first-time user how objects
cooperate.

## Library Entry Points

The rustdoc book says crate-level documentation should first summarize the
library's role, explain why it is useful, and provide a real-world example
without copy-and-paste shortcuts. It advises beginning incrementally with an
introduction, an example, and features. The first sentence should help a reader
decide whether the library fits the use case.
<https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html#getting-started>

The following reading path is methodological synthesis across the sources in
this document, not a required site structure:

| Reader question | Content that answers it | Primary mode |
| --- | --- | --- |
| What is this, and does it fit? | Definition, audience, role, boundaries | Overview |
| Can I make it work? | One complete, reproducible first success | Tutorial |
| How do I accomplish a task? | Goal-oriented procedure with prerequisites | How-to |
| Why does it behave this way? | Mental model, constraints, and trade-offs | Explanation |
| What exactly does this symbol accept or return? | Public contract and API inventory | Reference |
| What happens at the edge? | Errors, limits, compatibility, and troubleshooting | How-to and reference |

An overview is a routing page rather than a fifth documentation mode. A compact
overview can combine a definition, a credible minimal example, a small feature
and boundary summary, and links to the four modes without trying to contain
their full detail.

## Progressive Disclosure

Tutorials protect the learner's progress; explanation can be linked when it is
not needed for the next successful action. How-to guides assume competence and
can omit teaching already covered elsewhere. Reference follows the structure of
the interface it describes and remains neutral.
<https://diataxis.fr/start-here/>

Methodological consequences include:

- teach the smallest complete lifecycle before adding optional variations;
- put prerequisites before an action and verification immediately after it;
- move uncommon limits and alternative paths out of the main success path;
- link to deeper explanation instead of duplicating it inside every procedure;
- organize task guides by user goals, while allowing API reference to mirror
  modules, types, functions, and methods.

## Examples as Executable Evidence

The rustdoc book recommends realistic, copyable examples and says that examples
help readers understand what an item is, how it is used, and why it exists.
Rust documentation examples are compiled and run as documentation tests under
rustdoc's rules.
<https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html#documenting-components>
<https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html>

Python's doctest module can execute interactive examples embedded in docstrings
and text files. Its documentation also notes that doctest is exacting about
expected output and is not a substitute for a comprehensive test suite.
<https://docs.python.org/3/library/doctest.html>

The following is methodological synthesis:

- A runnable example should include the imports, setup, action, and observable
  result needed to establish that it succeeded.
- Omitted production concerns should be named when copying the example without
  them would be unsafe or materially misleading.
- Prefer one maintained source for a substantial example. Test that source and
  include or link it from prose rather than maintaining divergent copies.
- Use placeholders that describe what the reader must replace; unexplained
  values such as `xxx` obscure intent.
- Command output is useful when it verifies success or supplies a value needed
  by a later step; otherwise it adds maintenance cost without evidence.

Google's developer style guide likewise recommends descriptive placeholders and
showing command output only when it adds value.
<https://developers.google.com/style/placeholders>
<https://developers.google.com/style/code-syntax#output>

## Public API Reference

PEP 257 defines public Python module, function, class, and method docstrings. A
one-line docstring states the effect without repeating the signature. A
multi-line docstring begins with a summary, followed by a blank line and further
detail; function documentation covers applicable arguments, return values,
side effects, exceptions, and calling restrictions.
<https://peps.python.org/pep-0257/#one-line-docstrings>
<https://peps.python.org/pep-0257/#multi-line-docstrings>

Rust documentation similarly starts with a concise summary, then adds contract
details and examples. The Rust API Guidelines call for `Errors`, `Panics`, and
`Safety` sections when those conditions are part of the public contract.
<https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html#documenting-components>
<https://rust-lang.github.io/api-guidelines/documentation.html>

Methodological synthesis:

- Reference documents caller-visible behavior, not an implementation tour.
- Type annotations and signatures state shape; prose states meaning, units,
  state requirements, ownership, side effects, failures, and stability.
- Generated reference is only as complete as the source metadata and
  docstrings from which it is built.
- A public API inventory complements tutorials and task guides; it does not
  replace them.

## Voice and Terminology

Google's developer documentation guidance recommends a conversational,
friendly, respectful tone without becoming slangy, overly cute, or overly
formal. It advises direct language for readers who may be in a hurry, active
voice, present tense, second person for the reader, and imperative verbs for
instructions. Software and other actors should be named when they perform an
action.
<https://developers.google.com/style/tone>
<https://developers.google.com/style/person>
<https://developers.google.com/style/voice>

It also advises against idioms, clichés, unnecessary jargon, marketing
buzzwords, and words such as *easy* or *simply* that can dismiss a reader's
difficulty. Code identifiers, filenames, literal input, and similar entities
use semantic code formatting rather than quotation marks or manual font styles.
<https://developers.google.com/style/word-list>
<https://developers.google.com/style/code-in-text>
<https://developers.google.com/style/text-formatting>

Methodological synthesis:

- Use one established term for one concept and define an unfamiliar term at
  first use.
- State capabilities and boundaries concretely; avoid claims such as
  "next-generation" or "blazing fast" unless the page supplies a defined,
  reproducible comparison.
- Prefer short sentences and one main idea per paragraph.
- Describe what the reader should do, what the software does, and what result
  to expect with explicit subjects.
- Do not teach background that the intended audience already knows unless that
  background is required to use the interface correctly.

## Version-Pinned Observed Practice

These examples are observations, not universal rules:

- Requests 2.34.2 places a short identity statement and working interaction on
  its landing page, then separates the user guide, advanced usage, API
  documentation, and contributor guide.
  <https://docs.python-requests.org/en/stable/>
- hyper-h2 4.3.0 states both its protocol role and its no-I/O boundary before
  routing readers to installation, a getting-started path, negotiation,
  examples, advanced usage, low-level details, and API reference.
  <https://python-hyper.org/projects/hyper-h2/en/stable/>
- Serde 1.0.229 uses crate reference for the public Rust interface and links to
  a separate website for additional usage documentation and examples.
  <https://docs.rs/serde/1.0.229/serde/>
- Tokio's documentation separates a learning tutorial and task-oriented topics
  from API documentation hosted on docs.rs; its tutorial also describes the
  runtime's role and cases where it is not a good fit.
  <https://github.com/tokio-rs/website/blob/55335b4efd81aa676d038bd7320f7185642d39ea/content/tokio/tutorial/index.md>

Across these examples, the recurring pattern is separation of first success,
task guidance, conceptual understanding, and exact API lookup. The number and
names of top-level sections vary with the material and audience.
