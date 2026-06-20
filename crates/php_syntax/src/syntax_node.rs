use crate::SyntaxKind;
use php_source::TextRange;

/// A CST node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxNode {
    green: GreenNode,
}

/// Immutable, parser-owned CST node storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GreenNode {
    kind: SyntaxKind,
    range: TextRange,
    children: Vec<SyntaxElement>,
}

impl SyntaxNode {
    /// Creates a CST node.
    #[must_use]
    pub fn new(kind: SyntaxKind, range: TextRange, children: Vec<SyntaxElement>) -> Self {
        Self {
            green: GreenNode {
                kind,
                range,
                children,
            },
        }
    }

    /// Returns the node kind.
    #[must_use]
    pub const fn kind(&self) -> &SyntaxKind {
        &self.green.kind
    }

    /// Returns the node text range.
    #[must_use]
    pub const fn range(&self) -> TextRange {
        self.green.range
    }

    /// Returns the node text range.
    #[must_use]
    pub const fn text_range(&self) -> TextRange {
        self.range()
    }

    /// Returns child elements.
    #[must_use]
    pub fn children(&self) -> &[SyntaxElement] {
        &self.green.children
    }
}

/// A CST child element.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyntaxElement {
    /// Nested CST node.
    Node(SyntaxNode),
    /// Leaf token.
    Token(SyntaxToken),
}

/// A lossless CST token.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxToken {
    green: GreenToken,
}

/// Immutable, parser-owned CST token storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GreenToken {
    kind: SyntaxKind,
    text: String,
    range: TextRange,
    line: u32,
}

impl SyntaxToken {
    /// Creates a syntax token.
    #[must_use]
    pub fn new(kind: SyntaxKind, text: impl Into<String>, range: TextRange, line: u32) -> Self {
        Self {
            green: GreenToken {
                kind,
                text: text.into(),
                range,
                line,
            },
        }
    }

    /// Returns the token kind.
    #[must_use]
    pub const fn kind(&self) -> &SyntaxKind {
        &self.green.kind
    }

    /// Returns the original token text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.green.text
    }

    /// Returns the token text range.
    #[must_use]
    pub const fn range(&self) -> TextRange {
        self.green.range
    }

    /// Returns the token text range.
    #[must_use]
    pub const fn text_range(&self) -> TextRange {
        self.range()
    }

    /// Returns the one-based starting line.
    #[must_use]
    pub const fn line(&self) -> u32 {
        self.green.line
    }
}
