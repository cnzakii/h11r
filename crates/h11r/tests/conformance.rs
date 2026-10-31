//! RFC 9112 and RFC 9110 conformance by clause, through public APIs only.
//! Each receive matrix runs with a complete buffer and with polling after every byte.
//! An error means rejection, not permission to emit an invalid message; the caller
//! owns writing the suggested error response and actually closing its transport.

use h11r::{
    Connection, Event, Headers, Limits, Method, NextEvent, RemoteProtocolError, Request, Response,
    Role, State, StatusCode, Version,
};

fn headers<const N: usize>(fields: [(&[u8], &[u8]); N]) -> Headers {
    Headers::from(fields)
}

fn request(method: Method, fields: Headers) -> Request {
    Request {
        target: if method == Method::CONNECT {
            b"example.test:443".to_vec()
        } else {
            b"/".to_vec()
        },
        method,
        headers: fields,
        http_version: Version::Http11,
    }
}

fn response(status: u16, fields: Headers) -> Response {
    Response {
        status: StatusCode::from_u16(status).unwrap(),
        reason: Vec::new(),
        headers: fields,
        http_version: Version::Http11,
    }
}

fn event(connection: &mut Connection) -> Event<'_> {
    match connection.next_event().expect("valid peer input") {
        NextEvent::Event(event) => event,
        other => panic!("expected event, got {other:?}"),
    }
}

fn client(method: Method) -> Connection {
    let mut client = Connection::new(Role::Client, Limits::default());
    client
        .send_request(request(method, headers([(b"Host", b"example.test")])))
        .unwrap();
    client.end_of_message(Headers::new()).unwrap();
    client
}

fn server(method: &str) -> Connection {
    let mut server = Connection::new(Role::Server, Limits::default());
    let target = if method == "CONNECT" {
        "example.test:443"
    } else {
        "/"
    };
    server
        .receive_data(
            format!("{method} {target} HTTP/1.1\r\nHost: example.test\r\n\r\n").as_bytes(),
        )
        .unwrap();
    assert!(matches!(event(&mut server), Event::Request(_)));
    assert!(matches!(event(&mut server), Event::EndOfMessage(_)));
    server
}

#[derive(Debug, Default)]
struct Received {
    requests: Vec<Request>,
    responses: Vec<Response>,
    informational: Vec<Response>,
    body: Vec<u8>,
    ends: Vec<Headers>,
}

fn drain(connection: &mut Connection, received: &mut Received) -> Result<(), RemoteProtocolError> {
    loop {
        match connection.next_event()? {
            NextEvent::Event(Event::Request(head)) => received.requests.push(head),
            NextEvent::Event(Event::Response(head)) => received.responses.push(head),
            NextEvent::Event(Event::InformationalResponse(head)) => {
                received.informational.push(head)
            }
            NextEvent::Event(Event::Data(data)) => received.body.extend_from_slice(data.data),
            NextEvent::Event(Event::EndOfMessage(end)) => received.ends.push(end.trailers),
            NextEvent::Event(Event::ConnectionClosed) | NextEvent::NeedData | NextEvent::Paused => {
                return Ok(());
            }
        }
    }
}

fn feed(
    connection: &mut Connection,
    wire: &[u8],
    size: usize,
) -> Result<Received, RemoteProtocolError> {
    let mut received = Received::default();
    for part in wire.chunks(size) {
        connection.receive_data(part).unwrap();
        drain(connection, &mut received)?;
    }
    Ok(received)
}

fn peer(role: Role) -> Connection {
    match role {
        Role::Server => Connection::new(role, Limits::default()),
        Role::Client => client(Method::GET),
    }
}

fn message(role: Role, fields: &[u8], body: &[u8]) -> Vec<u8> {
    let start = match role {
        Role::Server => b"POST / HTTP/1.1\r\nHost: example.test\r\n".as_slice(),
        Role::Client => b"HTTP/1.1 200 OK\r\n".as_slice(),
    };
    [start, fields, b"\r\n", body].concat()
}

fn rejected(role: Role, wire: &[u8], status: u16) {
    for size in [1, wire.len()] {
        let mut connection = peer(role);
        let error = feed(&mut connection, wire, size).expect_err("malformed message accepted");
        assert_eq!(
            error.suggested_status_code(),
            (role == Role::Server).then_some(status)
        );
        assert_eq!(connection.peer_state(), State::Error);
        assert!(!connection.keep_alive());
        assert!(connection.start_next_cycle().is_err());
    }
}

// RFC 9112 §2.2: parse octets, preserving non-UTF-8 field/content bytes without reinterpretation.
#[test]
fn octets_are_not_unicode() {
    for role in [Role::Server, Role::Client] {
        let wire = message(
            role,
            b"X-Opaque: \xff\xc0\x80\r\nContent-Length: 6\r\n",
            b"\0\r\n\xff\xc0\x80",
        );
        for size in [1, wire.len()] {
            let got = feed(&mut peer(role), &wire, size).unwrap();
            let fields = if role == Role::Server {
                &got.requests[0].headers
            } else {
                &got.responses[0].headers
            };
            assert_eq!(
                fields.get_all(b"x-opaque").next(),
                Some(b"\xff\xc0\x80".as_slice())
            );
            assert_eq!(got.body, b"\0\r\n\xff\xc0\x80");
            assert_eq!(got.ends.len(), 1);
        }
    }
}

// RFC 9112 §2.2: bare CR outside content must be rejected or replaced with SP (strict choice).
#[test]
fn bare_cr_is_rejected_from_peer_elements() {
    for role in [Role::Server, Role::Client] {
        rejected(
            role,
            &message(role, b"X: a\rb\r\nContent-Length: 0\r\n", b""),
            400,
        );
    }
    rejected(
        Role::Server,
        b"GET /bad\rpath HTTP/1.1\r\nHost: x\r\n\r\n",
        400,
    );
    rejected(
        Role::Client,
        b"HTTP/1.1 200 O\rK\r\nContent-Length: 0\r\n\r\n",
        400,
    );
}

// RFC 9112 §2.2: send no extra CRLF; any body CRLF counts toward Content-Length.
#[test]
fn body_crlf_is_counted_and_no_extra_crlf_is_sent() {
    let mut client = Connection::new(Role::Client, Limits::default());
    assert_eq!(
        client
            .send_request_with_body(request(Method::POST, headers([(b"Host", b"x")])), b"x\r\n")
            .unwrap(),
        b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\n\r\nx\r\n"
    );
}

