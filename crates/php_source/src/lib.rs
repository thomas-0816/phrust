//! Source text primitives for PHP compatibility work.
//!
//! Positions are byte-oriented. This matches Rust slice indexing and PHP's
//! lexer input model, and avoids treating Unicode scalar values or grapheme
//! clusters as source columns. Line and column values are one-based display
//! coordinates for compatibility with PHP reference token lines.
//!
//! This crate intentionally contains no PHP lexer, parser, AST, CST, VM, or
//! runtime implementation.

use std::sync::Arc;

pub mod byte_kernel;
mod line_index;
mod span;

pub use line_index::{LineCol, LineIndex};
pub use span::{BytePos, TextRange};

/// Owned PHP source text with a byte-oriented line index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceText {
    text: Arc<str>,
    line_index: LineIndex,
}

impl SourceText {
    /// Creates source text and builds its line index.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        let text = Arc::<str>::from(text.into());
        let line_index = LineIndex::new(&text);
        Self { text, line_index }
    }

    /// Returns the original source text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Returns the original source bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.text.as_bytes()
    }

    /// Returns shared ownership of the immutable source buffer.
    #[must_use]
    pub fn shared_text(&self) -> Arc<str> {
        Arc::clone(&self.text)
    }

    /// Returns the source length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Returns true when the source is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Returns the source line index.
    #[must_use]
    pub const fn line_index(&self) -> &LineIndex {
        &self.line_index
    }

    /// Converts a byte position to a one-based line and byte-column pair.
    #[must_use]
    pub fn line_col(&self, pos: BytePos) -> LineCol {
        self.line_index.line_col(pos)
    }

    /// Returns a source slice when the range is valid UTF-8 boundary aligned.
    #[must_use]
    pub fn slice(&self, range: TextRange) -> Option<&str> {
        self.text
            .get(range.start().to_usize()..range.end().to_usize())
    }
}

/// Returns the pinned PHP reference version for Foundation.
#[must_use]
pub const fn reference_php_version() -> &'static str {
    "8.5.7"
}

#[cfg(test)]
mod tests {
    use super::{BytePos, LineCol, SourceText, TextRange, reference_php_version};
    use std::sync::Arc;

    #[test]
    fn exposes_foundation_reference_version() {
        assert_eq!(reference_php_version(), "8.5.7");
    }

    #[test]
    fn source_text_handles_empty_source() {
        let source = SourceText::new("");
        assert!(source.is_empty());
        assert_eq!(source.len(), 0);
        assert_eq!(source.line_col(BytePos::new(0)), LineCol::new(1, 1));
    }

    #[test]
    fn source_text_slices_valid_ranges() {
        let source = SourceText::new("<?php");
        let range = TextRange::new(0, 2);
        assert_eq!(source.slice(range), Some("<?"));
    }

    #[test]
    fn source_text_exposes_the_canonical_shared_buffer() {
        let source = SourceText::new("<?php echo 1;");
        let first = source.shared_text();
        let second = source.shared_text();

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.as_ref(), source.as_str());
    }
}
