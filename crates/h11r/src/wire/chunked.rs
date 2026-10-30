//! Chunked transfer coding: chunk-size lines and trailers.

use super::*;
use crate::method::is_token_byte;
use crate::{DEFAULT_MAX_HEADER_COUNT, Headers, RemoteProtocolError};
use std::borrow::Cow;

pub(crate) fn parse_number(value: &[u8], radix: u32) -> Option<usize> {
    if value.is_empty() {
        return None;
    }
    value.iter().try_fold(0usize, |number, byte| {
        number
            .checked_mul(radix as usize)?
            .checked_add((*byte as char).to_digit(radix)? as usize)
    })
}

pub(crate) fn parse_chunk_header(line: &[u8]) -> Option<usize> {
    let digits = line
        .iter()
        .take_while(|byte| byte.is_ascii_hexdigit())
        .count();
    let size = parse_number(&line[..digits], 16)?;
    let mut at = digits;
    while at < line.len() {
        at = skip_ows(line, at);
        if line.get(at) != Some(&b';') {
            return None;
        }
        at = skip_ows(line, at + 1);
        let name = at;
        while line.get(at).is_some_and(|byte| is_token_byte(*byte)) {
            at += 1;
        }
        if at == name {
            return None;
        }
        let after_name = at;
        at = skip_ows(line, at);
        if line.get(at) == Some(&b'=') {
            at = skip_ows(line, at + 1);
            if line.get(at) == Some(&b'"') {
                at = parse_quoted_string(line, at)?;
            } else {
                let value = at;
                while line.get(at).is_some_and(|byte| is_token_byte(*byte)) {
                    at += 1;
                }
                if at == value {
                    return None;
                }
            }
        } else if at != after_name && line.get(at) != Some(&b';') {
            // BWS after a name is allowed only before the next extension.
            return None;
        }
    }
    Some(size)
}

/// Finds the end of the CRLF at or after `start`, a position already scanned
/// in `data`.
pub(crate) fn find_crlf(data: &[u8], start: usize) -> Option<usize> {
    data[start..]
        .windows(2)
        .position(|pair| pair == b"\r\n")
        .map(|at| start + at + 2)
}

/// The length of an unterminated line, not counting a final CR that may
/// begin its CRLF.
pub(crate) fn incomplete_line_len(data: &[u8]) -> usize {
    data.len() - usize::from(data.last() == Some(&b'\r'))
}

/// Parses trailers from a prefix already capped at the head byte limit.
pub(crate) fn parse_trailers(
    data: &[u8],
    max_headers: usize,
    normalize_folding: bool,
) -> Result<Option<(usize, Headers)>, RemoteProtocolError> {
    // Bytes after the terminating empty line belong to whatever follows,
    // so they are neither unfolded into nor parsed with the trailers.
    let original_end = SectionProgress::default().poll(data, false);
    let data = &data[..original_end.unwrap_or(data.len())];
    let normalized = if normalize_folding {
        normalize_obs_fold(data)
    } else {
        Cow::Borrowed(data)
    };
    let mut inline = [httparse::EMPTY_HEADER; DEFAULT_MAX_HEADER_COUNT];
    let mut overflow =
        (max_headers > inline.len()).then(|| vec![httparse::EMPTY_HEADER; max_headers]);
    let slots = overflow
        .as_deref_mut()
        .unwrap_or_else(|| &mut inline[..max_headers]);
    match httparse::parse_headers(&normalized, slots)
        .map_err(|error| remote(format!("invalid trailer field: {error}"), Some(400)))?
    {
        httparse::Status::Complete((parsed_end, headers)) => Ok(Some((
            original_end.unwrap_or(parsed_end),
            copy_headers(&normalized, headers, false),
        ))),
        httparse::Status::Partial => Ok(None),
    }
}
