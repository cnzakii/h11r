//! HTTP/1.1 connection lifecycle and semantic event flow.
//!
use crate::state::{ConnectionState, StateEvent, SwitchKind, SwitchProposals};
use crate::wire::{
    Buffer, Chunk, Framing, Reader, SectionProgress, count_header, expects_continue, find_crlf,
    framing, has_framing_fields, has_token, incomplete_line_len, local, parse_chunk_header,
    parse_request_head, parse_response_head, parse_trailers, protocol_matches, reader, remote,
    response_framing, serialize_request, serialize_response, set_connection_close,
    upgrade_protocols, valid_host, valid_request_target, validate_headers, validate_reason,
    validate_request, validate_trailers, write_headers,
};
use crate::{
    Data, DataParts, EndOfMessage, Event, Headers, InformationalResponse, Limits,
    LocalProtocolError, Method, NextEvent, RemoteProtocolError, Request, Response, Role, State,
    StatusCode, Version,
};
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequestKind {
    Other,
    Head,
    Connect,
}

impl RequestKind {
    fn from_method(method: &[u8]) -> Self {
        match method {
            b"HEAD" => Self::Head,
            b"CONNECT" => Self::Connect,
            _ => Self::Other,
        }
    }
}

/// Owned poll outcome. Body bytes are described by count and only borrowed
/// from the input buffer in `next_event`, after error bookkeeping that needs
/// the connection mutably.
enum Polled {
    /// Any event except `Event::Data`.
    Event(Event<'static>),
    /// The next `count` input bytes are decoded body data.
    Data {
        count: usize,
        chunk_start: bool,
        chunk_end: bool,
    },
    NeedData,
    Paused,
}

/// One Sans-I/O HTTP/1 connection.
///
/// The connection owns the client and server protocol states, input buffering,
/// message framing, and switch negotiation. It never reads from or writes to a
/// transport: callers provide received bytes and write the bytes returned by
/// the send methods.
#[derive(Debug)]
pub struct Connection {
    role: Role,
    state: ConnectionState,
    limits: Limits,
    input: Buffer,
    eof: bool,
    peer_version: Option<Version>,
    cycle: Cycle,
}

/// State scoped to one request/response cycle, reset by `start_next_cycle`.
#[derive(Clone, Debug, Default)]
struct Cycle {
    progress: SectionProgress,
    reader: Reader,
    writer: Option<Framing>,
    chunk_extension_bytes: usize,
    request_kind: Option<RequestKind>,
    upgrade_proposals: Vec<Vec<u8>>,
    waiting_for_continue: bool,
    continue_before_upgrade: bool,
}

impl Connection {
    /// Creates an idle connection with independent inbound limits.
    #[must_use]
    pub fn new(role: Role, limits: Limits) -> Self {
        Self {
            role,
            state: ConnectionState::new(),
            limits,
            input: Buffer::default(),
            eof: false,
            peer_version: None,
            cycle: Cycle::default(),
        }
    }

    /// Returns the local actor's lifecycle state.
    #[must_use]
    pub fn local_state(&self) -> State {
        self.state.state(self.role)
    }

    /// Returns the peer actor's lifecycle state.
    #[must_use]
    pub fn peer_state(&self) -> State {
        self.state.state(self.role.peer())
    }

    /// Whether HTTP may continue on this connection after the current exchange.
    ///
    /// This becomes `false` once either actor requires closure, closes,
    /// fails, or switches protocols. When it remains `true` after both
    /// messages complete, [`Connection::start_next_cycle`] can reuse the
    /// connection.
    #[must_use]
    pub fn keep_alive(&self) -> bool {
        self.state.keep_alive()
            && [self.local_state(), self.peer_state()].iter().all(|state| {
                !matches!(
                    state,
                    State::MustClose | State::Closed | State::Error | State::SwitchedProtocol
                )
            })
    }

    /// Returns the most recently parsed peer HTTP version.
    #[must_use]
    pub const fn peer_http_version(&self) -> Option<Version> {
        self.peer_version
    }

    /// Whether the client has sent `Expect: 100-continue` but has not begun its body.
    #[must_use]
    pub const fn client_is_waiting_for_100_continue(&self) -> bool {
        self.cycle.waiting_for_continue
    }

    /// Returns bytes that belong to a pipelined message or switched protocol,
    /// plus whether transport EOF has already been received.
    #[must_use]
    pub fn trailing_data(&self) -> (&[u8], bool) {
        (self.input.as_slice(), self.eof)
    }

