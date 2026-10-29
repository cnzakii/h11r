//! Ordered header and trailer fields.

use std::{
    fmt,
    hash::{Hash, Hasher},
    ops::Range,
    slice,
};

/// Ordered header or trailer fields, preserving input order and name casing.
///
/// All names and values share one buffer, so a field section costs two
/// allocations however many fields it holds. Values exclude leading and
/// trailing whitespace, as required by
/// [RFC 9110 Section 5.5](https://www.rfc-editor.org/rfc/rfc9110.html#section-5.5).
///
/// ```
/// use h11r::Headers;
///
/// let mut headers = Headers::from([("Host", "example.test"), ("Accept", "*/*")]);
/// headers.push("accept", "text/html");
/// assert_eq!(headers.len(), 3);
/// assert_eq!(headers.get_all(b"ACCEPT").collect::<Vec<_>>(), [b"*/*".as_slice(), b"text/html"]);
/// ```
#[derive(Clone, Default)]
pub struct Headers {
    bytes: Vec<u8>,
    /// Byte ranges of each field's name and value in `bytes`.
    fields: Vec<(Range<usize>, Range<usize>)>,
}

impl Headers {
    /// Creates an empty field section without allocating.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bytes: Vec::new(),
            fields: Vec::new(),
        }
    }

    /// Creates an empty field section with room for `fields` fields whose
    /// names and values total `bytes` bytes.
    #[must_use]
    pub fn with_capacity(fields: usize, bytes: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(bytes),
            fields: Vec::with_capacity(fields),
        }
    }

    /// Copies a parsed section in one piece; `fields` index into `section`.
    pub(crate) fn from_section(section: &[u8], fields: Vec<(Range<usize>, Range<usize>)>) -> Self {
        Self {
            bytes: section.to_vec(),
            fields,
        }
    }

    /// Appends one field. Names and values are validated when sent.
    pub fn push(&mut self, name: impl AsRef<[u8]>, value: impl AsRef<[u8]>) {
        let name_start = self.bytes.len();
        self.bytes.extend_from_slice(name.as_ref());
        let value_start = self.bytes.len();
        self.bytes.extend_from_slice(value.as_ref());
        self.fields
            .push((name_start..value_start, value_start..self.bytes.len()));
    }

    /// Number of fields.
    #[must_use]
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// Whether there are no fields.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Iterates over `(name, value)` pairs in order.
    pub fn iter(&self) -> HeadersIter<'_> {
        HeadersIter {
            bytes: &self.bytes,
            fields: self.fields.iter(),
        }
    }

    /// Iterates over the values of every field named `name`, compared
    /// ASCII case-insensitively, in order.
    pub fn get_all<'a>(&'a self, name: &'a [u8]) -> impl Iterator<Item = &'a [u8]> {
        self.iter()
            .filter(move |(field, _)| field.eq_ignore_ascii_case(name))
            .map(|(_, value)| value)
    }

    /// Keeps only the fields for which `keep` returns `true`.
    pub fn retain(&mut self, mut keep: impl FnMut(&[u8], &[u8]) -> bool) {
        let bytes = &self.bytes;
        self.fields
            .retain(|(name, value)| keep(&bytes[name.clone()], &bytes[value.clone()]));
    }
}

impl PartialEq for Headers {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl Eq for Headers {}

impl Hash for Headers {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(self.len());
        for field in self {
            field.hash(state);
        }
    }
}

impl fmt::Debug for Headers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.iter().map(|(name, value)| {
                (
                    format!("{}", name.escape_ascii()),
                    format!("{}", value.escape_ascii()),
                )
            }))
            .finish()
    }
}

impl<N: AsRef<[u8]>, V: AsRef<[u8]>> Extend<(N, V)> for Headers {
    fn extend<I: IntoIterator<Item = (N, V)>>(&mut self, fields: I) {
        for (name, value) in fields {
            self.push(name, value);
        }
    }
}

impl<N: AsRef<[u8]>, V: AsRef<[u8]>> FromIterator<(N, V)> for Headers {
    fn from_iter<I: IntoIterator<Item = (N, V)>>(fields: I) -> Self {
        let mut headers = Self::new();
        headers.extend(fields);
        headers
    }
}

impl<N: AsRef<[u8]>, V: AsRef<[u8]>, const COUNT: usize> From<[(N, V); COUNT]> for Headers {
    fn from(fields: [(N, V); COUNT]) -> Self {
        fields.into_iter().collect()
    }
}

impl<'a> IntoIterator for &'a Headers {
    type Item = (&'a [u8], &'a [u8]);
    type IntoIter = HeadersIter<'a>;

    fn into_iter(self) -> HeadersIter<'a> {
        self.iter()
    }
}

/// Iterator over the `(name, value)` pairs of [`Headers`].
#[derive(Clone, Debug)]
pub struct HeadersIter<'a> {
    bytes: &'a [u8],
    fields: slice::Iter<'a, (Range<usize>, Range<usize>)>,
}

impl<'a> Iterator for HeadersIter<'a> {
    type Item = (&'a [u8], &'a [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        let (name, value) = self.fields.next()?;
        Some((&self.bytes[name.clone()], &self.bytes[value.clone()]))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.fields.size_hint()
    }
}

impl ExactSizeIterator for HeadersIter<'_> {}

#[cfg(test)]
mod tests {
    use super::Headers;
    use std::hash::{BuildHasher, RandomState};

    #[test]
    fn fields_keep_their_order_and_boundaries() {
        let mut headers = Headers::from([("A", ""), ("", "b"), ("Cc", "dd")]);
        headers.retain(|name, _| name != b"");
        headers.push("E", "f");
        assert_eq!(
            headers.iter().collect::<Vec<_>>(),
            [
                (b"A".as_slice(), b"".as_slice()),
                (b"Cc", b"dd"),
                (b"E", b"f")
            ]
        );
        assert_eq!(
            format!("{headers:?}"),
            r#"[("A", ""), ("Cc", "dd"), ("E", "f")]"#
        );
    }

    #[test]
    fn equal_fields_hash_equally_whatever_their_layout() {
        let mut retained = Headers::from([("X", "1"), ("A", "b")]);
        retained.retain(|name, _| name == b"A");
        let fresh = Headers::from([("A", "b")]);
        let state = RandomState::new();
        assert_eq!(retained, fresh);
        assert_eq!(state.hash_one(&retained), state.hash_one(&fresh));
    }
}
