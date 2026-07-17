---
title: Python/Rust extension boundaries
description: Official contracts and versioned behavior at Python, PyO3, and extension-module boundaries.
topics: [python, rust, pyo3, maturin, packaging, typing]
checked_at: 2026-07-14
---

# Python/Rust Extension Boundaries

This document records official contracts for Python extension modules written
with Rust. It does not select a project layout, dependency, API, version, or
release policy. PyO3 claims below are pinned to the 0.29.0 user guide; references
to Python 3.14 are explicitly versioned. Maturin and packaging specifications
are moving official documents and are qualified by the checked date above.

## Official Contracts

### Module And Export Boundary

PyO3's `#[pymodule]` macro creates the module initialization function exposed to
Python. The declared module name must match the shared-library filename, or
Python cannot find the expected `PyInit_<name>` symbol. Items become Python
exports only when added or exported through PyO3's module mechanisms.
<https://pyo3.rs/v0.29.0/module.html#python-modules>

By default, Python's import system loads extension modules through
`ExtensionFileLoader`; recognized extension suffixes and the module
initialization function are part of that loading contract.
<https://docs.python.org/3.14/c-api/extension-modules.html>
<https://docs.python.org/3.14/library/importlib.html#importlib.machinery.ExtensionFileLoader>

The Rust module tree and Python import tree are therefore different namespaces.
PyO3 can expose nested Python modules, but registering a submodule does not by
itself create an importable package hierarchy in `sys.modules`.
<https://pyo3.rs/v0.29.0/module.html#python-submodules>

### Attachment, Ownership, And Borrowing

A Rust thread must be attached to the Python interpreter before it interacts
with Python. `Python<'py>` is PyO3's proof of attachment, and the `'py` lifetime
binds interpreter-attached values such as `Bound<'py, T>` to that attachment.
<https://pyo3.rs/v0.29.0/python-from-rust.html#the-py-lifetime>

PyO3 distinguishes three Python-object smart pointers:

- `Py<T>` owns a Python reference without carrying the attachment lifetime;
- `Bound<'py, T>` owns a reference bound to an attached interpreter lifetime;
- `Borrowed<'a, 'py, T>` is a borrowed reference with both borrow and
  attachment lifetimes.

Their available operations and cloning behavior follow those ownership and
lifetime differences.
<https://pyo3.rs/v0.29.0/types.html#pyo3s-smart-pointers>

At the CPython level, Python objects are accessed through `PyObject*` pointers
and ownership is expressed through reference counts. New, borrowed, and stolen
references impose different reference-management obligations.
<https://docs.python.org/3.14/c-api/intro.html#objects-types-and-reference-counts>

### Errors And Exceptions

PyO3 represents a Python exception as `PyErr`; `PyResult<T>` is
`Result<T, PyErr>`. When an error result crosses from Rust to Python through a
PyO3 function boundary, the contained Python exception is raised.
<https://pyo3.rs/v0.29.0/function/error-handling.html#representing-python-exceptions>

Rust error types can be converted into `PyErr` where an appropriate conversion
exists. PyO3 also provides native exception types and facilities for declaring
custom Python exception classes.
<https://pyo3.rs/v0.29.0/exception.html#raising-an-exception>

Panics are not ordinary Python errors. PyO3 catches panics escaping a Python
callable and raises `PanicException`; the guide states that this exception is
derived from `BaseException`, not `Exception`.
<https://docs.rs/pyo3/0.29.0/pyo3/panic/struct.PanicException.html>

### GIL And Threading

On ordinary CPython builds, a thread must hold the global interpreter lock
before accessing Python objects or invoking the Python C API. Attaching a thread
state and holding the GIL are related requirements in GIL-enabled builds.
<https://docs.python.org/3.14/c-api/threads.html#thread-states-and-the-global-interpreter-lock>

Python 3.14 also documents free-threaded builds where the GIL is disabled, but
an attached thread state remains required for most C API use.
<https://docs.python.org/3.14/c-api/threads.html#thread-states-and-the-global-interpreter-lock>

PyO3's `Python::detach` permits Rust work to run without keeping the interpreter
attached. Python-bound values cannot be used by the detached closure, and code
must attach again before interacting with Python.
<https://pyo3.rs/v0.29.0/parallelism.html#parallelism-under-the-python-gil>

