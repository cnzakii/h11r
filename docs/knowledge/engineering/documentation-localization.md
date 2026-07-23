---
title: Technical documentation localization
description: Official terminology, translatable-source guidance, bilingual site semantics, and localization quality methods for developer documentation.
topics: [documentation, localization, translation, terminology, bilingual, chinese, english, internationalization]
checked_at: 2026-07-23
---

# Technical Documentation Localization

This document covers source-supported practices for English and Chinese
technical documentation. It records language and localization methods, not a
choice of canonical language, translation workflow, URL scheme, or release
policy for a particular project.

## Distinct Concepts

Google distinguishes:

- **translation**, changing content from one language to another;
- **localization**, adapting a product and its documentation for a locale,
  which can include formats and cultural conventions beyond words;
- **internationalization**, designing content and systems to reduce the work
  required for localization.

<https://developers.google.com/style/translation>

Microsoft's globalization glossary additionally defines a locale as a body of
language- and region-sensitive properties and a translation memory as a store
of source segments paired with translations. A language and a locale are
therefore related but not interchangeable concepts.
<https://learn.microsoft.com/en-us/globalization/reference/glossary>

## Write Translatable Source

Google's global-audience guidance recommends short, unambiguous sentences,
simple words, active voice, present tense, explicit subjects, consistent
terminology, and standard sentence structures. It advises avoiding idioms,
slang, culturally specific humor, ambiguous pronouns, and directional phrases
such as "above" when a stable link or label is available.
<https://developers.google.com/style/translation>

Microsoft similarly recommends short sentences and says that articles,
relative pronouns, and other apparently optional words can clarify grammatical
structure for readers and machine translation.
<https://learn.microsoft.com/en-us/style-guide/global-communications/writing-tips>

Methodological synthesis:

- Resolve ambiguity in the source before translating it. Translation should
  not be asked to infer which actor, object, state, or condition was intended.
- Keep headings descriptive and paragraphs focused on one main idea so that a
  changed segment has a clear translation unit.
- Use the same source phrase for the same recurring instruction; cosmetic
  paraphrase reduces terminology and translation-memory reuse.
- Prefer semantic Markdown for code, emphasis, links, lists, and tables so that
  language versions preserve meaning without copying presentation details.
- Put essential information in text. Screenshots and text embedded in images
  are harder to localize and can become stale independently.

## Terminology Management

Microsoft describes terminology management as an early localization activity:
approved terms reduce independent, inconsistent choices by translators. Its
terminology collection is distributed in TBX, a standard terminology-exchange
format, and its language-specific localization style guides cover technical
style, usage, and locale-specific formats.
<https://learn.microsoft.com/en-us/globalization/localization/managing-terminology>
<https://learn.microsoft.com/en-us/globalization/reference/microsoft-terminology>
<https://learn.microsoft.com/en-us/globalization/reference/microsoft-style-guides>

Translation memories reuse previously translated source segments. Microsoft
notes that memories require maintenance because duplicate translations and
changed terminology otherwise accumulate. Machine translation can assist a
translator, but its output still has to meet the selected quality goal.
<https://learn.microsoft.com/en-us/globalization/localization/translation-memories>
<https://learn.microsoft.com/en-us/globalization/reference/glossary>

A lightweight termbase can represent the same method without requiring a CAT
platform. The following fields are methodological synthesis:

| Field | Purpose |
| --- | --- |
| Source term | Canonical spelling and capitalization |
| Target term | Approved localized form |
| Definition | The concept, not merely a synonym |
| Context | API, protocol, UI, action, state, or ordinary prose |
| Status | Approved, provisional, deprecated, or do not translate |
| Source | Standard, official product documentation, or style authority |
| Notes | First-use form, abbreviation, ambiguity, or forbidden alternatives |

One spelling does not always imply one translation. A termbase needs context
when the same English word has different technical meanings. Conversely,
multiple Chinese translations for the same concept should be intentional and
documented rather than accidental variation.

Official bilingual documentation can supply domain-specific evidence. For
example, the Python 3.14 English and Simplified Chinese glossaries distinguish
an `argument`, the value supplied by a call, from a `parameter`, the named
entity in a function or method definition. The Chinese glossary uses “形参” for
`parameter` and uses both “参数” and “实参” while explaining `argument`; this is
evidence for Python terminology, not a universal rule for every domain.
<https://docs.python.org/3/glossary.html#term-argument>
<https://docs.python.org/zh-cn/3/glossary.html#term-argument>

A source hierarchy for a term is methodological synthesis:

1. use the governing standard for protocol and formal concepts;
2. use official language or product glossaries for their own interfaces;
3. use established domain convention when no authority defines a localized
   form;
4. record the chosen meaning, target form, and provenance in the termbase;
5. record rejected alternatives when they are plausible enough to recur.

## Code, Protocol Tokens, and Prose

W3C ITS 2.0 defines data categories for whether content is translatable,
localization notes, terminology, language information, and localization quality
issues. A localization note can explain ambiguous context or tell a translator
that a value must remain in the source language.
<https://www.w3.org/TR/its20/#datacategories-defaults-etc>
<https://www.w3.org/TR/its20/#locNote-datacat>