    /// Appends peer bytes. Empty bytes mark transport EOF.
    ///
    /// # Errors
    ///
    /// Returns [`LocalProtocolError`] if non-empty bytes are supplied after
    /// EOF has already been recorded.
    pub fn receive_data(&mut self, data: &[u8]) -> Result<(), LocalProtocolError> {
        if self.eof && !data.is_empty() {
            return Err(local("receive_data called after EOF"));
        }
        if data.is_empty() {
            self.eof = true;
        } else {
            self.input.extend(data);
        }
        Ok(())
    }

    #[doc(hidden)]
    /// Appends peer bytes from an iterator. An empty iterator marks transport EOF.
    ///
    /// # Errors
    ///
    /// Returns [`LocalProtocolError`] if the iterator supplies a byte after EOF
    /// has already been recorded.
    pub fn receive_data_iter(
        &mut self,
        data: impl IntoIterator<Item = u8>,
    ) -> Result<(), LocalProtocolError> {
        let mut data = data.into_iter();
        let Some(first) = data.next() else {
            self.eof = true;
            return Ok(());
        };
        if self.eof {
            return Err(local("receive_data called after EOF"));
        }
        self.input.extend_iter(std::iter::once(first).chain(data));
        Ok(())
    }

    /// Polls one peer event, `NeedData`, or `Paused`.
    ///
    /// # Errors
    ///
    /// Returns [`RemoteProtocolError`] if the peer input violates HTTP syntax,
    /// framing, configured limits, or the current protocol state.
    pub fn next_event(&mut self) -> Result<NextEvent<'_>, RemoteProtocolError> {
        let polled = self.next_event_inner().map_err(|mut error| {
            if self.role == Role::Client {
                error.suggested_status_code = None;
            }
            self.state.process_error(self.role.peer());
            error
        })?;
        Ok(match polled {
            Polled::Event(event) => NextEvent::Event(event),
            Polled::Data {
                count,
                chunk_start,
                chunk_end,
            } => NextEvent::Event(Event::Data(Data {
                data: self.input.take(count),
                chunk_start,
                chunk_end,
            })),
            Polled::NeedData => NextEvent::NeedData,
            Polled::Paused => NextEvent::Paused,
        })
    }

    fn next_event_inner(&mut self) -> Result<Polled, RemoteProtocolError> {
        match self.peer_state() {
            State::Closed => return Ok(Polled::Event(Event::ConnectionClosed)),
            State::Idle if self.eof && self.input.as_slice().is_empty() => {
                return self.peer_closed();
            }
            State::MightSwitchProtocol | State::SwitchedProtocol => {
                return Ok(Polled::Paused);
            }
            State::Done | State::MustClose if !self.input.as_slice().is_empty() => {
                return Ok(Polled::Paused);
            }
            State::Done | State::MustClose => {
                if self.eof {
                    return self.peer_closed();
                }
                return Ok(Polled::NeedData);
            }
            State::Error => return Err(remote("peer state is ERROR", None)),
            _ => {}
        }

        match self.cycle.reader {
            Reader::Head => self.read_head(),
            Reader::Fixed(0) => self.finish_peer(Headers::new()),
            Reader::Fixed(remaining) => {
                let count = remaining.min(self.input.as_slice().len());
                if count == 0 {
                    return self.need_data_or_eof("unexpected EOF in Content-Length body");
                }
                self.cycle.reader = Reader::Fixed(remaining - count);
                self.peer_data(count, false, false)
            }
            Reader::CloseDelimited => {
                let count = self.input.as_slice().len();
                if count > 0 {
                    return self.peer_data(count, false, false);
                }
                if self.eof {
                    return self.finish_peer(Headers::new());
                }
                Ok(Polled::NeedData)
            }
            Reader::Chunked(chunk) => self.read_chunked(chunk),
        }
    }

    fn advance_peer(&mut self, event: StateEvent) -> Result<(), RemoteProtocolError> {
        self.state
            .process_event(self.role.peer(), event)
            .map_err(|_| remote("event is invalid in the peer state", Some(400)))
    }

    fn advance_local(&mut self, event: StateEvent) -> Result<(), LocalProtocolError> {
        self.state
            .process_event(self.role, event)
            .map_err(|_| local("event is invalid in the local state"))
    }

    fn request_is(&self, kind: RequestKind) -> bool {
        self.cycle.request_kind == Some(kind)
    }

