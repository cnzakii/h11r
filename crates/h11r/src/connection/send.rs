//! Serializing local events into bytes.

use super::*;
use crate::state::StateEvent;
use crate::wire::{
    Fields, Framing, Reader, has_framing_fields, local, serialize_request, serialize_response,
    set_connection_option, upgrade_protocols, validate_headers, validate_reason, validate_request,
    validate_trailers, write_headers,
};
use crate::{
    DataParts, DataPrefix, Headers, InformationalResponse, LocalProtocolError, Method, Request,
    Response, Role, State, Version,
};

impl Connection {
    /// Serializes a client request head and begins its body.
    ///
    /// # Errors
    ///
    /// Returns [`LocalProtocolError`] for the wrong role or state, an invalid
    /// request target or field, or inconsistent message framing.
    pub fn send_request(&mut self, request: Request) -> Result<Vec<u8>, LocalProtocolError> {
        if self.role != Role::Client {
            return Err(local("only a client connection can send a request"));
        }
        // RFC 9112 Section 9.2: bytes that arrived with no outstanding
        // request are not a response, so they cannot answer this one.
        if !self.input.as_slice().is_empty() {
            return Err(local("the peer sent bytes before any request"));
        }
        let fields = Fields::scan(&request.headers);
        validate_request(&request, &fields)?;
        let framing = fields
            .framing(request.http_version, false)
            .map_err(|error| local(error.message))?;
        if content_forbidden(&request.method, framing) {
            return Err(local("CONNECT and TRACE requests cannot carry content"));
        }
        if framing == Framing::Fixed(0) && fields.expect_continue {
            return Err(local("100-continue requires request content"));
        }
        let upgrades =
            upgrade_protocols(&request.headers).ok_or_else(|| local("invalid Upgrade field"))?;
        if fields.upgrade && upgrades.is_empty() {
            return Err(local("Connection: upgrade requires an Upgrade field"));
        }
        let upgrades = if request.http_version == Version::Http11 {
            upgrades
        } else {
            Vec::new()
        };
        self.advance_local(request_event(&request.method, &upgrades))?;
        self.record_request(
            &request.method,
            request.http_version,
            &fields,
            framing,
            upgrades,
        );
        self.cycle.writer = Some(framing);
        Ok(serialize_request(&request, fields.host_at))
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
        response: InformationalResponse,
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
        Fields::scan(&response.headers).check_send()?;
        if has_framing_fields(&response.headers) {
            return Err(local("informational responses cannot carry framing fields"));
        }
        let event = self
            .informational_event(response.status, response.http_version, &response.headers)
            .ok_or_else(|| local("invalid 101 Upgrade response"))?;
        self.advance_local(event)?;
        self.record_informational(response.status);
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
    pub fn send_response(&mut self, response: Response) -> Result<Vec<u8>, LocalProtocolError> {
        let Response {
            status,
            reason,
            mut headers,
            http_version: version,
        } = response;
        if self.role != Role::Server || status.is_informational() {
            return Err(local(
                "a final response requires a server role and non-1xx status",
            ));
        }
        validate_headers(&headers)?;
        validate_reason(&reason)?;
        let fields = Fields::scan(&headers);
        fields.check_send()?;
        // RFC 9110 Section 7.8: a 426 names the protocols to upgrade to.
        if status.as_u16() == 426
            && upgrade_protocols(&headers).is_none_or(|protocols| protocols.is_empty())
        {
            return Err(local("a 426 response requires an Upgrade field"));
        }
        let connect = self.opens_tunnel(status);
        if (status.as_u16() == 204 || connect) && fields.has_framing() {
            return Err(local("this response status cannot carry framing fields"));
        }
        let http11 = self.peer_version == Some(Version::Http11) && version == Version::Http11;
        // Bodyless framing fields describe the response a GET would get, so
        // they need only be valid response framing; content this connection
        // sends must use a framing it can write.
        let bodyless = self.response_has_no_body(status);
        let declared_framing = |version| {
            fields
                .framing(version, bodyless)
                .map_err(|error| local(error.message))
        };
        // A chunked response to an HTTP/1.0 peer is sent close-delimited,
        // once its framing has been checked as it would be for HTTP/1.1.
        let stripped = !http11 && fields.has_transfer_encoding();
        if stripped {
            declared_framing(Version::Http11)?;
            headers.retain(|name, _| !name.eq_ignore_ascii_case(b"transfer-encoding"));
        }
        let declared = fields.has_framing() && !stripped;
        let framing = if bodyless {
            if declared {
                declared_framing(version)?;
            }
            Framing::Fixed(0)
        } else if status.as_u16() == 205 {
            // RFC 9110 Section 15.3.6: a 205 response has no content and
            // indicates that with a zero length or by closing.
            if !declared {
                headers.push(b"Content-Length", b"0");
            } else if declared_framing(version)? != Framing::Fixed(0) {
                return Err(local("a 205 response cannot carry content"));
            }
            Framing::Fixed(0)
        } else if !declared {
            if http11 {
                headers.push(b"Transfer-Encoding", b"chunked");
                Framing::Chunked
            } else {
                Framing::CloseDelimited
            }
        } else {
            declared_framing(version)?
        };
        self.advance_local(self.final_response_event(status))?;
        // Every reason to close is recorded in the state first, so the head
        // states exactly whether the connection persists. RFC 9110 Section
        // 10.1.1: responding before the request content has been fully read
        // is one; content that has all arrived and awaits only its events is
        // not.
        let unread = match self.cycle.reader {
            Reader::Fixed(remaining) => remaining > self.input.as_slice().len(),
            _ => true,
        };
        if framing == Framing::CloseDelimited
            || (self.peer_state() == State::SendBody && unread)
            || fields.close
        {
            self.state.disable_keep_alive();
        }
        self.record_response();
        if !connect {
            if !self.state.keep_alive() {
                set_connection_option(&mut headers, b"close");
            } else if !http11 {
                // RFC 9112 Section 9.3: an HTTP/1.0 recipient persists only
                // when the response carries the legacy option.
                set_connection_option(&mut headers, b"keep-alive");
            }
        }
        // RFC 9110 Section 9.3.6: a tunnel replaces the message body.
        self.cycle.writer = (!connect).then_some(framing);
        Ok(serialize_response(version, status, &reason, &headers))
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
        request: Request,
        body: &[u8],
    ) -> Result<Vec<u8>, LocalProtocolError> {
        let framing = self.send_request_with_body_framing(request, body.len())?;
        Ok(join_framing(framing, body))
    }

    #[doc(hidden)]
    /// Returns the bytes before and after a body of `length` bytes for
    /// [`Connection::send_request_with_body`], so a binding can write the
    /// body straight into its own buffer.
    ///
    /// # Errors
    ///
    /// As [`Connection::send_request_with_body`].
    pub fn send_request_with_body_framing(
        &mut self,
        mut request: Request,
        length: usize,
    ) -> Result<(Vec<u8>, Vec<u8>), LocalProtocolError> {
        let expects_content = [Method::POST, Method::PUT, Method::PATCH].contains(&request.method);
        if (expects_content || length != 0) && !has_framing_fields(&request.headers) {
            request.headers.push(b"Content-Length", length.to_string());
        }
        self.send_complete(|connection| connection.send_request(request), length)
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
        response: Response,
        body: &[u8],
    ) -> Result<Vec<u8>, LocalProtocolError> {
        let framing = self.send_response_with_body_framing(response, body.len())?;
        Ok(join_framing(framing, body))
    }

    #[doc(hidden)]
    /// Returns the bytes before and after a body of `length` bytes for
    /// [`Connection::send_response_with_body`], so a binding can write the
    /// body straight into its own buffer.
    ///
    /// # Errors
    ///
    /// As [`Connection::send_response_with_body`].
    pub fn send_response_with_body_framing(
        &mut self,
        mut response: Response,
        length: usize,
    ) -> Result<(Vec<u8>, Vec<u8>), LocalProtocolError> {
        if !has_framing_fields(&response.headers) && !self.response_has_no_body(response.status) {
            response.headers.push(b"Content-Length", length.to_string());
        }
        self.send_complete(|connection| connection.send_response(response), length)
    }

    /// Frames a head, a body of `length` bytes, and the end of message as
    /// one write, restoring the connection if any step fails so no partial
    /// message is committed.
    fn send_complete(
        &mut self,
        head: impl FnOnce(&mut Self) -> Result<Vec<u8>, LocalProtocolError>,
        length: usize,
    ) -> Result<(Vec<u8>, Vec<u8>), LocalProtocolError> {
        let saved = (self.state, self.cycle.clone());
        let result = head(self).and_then(|mut before| {
            // A successful CONNECT becomes a tunnel at the end of its head.
            if self.local_state() == State::SwitchedProtocol {
                return if length == 0 {
                    Ok((before, Vec::new()))
                } else {
                    Err(local("a successful CONNECT response cannot carry content"))
                };
            }
            let mut after = Vec::new();
            if length != 0 {
                let (prefix, suffix) = self.send_data_framing(length)?;
                before.extend_from_slice(&prefix);
                after.extend_from_slice(suffix);
            }
            after.extend(self.end_of_message(Headers::new())?);
            Ok((before, after))
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
        let (prefix, suffix) = self.send_data_framing(data.len())?;
        Ok([&prefix, data, suffix].concat())
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
    ) -> Result<(DataPrefix, &'static [u8]), LocalProtocolError> {
        let (next, prefix, suffix): (_, _, &'static [u8]) = match self
            .cycle
            .writer
            .ok_or_else(|| local("no message body is being sent"))?
        {
            Framing::Fixed(remaining) if length <= remaining => (
                Framing::Fixed(remaining - length),
                DataPrefix::default(),
                b"",
            ),
            Framing::Fixed(_) => return Err(local("body exceeds Content-Length")),
            Framing::CloseDelimited => (Framing::CloseDelimited, DataPrefix::default(), b""),
            Framing::Chunked if length == 0 => (Framing::Chunked, DataPrefix::default(), b""),
            Framing::Chunked => (Framing::Chunked, DataPrefix::chunk_size(length), b"\r\n"),
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
    pub fn end_of_message(&mut self, trailers: Headers) -> Result<Vec<u8>, LocalProtocolError> {
        validate_trailers(&trailers)?;
        let bytes = match self
            .cycle
            .writer
            .ok_or_else(|| local("no message body is being sent"))?
        {
            Framing::Fixed(0) | Framing::CloseDelimited if trailers.is_empty() => Vec::new(),
            Framing::Fixed(0) | Framing::CloseDelimited => {
                return Err(local("only chunked messages can carry trailers"));
            }
            Framing::Fixed(_) => return Err(local("body is shorter than Content-Length")),
            Framing::Chunked => {
                let mut out = b"0\r\n".to_vec();
                write_headers(&mut out, &trailers);
                out.extend_from_slice(b"\r\n");
                out
            }
        };
        self.advance_local(StateEvent::EndOfMessage)?;
        self.cycle.writer = None;
        self.cycle.waiting_for_continue = false;
        Ok(bytes)
    }
}

/// Joins framing around `body`, copying the body once.
fn join_framing((mut out, after): (Vec<u8>, Vec<u8>), body: &[u8]) -> Vec<u8> {
    out.reserve_exact(body.len() + after.len());
    out.extend_from_slice(body);
    out.extend_from_slice(&after);
    out
}
