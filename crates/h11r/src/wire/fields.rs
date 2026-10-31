//! Field syntax and the semantics of framing and connection fields.

use super::*;
use crate::method::is_token_byte;
use crate::{Headers, LocalProtocolError, RemoteProtocolError, Version};

pub(crate) fn has_field(headers: &Headers, name: &[u8]) -> bool {
    headers.get_all(name).next().is_some()
}

/// Whether `headers` carry `Transfer-Encoding` or `Content-Length`.
pub(crate) fn has_framing_fields(headers: &Headers) -> bool {
    has_field(headers, b"transfer-encoding") || has_field(headers, b"content-length")
}

pub(crate) fn has_token(headers: &Headers, name: &[u8], token: &[u8]) -> bool {
    headers
        .get_all(name)
        .flat_map(|value| elements(value, false))
        .any(|value| value.eq_ignore_ascii_case(token))
}

pub(crate) fn upgrade_protocols(headers: &Headers) -> Option<Vec<Vec<u8>>> {
    let present = has_field(headers, b"upgrade");
    let protocols = headers
        .get_all(b"upgrade")
        .flat_map(|value| elements(value, false))
        .filter(|value| !value.is_empty())
        .map(|value| valid_protocol(value).then(|| value.to_vec()))
        .collect::<Option<Vec<_>>>()?;
    (!present || !protocols.is_empty()).then_some(protocols)
}

pub(crate) fn protocol_matches(left: &[u8], right: &[u8]) -> bool {
    let mut left = left.splitn(2, |byte| *byte == b'/');
    let mut right = right.splitn(2, |byte| *byte == b'/');
    left.next()
        .zip(right.next())
        .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
        && left.next() == right.next()
}

fn valid_protocol(value: &[u8]) -> bool {
    let mut parts = value.split(|byte| *byte == b'/');
    parts.next().is_some_and(valid_name)
        && parts.next().is_none_or(valid_name)
        && parts.next().is_none()
}

/// Replaces any persistence option in `Connection` with `option`, `close` or
/// `keep-alive`, keeping the other options.
pub(crate) fn set_connection_option(headers: &mut Headers, option: &[u8]) {
    let mut options = headers
        .get_all(b"connection")
        .flat_map(|value| elements(value, false))
        .filter(|value| {
            !value.is_empty()
                && !value.eq_ignore_ascii_case(b"close")
                && !value.eq_ignore_ascii_case(b"keep-alive")
        })
        .map(Vec::from)
        .collect::<Vec<_>>();
    options.push(option.to_vec());
    headers.retain(|name, _| !name.eq_ignore_ascii_case(b"connection"));
    headers.push(b"Connection", options.join(&b", "[..]));
}

/// The framing and connection-management fields of a section, read in one
/// pass.
#[derive(Default)]
pub(crate) struct Fields {
    hosts: usize,
    valid_host: bool,
    pub(crate) host_at: usize,
    codings: Option<Codings>,
    length: Option<Result<usize, &'static str>>,
    lengths: usize,
    pub(crate) close: bool,
    keep_alive: bool,
    pub(crate) upgrade: bool,
    connection_te: bool,
    pub(crate) expect_continue: bool,
    upgrade_field: bool,
    te_field: bool,
    te_chunked: bool,
    empty_elements: bool,
}

/// What a `Transfer-Encoding` list says about chunked framing.
#[derive(Default)]
struct Codings {
    any: bool,
    empty: bool,
    malformed: bool,
    last_chunked: bool,
    chunked_before_last: bool,
    other_before_last: bool,
}

