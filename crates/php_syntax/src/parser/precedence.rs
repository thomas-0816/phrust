//! Expression binding powers.

use crate::SyntaxKind;
use crate::grammar::{named, php85, symbol};
use php_lexer::TokenName;

/// Binary operator associativity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Associativity {
    /// Left-associative operator.
    Left,
    /// Right-associative operator.
    Right,
}

/// Binding-power entry for a binary operator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BinaryOperator {
    /// Operator kind.
    pub kind: SyntaxKind,
    /// Left binding power.
    pub left_bp: u8,
    /// Right binding power.
    pub right_bp: u8,
    /// Associativity.
    pub associativity: Associativity,
}

impl BinaryOperator {
    const fn left(kind: SyntaxKind, bp: u8) -> Self {
        Self {
            kind,
            left_bp: bp,
            right_bp: bp + 1,
            associativity: Associativity::Left,
        }
    }

    const fn right(kind: SyntaxKind, bp: u8) -> Self {
        Self {
            kind,
            left_bp: bp,
            right_bp: bp,
            associativity: Associativity::Right,
        }
    }
}

/// Returns binary operator binding powers for the current token kind.
#[must_use]
pub fn binary_operator(kind: SyntaxKind) -> Option<BinaryOperator> {
    let operator = match kind {
        kind if kind == named(TokenName::LogicalOr) => BinaryOperator::left(kind, 10),
        kind if kind == named(TokenName::LogicalXor) => BinaryOperator::left(kind, 20),
        kind if kind == named(TokenName::LogicalAnd) => BinaryOperator::left(kind, 30),
        kind if is_assignment_operator(kind) => BinaryOperator::right(kind, 35),
        kind if kind == named(TokenName::Coalesce) => BinaryOperator::right(kind, 40),
        kind if kind == named(TokenName::BooleanOr) => BinaryOperator::left(kind, 50),
        kind if kind == named(TokenName::BooleanAnd) => BinaryOperator::left(kind, 60),
        kind if kind == symbol(b'|') => BinaryOperator::left(kind, 65),
        kind if kind == symbol(b'^') => BinaryOperator::left(kind, 66),
        kind if is_bitwise_and_operator(kind) => BinaryOperator::left(kind, 67),
        kind if is_equality_operator(kind) => BinaryOperator::left(kind, 70),
        kind if kind == named(TokenName::Instanceof) => BinaryOperator::left(kind, 126),
        kind if is_comparison_operator(kind) => BinaryOperator::left(kind, 80),
        kind if kind == named(TokenName::Sl) || kind == named(TokenName::Sr) => {
            BinaryOperator::left(kind, 90)
        }
        kind if kind == symbol(b'.') => BinaryOperator::left(kind, 100),
        kind if kind == symbol(b'+') || kind == symbol(b'-') => BinaryOperator::left(kind, 110),
        kind if php85::is_pipe_operator(kind) => BinaryOperator::left(kind, 115),
        kind if kind == symbol(b'*') || kind == symbol(b'/') || kind == symbol(b'%') => {
            BinaryOperator::left(kind, 120)
        }
        kind if kind == named(TokenName::Pow) => BinaryOperator::right(kind, 130),
        _ => return None,
    };
    Some(operator)
}

fn is_bitwise_and_operator(kind: SyntaxKind) -> bool {
    kind == symbol(b'&')
        || kind == named(TokenName::AmpersandFollowedByVarOrVararg)
        || kind == named(TokenName::AmpersandNotFollowedByVarOrVararg)
}

/// Ternary/elvis binding power.
pub const TERNARY_BP: u8 = 36;

/// Returns true for assignment operators.
#[must_use]
pub fn is_assignment_operator(kind: SyntaxKind) -> bool {
    kind == symbol(b'=')
        || kind == named(TokenName::PlusEqual)
        || kind == named(TokenName::MinusEqual)
        || kind == named(TokenName::MulEqual)
        || kind == named(TokenName::DivEqual)
        || kind == named(TokenName::ModEqual)
        || kind == named(TokenName::ConcatEqual)
        || kind == named(TokenName::AndEqual)
        || kind == named(TokenName::OrEqual)
        || kind == named(TokenName::XorEqual)
        || kind == named(TokenName::SlEqual)
        || kind == named(TokenName::SrEqual)
        || kind == named(TokenName::PowEqual)
        || kind == named(TokenName::CoalesceEqual)
}

fn is_equality_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        kind if kind == named(TokenName::IsEqual)
            || kind == named(TokenName::IsNotEqual)
            || kind == named(TokenName::IsIdentical)
            || kind == named(TokenName::IsNotIdentical)
    )
}

fn is_comparison_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        kind if kind == symbol(b'<')
            || kind == symbol(b'>')
            || kind == named(TokenName::IsSmallerOrEqual)
            || kind == named(TokenName::IsGreaterOrEqual)
            || kind == named(TokenName::Spaceship)
    )
}

/// Prefix operator right binding power.
pub const PREFIX_RIGHT_BP: u8 = 125;

/// Print construct right binding power.
///
/// PHP's `print` construct binds lower than most expression operators,
/// including concatenation, but higher than `and`, `xor`, and `or`.
pub const PRINT_RIGHT_BP: u8 = 31;

#[cfg(test)]
mod tests {
    use super::{Associativity, PREFIX_RIGHT_BP, binary_operator};
    use crate::grammar::{named, symbol};
    use php_lexer::TokenName;

    #[test]
    fn exponentiation_is_right_associative() {
        let op = binary_operator(named(TokenName::Pow)).expect("pow operator");

        assert_eq!(op.associativity, Associativity::Right);
        assert_eq!(op.left_bp, op.right_bp);
    }

    #[test]
    fn compound_assignment_is_right_associative() {
        let op = binary_operator(named(TokenName::CoalesceEqual)).expect("coalesce assignment");

        assert_eq!(op.associativity, Associativity::Right);
        assert_eq!(op.left_bp, op.right_bp);
    }

    #[test]
    fn instanceof_binds_tighter_than_logical_not() {
        let instanceof = binary_operator(named(TokenName::Instanceof)).expect("instanceof");
        let less_than = binary_operator(symbol(b'<')).expect("less-than");

        assert!(instanceof.left_bp >= PREFIX_RIGHT_BP);
        assert!(less_than.left_bp < PREFIX_RIGHT_BP);
    }
}
