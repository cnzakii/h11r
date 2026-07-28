//! Python extension module for `h11r`.

#![deny(unsafe_code)]

use pyo3::prelude::*;
use pyo3::types::PyModule;

mod api;
mod buffer;

#[pymodule(gil_used = false)]
fn _core(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add("__version__", env!("CARGO_PKG_VERSION"))?;
    api::register(module)
}