// RFC 9112 §2.2: reject whitespace-preceded lines between the start-line and first field.
#[test]
fn whitespace_before_first_field_is_rejected() {
    for wire in [
        b"GET / HTTP/1.1\r\n Host: hidden\r\nHost: x\r\n\r\n".as_slice(),
        b"GET / HTTP/1.1\r\n\tX: hidden\r\nHost: x\r\n\r\n",
    ] {
        rejected(Role::Server, wire, 400);
    }
    rejected(
        Role::Client,
        b"HTTP/1.1 200 OK\r\n X: hidden\r\nContent-Length: 0\r\n\r\n",
        400,
    );
}

// RFC 9112 §§2.3, 3, 4: HTTP is case-sensitive; optional lenient start-line whitespace is declined.
#[test]
fn version_and_start_line_syntax() {
    for wire in [
        b"GET / http/1.1\r\nHost: x\r\n\r\n".as_slice(),
        b"GET\t/ HTTP/1.1\r\nHost: x\r\n\r\n",
    ] {
        rejected(Role::Server, wire, 400);
    }
    for wire in [
        b"http/1.1 200 OK\r\n\r\n".as_slice(),
        b"HTTP/1.1\t200 OK\r\n\r\n",
        b"HTTP/1.1 20 OK\r\n\r\n",
        b"HTTP/1.1 2000 OK\r\n\r\n",
    ] {
        rejected(Role::Client, wire, 400);
    }
}

// RFC 9112 §3: senders and recipients should support request-lines of at least 8000 octets.
#[test]
fn eight_thousand_octet_request_line() {
    let mut req = request(Method::GET, headers([(b"Host", b"x")]));
    req.target = [b"/".as_slice(), &vec![b'a'; 7986]].concat();
    let wire = Connection::new(Role::Client, Limits::default())
        .send_request(req.clone())
        .unwrap();
    assert_eq!(wire.iter().position(|b| *b == b'\r'), Some(8000));
    for size in [1, wire.len()] {
        let got = feed(&mut peer(Role::Server), &wire, size).unwrap();
        assert_eq!(got.requests, [req.clone()]);
    }
}

// RFC 9112 §3.1: methods are case-sensitive tokens, including extension methods.
#[test]
fn methods_are_case_sensitive_tokens() {
    for name in [b"head".as_slice(), b"connect", b"X!#$%&'*+-.^_`|~"] {
        let mut client = Connection::new(Role::Client, Limits::default());
        let wire = client
            .send_request_with_body(
                request(
                    Method::from_bytes(name).unwrap(),
                    headers([(b"Host", b"x")]),
                ),
                b"",
            )
            .unwrap();
        for size in [1, wire.len()] {
            assert_eq!(
                feed(&mut peer(Role::Server), &wire, size).unwrap().requests[0]
                    .method
                    .as_bytes(),
                name
            );
        }
        let got = feed(
            &mut client,
            b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\nx",
            1,
        )
        .unwrap();
        assert_eq!(got.body, b"x");
    }
    for name in [b"".as_slice(), b"bad method", b"bad\0", b"\xff"] {
        assert!(Method::from_bytes(name).is_err());
    }
}

// RFC 9112 §3.2: invalid request-lines must not be silently autocorrected and processed.
#[test]
fn invalid_request_targets_are_not_processed() {
    for target in [
        b"/bad target".as_slice(),
        b"/bad\tpath",
        b"/bad\x7f",
        b"/bad\xff",
    ] {
        rejected(
            Role::Server,
            &[b"GET ", target, b" HTTP/1.1\r\nHost: x\r\n\r\n"].concat(),
            400,
        );
    }
}

// RFC 9112 §§3.2, 3.2.2: emit exactly one Host even for absolute-form; reject absent/duplicate Host.
#[test]
fn host_required_on_outbound_and_absolute_requests() {
    for target in [b"/".as_slice(), b"http://example.test/path"] {
        for fields in [Headers::new(), headers([(b"Host", b"x"), (b"hOsT", b"x")])] {
            let mut req = request(Method::GET, fields);
            req.target = target.to_vec();
            assert!(
                Connection::new(Role::Client, Limits::default())
                    .send_request(req)
                    .is_err()
            );
        }
        rejected(
            Role::Server,
            &[b"GET ", target, b" HTTP/1.1\r\n\r\n"].concat(),
            400,
        );
        rejected(
            Role::Server,
            &[b"GET ", target, b" HTTP/1.1\r\nHost: x\r\nhOsT: x\r\n\r\n"].concat(),
            400,
        );
    }
}

// RFC 9112 §4: the empty reason still needs its preceding SP; reason text has no framing semantics.
#[test]
fn empty_and_misleading_reason_phrases() {
    assert_eq!(
        server("GET")
            .send_response(response(204, Headers::new()))
            .unwrap(),
        b"HTTP/1.1 204 \r\n\r\n"
    );
    for wire in [
        b"HTTP/1.1 204 Internal Server Error\r\n\r\n".as_slice(),
        b"HTTP/1.1 204 \r\n\r\n",
    ] {
        for size in [1, wire.len()] {
            let got = feed(&mut client(Method::GET), wire, size).unwrap();
            assert_eq!(got.responses[0].status.as_u16(), 204);
            assert!(got.body.is_empty());
            assert_eq!(got.ends.len(), 1);
        }
    }
}

// RFC 9112 §5.1: whitespace before a request field colon requires 400, including in trailers.
#[test]
fn whitespace_before_colon() {
    for name in [b"Host ".as_slice(), b"Host\t"] {
        rejected(
            Role::Server,
            &[b"GET / HTTP/1.1\r\n", name, b": x\r\n\r\n"].concat(),
            400,
        );
    }
    for role in [Role::Server, Role::Client] {
        rejected(
            role,
            &message(
                role,
                b"Transfer-Encoding: chunked\r\n",
                b"0\r\nX : y\r\n\r\n",
            ),
            400,
        );
    }
}

// RFC 9112 §5.2: neither sender may generate obs-fold in headers or trailers.
#[test]
fn outbound_obs_fold_is_forbidden() {
    for value in [b"a\r\n b".as_slice(), b"a\r\n\tb"] {
        let mut server = server("GET");
        assert!(
            server
                .send_response(response(200, headers([(b"X", value)])))
                .is_err()
        );
        server
            .send_response(response(200, headers([(b"Transfer-Encoding", b"chunked")])))
            .unwrap();
        assert!(server.end_of_message(headers([(b"X", value)])).is_err());
    }
}

