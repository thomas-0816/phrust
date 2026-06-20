/// A byte offset in source text.
///
/// `usize` is used deliberately because Rust string and byte-slice indexing is
/// `usize`-based. Source compatibility logic must not interpret this as a
/// character, scalar-value, or grapheme index.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct BytePos(usize);

impl BytePos {
    /// Creates a byte position.
    #[must_use]
    pub const fn new(pos: usize) -> Self {
        Self(pos)
    }

    /// Returns the raw byte offset.
    #[must_use]
    pub const fn to_usize(self) -> usize {
        self.0
    }
}

/// A half-open source range `[start, end)`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TextRange {
    start: BytePos,
    end: BytePos,
}

impl TextRange {
    /// Creates a valid half-open byte range.
    ///
    /// If `end` is before `start`, the range is clamped to an empty range at
    /// `start`. Use [`TextRange::try_new`] when callers need to reject invalid
    /// bounds instead.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        let start = BytePos::new(start);
        let end = if end < start.to_usize() {
            start
        } else {
            BytePos::new(end)
        };
        Self { start, end }
    }

    /// Attempts to create a range, returning `None` for invalid ordering.
    #[must_use]
    pub const fn try_new(start: usize, end: usize) -> Option<Self> {
        if start <= end {
            Some(Self {
                start: BytePos::new(start),
                end: BytePos::new(end),
            })
        } else {
            None
        }
    }

    /// Creates an empty range at `pos`.
    #[must_use]
    pub const fn empty(pos: usize) -> Self {
        let pos = BytePos::new(pos);
        Self {
            start: pos,
            end: pos,
        }
    }

    /// Returns the inclusive start byte position.
    #[must_use]
    pub const fn start(self) -> BytePos {
        self.start
    }

    /// Returns the exclusive end byte position.
    #[must_use]
    pub const fn end(self) -> BytePos {
        self.end
    }

    /// Returns the byte length.
    #[must_use]
    pub const fn len(self) -> usize {
        self.end.to_usize() - self.start.to_usize()
    }

    /// Returns true when the range covers no bytes.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start.to_usize() == self.end.to_usize()
    }
}

#[cfg(test)]
mod tests {
    use super::{BytePos, TextRange};

    #[test]
    fn text_range_is_half_open() {
        let range = TextRange::new(2, 5);
        assert_eq!(range.start(), BytePos::new(2));
        assert_eq!(range.end(), BytePos::new(5));
        assert_eq!(range.len(), 3);
        assert!(!range.is_empty());
    }

    #[test]
    fn text_range_new_clamps_invalid_ordering() {
        let range = TextRange::new(10, 2);
        assert_eq!(range.start(), BytePos::new(10));
        assert_eq!(range.end(), BytePos::new(10));
        assert_eq!(range.len(), 0);
        assert!(range.is_empty());
        assert_eq!(TextRange::try_new(10, 2), None);
    }
}
