use crate::SyntaxKind;
use crate::parser::core::Parser;

/// Open node marker.
#[derive(Clone, Debug)]
pub struct Marker {
    pos: usize,
    completed: bool,
}

impl Marker {
    pub(crate) const fn new(pos: usize) -> Self {
        Self {
            pos,
            completed: false,
        }
    }

    /// Completes this marker with `kind`.
    #[must_use]
    pub fn complete(mut self, parser: &mut Parser<'_>, kind: SyntaxKind) -> CompletedMarker {
        parser.complete_marker(self.pos, kind);
        self.completed = true;
        CompletedMarker {
            pos: self.pos,
            kind,
        }
    }
}

impl Drop for Marker {
    fn drop(&mut self) {
        debug_assert!(self.completed, "parser marker dropped before completion");
    }
}

/// Completed node marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompletedMarker {
    pos: usize,
    kind: SyntaxKind,
}

impl CompletedMarker {
    /// Returns the event position where this node started.
    #[must_use]
    pub const fn pos(self) -> usize {
        self.pos
    }

    /// Returns the completed node kind.
    #[must_use]
    pub const fn kind(self) -> SyntaxKind {
        self.kind
    }
}