// RFC 9112 §6.1: senders must not apply chunked twice or leave requests without final chunked.
#[test]
fn outbound_transfer_coding_order() {
    for value in [b"chunked, chunked".as_slice(), b"chunked, gzip", b"gzip"] {
        assert!(
            Connection::new(Role::Client, Limits::default())
                .send_request(request(
                    Method::POST,
                    headers([(b"Host", b"x"), (b"Transfer-Encoding", value)])
                ))
                .is_err()
        );
    }
    for coding in [b"chunked, chunked".as_slice(), b"chunked, gzip", b"gzip"] {
        assert!(
            server("GET")
                .send_response(response(200, headers([(b"Transfer-Encoding", coding)])))
                .is_err()
        );
    }
}

// RFC 9112 §§6.1, 6.3: TE+CL may be rejected, but ambiguous input must permanently disable reuse.
#[test]
fn ambiguous_framing_is_rejected_and_closed() {
    for role in [Role::Server, Role::Client] {
        rejected(
            role,
            &message(
                role,
                b"Transfer-Encoding: chunked\r\nContent-Length: 1\r\n",
                b"0\r\n\r\n",
            ),
            400,
        );
    }
}

// RFC 9112 §6.2: no sender may emit Content-Length together with Transfer-Encoding, even on HEAD/304.
#[test]
fn no_outbound_te_and_cl() {
    let fields = headers([
        (b"Transfer-Encoding", b"chunked"),
        (b"Content-Length", b"0"),
    ]);
    let mut req = request(Method::POST, fields.clone());
    req.headers.push(b"Host", b"x");
    assert!(
        Connection::new(Role::Client, Limits::default())
            .send_request(req)
            .is_err()
    );
    let outcomes: Vec<_> = [("GET", 200), ("HEAD", 200), ("GET", 304)]
        .into_iter()
        .map(|(method, status)| {
            (
                method,
                status,
                server(method).send_response(response(status, fields.clone())),
            )
        })
        .collect();
    assert!(
        outcomes.iter().all(|(_, _, result)| match result {
            Err(_) => true,
            Ok(wire) => {
                let wire = wire.to_ascii_lowercase();
                !wire
                    .windows(b"transfer-encoding:".len())
                    .any(|w| w == b"transfer-encoding:")
                    || !wire
                        .windows(b"content-length:".len())
                        .any(|w| w == b"content-length:")
            }
        }),
        "{outcomes:?}"
    );
}

// RFC 9112 §6.1 / RFC 9110 §8.6: HEAD/304 MAY carry either TE or CL as bodyless metadata.
#[test]
fn bodyless_framing_metadata_can_be_sent() {
    for (method, status) in [("HEAD", 200), ("GET", 304)] {
        for fields in [
            headers([(b"Content-Length", b"9")]),
            headers([(b"Transfer-Encoding", b"chunked")]),
            headers([(b"Transfer-Encoding", b"gzip, chunked")]),
            headers([(b"Transfer-Encoding", b"gzip")]),
        ] {
            let mut sender = server(method);
            let wire = sender
                .send_response(response(status, fields.clone()))
                .unwrap();
            let mut receiver = client(Method::from_bytes(method.as_bytes()).unwrap());
            let got = feed(&mut receiver, &wire, 1).unwrap();
            assert_eq!(got.responses[0].headers, fields);
            assert_eq!(got.ends.len(), 1);
            assert!(got.body.is_empty());
            assert!(sender.send_data(b"x").is_err());
            assert!(sender.end_of_message(Headers::new()).unwrap().is_empty());
        }
    }
}

// RFC 9112 §§6.1, 6.3(4): invalid request transfer coding gets 400/close, unsupported final-chunked gets 501.
#[test]
fn invalid_request_transfer_coding_closes() {
    for (coding, status) in [
        (b"gzip".as_slice(), 400),
        (b"chunked, gzip", 400),
        (b"gzip, chunked", 501),
    ] {
        let fields = [b"Transfer-Encoding: ".as_slice(), coding, b"\r\n"].concat();
        rejected(Role::Server, &message(Role::Server, &fields, b""), status);
    }
}

// RFC 9112 §6.3(1): HEAD, 1xx, 204 and 304 end at the head regardless of conflicting framing fields.
#[test]
fn bodyless_response_precedence_in_both_directions() {
    for (method, status) in [(Method::HEAD, 200), (Method::GET, 204), (Method::GET, 304)] {
        let wire = format!(
            "HTTP/1.1 {status} Test\r\nContent-Length: bad\r\nTransfer-Encoding: nonsense\r\n\r\n"
        );
        for size in [1, wire.len()] {
            let got = feed(&mut client(method.clone()), wire.as_bytes(), size).unwrap();
            assert_eq!(got.ends.len(), 1);
            assert!(got.body.is_empty());
        }
        let mut sender = server(std::str::from_utf8(method.as_bytes()).unwrap());
        sender
            .send_response(response(status, Headers::new()))
            .unwrap();
        assert!(sender.send_data(b"x").is_err());
        assert!(sender.end_of_message(headers([(b"X", b"y")])).is_err());
        assert!(sender.end_of_message(Headers::new()).unwrap().is_empty());
    }
}

// RFC 9112 §6.3(2): every successful CONNECT ignores framing fields and hands off immediately.
#[test]
fn connect_client_ignores_framing_and_preserves_tunnel_bytes() {
    for status in [200, 201, 299] {
        let wire = format!(
            "HTTP/1.1 {status} Tunnel\r\nContent-Length: bad\r\nTransfer-Encoding: bad\r\n\r\ntunnel"
        );
        for size in [1, wire.len()] {
            let mut client = client(Method::CONNECT);
            let got = feed(&mut client, wire.as_bytes(), size).unwrap();
            assert_eq!(got.responses.len(), 1);
            assert!(got.body.is_empty());
            assert_eq!(client.peer_state(), State::SwitchedProtocol);
            assert_eq!(client.trailing_data().0, b"tunnel");
            assert_eq!(client.next_event().unwrap(), NextEvent::Paused);
            assert!(client.start_next_cycle().is_err());
        }
        for fields in [
            headers([(b"Content-Length", b"0")]),
            headers([(b"Transfer-Encoding", b"chunked")]),
        ] {
            assert!(
                server("CONNECT")
                    .send_response(response(status, fields))
                    .is_err()
            );
        }
    }
}

