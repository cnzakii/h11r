//! Request-target and Host authority validation.

use super::*;
use crate::{LocalProtocolError, Request, Version};
use std::net::Ipv6Addr;

pub(crate) fn validate_request(
    request: &Request,
    fields: &Fields,
) -> Result<(), LocalProtocolError> {
    if !valid_request_target(request.method.as_bytes(), &request.target) {
        return Err(local("invalid request target for the request method"));
    }
    validate_headers(&request.headers)?;
    fields.check_send()?;
    // RFC 9110 Section 7.2: a user agent generates Host in every version.
    if !fields.valid_host(Version::Http11) {
        return Err(local(
            "request has a missing, duplicate, or invalid Host field",
        ));
    }
    Ok(())
}

/// Checks the request-target form allowed for `method`.
///
/// CONNECT uses only authority-form with a valid port, asterisk-form is only
/// for OPTIONS, and other requests use origin-form or absolute-form. No form
/// has a fragment.
/// https://www.rfc-editor.org/rfc/rfc9112.html#section-3.2
/// https://www.rfc-editor.org/rfc/rfc9110.html#section-9.3.6
pub(crate) fn valid_request_target(method: &[u8], target: &[u8]) -> bool {
    if target.is_empty()
        || !target
            .iter()
            .all(|byte| byte.is_ascii_graphic() && *byte != b'#')
    {
        return false;
    }
    match method {
        b"CONNECT" => authority_port(target).flatten().is_some_and(valid_tcp_port),
        _ if target == b"*" => method == b"OPTIONS",
        _ => target.starts_with(b"/") || has_scheme(target),
    }
}

/// Whether `port` names a TCP port that a tunnel can connect to: 1 to 65535.
fn valid_tcp_port(port: &[u8]) -> bool {
    std::str::from_utf8(port)
        .ok()
        .and_then(|port| port.parse::<u16>().ok())
        .is_some_and(|port| port != 0)
}

/// Whether `target` begins with an RFC 3986 `scheme ":"`.
fn has_scheme(target: &[u8]) -> bool {
    target.first().is_some_and(u8::is_ascii_alphabetic)
        && split_at_byte(target, b':').is_some_and(|(scheme, _)| {
            scheme
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
        })
}

/// Parses `uri-host [ ":" port ]`, returning the port when one is present.
pub(crate) fn authority_port(value: &[u8]) -> Option<Option<&[u8]>> {
    let port = if value.starts_with(b"[") {
        let close = value.iter().position(|byte| *byte == b']')?;
        if !valid_ip_literal(&value[1..close]) {
            return None;
        }
        match &value[close + 1..] {
            b"" => None,
            rest => Some(rest.strip_prefix(b":")?),
        }
    } else {
        if value.iter().filter(|byte| **byte == b':').count() > 1 {
            return None;
        }
        let (host, port) =
            split_at_byte(value, b':').map_or((value, None), |(host, port)| (host, Some(port)));
        if !valid_reg_name(host) {
            return None;
        }
        port
    };
    port.is_none_or(|port| port.iter().all(u8::is_ascii_digit))
        .then_some(port)
}

fn valid_ip_literal(value: &[u8]) -> bool {
    std::str::from_utf8(value)
        .ok()
        .and_then(|value| value.parse::<Ipv6Addr>().ok())
        .is_some()
        || (value
            .first()
            .is_some_and(|byte| matches!(byte, b'v' | b'V'))
            && value
                .iter()
                .position(|byte| *byte == b'.')
                .is_some_and(|dot| {
                    dot > 1
                        && value[1..dot].iter().all(u8::is_ascii_hexdigit)
                        && !value[dot + 1..].is_empty()
                        && value[dot + 1..].iter().all(|byte| {
                            is_unreserved(*byte) || is_sub_delim(*byte) || *byte == b':'
                        })
                }))
}

fn valid_reg_name(value: &[u8]) -> bool {
    valid_percent_encoded(value, |byte| is_unreserved(byte) || is_sub_delim(byte))
}

fn valid_percent_encoded(value: &[u8], allowed: impl Fn(u8) -> bool) -> bool {
    let mut at = 0;
    while at < value.len() {
        if value[at] == b'%' {
            if !value
                .get(at + 1..at + 3)
                .is_some_and(|digits| digits.iter().all(u8::is_ascii_hexdigit))
            {
                return false;
            }
            at += 3;
        } else if allowed(value[at]) {
            at += 1;
        } else {
            return false;
        }
    }
    true
}

fn split_at_byte(value: &[u8], needle: u8) -> Option<(&[u8], &[u8])> {
    let at = value.iter().position(|byte| *byte == needle)?;
    Some((&value[..at], &value[at + 1..]))
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn is_sub_delim(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

#[cfg(test)]
mod tests {
    use super::valid_request_target;

    #[test]
    fn request_target_has_a_visible_ascii_boundary() {
        assert!(!valid_request_target(b"GET", b""));
        for byte in u8::MIN..=u8::MAX {
            assert_eq!(
                valid_request_target(b"GET", &[b'/', byte]),
                byte.is_ascii_graphic() && byte != b'#'
            );
        }
    }
}
