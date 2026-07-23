---
title: Zensical documentation tooling
description: Current official behavior and source-supported operating practices for Zensical sites, GitHub Pages, multilingual content, search, and Python API reference.
topics: [zensical, documentation, github-pages, mkdocstrings, griffe, python, search, localization, versioning]
checked_at: 2026-07-23
---

# Zensical Documentation Tooling

This document records current Zensical behavior and related official tool
contracts. Zensical is under active development, so claims about supported
features are dated by `checked_at`. The document does not select a repository
layout, URL, theme, release policy, or deployment destination for a particular
project.

## Status and Configuration

Zensical currently uses `0.0.x` development releases and states that it follows
semantic versioning. Its upgrade documentation advises reviewing changes when
moving between major versions.
<https://zensical.org/docs/upgrade/>

The native configuration file is `zensical.toml`. Zensical can also read
`mkdocs.yml` to ease migration from Material for MkDocs. The CLI exposes `new`,
`build`, and `serve` commands.
<https://zensical.org/docs/setup/basics/>
<https://zensical.org/docs/usage/cli/>

```console
zensical serve
zensical build --strict
```

`serve` starts a preview server with live reload and is not intended as a
production server. `build` writes the static result to `site_dir`, which
defaults to `site`; strict mode is available through `--strict`.
<https://zensical.org/docs/usage/preview/>
<https://zensical.org/docs/usage/build/>

Zensical can be installed as a Python development dependency with `uv add --dev
zensical` and invoked with `uv run zensical`. Its installation guide notes that
uv's symlink installation mode is not supported.
<https://zensical.org/docs/get-started/#install-with-uv>

## Content and Output Paths

`docs_dir` and `site_dir` are relative to the configuration file. `docs_dir`
must be a subdirectory and currently cannot be `.`.
<https://zensical.org/docs/setup/basics/#docs_dir>
<https://zensical.org/docs/setup/basics/#site_dir>

Methodological synthesis:

- Treat `docs_dir` as authored input and `site_dir` as reproducible generated
  output.
- Keep unrelated Markdown outside `docs_dir` when it must not become a public
  page. An explicit navigation list controls discoverability but should not be
  treated as a confidentiality boundary.
- Run a clean strict build in continuous integration so enabled validation
  issues fail before deployment.
- Keep configuration close enough to the source paths that file watching and
  relative path resolution match local preview behavior.

Zensical validates internal Markdown links and anchors during builds. Validation
issues are warnings by default; `zensical build --strict` aborts with a nonzero
exit status after reporting them. Several additional reference and footnote
checks remain available but are deprecated in their current form.
<https://zensical.org/docs/setup/validation/>

## Navigation and Page Layout

Zensical derives navigation from the content tree by default. An explicit
`project.nav` can define order, titles, sections, and external links; content
paths are relative to `docs_dir`. Theme feature flags provide instant
navigation, anchor tracking, tabs, sections, breadcrumbs, section indexes, and
navigation pruning.
<https://zensical.org/docs/setup/navigation/>

Important constraints include:

- instant navigation and instant previews require `site_url`;
- `navigation.prune` and `navigation.expand` are incompatible;
- `navigation.indexes` and `toc.integrate` are incompatible;
- a page can hide navigation or its table of contents through frontmatter.

Zensical supports additional CSS and JavaScript from `docs_dir`. Template
extension uses a configured `custom_dir` and MiniJinja templates; a page can
select a custom template in frontmatter. Colors, palettes, fonts, logos,
favicons, and many icons have configuration-level controls before template
overrides are needed.
<https://zensical.org/docs/customization/>
<https://zensical.org/docs/setup/colors/>
<https://zensical.org/docs/setup/fonts/>
<https://zensical.org/docs/setup/logo-and-icons/>

Methodological synthesis:

- Use built-in configuration and feature flags before CSS, JavaScript, or
  template overrides. Each override follows internal theme structure that can
  change as the pre-1.0 tool evolves.
- Keep navigation shallow enough to scan. Add tabs, expansion, or pruning only
  in response to actual content scale and reader movement.