// RFC 9112 §6.3(5): invalid or disagreeing lengths are unrecoverable, including at the client.
#[test]
fn invalid_content_lengths_poison_the_connection() {
    for role in [Role::Server, Role::Client] {
        for fields in [
            b"Content-Length: -1\r\n".as_slice(),
            b"Content-Length: +1\r\n",
            b"Content-Length: 0x1\r\n",
            b"Content-Length: 1 0\r\n",
            b"Content-Length: 1, 2\r\n",
            b"Content-Length:\r\n",
            b"Content-Length: 1\r\nContent-Length: 2\r\n",
        ] {
            rejected(role, &message(role, fields, b""), 400);
        }
    }
}

// RFC 9112 §6.3(5): identical decimal lengths may be accepted as a single effective body length.
#[test]
fn identical_lengths_and_leading_zeroes() {
    for role in [Role::Server, Role::Client] {
        for fields in [
            b"Content-Length: 02, 2\r\n".as_slice(),
            b"Content-Length: 2\r\nContent-Length: 002\r\n",
        ] {
            let wire = message(role, fields, b"ok");
            for size in [1, wire.len()] {
                let got = feed(&mut peer(role), &wire, size).unwrap();
                assert_eq!(got.body, b"ok");
                assert_eq!(got.ends.len(), 1);
            }
        }
    }
}

// RFC 9112 §6.3(7): an unframed request has zero body; following bytes stay for the next message.
#[test]
fn requests_are_never_close_delimited() {
    let wire = b"POST / HTTP/1.1\r\nHost: x\r\n\r\nnext";
    for size in [1, wire.len()] {
        let mut server = peer(Role::Server);
        let got = feed(&mut server, wire, size).unwrap();
        assert!(got.body.is_empty());
        assert_eq!(got.ends.len(), 1);
        assert_eq!(server.trailing_data().0, b"next");
    }
    let mut client = Connection::new(Role::Client, Limits::default());
    client
        .send_request(request(Method::POST, headers([(b"Host", b"x")])))
        .unwrap();
    assert!(client.send_data(b"x").is_err());
}

// RFC 9112 §§6.3, 8: EOF at any incomplete framing boundary must not produce message completion.
#[test]
fn truncated_messages_are_incomplete() {
    for role in [Role::Server, Role::Client] {
        for (fields, body) in [
            (b"Content-Length: 2\r\n".as_slice(), b"x".as_slice()),
            (b"Transfer-Encoding: chunked\r\n", b"1"),
            (b"Transfer-Encoding: chunked\r\n", b"2\r\nx"),
            (b"Transfer-Encoding: chunked\r\n", b"1\r\nx"),
            (b"Transfer-Encoding: chunked\r\n", b"1\r\nx\r\n"),
            (b"Transfer-Encoding: chunked\r\n", b"0\r\nX: y\r\n"),
        ] {
            let wire = message(role, fields, body);
            for size in [1, wire.len()] {
                let mut connection = peer(role);
                assert!(feed(&mut connection, &wire, size).unwrap().ends.is_empty());
                connection.receive_data(b"").unwrap();
                assert!(connection.next_event().is_err(), "{wire:?}");
                assert!(!connection.keep_alive());
            }
        }
        let wire = message(role, b"Content-Length: 0\r\n", b"");
        for size in [1, wire.len()] {
            let mut connection = peer(role);
            let got = feed(&mut connection, &wire[..wire.len() - 1], size).unwrap();
            assert!(got.requests.is_empty() && got.responses.is_empty());
            connection.receive_data(b"").unwrap();
            assert!(connection.next_event().is_err());
        }
    }
}

// RFC 9112 §§7, 7.1: decode case-insensitive chunked, hex sizes, zero chunks and opaque binary data.
#[test]
fn chunked_client_and_server_decode_hex_and_binary() {
    for role in [Role::Server, Role::Client] {
        let wire = message(
            role,
            b"tRaNsFeR-EnCoDiNg: ChUnKeD\r\n",
            b"A\r\n0123456789\r\n2\r\n\0\xff\r\n000;end=yes\r\n\r\n",
        );
        for size in [1, wire.len()] {
            let got = feed(&mut peer(role), &wire, size).unwrap();
            assert_eq!(got.body, b"0123456789\0\xff");
            assert_eq!(got.ends, [Headers::new()]);
        }
    }
}

// RFC 9112 §7.1: chunk sizes must not overflow and chunk data must have its CRLF terminator.
#[test]
fn invalid_chunk_framing_and_overflow() {
    for role in [Role::Server, Role::Client] {
        for body in [
            b"100000000000000000000000000000000\r\n".as_slice(),
            b"-1\r\n",
            b"+1\r\n",
            b"0x1\r\n",
            b"1\r\nx!!",
            b"g\r\n",
        ] {
            rejected(
                role,
                &message(role, b"Transfer-Encoding: chunked\r\n", body),
                400,
            );
        }
    }
}

// RFC 9112 §§7.1, 7.2: chunked and compression transfer codings define no parameters (SHOULD error).
#[test]
fn transfer_coding_parameters_are_errors() {
    let mut accepted = Vec::new();
    for role in [Role::Server, Role::Client] {
        for coding in ["chunked;x=1", "gzip;x=1", "compress;x=1", "deflate;x=1"] {
            let wire = message(
                role,
                format!("Transfer-Encoding: {coding}\r\n").as_bytes(),
                b"",
            );
            for size in [1, wire.len()] {
                if feed(&mut peer(role), &wire, size).is_ok() {
                    accepted.push((role, coding, size));
                }
            }
        }
    }
    assert!(
        accepted.is_empty(),
        "parameterized codings accepted: {accepted:?}"
    );
}

// RFC 9112 §7.1.1: ignore valid unknown extensions, including BWS before the next semicolon.
#[test]
fn chunk_extension_bws_after_valueless_name() {
    let mut rejected = Vec::new();
    for role in [Role::Server, Role::Client] {
        let wire = message(
            role,
            b"Transfer-Encoding: chunked\r\n",
            b"1;flag \t;next=ok\r\nx\r\n0\r\n\r\n",
        );
        for size in [1, wire.len()] {
            match feed(&mut peer(role), &wire, size) {
                Ok(got) => {
                    assert_eq!(got.body, b"x");
                    assert_eq!(got.ends.len(), 1);
                }
                Err(error) => rejected.push((role, size, error)),
            }
        }
    }
    assert!(
        rejected.is_empty(),
        "valid extensions rejected: {rejected:?}"
    );
}

