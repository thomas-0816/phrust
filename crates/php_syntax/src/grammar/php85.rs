//! PHP 8.5 grammar helpers.

use crate::SyntaxKind;
use crate::grammar::named;
use php_lexer::TokenName;

/// Returns true for the PHP 8.5 pipe operator token.
#[must_use]
pub(crate) fn is_pipe_operator(kind: SyntaxKind) -> bool {
    kind == named(TokenName::Pipe)
}

/// Returns true for the PHP 8.5 void-cast token.
#[must_use]
pub(crate) fn is_void_cast(kind: SyntaxKind) -> bool {
    kind == named(TokenName::VoidCast)
}

/// Returns true for the clone language construct token.
#[must_use]
pub(crate) fn is_clone_keyword(kind: SyntaxKind) -> bool {
    kind == named(TokenName::Clone)
}
