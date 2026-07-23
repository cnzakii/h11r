"""Serialize one client request and parse one complete response."""

from __future__ import annotations

import h11r


def main() -> None:
    client = h11r.Connection(h11r.Role.CLIENT)

    # h11r updates the client state and returns bytes for a transport to write.
    request_bytes = client.send_request(
        "GET",
        "/hello",
        [("Host", "example.test")],
    )
    request_bytes += client.end_of_message()

    print("client would send:")
    print(request_bytes.decode("ascii"), end="")

    # A real client would read these bytes from its socket or async stream.
    response_bytes = (
        b"HTTP/1.1 200 OK\r\n"
        b"Content-Type: text/plain; charset=utf-8\r\n"
        b"Content-Length: 17\r\n"
        b"\r\n"
        b"Hello from h11r!\n"
    )
    client.receive_data(response_bytes)

    status_code = None
    response_body = bytearray()

    while True:
        event = client.next_event()

        match event:
            case h11r.Response(status_code=code):
                status_code = code
            case h11r.Data(data=chunk):
                response_body.extend(chunk)
            case h11r.EndOfMessage():
                break

    print(f"client received {status_code} with {bytes(response_body)!r}")


if __name__ == "__main__":
    main()
