use crate::BytePos;

/// One-based display line and byte column.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineCol {
    /// One-based line number.
    pub line: usize,
    /// One-based byte column.
    pub column: usize,
}

impl LineCol {
    /// Creates a one-based line/column pair.
    #[must_use]
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// Byte-oriented line index for source text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineIndex {
    line_starts: Vec<usize>,
    source_len: usize,
}

impl LineIndex {
    /// Builds a line index for `source`.
    ///
    /// `\n`, `\r\n`, and standalone `\r` each start a new line. Lines and
    /// columns reported by [`LineIndex::line_col`] are one-based. Columns are
    /// byte columns, not Unicode grapheme columns.
    #[must_use]
    pub fn new(source: &str) -> Self {
        let bytes = source.as_bytes();
        let mut line_starts = vec![0];
        let mut index = 0;

        while index < bytes.len() {
            match bytes[index] {
                b'\n' => {
                    index += 1;
                    line_starts.push(index);
                }
                b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                    index += 2;
                    line_starts.push(index);
                }
                b'\r' => {
                    index += 1;
                    line_starts.push(index);
                }
                _ => {
                    index += 1;
                }
            }
        }

        Self {
            line_starts,
            source_len: bytes.len(),
        }
    }

    /// Returns the source length in bytes used to build the index.
    #[must_use]
    pub const fn source_len(&self) -> usize {
        self.source_len
    }

    /// Returns the number of indexed lines.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Converts a byte position to a one-based line and byte-column pair.
    ///
    /// Positions past EOF are clamped to EOF.
    #[must_use]
    pub fn line_col(&self, pos: BytePos) -> LineCol {
        let pos = pos.to_usize().min(self.source_len);
        let line_index = match self.line_starts.binary_search(&pos) {
            Ok(index) => index,
            Err(0) => 0,
            Err(index) => index - 1,
        };
        let line_start = self.line_starts[line_index];

        LineCol::new(line_index + 1, pos - line_start + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::{LineCol, LineIndex};
    use crate::BytePos;

    #[test]
    fn empty_source_has_one_display_line() {
        let index = LineIndex::new("");
        assert_eq!(index.source_len(), 0);
        assert_eq!(index.line_count(), 1);
        assert_eq!(index.line_col(BytePos::new(0)), LineCol::new(1, 1));
    }

    #[test]
    fn one_line_without_newline() {
        let index = LineIndex::new("abc");
        assert_eq!(index.line_count(), 1);
        assert_eq!(index.line_col(BytePos::new(0)), LineCol::new(1, 1));
        assert_eq!(index.line_col(BytePos::new(2)), LineCol::new(1, 3));
        assert_eq!(index.line_col(BytePos::new(3)), LineCol::new(1, 4));
    }

    #[test]
    fn multiple_lf_lines() {
        let index = LineIndex::new("a\nbc\n");
        assert_eq!(index.line_count(), 3);
        assert_eq!(index.line_col(BytePos::new(0)), LineCol::new(1, 1));
        assert_eq!(index.line_col(BytePos::new(2)), LineCol::new(2, 1));
        assert_eq!(index.line_col(BytePos::new(5)), LineCol::new(3, 1));
    }

    #[test]
    fn crlf_counts_as_one_line_break() {
        let index = LineIndex::new("a\r\nb");
        assert_eq!(index.line_count(), 2);
        assert_eq!(index.line_col(BytePos::new(3)), LineCol::new(2, 1));
        assert_eq!(index.line_col(BytePos::new(4)), LineCol::new(2, 2));
    }

    #[test]
    fn standalone_cr_counts_as_line_break() {
        let index = LineIndex::new("a\rb");
        assert_eq!(index.line_count(), 2);
        assert_eq!(index.line_col(BytePos::new(2)), LineCol::new(2, 1));
    }

    #[test]
    fn positions_past_eof_clamp_to_eof() {
        let index = LineIndex::new("abc");
        assert_eq!(index.line_col(BytePos::new(99)), LineCol::new(1, 4));
    }

    #[test]
    fn non_ascii_columns_are_byte_columns() {
        let index = LineIndex::new("éx");
        assert_eq!(index.source_len(), 3);
        assert_eq!(index.line_col(BytePos::new(2)), LineCol::new(1, 3));
        assert_eq!(index.line_col(BytePos::new(3)), LineCol::new(1, 4));
    }
}