impl Fields {
    pub(crate) fn scan(headers: &Headers) -> Self {
        let mut fields = Self::default();
        for (at, (name, value)) in headers.iter().enumerate() {
            // Matching the length first keeps unrelated fields to one compare.
            match name.len() {
                2 if name.eq_ignore_ascii_case(b"te") => {
                    fields.te_field = true;
                    fields.te_chunked |= elements(value, true)
                        .any(|element| token_name(element).eq_ignore_ascii_case(b"chunked"));
                    fields.note_empty(value, true);
                }
                4 if name.eq_ignore_ascii_case(b"host") => {
                    fields.hosts += 1;
                    fields.valid_host = authority_port(value).is_some();
                    fields.host_at = at;
                }
                6 if name.eq_ignore_ascii_case(b"expect") => {
                    fields.expect_continue |= elements(value, true)
                        .any(|element| element.eq_ignore_ascii_case(b"100-continue"));
                    fields.note_empty(value, true);
                }
                7 if name.eq_ignore_ascii_case(b"upgrade") => {
                    fields.upgrade_field = true;
                    fields.note_empty(value, false);
                }
                10 if name.eq_ignore_ascii_case(b"connection") => {
                    for option in elements(value, false) {
                        fields.close |= option.eq_ignore_ascii_case(b"close");
                        fields.keep_alive |= option.eq_ignore_ascii_case(b"keep-alive");
                        fields.upgrade |= option.eq_ignore_ascii_case(b"upgrade");
                        fields.connection_te |= option.eq_ignore_ascii_case(b"te");
                    }
                    fields.note_empty(value, false);
                }
                14 if name.eq_ignore_ascii_case(b"content-length") => {
                    for element in elements(value, false) {
                        fields.lengths += 1;
                        let parsed = parse_number(element, 10);
                        fields.length = Some(match fields.length {
                            None => parsed.ok_or("invalid Content-Length"),
                            Some(Ok(first)) if parsed == Some(first) => Ok(first),
                            Some(Ok(_)) => Err("conflicting Content-Length fields"),
                            Some(error) => error,
                        });
                    }
                }
                17 if name.eq_ignore_ascii_case(b"transfer-encoding") => {
                    fields.codings.get_or_insert_default().read(value);
                }
                _ => {}
            }
        }
        fields
    }

    /// RFC 9110 Section 5.6.1: an empty value is a list of no elements, not
    /// an empty element.
    fn note_empty(&mut self, value: &[u8], quoting: bool) {
        self.empty_elements |=
            !trim_ows(value).is_empty() && elements(value, quoting).any(<[u8]>::is_empty);
    }

    /// Field rules a sender must meet beyond what a recipient tolerates.
    ///
    /// RFC 9110 Section 5.6.1.1: no empty list elements. Section 8.6: one
    /// Content-Length value. Section 7.8: Upgrade comes with its connection
    /// option. RFC 9112 Section 7.4: TE never names chunked and comes with
    /// its connection option.
    pub(crate) fn check_send(&self) -> Result<(), LocalProtocolError> {
        if self.empty_elements || self.codings.as_ref().is_some_and(|codings| codings.empty) {
            return Err(local("a connection-level list field has an empty element"));
        }
        if self.lengths > 1 {
            return Err(local("Content-Length must be a single decimal value"));
        }
        if self.upgrade_field && !self.upgrade {
            return Err(local("Upgrade requires Connection: upgrade"));
        }
        if self.te_chunked {
            return Err(local("TE cannot name the chunked coding"));
        }
        if self.te_field && !self.connection_te {
            return Err(local("TE requires Connection: TE"));
        }
        Ok(())
    }

    pub(crate) fn has_framing(&self) -> bool {
        self.codings.is_some() || self.length.is_some()
    }

    pub(crate) fn has_transfer_encoding(&self) -> bool {
        self.codings.is_some()
    }

    /// RFC 9112 Section 3.2: one valid Host, which HTTP/1.0 may omit.
    pub(crate) fn valid_host(&self, version: Version) -> bool {
        match self.hosts {
            0 => version == Version::Http10,
            1 => self.valid_host,
            _ => false,
        }
    }

    /// RFC 9112 Section 9.3: HTTP/1.1 persists unless `close` is sent, and
    /// HTTP/1.0 persists only with the legacy `keep-alive` option.
    pub(crate) fn persistent(&self, version: Version) -> bool {
        !self.close && (version == Version::Http11 || self.keep_alive)
    }