// RFC 9112 §7.1.2 / RFC 9110 §§6.5.1–2: preserve trailers separately and in received order.
#[test]
fn trailers_are_separate_ordered_and_cannot_reframe() {
    for role in [Role::Server, Role::Client] {
        let wire = message(
            role,
            b"Transfer-Encoding: chunked\r\nX: head\r\n",
            b"1\r\nx\r\n0\r\nX: first\r\nx: second\r\nContent-Length: 999\r\n\r\n",
        );
        for size in [1, wire.len()] {
            let got = feed(&mut peer(role), &wire, size).unwrap();
            let fields = if role == Role::Server {
                &got.requests[0].headers
            } else {
                &got.responses[0].headers
            };
            assert_eq!(
                fields.get_all(b"x").collect::<Vec<_>>(),
                [b"head".as_slice()]
            );
            assert!(fields.get_all(b"content-length").next().is_none());
            assert_eq!(got.body, b"x");
            assert_eq!(
                got.ends[0].get_all(b"x").collect::<Vec<_>>(),
                [b"first".as_slice(), b"second"]
            );
        }
    }
}

// RFC 9112 §7.4: a client MUST NOT advertise chunked in TE; it is implicitly acceptable.
#[test]
fn te_must_not_advertise_chunked() {
    let result = Connection::new(Role::Client, Limits::default()).send_request(request(
        Method::GET,
        headers([(b"Host", b"x"), (b"Connection", b"TE"), (b"TE", b"chunked")]),
    ));
    assert!(result.is_err(), "emitted forbidden TE: {result:?}");
}

// RFC 9112 §7.4 / RFC 9110 §7.6.1: a sender of TE MUST include Connection: TE.
#[test]
fn te_requires_connection_option() {
    let result = Connection::new(Role::Client, Limits::default()).send_request(request(
        Method::GET,
        headers([(b"Host", b"x"), (b"TE", b"trailers")]),
    ));
    if let Ok(wire) = result {
        let got = feed(&mut peer(Role::Server), &wire, 1).unwrap();
        assert!(
            got.requests[0]
                .headers
                .get_all(b"connection")
                .any(|value| value
                    .split(|b| *b == b',')
                    .any(|token| token.eq_ignore_ascii_case(b"TE"))),
            "missing Connection: TE in {wire:?}"
        );
    }
}

// RFC 9112 §8: an incomplete request may receive an error before closure.
#[test]
fn incomplete_request_can_receive_error_response() {
    let mut server = peer(Role::Server);
    feed(
        &mut server,
        b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 3\r\n\r\nx",
        1,
    )
    .unwrap();
    server.receive_data(b"").unwrap();
    assert!(server.next_event().is_err());
    let wire = server
        .send_response_with_body(response(400, Headers::new()), b"")
        .unwrap();
    assert!(
        wire.windows(b"Connection: close".len())
            .any(|w| w == b"Connection: close")
    );
    assert!(!server.keep_alive());
}

// RFC 9112 §9.2: a client with no outstanding request MUST NOT accept an unsolicited response.
#[test]
fn unsolicited_response_is_not_valid() {
    let wire = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
    let mut accepted = Vec::new();
    for size in [1, wire.len()] {
        let mut client = Connection::new(Role::Client, Limits::default());
        let got = feed(&mut client, wire, size);
        if got.is_ok() {
            accepted.push((size, got));
        } else {
            assert!(!client.keep_alive());
        }
    }
    assert!(
        accepted.is_empty(),
        "unsolicited response accepted: {accepted:?}"
    );
}

// RFC 9112 §§9.2, 9.3.2: declining client pipelining preserves the outstanding request association.
#[test]
fn client_declines_pipelining() {
    let mut client = client(Method::GET);
    assert!(
        client
            .send_request(request(Method::HEAD, headers([(b"Host", b"x")])))
            .is_err()
    );
    let got = feed(
        &mut client,
        b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\nx",
        1,
    )
    .unwrap();
    assert_eq!(got.body, b"x");
    client.start_next_cycle().unwrap();
}

// RFC 9112 §9.3: neither side may reuse a connection before consuming its complete peer message.
#[test]
fn unread_response_prevents_reuse() {
    let mut client = client(Method::GET);
    feed(
        &mut client,
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nx",
        1,
    )
    .unwrap();
    assert!(client.start_next_cycle().is_err());
    assert!(
        client
            .send_request(request(Method::GET, headers([(b"Host", b"x")])))
            .is_err()
    );
    assert_eq!(feed(&mut client, b"y", 1).unwrap().ends.len(), 1);
    client.start_next_cycle().unwrap();
}

// RFC 9112 §9.6: a server receiving/sending close must not process subsequent pipelined requests.
#[test]
fn close_stops_a_server_pipeline() {
    for remote_close in [true, false] {
        let wire = [
            b"GET / HTTP/1.1\r\nHost: x\r\n".as_slice(),
            if remote_close {
                b"Connection: cLoSe\r\n"
            } else {
                b""
            },
            b"\r\nGET /next HTTP/1.1\r\nHost: x\r\n\r\n",
        ]
        .concat();
        for size in [1, wire.len()] {
            let mut server = peer(Role::Server);
            assert_eq!(feed(&mut server, &wire, size).unwrap().requests.len(), 1);
            server
                .send_response_with_body(response(200, headers([(b"Connection", b"close")])), b"")
                .unwrap();
            assert!(!server.keep_alive());
            assert!(server.start_next_cycle().is_err());
            assert_eq!(server.next_event().unwrap(), NextEvent::Paused);
        }
    }
}

// RFC 9110 §§5.1–3: preserve unknown fields, same-name order and separate Set-Cookie field lines.
#[test]
fn field_order_and_unknown_fields_survive_both_directions() {
    for role in [Role::Server, Role::Client] {
        let wire = message(role, b"X-New: first\r\nx-new: second\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\nContent-Length: 0\r\n", b"");
        for size in [1, wire.len()] {
            let got = feed(&mut peer(role), &wire, size).unwrap();
            let fields = if role == Role::Server {
                got.requests[0].headers.clone()
            } else {
                got.responses[0].headers.clone()
            };
            assert_eq!(
                fields.get_all(b"X-NEW").collect::<Vec<_>>(),
                [b"first".as_slice(), b"second"]
            );
            assert_eq!(
                fields.get_all(b"set-cookie").collect::<Vec<_>>(),
                [b"a=1".as_slice(), b"b=2"]
            );
            let emitted = match role {
                Role::Server => Connection::new(Role::Client, Limits::default())
                    .send_request(request(Method::POST, fields)),
                Role::Client => server("GET").send_response(response(200, fields)),
            }
            .unwrap();
            assert!(
                emitted
                    .windows(b"X-New: first\r\nx-new: second".len())
                    .any(|w| w == b"X-New: first\r\nx-new: second")
            );
            assert!(
                emitted
                    .windows(b"Set-Cookie: a=1\r\nSet-Cookie: b=2".len())
                    .any(|w| w == b"Set-Cookie: a=1\r\nSet-Cookie: b=2")
            );
        }
    }
}