PyO3 0.29.0 documents support for free-threaded Python and requires extension
modules to declare whether they support that mode; otherwise Python enables the
GIL when importing the module.
<https://pyo3.rs/v0.29.0/free-threading.html#supporting-free-threaded-python-with-pyo3>

### Python Objects And Rust Data

`FromPyObject` defines extraction from Python objects into Rust values, while
`IntoPyObject` defines conversion of Rust values into Python objects. These are
explicit boundary conversions and may fail where the trait contract permits a
conversion error.
<https://pyo3.rs/v0.29.0/conversions/traits.html#extract-and-the-frompyobject-trait>
<https://pyo3.rs/v0.29.0/conversions/traits.html#intopyobject>

Keeping a value as `Py<T>`, `Bound<'py, T>`, or `Borrowed<'a, 'py, T>` preserves
its identity as a Python object and subjects access to PyO3's attachment and
ownership rules. Extracting it into an owned Rust value instead uses the Rust
representation produced by `FromPyObject`.
<https://pyo3.rs/v0.29.0/types.html#pyo3s-smart-pointers>
<https://pyo3.rs/v0.29.0/conversions/traits.html#extract-and-the-frompyobject-trait>

`#[pyclass]` exposes a Rust-defined type as a Python class. PyO3 imposes class
constraints, including no non-static Rust lifetime parameters, because Python
owns such objects independently of Rust lexical lifetimes.
<https://pyo3.rs/v0.29.0/class.html#restrictions>

`PyMemoryView::from` creates a Python `memoryview` over an object implementing
the buffer protocol. Keeping that object preserves the buffer owner; calling
`memoryview.tobytes()` instead creates a bytes copy. Borrowing an existing
`PyBytes` slice avoids this preliminary conversion, though an owned Rust or
Python output can still require its own copy.
<https://docs.rs/pyo3/0.29.0/pyo3/types/struct.PyMemoryView.html>
<https://docs.python.org/3.14/library/stdtypes.html#memoryview.tobytes>

### Runtime Exports And Distributed Type Information

A `.pyi` stub describes a module's public interface without providing its
runtime implementation. The typing specification defines how inline typing,
stub files, stub-only packages, and the `py.typed` marker distribute type
information.
<https://typing.python.org/en/latest/spec/distributing.html#packaging-type-information>

Stub export rules are not identical to ordinary Python source export rules;
the typing specification defines explicit re-exports and the role of `__all__`.
<https://typing.python.org/en/latest/spec/distributing.html#import-conventions>

PyO3 0.29.0 supports experimental introspection-based stub generation, but
documents incomplete feature coverage and does not introspect function-style
`#[pymodule]` exports. Manually maintained `.pyi` files therefore remain the
stable boundary for extensions using that module form.
<https://pyo3.rs/v0.29.0/python-typing-hints.html#typing-and-ide-hints-for-your-python-package>
<https://pyo3.rs/v0.29.0/type-stub#constraints-and-limitations>

### Packaging Boundary

PyPA defines `[build-system]` as the declaration of build requirements and the
build backend used to create a distribution. This metadata belongs to the
Python distribution contract rather than the imported module's runtime API.
<https://packaging.python.org/en/latest/specifications/pyproject-toml/#declaring-build-system-dependencies-the-build-system-table>

Maturin documents both pure-Rust extension projects and mixed Rust/Python
projects. In a mixed project, `python-source` identifies Python source and
`module-name` identifies where the native module is installed in the Python
import hierarchy.
<https://www.maturin.rs/project_layout.html#mixed-rustpython-project>

A wheel is an installable binary distribution archive. Its filename and
metadata carry Python, ABI, and platform compatibility tags, so the package
artifact boundary includes interpreter and platform compatibility in addition
to source metadata.
<https://packaging.python.org/en/latest/specifications/binary-distribution-format/#file-name-convention>

## Methodological Synthesis

The statements below are synthesis from the official contracts above, not
requirements imposed by PyO3, Python, Maturin, or PyPA:

- The runtime import tree, Rust module tree, distributed stubs, and wheel
  contents are separate representations of one public boundary; agreement
  between them must be established rather than assumed.
- Converting a Python object into owned Rust data changes where identity,
  mutation, reference management, and validation live. Retaining a Python
  object keeps those concerns at the interpreter boundary.
- A boundary review can examine four independent paths: successful values,
  error conversion, attachment/thread transitions, and packaged exports.
- Type-checker acceptance establishes consistency with distributed type
  declarations; it does not establish that runtime exports or native loading
  behavior match those declarations.
