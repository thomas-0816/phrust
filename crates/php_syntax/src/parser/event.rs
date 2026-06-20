use crate::{ParseDiagnostic, SyntaxKind};

/// Event stream emitted by the parser before tree construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    /// Internal marker placeholder before a node kind is known.
    Placeholder,
    /// Starts a node.
    StartNode(SyntaxKind),
    /// Adds the next token from the token source.
    AddToken,
    /// Records a parse error.
    Error(ParseDiagnostic),
    /// Finishes the current node.
    FinishNode,
}
