/// Byte cursor over source text.
#[derive(Clone, Debug)]
pub(crate) struct Cursor<'src> {
    source: &'src str,
    offset: usize,
}

impl<'src> Cursor<'src> {
    pub(crate) const fn new(source: &'src str) -> Self {
        Self { source, offset: 0 }
    }

    pub(crate) const fn position(&self) -> usize {
        self.offset
    }

    pub(crate) const fn len(&self) -> usize {
        self.source.len()
    }

    pub(crate) const fn is_eof(&self) -> bool {
        self.offset >= self.source.len()
    }

    pub(crate) fn peek(&self) -> Option<u8> {
        self.source.as_bytes().get(self.offset).copied()
    }

    pub(crate) fn peek_n(&self, lookahead: usize) -> Option<u8> {
        self.source
            .as_bytes()
            .get(self.offset.saturating_add(lookahead))
            .copied()
    }

    pub(crate) fn starts_with(&self, needle: &[u8]) -> bool {
        self.source.as_bytes()[self.offset..].starts_with(needle)
    }

    pub(crate) fn bump(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.offset += 1;
        Some(byte)
    }
}

#[cfg(test)]
mod tests {
    use super::Cursor;

    #[test]
    fn cursor_reads_bytes_without_decoding() {
        let mut cursor = Cursor::new("é");
        assert_eq!(cursor.len(), 2);
        assert_eq!(cursor.position(), 0);
        assert_eq!(cursor.bump(), Some(0xc3));
        assert_eq!(cursor.bump(), Some(0xa9));
        assert_eq!(cursor.bump(), None);
        assert!(cursor.is_eof());
    }

    #[test]
    fn cursor_supports_lookahead_and_prefix_checks() {
        let mut cursor = Cursor::new("<?php");
        assert_eq!(cursor.peek(), Some(b'<'));
        assert_eq!(cursor.peek_n(1), Some(b'?'));
        assert_eq!(cursor.peek_n(4), Some(b'p'));
        assert_eq!(cursor.peek_n(5), None);
        assert!(cursor.starts_with(b"<?"));
        assert!(!cursor.starts_with(b"?>"));
        assert_eq!(cursor.bump(), Some(b'<'));
        assert!(cursor.starts_with(b"?php"));
    }
}