    /// Whether a final response with `status` turns a CONNECT into a tunnel.
    fn opens_tunnel(&self, status: StatusCode) -> bool {
        self.request_is(RequestKind::Connect) && status.is_success()
    }

    /// RFC 9112 Section 6.3 items 1 and 2: these final responses have no
    /// content, whatever framing fields they carry.
    /// https://www.rfc-editor.org/rfc/rfc9112.html#section-6.3
    fn response_has_no_body(&self, status: StatusCode) -> bool {
        matches!(status.as_u16(), 204 | 304)
            || self.request_is(RequestKind::Head)
            || self.opens_tunnel(status)
    }

    fn read_head(&mut self) -> Result<Polled, RemoteProtocolError> {
        if !self.section_due(true) {
            return self.wait_for_section(true);
        }
        let max_headers = self.limits.max_header_count;
        if self.role == Role::Server {
            let Some((end, request)) = parse_request_head(self.section(), max_headers)? else {
                return self.wait_for_section(true);
            };
            self.input.consume(end);
            self.cycle.progress = SectionProgress::default();
            self.accept_request(request)
        } else {
            let Some((end, response)) = parse_response_head(self.section(), max_headers)? else {
                return self.wait_for_section(true);
            };
            self.input.consume(end);
            self.cycle.progress = SectionProgress::default();
            self.accept_response(response)
        }
    }

    /// Whether the buffered field section should be handed to `httparse`.
    ///
    /// A section larger than the limit is parsed once more before it is
    /// rejected, so a syntax error within the limit wins under any
    /// segmentation.
    fn section_due(&mut self, skip_leading_empty: bool) -> bool {
        self.eof
            || self.input.as_slice().len() > self.limits.max_head_bytes
            || self
                .cycle
                .progress
                .due(self.input.as_slice(), skip_leading_empty)
    }

    /// The buffered field section prefix within the configured byte limit.
    fn section(&self) -> &[u8] {
        let data = self.input.as_slice();
        &data[..data.len().min(self.limits.max_head_bytes)]
    }

    fn wait_for_section(
        &mut self,
        skip_leading_empty: bool,
    ) -> Result<Polled, RemoteProtocolError> {
        let (limit_message, eof_message, status) = if skip_leading_empty {
            (
                "HTTP head exceeds max_head_bytes",
                "unexpected EOF in HTTP head",
                (self.role == Role::Server).then_some(431),
            )
        } else {
            (
                "HTTP trailers exceed max_head_bytes",
                "unexpected EOF in trailers",
                Some(400),
            )
        };
        if self.input.as_slice().len() > self.limits.max_head_bytes {
            return Err(remote(limit_message, status));
        }
        self.need_data_or_eof(eof_message)
    }

    fn accept_request(&mut self, request: Request) -> Result<Polled, RemoteProtocolError> {
        let Request {
            method,
            target,
            headers,
            http_version: version,
        } = request;
        if !valid_request_target(method.as_bytes(), &target) {
            return Err(remote("invalid request target", Some(400)));
        }
        if !valid_host(version, &headers) {
            return Err(remote(
                "request has a missing, duplicate, or invalid Host field",
                Some(400),
            ));
        }
        let framing = framing(&headers, version)?;
        let upgrades = self.proposed_upgrades(version, &headers);
        let proposals =
            SwitchProposals::from_flags(method.as_bytes() == b"CONNECT", !upgrades.is_empty());
        self.advance_peer(StateEvent::Request(proposals))?;
        self.apply_persistence(version, &headers);
        self.cycle.request_kind = Some(RequestKind::from_method(method.as_bytes()));
        self.cycle.waiting_for_continue = expects_continue(version, &headers, framing);
        self.cycle.continue_before_upgrade =
            self.cycle.waiting_for_continue && !upgrades.is_empty();
        self.cycle.upgrade_proposals = upgrades;
        self.peer_version = Some(version);
        self.cycle.reader = reader(framing);
        Ok(Polled::Event(Event::Request(Request {
            method,
            target,
            headers,
            http_version: version,
        })))
    }

