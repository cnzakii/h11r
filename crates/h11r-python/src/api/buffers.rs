//! Reading bytes from Python buffers and text.

use super::*;
use pyo3::buffer::{PyUntypedBuffer, ReadOnlyCell};
use pyo3::exceptions::{PyAttributeError, PyTypeError, PyValueError};
use pyo3::types::{PyAny, PyBytes, PyMemoryView, PyString};

pub(super) enum BufferBytes<'a> {
    Borrowed(&'a [u8]),
    Cells(&'a [ReadOnlyCell<u8>]),
}

/// Runs `consume` over a safe view of a buffer-protocol object.
///
/// Immutable `bytes` are borrowed directly. Contiguous, `u8`-compatible
/// buffers expose cells whose values must be read individually because another
/// thread may mutate them. Other buffers retain the previous copying behavior.
pub(super) fn with_buffer_bytes<R>(
    value: &Bound<'_, PyAny>,
    consume: impl FnOnce(BufferBytes<'_>) -> R,
) -> PyResult<R> {
    if let Ok(bytes) = value.cast::<PyBytes>() {
        return Ok(consume(BufferBytes::Borrowed(bytes.as_bytes())));
    }
    let buffer = PyUntypedBuffer::get(value)?;
    if let Ok(buffer) = buffer.as_typed::<u8>()
        && let Some(cells) = buffer.as_slice(value.py())
    {
        return Ok(consume(BufferBytes::Cells(cells)));
    }
    drop(buffer);
    let view = PyMemoryView::from(value)?;
    let bytes = view.call_method0("tobytes")?.cast_into::<PyBytes>()?;
    Ok(consume(BufferBytes::Borrowed(bytes.as_bytes())))
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

/// Runs `consume` over the bytes of a buffer-protocol object.
pub(super) fn with_buffer<R>(
    value: &Bound<'_, PyAny>,
    consume: impl FnOnce(&[u8]) -> R,
) -> PyResult<R> {
    with_buffer_bytes(value, |data| match data {
        BufferBytes::Borrowed(bytes) => consume(bytes),
        BufferBytes::Cells(cells) => {
            consume(&cells.iter().map(ReadOnlyCell::get).collect::<Vec<_>>())
        }
    })
}

/// Writes `before`, the bytes of `value`, and `after` into one new `bytes`,
/// so the body is copied once. `framing` gets the body length.
pub(super) fn framed<B: AsRef<[u8]>, A: AsRef<[u8]>>(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
    framing: impl FnOnce(usize) -> Result<(B, A), core::LocalProtocolError>,
) -> PyResult<Py<PyBytes>> {
    with_buffer_bytes(value, |data| {
        let length = match &data {
            BufferBytes::Borrowed(bytes) => bytes.len(),
            BufferBytes::Cells(cells) => cells.len(),
        };
        let (before, after) = framing(length).map_err(local_error)?;
        let (before, after) = (before.as_ref(), after.as_ref());
        let wire = PyBytes::new_with(py, before.len() + length + after.len(), |out| {
            let (head, rest) = out.split_at_mut(before.len());
            let (body, tail) = rest.split_at_mut(length);
            head.copy_from_slice(before);
            match data {
                BufferBytes::Borrowed(bytes) => body.copy_from_slice(bytes),
                BufferBytes::Cells(cells) => {
                    for (byte, cell) in body.iter_mut().zip(cells) {
                        *byte = cell.get();
                    }
                }
            }
            tail.copy_from_slice(after);
            Ok(())
        })?;
        Ok(wire.unbind())
    })?
}
