//! Start-line and field-section parsing and serialization.

use super::*;
use crate::{DEFAULT_MAX_HEADER_COUNT, Headers, RemoteProtocolError, Request, Response, Version};
use crate::{Method, StatusCode};
use std::{borrow::Cow, mem::MaybeUninit};

/// Parses a request head from a prefix already capped at the head byte limit.
pub(crate) fn parse_request_head(
    head: &[u8],
    max_headers: usize,
) -> Result<Option<(usize, Request)>, RemoteProtocolError> {
    let mut inline = [MaybeUninit::uninit(); DEFAULT_MAX_HEADER_COUNT];
    let mut overflow =
        (max_headers > inline.len()).then(|| vec![MaybeUninit::uninit(); max_headers]);
    let slots = overflow
        .as_deref_mut()
        .unwrap_or_else(|| &mut inline[..max_headers]);
    let mut parsed = httparse::Request::new(&mut []);
    let status = httparse::ParserConfig::default()
        .parse_request_with_uninit_headers(&mut parsed, head, slots)
        .map_err(|error| {
            remote(
                format!("invalid request head: {error}"),
                Some(if error == httparse::Error::TooManyHeaders {
                    431
                } else {
                    400
                }),
            )
        })?;
    let httparse::Status::Complete(end) = status else {
        return Ok(None);
    };
    let http_version = version(parsed.version);
    // A complete httparse request has a method made of token bytes and a target.
    let method = Method::from_bytes(parsed.method.unwrap_or_default().as_bytes())
        .expect("httparse validates the method token");
    let target = parsed.path.unwrap_or_default().as_bytes().to_vec();
    Ok(Some((
        end,
        Request {
            method,
            target,
            headers: copy_headers(head, parsed.headers, false),
            http_version,
        },
    )))
}

pub(crate) fn normalize_obs_fold(data: &[u8]) -> Cow<'_, [u8]> {
    if !data
        .windows(2)
        .any(|bytes| bytes[0] == b'\n' && matches!(bytes[1], b' ' | b'\t'))
    {
        return Cow::Borrowed(data);
    }

    let mut normalized = Vec::with_capacity(data.len());
    let mut at = 0;
    while at < data.len() {
        if data[at] == b'\n'
            && data
                .get(at + 1)
                .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
        {
            if normalized.last() == Some(&b'\r') {
                normalized.pop();
            }
            normalized.push(b' ');
            at += 1;
            while data
                .get(at)
                .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
            {
                at += 1;
            }
        } else {
            normalized.push(data[at]);
            at += 1;
        }
    }
    Cow::Owned(normalized)
}

