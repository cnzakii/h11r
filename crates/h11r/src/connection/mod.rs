//! HTTP/1.1 connection lifecycle and semantic event flow.

mod receive;
mod send;

use crate::state::{ConnectionState, StateEvent, SwitchKind, SwitchProposals};
use crate::wire::{
    Buffer, Fields, Framing, Reader, SectionProgress, has_token, local, protocol_matches, remote,
    upgrade_protocols,
};
use crate::{
    Event, Headers, Limits, LocalProtocolError, Method, RemoteProtocolError, Role, State,
    StatusCode, Version,
};

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
    lenient: bool,
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
            lenient: false,
            input: Buffer::default(),
            eof: false,
            peer_version: None,
            cycle: Cycle::default(),
        }
    }

    /// Accepts response heads that break HTTP syntax in ways mainstream
    /// clients tolerate, where doing so cannot change message framing.
    ///
    /// By default the connection is as strict as RFC 9110 and RFC 9112
    /// allow. A lenient connection additionally, for received responses
    /// only:
    ///
    /// - drops field lines that are not valid fields, such as a name
    ///   containing `/`, instead of failing;
    /// - accepts whitespace between a field name and its colon;
    /// - accepts more than one space between status-line elements.
    ///
    /// A connection that drops a field line is not reused, since the line
    /// may have framed the message.
    ///
    /// Requests are never parsed leniently, because a server that tolerates
    /// malformed requests can be led to frame them differently from another
    /// server on the path.
    #[must_use]
    pub const fn lenient(mut self) -> Self {
        self.lenient = true;
        self
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

    /// The event for a final response: CONNECT success switches protocols.
    fn final_response_event(&self, status: StatusCode) -> StateEvent {
        if self.opens_tunnel(status) {
            StateEvent::ProtocolSwitch(SwitchKind::Connect)
        } else {
            StateEvent::Response
        }
    }

    /// The event for a 1xx response, or `None` for a 101 that does not
    /// select an offered upgrade.
    fn informational_event(
        &self,
        status: StatusCode,
        version: Version,
        headers: &Headers,
    ) -> Option<StateEvent> {
        if status.as_u16() != 101 {
            return Some(StateEvent::InformationalResponse);
        }
        (!self.cycle.continue_before_upgrade && self.valid_upgrade_response(version, headers))
            .then_some(StateEvent::ProtocolSwitch(SwitchKind::Upgrade))
    }

    // The `record_*` methods update the cycle after a message, sent or
    // received, has been accepted by the state machine.

    fn record_request(
        &mut self,
        method: &Method,
        version: Version,
        fields: &Fields,
        framing: Framing,
        upgrades: Vec<Vec<u8>>,
    ) {
        if !fields.persistent(version) {
            self.state.disable_keep_alive();
        }
        self.cycle.request_kind = Some(RequestKind::from_method(method.as_bytes()));
        self.cycle.waiting_for_continue = fields.expects_continue(version, framing);
        self.cycle.continue_before_upgrade =
            self.cycle.waiting_for_continue && !upgrades.is_empty();
        self.cycle.upgrade_proposals = upgrades;
    }

    fn record_informational(&mut self, status: StatusCode) {
        if status.as_u16() == 100 {
            self.cycle.waiting_for_continue = false;
            self.cycle.continue_before_upgrade = false;
        }
    }

    fn record_response(&mut self) {
        self.cycle.waiting_for_continue = false;
        self.cycle.continue_before_upgrade = false;
        self.cycle.upgrade_proposals.clear();
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

/// The event that starts a request, proposing any switch it offers.
fn request_event(method: &Method, upgrades: &[Vec<u8>]) -> StateEvent {
    StateEvent::Request(SwitchProposals::from_flags(
        *method == Method::CONNECT,
        !upgrades.is_empty(),
    ))
}

/// RFC 9110 Sections 9.3.6 and 9.3.8: CONNECT and TRACE requests have no
/// content.
fn content_forbidden(method: &Method, framing: Framing) -> bool {
    framing != Framing::Fixed(0) && [Method::CONNECT, Method::TRACE].contains(method)
}