Google's code-formatting guidance treats identifiers, filenames, literal input,
and code values as semantic code elements and warns against inflecting code
identifiers as ordinary prose.
<https://developers.google.com/style/code-in-text>

Methodological synthesis for developer documentation:

- Preserve executable identifiers, command names, flags, filenames, wire
  tokens, literal values, and code syntax unless the software itself exposes a
  localized value.
- Translate the surrounding explanation and code comments when doing so does
  not change the executable example. Re-run or otherwise verify the resulting
  sample after any edit.
- On first use, pair a localized term with its established source term when the
  source term is needed for API lookup, standards research, or disambiguation.
  Repeating both forms everywhere usually reduces readability.
- Mark product names and intentionally untranslated terms in the termbase
  rather than relying on translator memory.
- Preserve links to the governing specification when translating normative
  protocol claims.

RFC 8174 assigns special meanings to the uppercase requirement words `MUST`,
`SHOULD`, `MAY`, and their related forms only when a document states that it is
using the BCP 14 interpretation. When translated prose quotes or summarizes a
normative requirement, retaining the BCP 14 keyword and its source citation
keeps the requirement level auditable.
<https://www.rfc-editor.org/rfc/rfc8174.html>

## English and Chinese Technical Prose

There is no single universal English-to-Chinese glossary for software APIs.
Language-specific style guides, authoritative standards, product terminology,
and domain convention can conflict, so terminology decisions require recorded
context and provenance.

The following are methodological writing rules rather than language standards:

- Translate meaning, not English word order. Chinese prose normally benefits
  from explicit conditions followed by direct actions and results.
- Keep API names visually distinct with code formatting; do not force a Chinese
  grammatical ending into an identifier.
- Prefer a stable, specific verb for each repeated action. Avoid alternating
  among several near-synonyms solely for stylistic variety.
- Avoid unnecessary subjects such as “the user” in procedures; give the action
  directly when the actor is the reader.
- Do not add promotional intensity, certainty, or normative force that is not
  present in the source.
- Review punctuation, spacing, dates, units, and link text as locale-sensitive
  presentation rather than assuming word substitution completes localization.

The following patterns illustrate those rules; they are not a mandatory phrase
book:

| Purpose | English pattern | Simplified Chinese pattern |
| --- | --- | --- |
| Direct instruction | Create the connection. | 创建连接。 |
| Software action | The parser returns an event. | 解析器返回一个事件。 |
| Condition before action | If the buffer is empty, return `None`. | 如果缓冲区为空，则返回 `None`。 |
| Optional action | You can reuse the connection. | 可以复用该连接。 |
| Required action | You must call `close()`. | 必须调用 `close()`。 |
| Failure contract | Raises `ValueError` if the value is invalid. | 如果值无效，则抛出 `ValueError`。 |

The Chinese patterns omit a literal translation of “you” where the reader is
already the unambiguous actor. They retain the condition, actor, action, API
identifier, and requirement level rather than mirroring English word order.

## Page Language and Language Choice

W3C guidance says HTML pages should declare their default language with the
`lang` attribute on the `html` element. Content in another language can declare
its own language on the closest enclosing element.
<https://www.w3.org/International/questions/qa-html-language-declarations.html>

For translated sites, W3C recommends making explicit links to each language
available even when server-side language negotiation selects an initial page.
This leaves the reader in control of language choice.
<https://www.w3.org/International/questions/qa-site-conneg.en.html>

Methodological synthesis:

- Give each page one correct default language instead of presenting an entire
  parallel article as mixed-language text.
- Link equivalent language pages explicitly and preserve a reader's selected
  language when navigating where the site generator supports it.
- Treat missing translations visibly; silently serving stale or unrelated
  content under an expected language path obscures coverage.
- Use stable page identities or an explicit mapping so renamed pages do not
  break cross-language relationships.

## Localization Review

W3C ITS distinguishes terminology, mistranslation, omission, untranslated text,
formatting, and other localization-quality issue categories. Automated checks
can identify candidates, but review determines whether a candidate is an
actual error.
<https://www.w3.org/TR/its20/#lqissue-typevalues>
<https://www.w3.org/TR/its20/#lqissue-global>

A compact technical-documentation review is methodological synthesis:

1. Confirm that headings, paragraphs, lists, code blocks, links, and warnings
   have corresponding content in both language versions.
2. Check approved terminology and intentional untranslated terms.
3. Compare numbers, units, version constraints, requirement keywords, and
   negative conditions with the source.
4. Run every copied command or executable example that translation touched.
5. Build the site and check anchors, navigation, language links, search results,
   overflow, and code wrapping.
6. Have a technically competent reviewer examine protocol meaning, API names,
   state transitions, errors, and ownership language.

Parity is not proved by equal file counts. It depends on equivalent meaning,
working examples, navigable relationships, and an explicit account of content
that is intentionally untranslated or pending.
