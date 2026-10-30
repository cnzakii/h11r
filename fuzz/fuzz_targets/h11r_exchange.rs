#![no_main]

mod common;

use common::{MAX_EVENTS, headers};
use h11r::{
    Connection, Event, Headers, Limits, NextEvent, Response, Role, State, StatusCode, Version,
};
use libfuzzer_sys::fuzz_target;

const SEED: &[u8] = b"GET / HTTP/1.1\r\nHost: example.test\r\n\r\n\
POST / HTTP/1.1\r\nHost: example.test\r\nContent-Length: 4\r\n\r\nbody\r\n\
CONNECT example.test:443 HTTP/1.1\r\nHost: example.test:443\r\n\r\n";

/// Whether a response head carries a `close` connection option.
fn announces_close(head: &[u8]) -> bool {
    let end = head
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap_or(head.len());
    head[..end].split(|byte| *byte == b'\n').any(|line| {
        let line = line.to_ascii_lowercase();
        line.starts_with(b"connection:")
            && line[11..]
                .split(|byte| *byte == b',')
                .any(|option| option.trim_ascii() == b"close")
    })
}

/// Sends one response chosen by `choice`. Rejected responses are legitimate
/// outcomes; an accepted head must announce closure exactly when the
/// connection will not persist.
fn respond(server: &mut Connection, choice: u8) {
    let status = [200, 204, 205, 304, 400, 403, 408, 500][usize::from(choice & 0x07)];
    let mut fields = Headers::new();
    if choice & 0x08 != 0 {
        fields = headers([(b"Connection", b"close")]);
    }
    let response = Response {
        status: StatusCode::from_u16(status).unwrap(),
        reason: b"R".to_vec(),
        headers: fields,
        http_version: if choice & 0x10 == 0 {
            Version::Http11
        } else {
            Version::Http10
        },
    };
    let head = if choice & 0x20 == 0 {
        server.send_response_with_body(response, b"x")
    } else {
        server.send_response(response)
    };
    let Ok(head) = head else {
        return;
    };
    if server.local_state() != State::SwitchedProtocol {
        assert_eq!(announces_close(&head), !server.keep_alive(), "{head:?}");
    }
    if server.local_state() == State::SendBody {
        let _ = server.send_data(b"x");
        let _ = server.end_of_message(Headers::new());
    }
}

fuzz_target!(|input: &[u8]| {
    let Some((&control, rest)) = input.split_first() else {
        return;
    };
    let (choices, wire) = rest.split_at(rest.len().min(8));
    let wire = if wire.is_empty() { SEED } else { wire };
    let mut choices = choices.iter().copied().cycle();
    let mut next_choice = || choices.next().unwrap_or(control);
    let limits = if control & 0x80 == 0 {
        Limits::default()
    } else {
        Limits::new(512, 8).unwrap()
    };
    let mut server = Connection::new(Role::Server, limits);
    server.receive_data(wire).unwrap();
    // Without EOF the client never closes, so only the server's choices and
    // the request can end keep-alive.
    for _ in 0..MAX_EVENTS {
        let responding = matches!(server.local_state(), State::Idle | State::SendResponse);
        match server.next_event() {
            Err(_) | Ok(NextEvent::Event(Event::ConnectionClosed)) => return,
            Ok(NextEvent::Event(Event::Request(_))) if responding => {
                // Answer some requests before reading their content.
                let choice = next_choice();
                if choice & 0x40 != 0 {
                    respond(&mut server, choice);
                }
            }
            Ok(NextEvent::Event(Event::EndOfMessage(_))) if responding => {
                respond(&mut server, next_choice());
            }
            Ok(NextEvent::Event(_)) => {}
            Ok(NextEvent::NeedData | NextEvent::Paused) => {
                if server.local_state() == State::Idle && control & 0x40 != 0 {
                    // A response before any request, such as a 408 timeout.
                    respond(&mut server, next_choice());
                } else if server.start_next_cycle().is_err() {
                    return;
                }
            }
        }
    }
});