// RFC 9110 §5.3: the entire header section must be received before a request is exposed.
#[test]
fn no_request_before_complete_headers() {
    let wire = b"GET / HTTP/1.1\r\nHost: x\r\nAuthorization: secret\r\n\r\n";
    let mut server = peer(Role::Server);
    for byte in &wire[..wire.len() - 1] {
        server.receive_data(&[*byte]).unwrap();
        assert_eq!(server.next_event().unwrap(), NextEvent::NeedData);
    }
    server.receive_data(&wire[wire.len() - 1..]).unwrap();
    assert!(matches!(event(&mut server), Event::Request(_)));
}

// RFC 9110 §5.5: trim header OWS, preserving empty values and internal SP/HTAB.
#[test]
fn field_value_ows_is_excluded() {
    for role in [Role::Server, Role::Client] {
        let wire = message(
            role,
            b"X: \t a \tb \t\r\nEmpty:\t \r\nContent-Length: 0\r\n",
            b"",
        );
        for size in [1, wire.len()] {
            let got = feed(&mut peer(role), &wire, size).unwrap();
            let fields = if role == Role::Server {
                &got.requests[0].headers
            } else {
                &got.responses[0].headers
            };
            assert_eq!(fields.get_all(b"x").next(), Some(b"a \tb".as_slice()));
            assert_eq!(fields.get_all(b"empty").next(), Some(b"".as_slice()));
        }
    }
}

// RFC 9110 §§5.1, 5.5: reject invalid field names and NUL/CTL values at either peer boundary.
#[test]
fn invalid_field_octets_are_rejected() {
    for role in [Role::Server, Role::Client] {
        for fields in [
            b"Bad(Name): x\r\n".as_slice(),
            b": x\r\n",
            b"X: a\0b\r\n",
            b"X: a\x01b\r\n",
            b"X: a\x7fb\r\n",
        ] {
            rejected(role, &message(role, fields, b""), 400);
        }
    }
}