    pub(crate) fn expects_continue(&self, version: Version, framing: Framing) -> bool {
        version == Version::Http11 && framing != Framing::Fixed(0) && self.expect_continue
    }

    /// RFC 9112 Section 6.3: the message body length. A received response
    /// without a determinable length ends at close; a request without one
    /// has none.
    pub(crate) fn framing(
        &self,
        version: Version,
        response: bool,
    ) -> Result<Framing, RemoteProtocolError> {
        match (&self.codings, self.length) {
            (Some(_), Some(_)) => Err(remote(
                "Transfer-Encoding and Content-Length cannot be combined",
                Some(400),
            )),
            (Some(_), None) if version == Version::Http10 => Err(remote(
                "Transfer-Encoding is not valid HTTP/1.0 framing",
                Some(400),
            )),
            (Some(codings), None) => codings.framing(response),
            (None, Some(length)) => length
                .map(Framing::Fixed)
                .map_err(|message| remote(message, Some(400))),
            (None, None) if response => Ok(Framing::CloseDelimited),
            (None, None) => Ok(Framing::Fixed(0)),
        }
    }
}

impl Codings {
    /// Reads `transfer-coding *( OWS ";" OWS transfer-parameter )` list
    /// elements, so commas inside quoted parameters do not split codings.
    /// https://www.rfc-editor.org/rfc/rfc9110.html#section-10.1.4
    fn read(&mut self, value: &[u8]) {
        let mut at = 0;
        loop {
            let start = skip_ows(value, at);
            // RFC 9110 Section 5.6.1.2: empty list elements are ignored.
            if matches!(value.get(start), None | Some(b',')) {
                self.empty = true;
                if start == value.len() {
                    return;
                }
                at = start + 1;
                continue;
            }
            at = token_end(value, start);
            let name = &value[start..at];
            let chunked = name.eq_ignore_ascii_case(b"chunked");
            let mut valid = at > start;
            let mut parameters = false;
            at = skip_ows(value, at);
            while valid && value.get(at) == Some(&b';') {
                parameters = true;
                let name = skip_ows(value, at + 1);
                at = skip_ows(value, token_end(value, name));
                valid = at > name && value.get(at) == Some(&b'=');
                let parameter = skip_ows(value, at + 1);
                at = if value.get(parameter) == Some(&b'"') {
                    parse_quoted_string(value, parameter).unwrap_or(parameter)
                } else {
                    token_end(value, parameter)
                };
                valid &= at > parameter;
                at = skip_ows(value, at);
            }
            // RFC 9112 Sections 7.1 and 7.2: the registered codings define
            // no parameters, so their presence is an error.
            valid &= !(parameters && is_registered_coding(name))
                && matches!(value.get(at), None | Some(b','));
            if self.any {
                if self.last_chunked {
                    self.chunked_before_last = true;
                } else {
                    self.other_before_last = true;
                }
            }
            self.any = true;
            self.malformed |= !valid;
            self.last_chunked = chunked && valid;
            if !valid || at == value.len() {
                return;
            }
            at += 1;
        }
    }

    /// RFC 9112 Section 6.3 item 4: a response whose final coding is not
    /// chunked ends at close; a request without a final chunked coding is a
    /// 400. Section 6.1: an unsupported request coding is a 501.
    fn framing(&self, response: bool) -> Result<Framing, RemoteProtocolError> {
        if self.malformed || !self.any {
            return Err(remote("invalid Transfer-Encoding field", Some(400)));
        }
        if response && !self.last_chunked {
            return Ok(Framing::CloseDelimited);
        }
        if !self.last_chunked || self.chunked_before_last {
            return Err(remote(
                "chunked must be the final transfer coding",
                Some(400),
            ));
        }
        if self.other_before_last && !response {
            return Err(remote(
                "only Transfer-Encoding: chunked is supported",
                Some(501),
            ));
        }
        Ok(Framing::Chunked)
    }
}