    fn accept_response(&mut self, response: Response) -> Result<Polled, RemoteProtocolError> {
        let Response {
            status,
            reason,
            headers,
            http_version: version,
        } = response;
        if status.is_informational() {
            if status.as_u16() == 101
                && (self.cycle.continue_before_upgrade
                    || !self.valid_upgrade_response(version, &headers))
            {
                return Err(remote("invalid 101 Upgrade response", None));
            }
            let event = if status.as_u16() == 101 {
                StateEvent::ProtocolSwitch(SwitchKind::Upgrade)
            } else {
                StateEvent::InformationalResponse
            };
            self.advance_peer(event)?;
            if status.as_u16() == 100 {
                self.cycle.waiting_for_continue = false;
                self.cycle.continue_before_upgrade = false;
            }
            self.peer_version = Some(version);
            return Ok(Polled::Event(Event::InformationalResponse(
                InformationalResponse {
                    status,
                    reason,
                    headers,
                    http_version: version,
                },
            )));
        }
        let connect = self.opens_tunnel(status);
        self.advance_peer(if connect {
            StateEvent::ProtocolSwitch(SwitchKind::Connect)
        } else {
            StateEvent::Response
        })?;
        self.apply_persistence(version, &headers);
        self.peer_version = Some(version);
        self.cycle.waiting_for_continue = false;
        self.cycle.continue_before_upgrade = false;
        self.cycle.upgrade_proposals.clear();
        let framing = if self.response_has_no_body(status) {
            Framing::Fixed(0)
        } else {
            response_framing(&headers, version)?
        };
        if framing == Framing::CloseDelimited {
            self.state.disable_keep_alive();
        }
        self.cycle.reader = reader(framing);
        Ok(Polled::Event(Event::Response(Response {
            status,
            reason,
            headers,
            http_version: version,
        })))
    }

    fn read_chunked(&mut self, chunk: Chunk) -> Result<Polled, RemoteProtocolError> {
        match chunk {
            Chunk::Size { scanned } => {
                let Some(end) = find_crlf(self.input.as_slice(), scanned) else {
                    if incomplete_line_len(self.input.as_slice(), 0) > self.limits.max_head_bytes {
                        return Err(remote(
                            "chunk line exceeds max_head_bytes",
                            (self.role == Role::Server).then_some(400),
                        ));
                    }
                    self.cycle.reader = Reader::Chunked(Chunk::Size {
                        scanned: self.input.as_slice().len().saturating_sub(1),
                    });
                    return self.need_data_or_eof("unexpected EOF in chunk size");
                };
                if end - 2 > self.limits.max_head_bytes {
                    return Err(remote(
                        "chunk line exceeds max_head_bytes",
                        (self.role == Role::Server).then_some(400),
                    ));
                }
                let line = &self.input.as_slice()[..end - 2];
                let size = parse_chunk_header(line)
                    .ok_or_else(|| remote("invalid chunk size", Some(400)))?;
                // RFC 9112 Section 7.1.1: bound extension bytes across the
                // whole message, not only within one chunk-size line.
                let digits = line
                    .iter()
                    .take_while(|byte| byte.is_ascii_hexdigit())
                    .count();
                self.cycle.chunk_extension_bytes += line.len() - digits;
                if self.cycle.chunk_extension_bytes > self.limits.max_head_bytes {
                    return Err(remote(
                        "chunk extensions exceed max_head_bytes",
                        (self.role == Role::Server).then_some(400),
                    ));
                }
                self.input.consume(end);
                if size == 0 {
                    self.cycle.reader = Reader::Chunked(Chunk::Trailers);
                    self.cycle.progress = SectionProgress::default();
                } else {
                    self.cycle.reader = Reader::Chunked(Chunk::Data {
                        remaining: size,
                        first: true,
                    });
                }
                self.next_event_inner()
            }
            Chunk::Data { remaining, first } => {
                let count = remaining.min(self.input.as_slice().len());
                if count == 0 {
                    return self.need_data_or_eof("unexpected EOF in chunk data");
                }
                let last = count == remaining;
                self.cycle.reader = if last {
                    Reader::Chunked(Chunk::DataEnd)
                } else {
                    Reader::Chunked(Chunk::Data {
                        remaining: remaining - count,
                        first: false,
                    })
                };
                self.peer_data(count, first, last)
            }
            Chunk::DataEnd => {
                if self.input.as_slice().len() < 2 {
                    return self.need_data_or_eof("unexpected EOF after chunk data");
                }
                if &self.input.as_slice()[..2] != b"\r\n" {
                    return Err(remote("chunk data is not followed by CRLF", Some(400)));
                }
                self.input.consume(2);
                self.cycle.reader = Reader::Chunked(Chunk::Size { scanned: 0 });
                self.next_event_inner()
            }
            Chunk::Trailers => {
                if !self.section_due(false) {
                    return self.wait_for_section(false);
                }
                let Some((end, trailers)) = parse_trailers(
                    self.section(),
                    self.limits.max_header_count,
                    self.role == Role::Client,
                )?
                else {
                    return self.wait_for_section(false);
                };
                self.input.consume(end);
                self.cycle.progress = SectionProgress::default();
                self.finish_peer(trailers)
            }
        }
    }