- Use admonitions for exceptional side information and tabs for genuine
  alternatives, not as a way to hide the main procedure.
- Preserve semantic headings and a logical source order; visual layout does not
  replace accessible document structure.

Admonitions and content tabs are implemented through supported Markdown
extensions, including `admonition`, `pymdownx.details`, `pymdownx.superfences`,
and `pymdownx.tabbed`.
<https://zensical.org/docs/authoring/admonitions/>
<https://zensical.org/docs/authoring/content-tabs/>

## Search

Zensical's client-side search is enabled by default, works offline, and indexes
multiple languages. As of the check date, the search interface itself is only
available in English; this limitation does not disable multilingual content
search.
<https://zensical.org/docs/setup/search/>

`search.highlight` highlights matching terms after a reader follows a result.
Pages can set `search.exclude: true` in frontmatter, and sections or blocks can
use `data-search-exclude` when Attribute Lists are enabled.

Methodological synthesis:

- Exclude generated indexes, duplicate landing content, and non-reader metadata
  only when they reduce result quality; a missing result is harder to discover
  than a lower-ranked result.
- Give pages distinct, descriptive titles and use the same terminology readers
  are likely to search.
- Test search separately in each content language, including untranslated API
  names and localized task vocabulary.

## Multiple Languages

Zensical sets one canonical theme language for a project because each generated
HTML document has one default language. Its templates include Simplified
Chinese (`zh`), Traditional Chinese (`zh-Hant`), Taiwanese Chinese (`zh-TW`),
English (`en`), and many other interface translations.
<https://zensical.org/docs/setup/language/>

`project.extra.alternate` adds language-selector entries. Each entry includes a
display name, an absolute link, and a `lang` value used for `hreflang`. The link
may point to another path, domain, or separately generated site. Zensical also
permits custom interface translations through theme extension.

Zensical's page describes `alternate.lang` as an ISO 639-1 code, while the same
page and its supported-language list use values such as `zh-Hant` that include
a script subtag. W3C guidance says language tags used by HTML and XML follow
BCP 47 and draws a distinction between a complete language tag and its ISO-
derived component subtags. This is a terminology conflict in the current
Zensical documentation; interoperable Web markup should use a well-formed BCP
47 language tag.
<https://www.w3.org/International/articles/language-tags/Overview.en>

This selector connects language sites; it does not translate Markdown content
or define how translations are synchronized. Source organization, canonical
language, fallback behavior, and release parity remain responsibilities of the
documentation workflow.

## Python API Reference

Zensical has preliminary mkdocstrings support as of Zensical 0.0.11. The
integration requires installing `mkdocstrings-python`; backlinks are explicitly
listed as unsupported in the preliminary integration.
<https://zensical.org/docs/setup/extensions/mkdocstrings/>

```toml
[project.plugins.mkdocstrings.handlers.python]
inventories = ["https://docs.python.org/3/objects.inv"]
paths = ["src"]

[project.plugins.mkdocstrings.handlers.python.options]
docstring_style = "google"
show_source = false
```

An API page injects an object by dotted name:

```markdown
::: package.module.PublicType
```

The Python handler uses Griffe to collect API data. `paths` provides module
search locations. Sphinx-compatible `objects.inv` inventories enable external
cross-references, and mkdocstrings generates its own inventory by default when
the Python handler is used.
<https://mkdocstrings.github.io/python/usage/>
<https://mkdocstrings.github.io/usage/#cross-references-to-other-projects-inventories>

Zensical currently does not watch Python handler paths outside the project
folder, so changes there do not trigger live rebuilds.
<https://zensical.org/docs/setup/extensions/mkdocstrings/#configuration>

Griffe normally performs static source analysis and can inspect objects at
runtime. Compiled Python extension modules require dynamic inspection when
source information is unavailable. The Python handler exposes
`force_inspection`, but its documentation warns against enabling it blindly;
inspection imports the package, and a package is collected and cached once per
build.
<https://mkdocstrings.github.io/griffe/introduction/>
<https://mkdocstrings.github.io/python/usage/configuration/general/#force_inspection>
<https://mkdocstrings.github.io/griffe/guide/users/recommendations/python-code/#make-your-compiled-objects-tell-their-true-location>

