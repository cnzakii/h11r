//! Python bindings for the HTTP/1.1 library.

use h11r as core;
use h11r::StatusCode;
use pyo3::PyTypeInfo;
use pyo3::exceptions::{PyException, PyOverflowError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyAny, PyModule};

mod buffers;
mod connection;
mod events;
mod headers;

use buffers::*;
use connection::*;
use events::*;
use headers::*;

pyo3::create_exception!(
    h11r,
    ProtocolError,
    PyException,
    "Base HTTP/1 protocol error."
);
pyo3::create_exception!(
    h11r,
    LocalProtocolError,
    ProtocolError,
    "The caller attempted an invalid local HTTP operation."
);
pyo3::create_exception!(
    h11r,
    RemoteProtocolError,
    ProtocolError,
    "The peer sent invalid HTTP or exceeded an inbound limit.\n\n\
     Attributes:\n    \
         suggested_status_code (int | None): A suitable HTTP response status, \
         when one is known."
);

/// An HTTP actor role.
///
/// Attributes:
///     CLIENT (Role): The actor that sends requests and receives responses.
///     SERVER (Role): The actor that receives requests and sends responses.
#[pyclass(
    name = "Role",
    module = "h11r",
    eq,
    hash,
    frozen,
    rename_all = "SCREAMING_SNAKE_CASE",
    from_py_object
)]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum PyRole {
    Client,
    Server,
}

impl From<PyRole> for core::Role {
    fn from(value: PyRole) -> Self {
        match value {
            PyRole::Client => Self::Client,
            PyRole::Server => Self::Server,
        }
    }
}

/// An observable HTTP/1 actor lifecycle state.
///
/// Attributes:
///     IDLE (State): No message has started.
///     SEND_RESPONSE (State): The server can send informational or final responses.
///     SEND_BODY (State): The actor can send or receive body data and message end.
///     DONE (State): The actor completed its message for the current cycle.
///     MIGHT_SWITCH_PROTOCOL (State): The request ended while a switch decision is pending.
///     SWITCHED_PROTOCOL (State): HTTP processing ended after a successful switch.
///     MUST_CLOSE (State): The actor completed its message but reuse is forbidden.
///     CLOSED (State): The actor closed its transport side.
///     ERROR (State): A protocol error permanently poisoned the actor.
#[pyclass(
    name = "State",
    module = "h11r",
    eq,
    hash,
    frozen,
    rename_all = "SCREAMING_SNAKE_CASE",
    skip_from_py_object
)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum PyState {
    Idle,
    SendResponse,
    SendBody,
    Done,
    MightSwitchProtocol,
    SwitchedProtocol,
    MustClose,
    Closed,
    Error,
}

impl From<core::State> for PyState {
    fn from(value: core::State) -> Self {
        match value {
            core::State::Idle => Self::Idle,
            core::State::SendResponse => Self::SendResponse,
            core::State::SendBody => Self::SendBody,
            core::State::Done => Self::Done,
            core::State::MightSwitchProtocol => Self::MightSwitchProtocol,
            core::State::SwitchedProtocol => Self::SwitchedProtocol,
            core::State::MustClose => Self::MustClose,
            core::State::Closed => Self::Closed,
            core::State::Error => Self::Error,
        }
    }
}

/// A non-event result returned by `Connection.next_event()`.
///
/// Attributes:
///     NEED_DATA (ReceiveStatus): Supply more peer bytes before polling again.
///     PAUSED (ReceiveStatus): Complete the active cycle or hand off a switched protocol before
///         polling HTTP again.
#[pyclass(
    name = "ReceiveStatus",
    module = "h11r",
    eq,
    hash,
    frozen,
    rename_all = "SCREAMING_SNAKE_CASE",
    skip_from_py_object
)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum PyReceiveStatus {
    NeedData,
    Paused,
}

fn status(value: &Bound<'_, PyAny>) -> PyResult<StatusCode> {
    // An int beyond `u16` is an invalid status like any other out of range.
    let code = match value.extract::<u16>() {
        Ok(code) => code,
        Err(error) if error.is_instance_of::<PyOverflowError>(value.py()) => 0,
        Err(error) => return Err(error),
    };
    StatusCode::from_u16(code).map_err(|error| PyValueError::new_err(error.to_string()))
}

fn local_error(error: core::LocalProtocolError) -> PyErr {
    LocalProtocolError::new_err(error.to_string())
}

fn remote_error(py: Python<'_>, error: core::RemoteProtocolError) -> PyErr {
    let status = error.suggested_status_code();
    let exception = RemoteProtocolError::new_err(error.to_string());
    if let Err(set_error) = exception.value(py).setattr("suggested_status_code", status) {
        return set_error;
    }
    exception
}

/// Returns a class attribute, looking it up once per process.
fn cached_member<T: PyTypeInfo>(
    py: Python<'_>,
    cell: &'static PyOnceLock<Py<PyAny>>,
    name: &str,
) -> PyResult<Py<PyAny>> {
    cell.get_or_try_init(py, || {
        Ok::<_, PyErr>(py.get_type::<T>().getattr(name)?.unbind())
    })
    .map(|member| member.clone_ref(py))
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyRole>()?;
    module.add_class::<PyState>()?;
    module.add_class::<PyReceiveStatus>()?;
    module.add_class::<PyHeaders>()?;
    module.add_class::<PyRequest>()?;
    module.add_class::<PyInformationalResponse>()?;
    module.add_class::<PyResponse>()?;
    module.add_class::<PyData>()?;
    module.add_class::<PyEndOfMessage>()?;
    module.add_class::<PyConnectionClosed>()?;
    module.add_class::<PyConnection>()?;
    module.add("ProtocolError", module.py().get_type::<ProtocolError>())?;
    module.add(
        "LocalProtocolError",
        module.py().get_type::<LocalProtocolError>(),
    )?;
    let remote = module.py().get_type::<RemoteProtocolError>();
    // Instances raised by the connection set their own value.
    remote.setattr("suggested_status_code", module.py().None())?;
    module.add("RemoteProtocolError", remote)?;
    Ok(())
}
