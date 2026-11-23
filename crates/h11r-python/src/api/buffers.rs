//! Reading bytes from Python buffers and text.

use super::*;
use pyo3::exceptions::{PyAttributeError, PyTypeError, PyValueError};
use pyo3::types::{PyAny, PyBytes, PyMemoryView, PyString};

/// Runs `consume` over the bytes of a buffer-protocol object.
///
/// Immutable `bytes` are borrowed directly. Other buffers are copied first
/// because another thread may mutate them while they are read.
pub(super) fn with_buffer<R>(
    value: &Bound<'_, PyAny>,
    consume: impl FnOnce(&[u8]) -> R,
) -> PyResult<R> {
    if let Ok(bytes) = value.cast::<PyBytes>() {
        return Ok(consume(bytes.as_bytes()));
    }
    Ok(consume(owned_bytes(value)?.as_bytes()))
}

/// Returns `value` itself when it is exactly `bytes`, and otherwise copies the
/// buffer once into a new `bytes`, which nothing else can then mutate.
pub(super) fn owned_bytes<'py>(value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyBytes>> {
    if let Ok(bytes) = value.cast_exact::<PyBytes>() {
        return Ok(bytes.clone());
    }
    // `tobytes()` copies straight into new storage; `PyBytes::new_with` would
    // zero the storage first, an extra pass over large bodies.
    let view = PyMemoryView::from(value)?;
    Ok(view.call_method0("tobytes")?.cast_into::<PyBytes>()?)
}

pub(super) fn contiguous_buffer_len(value: &Bound<'_, PyAny>) -> PyResult<usize> {
    if let Ok(bytes) = value.cast::<PyBytes>() {
        return Ok(bytes.as_bytes().len());
    }
    let view = PyMemoryView::from(value)?;
    if !view.getattr("contiguous")?.extract::<bool>()? {
        return Err(PyValueError::new_err(
            "send_data_parts requires a contiguous buffer",
        ));
    }
    view.getattr("nbytes")?.extract()
}

pub(super) fn body_nbytes(value: &Bound<'_, PyAny>) -> PyResult<usize> {
    match contiguous_buffer_len(value) {
        Ok(length) => Ok(length),
        Err(buffer_error) if buffer_error.is_instance_of::<PyTypeError>(value.py()) => {
            match value.getattr("nbytes") {
                Ok(length) => length.extract(),
                Err(error) if error.is_instance_of::<PyAttributeError>(value.py()) => {
                    Err(buffer_error)
                }
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

pub(super) fn text_or_buffer(value: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    with_text_or_buffer(value, <[u8]>::to_vec)
}

/// Runs `consume` over the bytes of an ASCII `str` or a buffer-protocol
/// object, borrowing them when Python allows it.
pub(super) fn with_text_or_buffer<R>(
    value: &Bound<'_, PyAny>,
    consume: impl FnOnce(&[u8]) -> R,
) -> PyResult<R> {
    if let Ok(text) = value.cast::<PyString>() {
        let text = text.to_str()?;
        if !text.is_ascii() {
            return Err(PyValueError::new_err("HTTP str inputs must be ASCII"));
        }
        Ok(consume(text.as_bytes()))
    } else {
        with_buffer(value, consume)
    }
}

/// Writes `before`, the bytes of `value`, and `after` into one new `bytes`,
/// so the body is copied once. `framing` gets the body length.
pub(super) fn framed<B: AsRef<[u8]>, A: AsRef<[u8]>>(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
    framing: impl FnOnce(usize) -> Result<(B, A), core::LocalProtocolError>,
) -> PyResult<Py<PyBytes>> {
    with_buffer(value, |data| {
        let length = data.len();
        let (before, after) = framing(length).map_err(local_error)?;
        let (before, after) = (before.as_ref(), after.as_ref());
        let wire = PyBytes::new_with(py, before.len() + length + after.len(), |out| {
            let (head, rest) = out.split_at_mut(before.len());
            let (body, tail) = rest.split_at_mut(length);
            head.copy_from_slice(before);
            body.copy_from_slice(data);
            tail.copy_from_slice(after);
            Ok(())
        })?;
        Ok(wire.unbind())
    })?
}
