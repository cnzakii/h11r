//! CPython buffer exporters for collected bodies and receive leases.
//!
//! Safety invariants:
//! - an exporter owns the `Vec<u8>` allocation exposed by every view;
//! - collected storage is frozen and never resized after its first export;
//! - receive scratch storage is never resized or returned to its connection
//!   while its active export count is nonzero;
//! - `view.obj` owns the exporter until CPython releases that view; and
//! - aborting an escaped receive view defers connection release until the final
//!   view is released.

#![allow(unsafe_code)]

use crate::api::{PyConnection, local_error, nonnegative_platform_length, receive_unusable_error};
use pyo3::exceptions::{PyBufferError, PyRuntimeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyAny;
use std::ffi::{c_int, c_void};
use std::ptr;
use std::sync::{Mutex, MutexGuard};

/// Private immutable storage exported through `CollectedBody.data`.
#[pyclass(name = "_BodyBuffer", module = "h11r._core", frozen)]
#[derive(Debug)]
pub(super) struct PyBodyBuffer {
    pub(super) data: Vec<u8>,
}

#[pymethods]
impl PyBodyBuffer {
    unsafe fn __getbuffer__(
        slf: Bound<'_, Self>,
        view: *mut ffi::Py_buffer,
        flags: c_int,
    ) -> PyResult<()> {
        if view.is_null() {
            return Err(PyBufferError::new_err("buffer view is null"));
        }
        if (flags & ffi::PyBUF_WRITABLE) == ffi::PyBUF_WRITABLE {
            return Err(PyBufferError::new_err(
                "collected body storage is read-only",
            ));
        }

        let data = slf.get().data.as_ptr();
        let length = slf.get().data.len() as isize;

        // SAFETY: `view` was checked for null. `PyBodyBuffer` is frozen, so
        // its vector is never resized, and `view.obj` owns `slf` until CPython
        // releases the export.
        unsafe {
            (*view).buf = data.cast_mut().cast::<c_void>();
            (*view).obj = slf.into_ptr();
            (*view).len = length;
            (*view).readonly = 1;
            (*view).itemsize = 1;
            (*view).format = if (flags & ffi::PyBUF_FORMAT) == ffi::PyBUF_FORMAT {
                c"B".as_ptr().cast_mut()
            } else {
                ptr::null_mut()
            };
            (*view).ndim = 1;
            (*view).shape = if (flags & ffi::PyBUF_ND) == ffi::PyBUF_ND {
                &mut (*view).len
            } else {
                ptr::null_mut()
            };
            (*view).strides = if (flags & ffi::PyBUF_STRIDES) == ffi::PyBUF_STRIDES {
                &mut (*view).itemsize
            } else {
                ptr::null_mut()
            };
            (*view).suboffsets = ptr::null_mut();
            (*view).internal = ptr::null_mut();
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReceiveBufferLifecycle {
    Reserved,
    Acquired,
    AbortPending,
    Committed,
    Aborted,
}

#[derive(Debug)]
pub(super) struct ReceiveBufferState {
    pub(super) scratch: Option<Vec<u8>>,
    pub(super) exports: usize,
    pub(super) lifecycle: ReceiveBufferLifecycle,
}

/// Reusable writable storage reserved from a `Connection`.
///
/// Obtain a lease with `Connection.receive_buffer()`. Call `acquire()` before
/// passing it to `socket.recv_into()` or returning it from
/// `asyncio.BufferedProtocol.get_buffer()`, then call `commit()` with the
/// number of initialized bytes. A zero-byte commit records transport EOF.
///
/// An unfinished context manager aborts automatically. If a `memoryview` or
/// another buffer export escapes, `commit()` raises `BufferError`; `abort()`
/// waits until the final export is released before releasing the connection.
///
/// `ReceiveBuffer` cannot be constructed directly.
#[pyclass(name = "ReceiveBuffer", module = "h11r", frozen)]
#[derive(Debug)]
pub(super) struct PyReceiveBuffer {
    pub(super) connection: Py<PyConnection>,
    pub(super) size: usize,
    pub(super) state: Mutex<ReceiveBufferState>,
}

impl PyReceiveBuffer {
    fn lock(&self) -> MutexGuard<'_, ReceiveBufferState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn acquire_inner(&self) -> PyResult<()> {
        let mut state = self.lock();
        if state.lifecycle != ReceiveBufferLifecycle::Reserved {
            return Err(PyRuntimeError::new_err(
                "receive buffer can only be acquired once",
            ));
        }
        state.lifecycle = ReceiveBufferLifecycle::Acquired;
        Ok(())
    }

    fn abort_inner(&self) {
        let mut state = self.lock();
        match state.lifecycle {
            ReceiveBufferLifecycle::Committed | ReceiveBufferLifecycle::Aborted => return,
            ReceiveBufferLifecycle::AbortPending => return,
            ReceiveBufferLifecycle::Reserved | ReceiveBufferLifecycle::Acquired => {}
        }
        if state.exports != 0 {
            state.lifecycle = ReceiveBufferLifecycle::AbortPending;
            return;
        }
        let scratch = state
            .scratch
            .take()
            .expect("unfinished receive buffer owns its scratch storage");
        state.lifecycle = ReceiveBufferLifecycle::Aborted;
        self.connection.get().return_receive_scratch(scratch);
    }

    fn finish_deferred_abort(&self, state: &mut ReceiveBufferState) {
        let scratch = state
            .scratch
            .take()
            .expect("pending receive buffer owns its scratch storage");
        state.lifecycle = ReceiveBufferLifecycle::Aborted;
        self.connection.get().return_receive_scratch(scratch);
    }
}

impl Drop for PyReceiveBuffer {
    fn drop(&mut self) {
        self.abort_inner();
    }
}

#[pymethods]
impl PyReceiveBuffer {
    /// Activate writable buffer exports and return this lease.
    fn acquire(slf: Py<Self>) -> PyResult<Py<Self>> {
        slf.get().acquire_inner()?;
        Ok(slf)
    }

    /// Commit the initialized prefix to the connection.
    ///
    /// Args:
    ///     nbytes (int): Initialized bytes, from zero through `len(buffer)`.
    ///
    /// Raises:
    ///     ValueError: If `nbytes` is negative or exceeds the buffer length.
    ///     OverflowError: If `nbytes` does not fit in a platform length.
    ///     BufferError: If a buffer export is still active.
    ///     RuntimeError: If the lease is not acquired or is already finished.
    ///     LocalProtocolError: If non-empty data follows EOF.
    fn commit(&self, nbytes: &Bound<'_, PyAny>) -> PyResult<()> {
        let nbytes = nonnegative_platform_length(nbytes, "nbytes")?;
        if nbytes > self.size {
            return Err(PyValueError::new_err(
                "nbytes exceeds the receive buffer length",
            ));
        }

        let mut lease = self.lock();
        if lease.lifecycle != ReceiveBufferLifecycle::Acquired {
            return Err(PyRuntimeError::new_err(
                "receive buffer must be acquired exactly once before commit",
            ));
        }
        if lease.exports != 0 {
            return Err(PyBufferError::new_err(
                "cannot commit while the receive buffer is exported",
            ));
        }

        let mut connection = self.connection.get().lock();
        if connection.receive_unusable {
            return Err(receive_unusable_error());
        }
        let scratch = lease
            .scratch
            .as_ref()
            .expect("active receive buffer owns its scratch storage");
        connection
            .core
            .receive_data(&scratch[..nbytes])
            .map_err(local_error)?;

        let scratch = lease
            .scratch
            .take()
            .expect("committed receive buffer owns its scratch storage");
        lease.lifecycle = ReceiveBufferLifecycle::Committed;
        connection.receive_scratch = Some(scratch);
        connection.receive_reserved = false;
        Ok(())
    }

    /// Discard uncommitted input and release the connection when safe.
    fn abort(&self) {
        self.abort_inner();
    }

    fn __enter__(slf: Py<Self>) -> PyResult<Py<Self>> {
        slf.get().acquire_inner()?;
        Ok(slf)
    }

    fn __exit__(
        &self,
        _exception_type: &Bound<'_, PyAny>,
        _exception: &Bound<'_, PyAny>,
        _traceback: &Bound<'_, PyAny>,
    ) {
        self.abort_inner();
    }

    fn __len__(&self) -> usize {
        self.size
    }

    unsafe fn __getbuffer__(
        slf: Bound<'_, Self>,
        view: *mut ffi::Py_buffer,
        flags: c_int,
    ) -> PyResult<()> {
        if view.is_null() {
            return Err(PyBufferError::new_err("buffer view is null"));
        }

        let mut state = slf.get().lock();
        if state.lifecycle != ReceiveBufferLifecycle::Acquired {
            return Err(PyRuntimeError::new_err(
                "receive buffer exports require an active acquisition",
            ));
        }
        let scratch = state
            .scratch
            .as_mut()
            .expect("acquired receive buffer owns its scratch storage");
        let data = scratch.as_mut_ptr();
        let length = scratch.len() as isize;
        state.exports += 1;
        drop(state);

        // SAFETY: `view` was checked for null. The vector allocation remains
        // fixed while `exports` is nonzero, and `view.obj` owns `slf` until
        // CPython calls `__releasebuffer__`.
        unsafe {
            (*view).buf = data.cast::<c_void>();
            (*view).obj = slf.into_ptr();
            (*view).len = length;
            (*view).readonly = 0;
            (*view).itemsize = 1;
            (*view).format = if (flags & ffi::PyBUF_FORMAT) == ffi::PyBUF_FORMAT {
                c"B".as_ptr().cast_mut()
            } else {
                ptr::null_mut()
            };
            (*view).ndim = 1;
            (*view).shape = if (flags & ffi::PyBUF_ND) == ffi::PyBUF_ND {
                &mut (*view).len
            } else {
                ptr::null_mut()
            };
            (*view).strides = if (flags & ffi::PyBUF_STRIDES) == ffi::PyBUF_STRIDES {
                &mut (*view).itemsize
            } else {
                ptr::null_mut()
            };
            (*view).suboffsets = ptr::null_mut();
            (*view).internal = ptr::null_mut();
        }
        Ok(())
    }

    unsafe fn __releasebuffer__(&self, _view: *mut ffi::Py_buffer) {
        let mut state = self.lock();
        debug_assert!(state.exports > 0);
        state.exports = state.exports.saturating_sub(1);
        if state.exports == 0 && state.lifecycle == ReceiveBufferLifecycle::AbortPending {
            self.finish_deferred_abort(&mut state);
        }
    }
}