Methodological synthesis for compiled extensions:

- Build and install an importable extension before the documentation build when
  runtime inspection supplies the public API.
- Keep package import deterministic and free of network, service, or mutable
  environment requirements; documentation builds may import it.
- Select one documented public import path and ensure runtime objects report
  locations consistent with that path.
- Use type stubs, runtime docstrings, or a Griffe extension according to which
  source contains the authoritative signatures and prose. Confirm the rendered
  result rather than assuming static and runtime metadata are merged.
- Generate reference for the public surface, not every internal member.

Griffe and mkdocstrings support Google, NumPy, and Sphinx docstring structures.
A structured style allows parameter, return, raised-exception, and other
sections to render separately; the style is a parsing contract, not evidence
that the prose is complete.
<https://mkdocstrings.github.io/griffe/guide/users/recommendations/docstrings/>

## GitHub Pages

GitHub Pages hosts static HTML, CSS, and JavaScript. GitHub distinguishes one
account site, whose repository is named `<owner>.github.io`, from project sites,
whose default locations are `<owner>.github.io/<repository>` and which can be
published from their project repositories.
<https://docs.github.com/en/pages/getting-started-with-github-pages/what-is-github-pages>

Zensical's official publishing guide uses a GitHub Actions workflow and requires
the repository's Pages source to be configured as GitHub Actions. The generated
site is uploaded as a Pages artifact and then deployed.
<https://zensical.org/docs/publish-your-site/#with-github-actions>

GitHub documents the custom-workflow sequence as checkout, optional static-site
build, artifact upload with `actions/upload-pages-artifact`, and deployment with
`actions/deploy-pages`. The deployment job uses the `github-pages` environment.
<https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages>

Methodological synthesis:

- Build the same locked dependency set locally and in Actions.
- Run the strict build for pull requests without deploying; deploy only from an
  authorized branch or release event.
- Grant workflow permissions at the smallest job scope needed for Pages and
  avoid combining unrelated release credentials with documentation deployment.
- Set `site_url` to the final base URL and test root-relative and absolute links
  under a project-site subpath.
- Upload generated static output as an artifact rather than committing it to the
  source branch unless the chosen versioning tool requires a deployment branch.

## Documentation Versions

Zensical currently integrates with its fork of `mike` as a bridge for versioned
documentation on GitHub Pages. The fork is installed directly from GitHub, is
not published on PyPI, and is described as temporary until native versioning is
available.
<https://zensical.org/docs/setup/versioning/>

The integration enables the version selector with `extra.version.provider =
"mike"`. `mike deploy` writes versions to subdirectories of `site_url`; aliases
such as `latest` can point to a version, and `mike set-default` creates the root
redirect. The official guide recommends setting `site_url` explicitly and a
trailing slash.

Methodological synthesis:

- Treat versioned documentation as immutable release output; changing an old
  version can invalidate links and historical API expectations.
- Distinguish a mutable alias such as `latest` from an immutable version path.
- Build documentation from the source and API corresponding to the version
  label, not from an unrelated working tree.
- Because the current integration is explicitly transitional, isolate its
  commands in deployment configuration and avoid building other content logic
  around mike-specific internals.

## Repository Links and Privacy

`repo_url` displays a repository link. For GitHub repositories, Zensical can
derive view-source and edit-page links when `content.action.view` or
`content.action.edit` is enabled; `edit_uri` adjusts paths for a custom branch,
source directory, or separate documentation repository.
<https://zensical.org/docs/setup/repository/>

Search is local to the browser, but external fonts, analytics, repository
metadata, and custom scripts can create network requests. Zensical supports
self-hosted fonts and a consent mechanism for integrations that use cookies.
<https://zensical.org/docs/setup/fonts/#autoloading>
<https://zensical.org/docs/setup/data-privacy/>

Methodological synthesis:

- Add analytics or custom JavaScript only for a defined information need.
- Check external requests, consent behavior, and offline behavior after theme or
  integration changes.
- Prefer repository edit links over a separate feedback system when the target
  audience can use the repository workflow.
