"""Sans-I/O HTTP/1.1 library."""

from collections.abc import Sequence as _Sequence

from ._core import (
    Connection,
    ConnectionClosed,
    Data,
    EndOfMessage,
    Headers,
    InformationalResponse,
    LocalProtocolError,
    ProtocolError,
    ReceiveStatus,
    RemoteProtocolError,
    Request,
    Response,
    Role,
    State,
    __version__,
)

_Sequence.register(Headers)

__all__ = [
    "Connection",
    "ConnectionClosed",
    "Data",
    "EndOfMessage",
    "Headers",
    "InformationalResponse",
    "LocalProtocolError",
    "ProtocolError",
    "ReceiveStatus",
    "RemoteProtocolError",
    "Request",
    "Response",
    "Role",
    "State",
    "__version__",
]
