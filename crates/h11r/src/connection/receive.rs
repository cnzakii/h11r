//! Reading peer bytes into events.

use super::*;
use crate::state::StateEvent;
use crate::wire::{
    Chunk, Fields, Framing, Reader, SectionProgress, find_crlf, incomplete_line_len, local,
    only_empty_lines, parse_chunk_header, parse_request_head, parse_response_head, parse_trailers,
    reader, remote, start_line_unfinished, upgrade_protocols, valid_request_target,
};
use crate::{
    Data, EndOfMessage, Event, Headers, LocalProtocolError, NextEvent, RemoteProtocolError,
    Request, Response, Role, State, Version,
};

impl Connection {
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
            self.record_eof();
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
            self.record_eof();
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

    /// A server that closed cannot answer another request. A server keeps
    /// draining requests that a client buffered before its EOF.
    fn record_eof(&mut self) {
        self.eof = true;
        if self.role == Role::Client {
            self.state.disable_keep_alive();
        }
    }

    fn next_event_inner(&mut self) -> Result<Polled, RemoteProtocolError> {
        match self.peer_state() {
            State::Closed => return Ok(Polled::Event(Event::ConnectionClosed)),
            // RFC 9112 Section 2.2: empty lines before a message are ignored,
            // so they cannot turn a clean close into a truncated head. Lines
            // beyond the head limit still fail it, whatever the poll order.
            State::Idle
                if self.eof
                    && self.input.as_slice().len() <= self.limits.max_head_bytes
                    && only_empty_lines(self.input.as_slice()) =>
            {
                self.input.consume(self.input.as_slice().len());
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
            let Some((end, response, dropped)) =
                parse_response_head(self.section(), max_headers, self.lenient)?
            else {
                return self.wait_for_section(true);
            };
            // A dropped line may have been a framing field, so the next
            // message's boundary cannot be trusted.
            if dropped {
                self.state.disable_keep_alive();
            }
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
        if self.input.as_slice().len() <= self.limits.max_head_bytes {
            return self.need_data_or_eof(if skip_leading_empty {
                "unexpected EOF in HTTP head"
            } else {
                "unexpected EOF in trailers"
            });
        }
        if !skip_leading_empty {
            return Err(remote("HTTP trailers exceed max_head_bytes", Some(400)));
        }
        // RFC 9112 Section 3: a request line too long to parse is a 414.
        let status = if start_line_unfinished(self.section()) {
            414
        } else {
            431
        };
        Err(remote(
            "HTTP head exceeds max_head_bytes",
            (self.role == Role::Server).then_some(status),
        ))
    }

    fn accept_request(&mut self, request: Request) -> Result<Polled, RemoteProtocolError> {
        let version = request.http_version;
        if !valid_request_target(request.method.as_bytes(), &request.target) {
            return Err(remote("invalid request target", Some(400)));
        }
        let fields = Fields::scan(&request.headers);
        if !fields.valid_host(version) {
            return Err(remote(
                "request has a missing, duplicate, or invalid Host field",
                Some(400),
            ));
        }
        let framing = fields.framing(version, false)?;
        if content_forbidden(&request.method, framing) {
            return Err(remote(
                "CONNECT and TRACE requests cannot carry content",
                Some(400),
            ));
        }
        let upgrades = if version == Version::Http11 && fields.upgrade {
            upgrade_protocols(&request.headers).unwrap_or_default()
        } else {
            Vec::new()
        };
        self.advance_peer(request_event(&request.method, &upgrades))?;
        self.record_request(&request.method, version, &fields, framing, upgrades);
        self.peer_version = Some(version);
        self.cycle.reader = reader(framing);
        Ok(Polled::Event(Event::Request(request)))
    }

    fn accept_response(&mut self, response: Response) -> Result<Polled, RemoteProtocolError> {
        // RFC 9112 Section 9.2: data without an outstanding request is not a
        // valid response.
        if self.local_state() == State::Idle {
            return Err(remote(
                "response received without an outstanding request",
                None,
            ));
        }
        let (status, version) = (response.status, response.http_version);
        let fields = Fields::scan(&response.headers);
        let framing = fields.framing(version, true);
        if status.is_informational() {
            let event = self
                .informational_event(status, version, &response.headers)
                .ok_or_else(|| remote("invalid 101 Upgrade response", None))?;
            self.advance_peer(event)?;
            // RFC 9112 Sections 6.1 and 6.3: faulty framing fields end the
            // connection even on a message that has no content.
            if framing.is_err() {
                self.state.disable_keep_alive();
            }
            self.record_informational(status);
            self.peer_version = Some(version);
            return Ok(Polled::Event(Event::InformationalResponse(response)));
        }
        self.advance_peer(self.final_response_event(status))?;
        if !fields.persistent(version) {
            self.state.disable_keep_alive();
        }
        self.record_response();
        self.peer_version = Some(version);
        let framing = if self.response_has_no_body(status) {
            if framing.is_err() {
                self.state.disable_keep_alive();
            }
            Framing::Fixed(0)
        } else {
            framing?
        };
        if framing == Framing::CloseDelimited {
            self.state.disable_keep_alive();
        }
        self.cycle.reader = reader(framing);
        Ok(Polled::Event(Event::Response(response)))
    }

    fn read_chunked(&mut self, chunk: Chunk) -> Result<Polled, RemoteProtocolError> {
        match chunk {
            Chunk::Size { scanned } => {
                let Some(end) = find_crlf(self.input.as_slice(), scanned) else {
                    if incomplete_line_len(self.input.as_slice()) > self.limits.max_head_bytes {
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
}
