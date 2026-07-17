from __future__ import annotations

import http11
import http11._core


def test_public_import_surface() -> None:
    assert http11.__all__ == [
        "Connection",
        "ConnectionClosed",
        "Data",
        "EndOfMessage",
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
    assert http11.__version__
    assert http11._core.__version__ == http11.__version__
    assert http11.Connection.__module__ == "http11"
    for name in http11.__all__:
        assert hasattr(http11._core, name)