    fn peer_data(
        &mut self,
        count: usize,
        chunk_start: bool,
        chunk_end: bool,
    ) -> Result<Polled, RemoteProtocolError> {
        self.advance_peer(StateEvent::Data)?;
        self.cycle.waiting_for_continue = false;
        Ok(Polled::Data {
            count,
            chunk_start,
            chunk_end,
        })
    }

    fn finish_peer(&mut self, trailers: Headers) -> Result<Polled, RemoteProtocolError> {
        self.advance_peer(StateEvent::EndOfMessage)?;
        self.cycle.reader = Reader::Head;
        Ok(Polled::Event(Event::EndOfMessage(EndOfMessage {
            trailers,
        })))
    }

    fn peer_closed(&mut self) -> Result<Polled, RemoteProtocolError> {
        self.advance_peer(StateEvent::ConnectionClosed)?;
        Ok(Polled::Event(Event::ConnectionClosed))
    }

    fn need_data_or_eof(&mut self, message: &str) -> Result<Polled, RemoteProtocolError> {
        if self.eof {
            Err(remote(message, Some(400)))
        } else {
            Ok(Polled::NeedData)
        }
    }

    fn apply_persistence(&mut self, version: Version, headers: &Headers) {
        let persistent = version == Version::Http11 && !has_token(headers, b"connection", b"close");
        if !persistent {
            self.state.disable_keep_alive();
        }
    }