// RFC 9110 §5.6.1.1: senders MUST NOT generate empty elements in engine-interpreted lists.
#[test]
fn outbound_lists_have_no_empty_elements() {
    let mut emitted = Vec::new();
    for fields in [
        headers([(b"Host", b"x"), (b"Connection", b",close")]),
        headers([
            (b"Host", b"x"),
            (b"Connection", b"upgrade"),
            (b"Upgrade", b",websocket"),
        ]),
        headers([
            (b"Host", b"x"),
            (b"Content-Length", b"1"),
            (b"Expect", b",100-continue"),
        ]),
    ] {
        if let Ok(wire) = Connection::new(Role::Client, Limits::default())
            .send_request(request(Method::POST, fields))
        {
            // A serializer may normalize instead of rejecting the input.
            if wire.windows(2).any(|w| w == b" ,") {
                emitted.push(wire);
            }
        }
    }
    assert!(
        emitted.is_empty(),
        "emitted empty list elements: {emitted:?}"
    );

    // An empty value is a list of no elements, and quoted commas separate none.
    for fields in [
        headers([(b"Host", b"x"), (b"Connection", b"TE"), (b"TE", b"")]),
        headers([
            (b"Host", b"x"),
            (b"Connection", b"TE"),
            (b"TE", br#"foo;p="a,chunked,b""#),
        ]),
        headers([
            (b"Host", b"x"),
            (b"Connection", b"TE"),
            (b"TE", br#"foo;p="a\",chunked""#),
        ]),
        headers([(b"Host", b"x"), (b"Expect", br#"foo="a,,b""#)]),
    ] {
        Connection::new(Role::Client, Limits::default())
            .send_request(request(Method::GET, fields))
            .unwrap();
    }
}

// RFC 9110 §5.6.1.2: ignore a reasonable number of empty list elements in Transfer-Encoding.
#[test]
fn transfer_encoding_ignores_empty_list_elements() {
    let mut failures = Vec::new();
    for role in [Role::Server, Role::Client] {
        let wire = message(
            role,
            b"Transfer-Encoding: , chunked,,\r\n",
            b"1\r\nx\r\n0\r\n\r\n",
        );
        for size in [1, wire.len()] {
            match feed(&mut peer(role), &wire, size) {
                Ok(got) => {
                    assert_eq!(got.body, b"x");
                    assert_eq!(got.ends.len(), 1);
                }
                Err(error) => failures.push((role, size, error)),
            }
        }
    }
    assert!(failures.is_empty(), "empty elements rejected: {failures:?}");
}

// RFC 9110 §§5.6.1.2, 7.6.1: case-insensitive Connection tokens span field lines and ignore empties.
// Connection has no quoted strings, so a stray quote does not hide a later option.
#[test]
fn connection_lists_combine_in_order() {
    for role in [Role::Server, Role::Client] {
        let wire = message(
            role,
            b"Connection: keep-alive ,,\r\nConnection: ,x\", cLoSe\r\nContent-Length: 0\r\n",
            b"",
        );
        for size in [1, wire.len()] {
            let mut connection = peer(role);
            assert_eq!(feed(&mut connection, &wire, size).unwrap().ends.len(), 1);
            assert!(!connection.keep_alive());
        }
    }
}

// RFC 9110 §§5.6.1.2, 7.8, 10.1.1: ignore empty elements in Upgrade and Expect as well as Connection.
#[test]
fn upgrade_and_expect_ignore_empty_elements() {
    let wire =
        b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 1\r\nExpect: , 100-CoNtInUe,,\r\n\r\n";
    for size in [1, wire.len()] {
        let mut server = peer(Role::Server);
        assert_eq!(feed(&mut server, wire, size).unwrap().requests.len(), 1);
        assert!(server.client_is_waiting_for_100_continue());
    }
    let proposal =
        b"GET / HTTP/1.1\r\nHost: x\r\nConnection: , Upgrade,,\r\nUpgrade: , websocket,,\r\n\r\n";
    for size in [1, proposal.len()] {
        let mut server = peer(Role::Server);
        feed(&mut server, proposal, size).unwrap();
        assert_eq!(server.peer_state(), State::MightSwitchProtocol);
        server
            .send_informational_response(response(
                101,
                headers([(b"Connection", b"Upgrade"), (b"Upgrade", b"websocket")]),
            ))
            .unwrap();
    }
    let reply =
        b"HTTP/1.1 101 Switching\r\nConnection: ,upgrade,,\r\nUpgrade: ,websocket,,\r\n\r\n";
    for size in [1, reply.len()] {
        let mut client = Connection::new(Role::Client, Limits::default());
        client
            .send_request(request(
                Method::GET,
                headers([
                    (b"Host", b"x"),
                    (b"Connection", b"upgrade"),
                    (b"Upgrade", b"websocket"),
                ]),
            ))
            .unwrap();
        client.end_of_message(Headers::new()).unwrap();
        feed(&mut client, reply, size).unwrap();
        assert_eq!(client.peer_state(), State::SwitchedProtocol);
    }
}

// RFC 9110 §6.2: higher minor versions SHOULD be processed as the highest supported HTTP/1.x.
#[test]
#[ignore = "known deviation: httparse rejects minor versions above 1"]
fn higher_minor_version_is_http11() {
    let mut failures = Vec::new();
    for role in [Role::Server, Role::Client] {
        for version in ["1.2", "1.9"] {
            let wire = match role {
                Role::Server => format!("GET / HTTP/{version}\r\nHost: x\r\n\r\n"),
                Role::Client => format!("HTTP/{version} 200 OK\r\nContent-Length: 0\r\n\r\n"),
            };
            for size in [1, wire.len()] {
                let mut connection = peer(role);
                match feed(&mut connection, wire.as_bytes(), size) {
                    Ok(got) => {
                        assert_eq!(got.ends.len(), 1);
                        assert!(connection.keep_alive());
                    }
                    Err(error) => failures.push((role, version, size, error)),
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "higher minor versions rejected: {failures:?}"
    );
}

// RFC 9110 §7.2: a user agent SHOULD send Host first in its header section.
#[test]
fn host_is_sent_first() {
    let wire = Connection::new(Role::Client, Limits::default())
        .send_request(request(
            Method::GET,
            headers([(b"X", b"y"), (b"Host", b"x")]),
        ))
        .unwrap();
    assert!(
        wire.starts_with(b"GET / HTTP/1.1\r\nHost: x\r\n"),
        "Host was not first: {wire:?}"
    );
}

// RFC 9110 §7.2: user-agent requests need Host even when choosing the HTTP/1.0 wire version.
#[test]
fn http10_user_agent_emits_host() {
    let mut req = request(Method::GET, Headers::new());
    req.http_version = Version::Http10;
    let result = Connection::new(Role::Client, Limits::default()).send_request(req);
    assert!(
        result.is_err(),
        "HTTP/1.0 request without Host emitted: {result:?}"
    );
}

// RFC 9110 §7.8: protocol names compare case-insensitively; a server may select a later preference.
#[test]
fn upgrade_selection_and_decline() {
    let req = request(
        Method::GET,
        headers([
            (b"Host", b"x"),
            (b"Connection", b"upgrade"),
            (b"Upgrade", b"first/1, WebSocket"),
        ]),
    );
    for size in [1, 1024] {
        let mut client = Connection::new(Role::Client, Limits::default());
        client.send_request(req.clone()).unwrap();
        client.end_of_message(Headers::new()).unwrap();
        feed(
            &mut client,
            b"HTTP/1.1 101 Switching\r\nConnection: upgrade\r\nUpgrade: websocket\r\n\r\nnext",
            size,
        )
        .unwrap();
        assert_eq!(client.peer_state(), State::SwitchedProtocol);
    }
    let mut client = Connection::new(Role::Client, Limits::default());
    let wire = client.send_request(req).unwrap();
    client.end_of_message(Headers::new()).unwrap();
    let mut server = peer(Role::Server);
    feed(&mut server, &wire, 1).unwrap();
    let wire = server
        .send_response_with_body(response(200, Headers::new()), b"")
        .unwrap();
    feed(&mut client, &wire, 1).unwrap();
    client.start_next_cycle().unwrap();
    server.start_next_cycle().unwrap();
}

// RFC 9110 §7.8: a 426 response MUST carry Upgrade identifying acceptable protocols.
#[test]
fn upgrade_required_response_has_upgrade() {
    let result = server("GET").send_response(response(426, Headers::new()));
    assert!(result.is_err(), "426 without Upgrade emitted: {result:?}");
}

// RFC 9110 §7.8: every sender of Upgrade MUST send Connection: Upgrade, including advertisements.
#[test]
fn upgrade_advertisements_require_connection_option() {
    let mut missing = Vec::new();
    for status in [103, 200, 426] {
        let mut server = server("GET");
        let head = response(status, headers([(b"Upgrade", b"websocket")]));
        let result = if status < 200 {
            server.send_informational_response(head)
        } else {
            server.send_response(head)
        };
        if let Ok(wire) = result
            && !wire
                .windows(b"Connection: upgrade".len())
                .any(|w| w.eq_ignore_ascii_case(b"Connection: upgrade"))
        {
            missing.push((status, wire));
        }
    }
    assert!(
        missing.is_empty(),
        "Upgrade without its connection option: {missing:?}"
    );
}

// RFC 9110 §7.8: an upgraded client must finish its request before switching its sending protocol.
#[test]
fn upgrade_waits_for_request_completion() {
    let mut client = Connection::new(Role::Client, Limits::default());
    client
        .send_request(request(
            Method::POST,
            headers([
                (b"Host", b"x"),
                (b"Connection", b"upgrade"),
                (b"Upgrade", b"websocket"),
                (b"Content-Length", b"1"),
            ]),
        ))
        .unwrap();
    feed(
        &mut client,
        b"HTTP/1.1 101 Switching\r\nConnection: upgrade\r\nUpgrade: websocket\r\n\r\n",
        1,
    )
    .unwrap();
    assert_eq!(client.local_state(), State::SendBody);
    assert_eq!(client.send_data(b"x").unwrap(), b"x");
    client.end_of_message(Headers::new()).unwrap();
    assert_eq!(client.local_state(), State::SwitchedProtocol);
}

// RFC 9110 §8.6: large decimal lengths must not wrap or lose precision.
#[test]
fn content_length_overflow_is_rejected() {
    for role in [Role::Server, Role::Client] {
        rejected(
            role,
            &message(
                role,
                b"Content-Length: 999999999999999999999999999999999999999\r\n",
                b"",
            ),
            400,
        );
    }
}

// RFC 9110 §8.6: forwarding repeated Content-Length requires rejection or a single decimal value.
#[test]
fn outbound_content_length_is_one_decimal_value() {
    let mut bad = Vec::new();
    for role in [Role::Server, Role::Client] {
        for fields in [
            headers([(b"Content-Length", b"2, 2")]),
            headers([(b"Content-Length", b"2"), (b"Content-Length", b"2")]),
        ] {
            let result = match role {
                Role::Server => server("GET").send_response(response(200, fields)),
                Role::Client => {
                    let mut req = request(Method::POST, fields);
                    req.headers.push(b"Host", b"x");
                    Connection::new(Role::Client, Limits::default()).send_request(req)
                }
            };
            if let Ok(wire) = result {
                let receiver_role = if role == Role::Server {
                    Role::Client
                } else {
                    Role::Server
                };
                let got = feed(&mut peer(receiver_role), &wire, 1).unwrap();
                let fields = if role == Role::Server {
                    &got.responses[0].headers
                } else {
                    &got.requests[0].headers
                };
                let values: Vec<_> = fields.get_all(b"content-length").collect();
                if values.len() != 1 || !values[0].iter().all(u8::is_ascii_digit) {
                    bad.push((role, wire));
                }
            }
        }
    }
    assert!(
        bad.is_empty(),
        "invalid Content-Length forwarded unchanged: {bad:?}"
    );
}

// RFC 9110 §8.6: bodyless response Content-Length metadata still has to be valid decimal syntax.
#[test]
fn bodyless_content_length_metadata_is_valid() {
    let mut bad = Vec::new();
    for (method, status) in [("HEAD", 200), ("GET", 304)] {
        for value in [b"nope".as_slice(), b"-1"] {
            if let Ok(wire) = server(method)
                .send_response(response(status, headers([(b"Content-Length", value)])))
            {
                bad.push((method, status, wire));
            }
        }
    }
    assert!(bad.is_empty(), "invalid metadata emitted: {bad:?}");
}

// RFC 9110 §8.6: never send content inconsistent with a declared length, even when streamed.
#[test]
fn sender_enforces_exact_content_length() {
    for role in [Role::Server, Role::Client] {
        let mut sender = if role == Role::Server {
            server("GET")
        } else {
            Connection::new(Role::Client, Limits::default())
        };
        if role == Role::Server {
            sender
                .send_response(response(200, headers([(b"Content-Length", b"2")])))
                .unwrap();
        } else {
            sender
                .send_request(request(
                    Method::POST,
                    headers([(b"Host", b"x"), (b"Content-Length", b"2")]),
                ))
                .unwrap();
        }
        assert!(sender.send_data(b"abc").is_err());
        assert_eq!(sender.send_data(b"a").unwrap(), b"a");
        assert!(sender.end_of_message(Headers::new()).is_err());
        assert_eq!(sender.send_data(b"b").unwrap(), b"b");
        assert!(sender.end_of_message(Headers::new()).unwrap().is_empty());
    }
}

// RFC 9110 §10.1.1: a client MAY send its content without waiting for 100 Continue.
#[test]
fn expect_continue_does_not_block_content() {
    let mut client = Connection::new(Role::Client, Limits::default());
    client
        .send_request(request(
            Method::POST,
            headers([
                (b"Host", b"x"),
                (b"Content-Length", b"1"),
                (b"Expect", b"100-continue"),
            ]),
        ))
        .unwrap();
    assert_eq!(client.send_data(b"x").unwrap(), b"x");
    assert!(!client.client_is_waiting_for_100_continue());
    client.end_of_message(Headers::new()).unwrap();
}

// RFC 9110 §10.1.1: after 100 the server still owes a final response before reuse.
#[test]
fn continue_requires_final_response() {
    let mut server = peer(Role::Server);
    feed(
        &mut server,
        b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 1\r\nExpect: 100-continue\r\n\r\n",
        1,
    )
    .unwrap();
    server
        .send_informational_response(response(100, Headers::new()))
        .unwrap();
    assert_eq!(server.local_state(), State::SendResponse);
    feed(&mut server, b"x", 1).unwrap();
    assert!(server.start_next_cycle().is_err());
    server
        .send_response_with_body(response(200, Headers::new()), b"")
        .unwrap();
    server.start_next_cycle().unwrap();
}

// RFC 9110 §15.2: parse multiple unsolicited 1xx before the final response, without body/trailers.
#[test]
fn informational_sequence_has_one_final_message() {
    let wire = b"HTTP/1.1 103 Early\r\nContent-Length: 8\r\n\r\nHTTP/1.1 199 Extension\r\nTransfer-Encoding: chunked\r\n\r\nHTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\nx";
    for size in [1, wire.len()] {
        let got = feed(&mut client(Method::GET), wire, size).unwrap();
        assert_eq!(
            got.informational
                .iter()
                .map(|r| r.status.as_u16())
                .collect::<Vec<_>>(),
            [103, 199, 100]
        );
        assert_eq!(got.responses.len(), 1);
        assert_eq!(got.ends.len(), 1);
        assert_eq!(got.body, b"x");
    }
}

// RFC 9112 §6.1 / RFC 9110 §§8.6, 15.2: 1xx senders must not emit CL, TE, content or trailers.
#[test]
fn informational_sender_cannot_frame_a_body() {
    for status in [100, 103, 199] {
        for fields in [
            headers([(b"Content-Length", b"0")]),
            headers([(b"Transfer-Encoding", b"chunked")]),
        ] {
            assert!(
                server("GET")
                    .send_informational_response(response(status, fields))
                    .is_err()
            );
        }
        let mut server = server("GET");
        server
            .send_informational_response(response(status, Headers::new()))
            .unwrap();
        assert!(server.send_data(b"x").is_err());
        assert!(server.end_of_message(headers([(b"X", b"y")])).is_err());
        assert_eq!(server.local_state(), State::SendResponse);
    }
}

// RFC 9110 §15.5.9 / RFC 9112 §§8, 9.6: 408 for an incomplete request makes HTTP reuse impossible.
#[test]
fn timeout_response_closes_incomplete_request() {
    let mut server = peer(Role::Server);
    feed(
        &mut server,
        b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 2\r\n\r\nx",
        1,
    )
    .unwrap();
    let wire = server
        .send_response_with_body(response(408, Headers::new()), b"")
        .unwrap();
    assert!(
        wire.windows(b"Connection: close".len())
            .any(|w| w == b"Connection: close")
    );
    assert!(!server.keep_alive());
    assert!(server.start_next_cycle().is_err());
}
