---
title: Python/Rust tooling
description: Official configuration and packaging facts for mixed Python and Rust projects.
topics: [python, rust, cargo, pyproject, maturin, uv]
checked_at: 2026-07-12
---

# Python/Rust Tooling

This document records official configuration and packaging facts. It does not
select versions, dependencies, layouts, or release policy for a project.

## Python Packaging

PyPA documents three major `pyproject.toml` areas:

- `[build-system]` declares the build backend and its build requirements;
- `[project]` contains standardized project metadata used by most backends;
- `[tool.*]` contains tool-specific configuration defined by each tool.

PyPA strongly recommends a `[build-system]` table and recommends `[project]`
for new projects. `requires-python` describes the supported Python version
floor. Runtime dependencies and optional distribution extras are represented
by `dependencies` and `optional-dependencies`.

The PyPA src-layout discussion distinguishes importable source placed at the
repository root from source placed below a dedicated directory. A dedicated
source root prevents an ordinary working-directory import from silently using
files that were not installed.

Sources:

- <https://packaging.python.org/en/latest/guides/writing-pyproject-toml/>
- <https://packaging.python.org/en/latest/specifications/pyproject-toml/>
- <https://packaging.python.org/en/latest/discussions/src-layout-vs-flat-layout/>

## Cargo

Cargo uses `Cargo.toml` for package/workspace metadata, targets, dependencies,
features, and profiles. `Cargo.lock` records a resolved dependency graph.

A Cargo workspace coordinates member packages and shares a lockfile and target
directory. A virtual workspace has `[workspace]` without a root `[package]`.
Workspace package metadata, dependencies, and lints can be inherited only when
members opt into the corresponding workspace keys.

Cargo's conventional package targets include `src/lib.rs`, `src/main.rs`,
`tests/`, `examples/`, and `benches/`; explicit target tables are available
when conventions do not fit.

Sources:

- <https://doc.rust-lang.org/cargo/reference/manifest.html>
- <https://doc.rust-lang.org/cargo/reference/workspaces.html>
- <https://doc.rust-lang.org/cargo/guide/project-layout.html>
- <https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html>

## Maturin And PyO3

Maturin documents mixed Rust/Python projects in which Python source and a Rust
extension are packaged together. `tool.maturin.python-source` selects the
Python source root, and `module-name` can place the extension inside a Python
package. Maturin documents this layout as a way to avoid importing a local
package that lacks its compiled native module.

PyO3 exposes Python modules through `#[pymodule]` and Python classes/functions
through its attribute macros. PyO3's typing documentation describes manual
stub files and its evolving stub-generation facilities; runtime exports and
distributed stubs remain separate artifacts that can drift.

Sources:

- <https://www.maturin.rs/project_layout.html>
- <https://www.maturin.rs/config>
- <https://pyo3.rs/main/module.html>
- <https://pyo3.rs/main/python-typing-hints>
- <https://pyo3.rs/main/type-stub>

Runtime exports and distributed type information are covered in
[`boundary.md`](boundary.md).