/// Comma-separated list elements without surrounding whitespace. When the
/// field's grammar has quoted strings (`quoting`), a comma in one does not
/// separate elements; elsewhere a quote is an ordinary byte.
fn elements(value: &[u8], quoting: bool) -> impl Iterator<Item = &[u8]> {
    let (mut quoted, mut escaped) = (false, false);
    value
        .split(move |byte| {
            match *byte {
                _ if escaped => escaped = false,
                b'\\' if quoted => escaped = true,
                b'"' if quoting => quoted = !quoted,
                b',' => return !quoted,
                _ => {}
            }
            false
        })
        .map(trim_ows)
}

fn is_registered_coding(name: &[u8]) -> bool {
    [
        b"chunked".as_slice(),
        b"compress",
        b"deflate",
        b"gzip",
        b"x-compress",
        b"x-gzip",
    ]
    .iter()
    .any(|coding| name.eq_ignore_ascii_case(coding))
}

/// The token that starts `element`, before any parameters.
fn token_name(element: &[u8]) -> &[u8] {
    &element[..token_end(element, 0)]
}

fn token_end(value: &[u8], start: usize) -> usize {
    value[start.min(value.len())..]
        .iter()
        .position(|byte| !is_token_byte(*byte))
        .map_or(value.len(), |end| start + end)
}

pub(crate) fn parse_quoted_string(value: &[u8], mut at: usize) -> Option<usize> {
    at += 1;
    while let Some(&byte) = value.get(at) {
        match byte {
            b'"' => return Some(at + 1),
            b'\\' => {
                at += 1;
                if !value.get(at).is_some_and(|byte| is_line_value_byte(*byte)) {
                    return None;
                }
            }
            b'\t' | b' ' | b'!' | b'#'..=b'[' | b']'..=b'~' | 0x80..=0xff => {}
            _ => return None,
        }
        at += 1;
    }
    None
}

pub(crate) fn skip_ows(value: &[u8], mut at: usize) -> usize {
    while value
        .get(at)
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        at += 1;
    }
    at
}

pub(crate) fn validate_headers(headers: &Headers) -> Result<(), LocalProtocolError> {
    if headers
        .iter()
        .any(|(name, value)| !valid_name(name) || !valid_value(value))
    {
        Err(local("invalid HTTP header field"))
    } else {
        Ok(())
    }
}

pub(crate) fn validate_reason(reason: &[u8]) -> Result<(), LocalProtocolError> {
    if reason.iter().copied().all(is_line_value_byte) {
        Ok(())
    } else {
        Err(local("invalid HTTP reason phrase"))
    }
}

pub(crate) fn valid_name(name: &[u8]) -> bool {
    !name.is_empty() && name.iter().copied().all(is_token_byte)
}

pub(crate) fn valid_value(value: &[u8]) -> bool {
    value.is_empty()
        || (value.first().is_some_and(|byte| is_field_vchar(*byte))
            && value.last().is_some_and(|byte| is_field_vchar(*byte))
            // A non-short-circuiting fold lets the compiler vectorize the scan.
            && value
                .iter()
                .fold(true, |valid, byte| valid & is_line_value_byte(*byte)))
}

pub(crate) fn is_line_value_byte(byte: u8) -> bool {
    matches!(byte, b'\t' | b' '..=b'~' | 0x80..=0xff)
}

fn is_field_vchar(byte: u8) -> bool {
    matches!(byte, b'!'..=b'~' | 0x80..=0xff)
}

pub(crate) fn validate_trailers(trailers: &Headers) -> Result<(), LocalProtocolError> {
    validate_headers(trailers)?;
    if trailers.iter().any(|(name, _)| {
        [
            b"content-length".as_slice(),
            b"transfer-encoding",
            b"host",
            b"connection",
            b"trailer",
            b"upgrade",
            b"expect",
            b"te",
        ]
        .iter()
        .any(|forbidden| name.eq_ignore_ascii_case(forbidden))
    }) {
        return Err(local("field is not permitted in an HTTP trailer"));
    }
    Ok(())
}

pub(crate) fn trim_ows(mut value: &[u8]) -> &[u8] {
    while value
        .first()
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        value = &value[1..];
    }
    while value
        .last()
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        value = &value[..value.len() - 1];
    }
    value
}
