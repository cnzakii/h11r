from collections.abc import Iterable, Iterator, Sequence
from typing import (
    ClassVar,
    Protocol,
    SupportsIndex,
    TypeAlias,
    TypeVar,
    final,
    overload,
)

__version__: str

class _ByteSized(Protocol):
    @property
    def nbytes(self) -> int: ...

_Buffer: TypeAlias = bytes | bytearray | memoryview
_DataT = TypeVar("_DataT", bound=_Buffer | _ByteSized)
_HeaderInput: TypeAlias = Headers | Iterable[tuple[_Buffer | str, _Buffer | str]]
_T = TypeVar("_T")

@final
class Role:
    CLIENT: ClassVar[Role]
    SERVER: ClassVar[Role]

@final
class State:
    IDLE: ClassVar[State]
    SEND_RESPONSE: ClassVar[State]
    SEND_BODY: ClassVar[State]
    DONE: ClassVar[State]
    MIGHT_SWITCH_PROTOCOL: ClassVar[State]
    SWITCHED_PROTOCOL: ClassVar[State]
    MUST_CLOSE: ClassVar[State]
    CLOSED: ClassVar[State]
    ERROR: ClassVar[State]

@final
class ReceiveStatus:
    NEED_DATA: ClassVar[ReceiveStatus]
    PAUSED: ClassVar[ReceiveStatus]

class ProtocolError(Exception): ...
class LocalProtocolError(ProtocolError): ...

class RemoteProtocolError(ProtocolError):
    suggested_status_code: int | None

@final
class Headers(Sequence[tuple[bytes, bytes]]):
    def __new__(cls, fields: _HeaderInput | None = None) -> Headers: ...
    @overload
    def get(self, name: _Buffer | str) -> bytes | None: ...
    @overload
    def get(self, name: _Buffer | str, default: _T) -> bytes | _T: ...
    def get_all(self, name: _Buffer | str) -> tuple[bytes, ...]: ...
    def __len__(self) -> int: ...
    def index(
        self, pair: object, start: SupportsIndex = 0, stop: SupportsIndex = ..., /
    ) -> int: ...
    def count(self, pair: object, /) -> int: ...
    @overload
    def __getitem__(self, index: SupportsIndex) -> tuple[bytes, bytes]: ...
    @overload
    def __getitem__(self, index: slice) -> tuple[tuple[bytes, bytes], ...]: ...
    def __iter__(self) -> Iterator[tuple[bytes, bytes]]: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

@final
class Request:
    __match_args__: ClassVar[tuple[str, str, str]] = ("method", "target", "headers")
    def __new__(
        cls,
        method: _Buffer | str,
        target: _Buffer | str,
        headers: _HeaderInput,
        *,
        http_version: _Buffer | str | None = None,
    ) -> Request: ...
    @property
    def method(self) -> bytes: ...
    @property
    def target(self) -> bytes: ...
    @property
    def headers(self) -> Headers: ...
    @property
    def http_version(self) -> bytes: ...

@final
class InformationalResponse:
    __match_args__: ClassVar[tuple[str, str]] = ("status_code", "headers")
    def __new__(
        cls,
        status_code: int,
        headers: _HeaderInput | None = None,
        *,
        reason: _Buffer | str | None = None,
        http_version: _Buffer | str | None = None,
    ) -> InformationalResponse: ...
    @property
    def status_code(self) -> int: ...
    @property
    def reason(self) -> bytes: ...
    @property
    def headers(self) -> Headers: ...
    @property
    def http_version(self) -> bytes: ...

@final
class Response:
    __match_args__: ClassVar[tuple[str, str]] = ("status_code", "headers")
    def __new__(
        cls,
        status_code: int,
        headers: _HeaderInput | None = None,
        *,
        reason: _Buffer | str | None = None,
        http_version: _Buffer | str | None = None,
    ) -> Response: ...
    @property
    def status_code(self) -> int: ...
    @property
    def reason(self) -> bytes: ...
    @property
    def headers(self) -> Headers: ...
    @property
    def http_version(self) -> bytes: ...

@final
class Data:
    __match_args__: ClassVar[tuple[str]] = ("data",)
    def __new__(
        cls, data: _Buffer, *, chunk_start: bool = False, chunk_end: bool = False
    ) -> Data: ...
    @property
    def data(self) -> bytes: ...
    @property
    def chunk_start(self) -> bool: ...
    @property
    def chunk_end(self) -> bool: ...

@final
class EndOfMessage:
    __match_args__: ClassVar[tuple[str]] = ("trailers",)
    def __new__(cls, trailers: _HeaderInput | None = None) -> EndOfMessage: ...
    @property
    def trailers(self) -> Headers: ...

@final
class ConnectionClosed:
    def __init__(self) -> None: ...

_Event: TypeAlias = (
    Request | InformationalResponse | Response | Data | EndOfMessage | ConnectionClosed
)

@final
class Connection:
    def __new__(
        cls,
        role: Role,
        *,
        max_head_bytes: int = 65536,
        max_header_count: int = 100,
        lenient: bool = False,
    ) -> Connection: ...
    def receive_data(self, data: _Buffer) -> None: ...
    def next_event(self) -> _Event | ReceiveStatus: ...
    def send_request(
        self,
        method: _Buffer | str,
        target: _Buffer | str,
        headers: _HeaderInput,
        *,
        body: _Buffer | None = None,
        http_version: _Buffer | str | None = None,
    ) -> bytes: ...
    def send_informational_response(
        self,
        status_code: int,
        headers: _HeaderInput | None = None,
        *,
        reason: _Buffer | str | None = None,
        http_version: _Buffer | str | None = None,
    ) -> bytes: ...
    def send_response(
        self,
        status_code: int,
        headers: _HeaderInput | None = None,
        *,
        body: _Buffer | None = None,
        reason: _Buffer | str | None = None,
        http_version: _Buffer | str | None = None,
    ) -> bytes: ...
    def send_data(self, data: _Buffer) -> bytes: ...
    def send_data_parts(self, data: _DataT) -> tuple[bytes, _DataT, bytes]: ...
    def end_of_message(self, trailers: _HeaderInput | None = None) -> bytes: ...
    def start_next_cycle(self) -> None: ...
    def close(self) -> None: ...
    @property
    def local_state(self) -> State: ...
    @property
    def peer_state(self) -> State: ...
    @property
    def peer_http_version(self) -> bytes | None: ...
    @property
    def keep_alive(self) -> bool: ...
    @property
    def client_is_waiting_for_100_continue(self) -> bool: ...
    @property
    def trailing_data(self) -> tuple[bytes, bool]: ...
    @property
    def buffered_nbytes(self) -> int: ...

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
