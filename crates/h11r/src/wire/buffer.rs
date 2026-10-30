//! Receive buffering, framing state, and field-section scheduling.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Framing {
    Fixed(usize),
    Chunked,
    CloseDelimited,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Chunk {
    Size { scanned: usize },
    Data { remaining: usize, first: bool },
    DataEnd,
    Trailers,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Reader {
    #[default]
    Head,
    Fixed(usize),
    Chunked(Chunk),
    CloseDelimited,
}

#[derive(Debug, Default)]
pub(crate) struct Buffer {
    bytes: Vec<u8>,
    start: usize,
}

impl Buffer {
    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.bytes[self.start..]
    }
    pub(crate) fn extend(&mut self, data: &[u8]) {
        self.compact();
        self.bytes.extend_from_slice(data);
    }
    pub(crate) fn extend_iter(&mut self, data: impl IntoIterator<Item = u8>) {
        self.compact();
        self.bytes.extend(data);
    }
    fn compact(&mut self) {
        // Moving no more live bytes than were discarded keeps compaction
        // amortized linear across adversarial receive boundaries.
        let live = self.bytes.len() - self.start;
        if self.start > 0 && self.start >= live {
            self.bytes.copy_within(self.start.., 0);
            self.bytes.truncate(live);
            self.start = 0;
        }
    }
    pub(crate) fn consume(&mut self, count: usize) {
        self.start += count;
    }
    pub(crate) fn take(&mut self, count: usize) -> &[u8] {
        let start = self.start;
        self.consume(count);
        &self.bytes[start..start + count]
    }
}

/// Schedules `httparse` runs over an incomplete field section.
///
/// `httparse` remains the sole syntax authority. A section is parsed once its
/// terminating empty line arrives, and its incomplete prefix is re-parsed each
/// time the buffer doubles. Invalid bytes are therefore rejected soon after
/// they arrive, at amortized linear cost under any transport segmentation.
#[derive(Clone, Debug, Default)]
pub(crate) struct SectionProgress {
    at: usize,
    line_start: usize,
    started: bool,
    parsed: usize,
}

impl SectionProgress {
    /// Whether the buffered section prefix should be parsed now.
    pub(crate) fn due(&mut self, data: &[u8], skip_leading_empty: bool) -> bool {
        // The cheap size check first: when it holds, the parser scans the
        // bytes anyway, and the line scan resumes from where it stopped.
        let due = data.len() >= self.parsed.saturating_mul(2)
            || self.poll(data, skip_leading_empty).is_some();
        if due {
            self.parsed = data.len();
        }
        due
    }

    pub(crate) fn poll(&mut self, data: &[u8], skip_leading_empty: bool) -> Option<usize> {
        while self.at < data.len() {
            self.at += 1;
            if data[self.at - 1] == b'\n' {
                let line = &data[self.line_start..self.at - 1];
                let empty = line.is_empty() || line == b"\r";
                self.line_start = self.at;
                if empty {
                    if self.started || !skip_leading_empty {
                        return Some(self.at);
                    }
                } else {
                    self.started = true;
                }
            }
        }
        None
    }
}

/// Whether `data` holds only complete empty lines, CRLF or bare LF, which the
/// head parser skips before a start line.
pub(crate) fn only_empty_lines(data: &[u8]) -> bool {
    data.split_inclusive(|byte| *byte == b'\n')
        .all(|line| line == b"\n" || line == b"\r\n")
}

pub(crate) fn reader(framing: Framing) -> Reader {
    match framing {
        Framing::Fixed(size) => Reader::Fixed(size),
        Framing::Chunked => Reader::Chunked(Chunk::Size { scanned: 0 }),
        Framing::CloseDelimited => Reader::CloseDelimited,
    }
}

#[cfg(test)]
mod tests {
    use super::Buffer;

    #[test]
    fn buffer_compaction_never_moves_more_live_bytes_than_it_discards() {
        let mut buffer = Buffer {
            bytes: vec![b'a'; 12 * 1024],
            start: 0,
        };

        buffer.consume(4 * 1024 + 1);
        buffer.extend(b"b");
        assert_eq!(buffer.start, 4 * 1024 + 1);

        buffer.consume(4 * 1024);
        let expected = buffer.as_slice().to_vec();
        buffer.extend(b"c");
        assert_eq!(buffer.start, 0);
        assert_eq!(buffer.as_slice(), [expected.as_slice(), b"c"].concat());
    }
}