    fn proposed_upgrades(&self, version: Version, headers: &Headers) -> Vec<Vec<u8>> {
        if version == Version::Http11 && has_token(headers, b"connection", b"upgrade") {
            upgrade_protocols(headers).unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    fn valid_upgrade_response(&self, version: Version, headers: &Headers) -> bool {
        version == Version::Http11
            && has_token(headers, b"connection", b"upgrade")
            && upgrade_protocols(headers).is_some_and(|selected| {
                !selected.is_empty()
                    && selected.iter().all(|selected| {
                        self.cycle
                            .upgrade_proposals
                            .iter()
                            .any(|offered| protocol_matches(offered, selected))
                    })
            })
    }

    /// Serializes a client request head and begins its body.
    ///
    /// # Errors
    ///
    /// Returns [`LocalProtocolError`] for the wrong role or state, an invalid
    /// request target or field, or inconsistent message framing.
    pub fn send_request(&mut self, request: &Request) -> Result<Vec<u8>, LocalProtocolError> {
        if self.role != Role::Client {
            return Err(local("only a client connection can send a request"));
        }
        validate_request(request)?;
        let framing = framing(&request.headers, request.http_version)
            .map_err(|error| local(error.message))?;
        // RFC 9110 Sections 9.3.6 and 9.3.8: CONNECT and TRACE requests have
        // no content.
        if framing != Framing::Fixed(0)
            && [Method::CONNECT, Method::TRACE].contains(&request.method)
        {
            return Err(local("CONNECT and TRACE requests cannot carry content"));
        }
        if framing == Framing::Fixed(0) && has_token(&request.headers, b"expect", b"100-continue") {
            return Err(local("100-continue requires request content"));
        }
        let upgrades =
            upgrade_protocols(&request.headers).ok_or_else(|| local("invalid Upgrade field"))?;
        if has_token(&request.headers, b"connection", b"upgrade") == upgrades.is_empty() {
            return Err(local("Upgrade requires Connection: upgrade"));
        }
        let upgrades = if request.http_version == Version::Http11 {
            upgrades
        } else {
            Vec::new()
        };
        let proposals = SwitchProposals::from_flags(
            request.method.as_bytes() == b"CONNECT",
            !upgrades.is_empty(),
        );
        self.advance_local(StateEvent::Request(proposals))?;
        self.apply_persistence(request.http_version, &request.headers);
        self.cycle.request_kind = Some(RequestKind::from_method(request.method.as_bytes()));
        self.cycle.writer = Some(framing);
        self.cycle.waiting_for_continue =
            expects_continue(request.http_version, &request.headers, framing);
        self.cycle.continue_before_upgrade =
            self.cycle.waiting_for_continue && !upgrades.is_empty();
        self.cycle.upgrade_proposals = upgrades;
        Ok(serialize_request(request))
    }

    /// Serializes a server informational response head.
    ///
    /// # Errors
    ///
    /// Returns [`LocalProtocolError`] for the wrong role or state, an invalid
    /// response, an HTTP/1.0 client, or a protocol switch that was not validly
    /// proposed.
    pub fn send_informational_response(
        &mut self,
        response: &InformationalResponse,
    ) -> Result<Vec<u8>, LocalProtocolError> {
        if self.role != Role::Server || !response.status.is_informational() {
            return Err(local(
                "an informational response requires a server role and 1xx status",
            ));
        }
        // RFC 9110 Section 15.2: HTTP/1.0 did not define any 1xx status codes.
        if self.peer_version == Some(Version::Http10) {
            return Err(local("a 1xx response cannot be sent to an HTTP/1.0 client"));
        }
        validate_headers(&response.headers)?;
        validate_reason(&response.reason)?;
        if has_framing_fields(&response.headers) {
            return Err(local("informational responses cannot carry framing fields"));
        }
        if response.status.as_u16() == 101
            && (self.cycle.continue_before_upgrade
                || !self.valid_upgrade_response(response.http_version, &response.headers))
        {
            return Err(local("invalid 101 Upgrade response"));
        }
        let event = if response.status.as_u16() == 101 {
            StateEvent::ProtocolSwitch(SwitchKind::Upgrade)
        } else {
            StateEvent::InformationalResponse
        };
        self.advance_local(event)?;
        if response.status.as_u16() == 100 {
            self.cycle.waiting_for_continue = false;
            self.cycle.continue_before_upgrade = false;
        }
        Ok(serialize_response(
            response.http_version,
            response.status,
            &response.reason,
            &response.headers,
        ))
    }

    /// Serializes a final server response head and begins its body.
    ///
    /// # Errors
    ///
    /// Returns [`LocalProtocolError`] for the wrong role or state, an invalid
    /// response, or framing forbidden by the request method or status.
    pub fn send_response(&mut self, response: &Response) -> Result<Vec<u8>, LocalProtocolError> {
        if self.role != Role::Server || response.status.is_informational() {
            return Err(local(
                "a final response requires a server role and non-1xx status",
            ));
        }
        validate_headers(&response.headers)?;
        validate_reason(&response.reason)?;
        let connect = self.opens_tunnel(response.status);
        if (response.status.as_u16() == 204 || connect) && has_framing_fields(&response.headers) {
            return Err(local("this response status cannot carry framing fields"));
        }
        let no_body = self.response_has_no_body(response.status);
        // Borrow the common path; normalization clones only when fields change.
        let mut headers = Cow::Borrowed(&response.headers);
        if (self.peer_version != Some(Version::Http11) || response.http_version != Version::Http11)
            && count_header(&headers, b"transfer-encoding") > 0
        {
            headers.to_mut().retain(|name, _| {
                !name.eq_ignore_ascii_case(b"transfer-encoding")
                    && !name.eq_ignore_ascii_case(b"content-length")
            });
        }
        let mut force_close = false;
        let framing = if no_body {
            Framing::Fixed(0)
        } else if response.status.as_u16() == 205 {
            // RFC 9110 Section 15.3.6: a 205 response has no content and
            // indicates that with a zero length or by closing.
            if !has_framing_fields(&headers) {
                headers.to_mut().push(b"Content-Length", b"0");
            }
            let framing =
                framing(&headers, response.http_version).map_err(|error| local(error.message))?;
            if framing != Framing::Fixed(0) {
                return Err(local("a 205 response cannot carry content"));
            }
            framing
        } else if !has_framing_fields(&headers) {
            if self.peer_version == Some(Version::Http11)
                && response.http_version == Version::Http11
            {
                headers.to_mut().push(b"Transfer-Encoding", b"chunked");
                Framing::Chunked
            } else {
                force_close = true;
                Framing::CloseDelimited
            }
        } else {
            framing(&headers, response.http_version).map_err(|error| local(error.message))?
        };
        if !connect
            && (!self.state.keep_alive()
                || self.peer_version != Some(Version::Http11)
                || response.http_version != Version::Http11
                || force_close
                // RFC 9110 Section 10.1.1: announce closure when responding
                // before the request content has been fully read. A request
                // whose content has all arrived only awaits its end event.
                || (self.peer_state() == State::SendBody
                    && !matches!(self.cycle.reader, Reader::Fixed(0)))
                || has_token(&headers, b"connection", b"close"))
        {
            set_connection_close(headers.to_mut());
        }
        self.advance_local(if connect {
            StateEvent::ProtocolSwitch(SwitchKind::Connect)
        } else {
            StateEvent::Response
        })?;
        self.apply_persistence(response.http_version, headers.as_ref());
        self.cycle.writer = Some(framing);
        self.cycle.waiting_for_continue = false;
        self.cycle.continue_before_upgrade = false;
        self.cycle.upgrade_proposals.clear();
        Ok(serialize_response(
            response.http_version,
            response.status,
            &response.reason,
            headers.as_ref(),
        ))
    }

    /// Serializes a complete client request: its head, `body`, and the end of
    /// the message.
    ///
    /// A request without `Content-Length` or `Transfer-Encoding` gets
    /// `Content-Length` when `body` is non-empty or the method defines
    /// content (`POST`, `PUT`, `PATCH`), as RFC 9110 Section 8.6 recommends.
    ///
    /// # Errors
    ///
    /// Returns [`LocalProtocolError`] under the conditions of
    /// [`Connection::send_request`], or if `body` does not match the request
    /// framing. The connection is unchanged after an error.
    pub fn send_request_with_body(
        &mut self,
        mut request: Request,
        body: &[u8],
    ) -> Result<Vec<u8>, LocalProtocolError> {
        let expects_content = [Method::POST, Method::PUT, Method::PATCH].contains(&request.method);
        if (expects_content || !body.is_empty()) && !has_framing_fields(&request.headers) {
            request
                .headers
                .push(b"Content-Length", body.len().to_string());
        }
        self.send_complete(|connection| connection.send_request(&request), body)
    }

    /// Serializes a complete final response: its head, `body`, and the end of
    /// the message.
    ///
    /// A response without `Content-Length` or `Transfer-Encoding` gets
    /// `Content-Length` unless its status or request method forbids content,
    /// so a known body is never chunked or close-delimited. A successful
    /// `CONNECT` response returns only its head, where the tunnel starts.
    ///
    /// # Errors
    ///
    /// Returns [`LocalProtocolError`] under the conditions of
    /// [`Connection::send_response`], or if `body` does not match the response
    /// framing, including any body for a response that cannot carry content.
    /// The connection is unchanged after an error.
    pub fn send_response_with_body(
        &mut self,
        mut response: Response,
        body: &[u8],
    ) -> Result<Vec<u8>, LocalProtocolError> {
        if !has_framing_fields(&response.headers) && !self.response_has_no_body(response.status) {
            response
                .headers
                .push(b"Content-Length", body.len().to_string());
        }
        self.send_complete(|connection| connection.send_response(&response), body)
    }

    /// Sends a head, `body`, and end of message as one write, restoring the
    /// connection if any step fails so no partial message is committed.
    fn send_complete(
        &mut self,
        head: impl FnOnce(&mut Self) -> Result<Vec<u8>, LocalProtocolError>,
        body: &[u8],
    ) -> Result<Vec<u8>, LocalProtocolError> {
        let saved = (self.state, self.cycle.clone());
        let result = head(self).and_then(|mut out| {
            // A successful CONNECT becomes a tunnel at the end of its head.
            if self.local_state() == State::SwitchedProtocol {
                return if body.is_empty() {
                    Ok(out)
                } else {
                    Err(local("a successful CONNECT response cannot carry content"))
                };
            }
            if !body.is_empty() {
                out.extend(self.send_data(body)?);
            }
            out.extend(self.end_of_message(&Headers::new())?);
            Ok(out)
        });
        if result.is_err() {
            (self.state, self.cycle) = saved;
        }
        result
    }

    /// Serializes body bytes according to the selected message framing.
    ///
    /// # Errors
    ///
    /// Returns [`LocalProtocolError`] if no message body is active, the current
    /// state forbids data, or the bytes exceed `Content-Length`.
    pub fn send_data(&mut self, data: &[u8]) -> Result<Vec<u8>, LocalProtocolError> {
        let parts = self.send_data_parts(data)?;
        let mut out = parts.prefix;
        out.extend_from_slice(parts.data);
        out.extend_from_slice(&parts.suffix);
        Ok(out)
    }

    /// Returns framing around the unchanged body object, avoiding a body copy.
    ///
    /// Write the returned prefix, body, and suffix in order without changing
    /// the body until all three writes complete.
    ///
    /// # Errors
    ///
    /// Returns [`LocalProtocolError`] under the same conditions as
    /// [`Connection::send_data`].
    pub fn send_data_parts<T: AsRef<[u8]>>(
        &mut self,
        data: T,
    ) -> Result<DataParts<T>, LocalProtocolError> {
        let (prefix, suffix) = self.send_data_framing(data.as_ref().len())?;
        Ok(DataParts {
            prefix,
            data,
            suffix,
        })
    }

    #[doc(hidden)]
    /// Returns framing bytes for a body object with `length` bytes.
    ///
    /// The caller must write exactly `length` body bytes between the returned
    /// prefix and suffix.
    ///
    /// # Errors
    ///
    /// Returns [`LocalProtocolError`] if the current message cannot accept a
    /// body object of that length.
    pub fn send_data_framing(
        &mut self,
        length: usize,
    ) -> Result<(Vec<u8>, Vec<u8>), LocalProtocolError> {
        let (next, prefix, suffix) = match self
            .cycle
            .writer
            .ok_or_else(|| local("message framing is not initialized"))?
        {
            Framing::Fixed(remaining) if length <= remaining => {
                (Framing::Fixed(remaining - length), Vec::new(), Vec::new())
            }
            Framing::Fixed(_) => return Err(local("body exceeds Content-Length")),
            Framing::CloseDelimited => (Framing::CloseDelimited, Vec::new(), Vec::new()),
            Framing::Chunked if length == 0 => (Framing::Chunked, Vec::new(), Vec::new()),
            Framing::Chunked => (
                Framing::Chunked,
                format!("{length:x}\r\n").into_bytes(),
                b"\r\n".to_vec(),
            ),
        };
        self.advance_local(StateEvent::Data)?;
        self.cycle.writer = Some(next);
        self.cycle.waiting_for_continue = false;
        Ok((prefix, suffix))
    }

    /// Finishes the local message and serializes any required terminator.
    ///
    /// Core fields that require header-time processing are rejected in
    /// trailers. Callers remain responsible for ensuring that extension field
    /// definitions permit trailer use.
    ///
    /// # Errors
    ///
    /// Returns [`LocalProtocolError`] if the body length is incomplete, the
    /// framing forbids trailers, a trailer field is invalid, or the state does
    /// not permit message completion.
    pub fn end_of_message(&mut self, trailers: &Headers) -> Result<Vec<u8>, LocalProtocolError> {
        validate_trailers(trailers)?;
        let bytes = match self
            .cycle
            .writer
            .ok_or_else(|| local("message framing is not initialized"))?
        {
            Framing::Fixed(0) | Framing::CloseDelimited if trailers.is_empty() => Vec::new(),
            Framing::Fixed(_) => return Err(local("body is shorter than Content-Length")),
            Framing::CloseDelimited => {
                return Err(local("close-delimited messages cannot carry trailers"));
            }
            Framing::Chunked => {
                let mut out = b"0\r\n".to_vec();
                write_headers(&mut out, trailers);
                out.extend_from_slice(b"\r\n");
                out
            }
        };
        self.advance_local(StateEvent::EndOfMessage)?;
        self.cycle.writer = None;
        self.cycle.waiting_for_continue = false;
        Ok(bytes)
    }

    /// Resets a completed exchange, including to drain messages buffered before EOF.
    ///
    /// # Errors
    ///
    /// Returns [`LocalProtocolError`] unless both actors completed a reusable
    /// HTTP exchange with no pending switch. A client also refuses reuse, and
    /// disables keep-alive, when peer bytes follow the complete response.
    pub fn start_next_cycle(&mut self) -> Result<(), LocalProtocolError> {
        // RFC 9112 Section 6.3: a client must not process bytes that follow
        // a complete response as a separate response.
        if self.role == Role::Client
            && self.peer_state() == State::Done
            && !self.input.as_slice().is_empty()
        {
            self.state.disable_keep_alive();
            return Err(local(
                "unsolicited bytes follow the response; the connection is not reusable",
            ));
        }
        self.state
            .start_next_cycle()
            .map_err(|_| local("connection is not reusable"))?;
        self.cycle = Cycle::default();
        Ok(())
    }

    /// Marks the local protocol actor as closed.
    ///
    /// # Errors
    ///
    /// Returns [`LocalProtocolError`] if closing is invalid in the current
    /// local state.
    pub fn close(&mut self) -> Result<(), LocalProtocolError> {
        self.advance_local(StateEvent::ConnectionClosed)?;
        self.cycle.writer = None;
        Ok(())
    }
}
