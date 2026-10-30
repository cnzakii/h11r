//! HTTP/1 wire parsing, message framing, buffering, and serialization.

mod buffer;
mod chunked;
mod fields;
mod head;
mod target;

pub(crate) use buffer::*;
pub(crate) use chunked::*;
pub(crate) use fields::*;
pub(crate) use head::*;
pub(crate) use target::*;

use crate::{LocalProtocolError, RemoteProtocolError};

pub(crate) fn local(message: impl Into<String>) -> LocalProtocolError {
    LocalProtocolError(message.into())
}

pub(crate) fn remote(message: impl Into<String>, status: Option<u16>) -> RemoteProtocolError {
    RemoteProtocolError {
        message: message.into(),
        suggested_status_code: status,
    }
}