/// Parses a response head from a prefix already capped at the head byte limit.
pub(crate) fn parse_response_head(
    head: &[u8],
    max_headers: usize,
    lenient: bool,
) -> Result<Option<(usize, Response, bool)>, RemoteProtocolError> {
    let mut inline = [MaybeUninit::uninit(); DEFAULT_MAX_HEADER_COUNT];
    let mut overflow =
        (max_headers > inline.len()).then(|| vec![MaybeUninit::uninit(); max_headers]);
    let slots = overflow
        .as_deref_mut()
        .unwrap_or_else(|| &mut inline[..max_headers]);
    let mut parsed = httparse::Response::new(&mut []);
    let mut config = httparse::ParserConfig::default();
    config
        .allow_obsolete_multiline_headers_in_responses(true)
        .ignore_invalid_headers_in_responses(lenient)
        .allow_spaces_after_header_name_in_responses(lenient)
        .allow_multiple_spaces_in_response_status_delimiters(lenient);
    let status = config
        .parse_response_with_uninit_headers(&mut parsed, head, slots)
        .map_err(|error| remote(format!("invalid response head: {error}"), None))?;
    let httparse::Status::Complete(end) = status else {
        return Ok(None);
    };
    let http_version = version(parsed.version);
    // A complete httparse response has a three-digit code.
    let status = StatusCode::from_u16(parsed.code.unwrap_or_default())
        .map_err(|_| remote("invalid response status", None))?;
    // httparse exposes the reason phrase as UTF-8 and returns an empty value
    // for obs-text, so preserve the bytes from the validated line.
    // https://github.com/seanmonstar/httparse/blob/v1.10.1/src/lib.rs#L1732-L1739
    // httparse skips empty lines before the status line.
    let line = head[..end]
        .split(|byte| *byte == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .find(|line| !line.is_empty())
        .unwrap_or_default();
    let reason = if lenient {
        // httparse's lenient delimiters are runs of SP only.
        let spaces = |at: usize| at + line[at..].iter().take_while(|b| **b == b' ').count();
        line.get(spaces(spaces(8) + 3)..)
    } else {
        line.get(13..)
    };
    // A lenient parse drops invalid field lines, and a whitespace-led line
    // that follows no field is dropped rather than unfolded.
    let dropped = lenient && dropped_lines(head, line, end, parsed.headers);
    Ok(Some((
        end,
        Response {
            status,
            reason: reason.unwrap_or_default().to_vec(),
            headers: copy_headers(head, parsed.headers, true),
            http_version,
        },
        dropped,
    )))
}

/// Whether `data` ends inside a start line that follows any empty lines.
pub(crate) fn start_line_unfinished(data: &[u8]) -> bool {
    data.split_inclusive(|byte| *byte == b'\n')
        .find(|line| *line != b"\n" && *line != b"\r\n")
        .is_some_and(|line| !line.ends_with(b"\n"))
}

/// Whether a lenient parse dropped a field line. Each parsed field is a slice
/// of `head`, folded continuation lines included, so only whitespace and
/// line breaks lie between fields unless a line with content was dropped.
fn dropped_lines(head: &[u8], status_line: &[u8], end: usize, fields: &[httparse::Header]) -> bool {
    let at = |part: &[u8]| part.as_ptr().addr() - head.as_ptr().addr();
    let blank = |gap: &[u8]| {
        gap.iter()
            .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
    };
    let mut from = at(status_line) + status_line.len();
    for field in fields {
        if !blank(&head[from..at(field.name.as_bytes())]) {
            return true;
        }
        from = at(field.value) + field.value.len();
    }
    !blank(&head[from..end])
}

/// httparse accepts only minor versions 0 and 1, and RFC 9110 Section 6.2
/// handles any higher minor version as 1.1.
fn version(minor: Option<u8>) -> Version {
    if minor == Some(0) {
        Version::Http10
    } else {
        Version::Http11
    }
}

/// Copies parsed fields, taking the section bytes that hold them in one copy
/// when no obs-fold must be normalized.
pub(crate) fn copy_headers(
    section: &[u8],
    headers: &[httparse::Header<'_>],
    unfold: bool,
) -> Headers {
    if !(unfold && headers.iter().any(|header| header.value.contains(&b'\n'))) {
        return copy_section(section, headers);
    }
    let mut copied = Headers::with_capacity(
        headers.len(),
        headers
            .iter()
            .map(|header| header.name.len() + header.value.len())
            .sum(),
    );
    for header in headers {
        copied.push(header.name, normalize_obs_fold(header.value));
    }
    copied
}

/// Copies the bytes spanning `headers` in one piece. httparse returns every
/// name and value as a slice of `section`.
fn copy_section(section: &[u8], headers: &[httparse::Header<'_>]) -> Headers {
    let offset = |part: &[u8]| part.as_ptr().addr() - section.as_ptr().addr();
    let start = headers
        .first()
        .map_or(0, |first| offset(first.name.as_bytes()));
    let mut fields = Vec::with_capacity(headers.len());
    let mut end = start;
    for header in headers {
        let name = offset(header.name.as_bytes()) - start;
        let value = offset(header.value) - start;
        end = end.max(start + value + header.value.len());
        fields.push((
            name..name + header.name.len(),
            value..value + header.value.len(),
        ));
    }
    Headers::from_section(&section[start..end], fields)
}

/// Serializes a request head with the field at `host_at` written first, as
/// RFC 9110 Section 7.2 asks of a user agent.
pub(crate) fn serialize_request(request: &Request, host_at: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(head_len(
        request.method.as_bytes().len() + request.target.len() + 12,
        &request.headers,
    ));
    out.extend_from_slice(request.method.as_bytes());
    out.push(b' ');
    out.extend_from_slice(&request.target);
    out.extend_from_slice(b" HTTP/");
    out.extend_from_slice(request.http_version.wire());
    out.extend_from_slice(b"\r\n");
    if let Some(host) = request.headers.iter().nth(host_at) {
        write_field(&mut out, host);
    }
    for (at, field) in request.headers.iter().enumerate() {
        if at != host_at {
            write_field(&mut out, field);
        }
    }
    out.extend_from_slice(b"\r\n");
    out
}

pub(crate) fn serialize_response(
    version: Version,
    status: StatusCode,
    reason: &[u8],
    headers: &Headers,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(head_len(15 + reason.len(), headers));
    out.extend_from_slice(b"HTTP/");
    out.extend_from_slice(version.wire());
    out.push(b' ');
    let status = status.as_u16();
    out.extend_from_slice(&[
        b'0' + (status / 100) as u8,
        b'0' + (status / 10 % 10) as u8,
        b'0' + (status % 10) as u8,
    ]);
    out.push(b' ');
    out.extend_from_slice(reason);
    out.extend_from_slice(b"\r\n");
    write_headers(&mut out, headers);
    out.extend_from_slice(b"\r\n");
    out
}

fn head_len(start_line: usize, headers: &Headers) -> usize {
    start_line
        + headers
            .iter()
            .map(|(name, value)| name.len() + value.len() + 4)
            .sum::<usize>()
        + 2
}

pub(crate) fn write_headers(out: &mut Vec<u8>, headers: &Headers) {
    for field in headers {
        write_field(out, field);
    }
}

fn write_field(out: &mut Vec<u8>, (name, value): (&[u8], &[u8])) {
    out.extend_from_slice(name);
    out.extend_from_slice(b": ");
    out.extend_from_slice(value);
    out.extend_from_slice(b"\r\n");
}
