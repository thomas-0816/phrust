//! Shared parser recovery sets.

use crate::grammar::{named, symbol};
use crate::parser::core::Parser;
use php_lexer::TokenName;

/// Statement recovery tokens: `;`, `}`, `T_CLOSE_TAG`, EOF.
pub const STMT_RECOVERY: &[&str] = &[";", "}", "T_CLOSE_TAG", "EOF"];
/// Expression recovery tokens: `,`, `;`, `)`, `]`, `}`, `T_CLOSE_TAG`, EOF.
pub const EXPR_RECOVERY: &[&str] = &[",", ";", ")", "]", "}", "T_CLOSE_TAG", "EOF"];
/// Parameter recovery tokens: `,`, `)`, EOF.
pub const PARAM_RECOVERY: &[&str] = &[",", ")", "EOF"];
/// Argument recovery tokens: `,`, `)`, EOF.
pub const ARG_RECOVERY: &[&str] = &[",", ")", "EOF"];
/// Class member recovery tokens: `;`, `}`, EOF.
pub const MEMBER_RECOVERY: &[&str] = &[";", "}", "EOF"];
/// Type recovery tokens: `,`, `)`, `;`, `=`, `{`, EOF.
pub const TYPE_RECOVERY: &[&str] = &[",", ")", ";", "=", "{", "EOF"];
/// Attribute recovery tokens: `]`, `)`, EOF.
pub const ATTRIBUTE_RECOVERY: &[&str] = &["]", ")", "EOF"];

/// Expected terminators for a simple PHP statement.
pub const STATEMENT_TERMINATORS: &[&str] = STMT_RECOVERY;

/// Returns true when the current token can synchronize statement recovery.
#[must_use]
pub fn at_statement_recovery_token(parser: &Parser<'_>) -> bool {
    parser.is_eof()
        || parser.at(symbol(b';'))
        || parser.at(symbol(b'}'))
        || parser.at(named(TokenName::CloseTag))
}

/// Returns true when the current token can synchronize expression recovery.
#[must_use]
pub fn at_expr_recovery_token(parser: &Parser<'_>) -> bool {
    at_statement_recovery_token(parser)
        || parser.at(symbol(b','))
        || parser.at(symbol(b')'))
        || parser.at(symbol(b']'))
}

/// Returns true when the current token can synchronize parameter recovery.
#[must_use]
pub fn at_param_recovery_token(parser: &Parser<'_>) -> bool {
    parser.is_eof() || parser.at(symbol(b',')) || parser.at(symbol(b')'))
}

/// Returns true when the current token can synchronize argument recovery.
#[must_use]
pub fn at_arg_recovery_token(parser: &Parser<'_>) -> bool {
    at_param_recovery_token(parser)
}

/// Returns true when the current token can synchronize class member recovery.
#[must_use]
pub fn at_member_recovery_token(parser: &Parser<'_>) -> bool {
    parser.is_eof() || parser.at(symbol(b';')) || parser.at(symbol(b'}'))
}

/// Returns true when the current token can synchronize type recovery.
#[must_use]
pub fn at_type_recovery_token(parser: &Parser<'_>) -> bool {
    parser.is_eof()
        || parser.at(symbol(b','))
        || parser.at(symbol(b')'))
        || parser.at(symbol(b';'))
        || parser.at(symbol(b'='))
        || parser.at(symbol(b'{'))
}

/// Returns true when the current token can synchronize attribute recovery.
#[must_use]
pub fn at_attribute_recovery_token(parser: &Parser<'_>) -> bool {
    parser.is_eof() || parser.at(symbol(b']')) || parser.at(symbol(b')'))
}

/// Consumes tokens until a statement recovery token is reached.
pub fn recover_to_statement_boundary(parser: &mut Parser<'_>) {
    recover_until(parser, at_statement_recovery_token);
}

/// Consumes tokens until an expression recovery token is reached.
pub fn recover_to_expression_boundary(parser: &mut Parser<'_>) {
    recover_until(parser, at_expr_recovery_token);
}

/// Consumes tokens until a parameter recovery token is reached.
pub fn recover_to_parameter_boundary(parser: &mut Parser<'_>) {
    recover_until(parser, at_param_recovery_token);
}

/// Consumes tokens until an argument recovery token is reached.
pub fn recover_to_argument_boundary(parser: &mut Parser<'_>) {
    recover_until(parser, at_arg_recovery_token);
}

/// Consumes tokens until a member recovery token is reached.
pub fn recover_to_member_boundary(parser: &mut Parser<'_>) {
    recover_until(parser, at_member_recovery_token);
}

/// Consumes tokens until a type recovery token is reached.
pub fn recover_to_type_boundary(parser: &mut Parser<'_>) {
    recover_until(parser, at_type_recovery_token);
}

/// Consumes tokens until an attribute recovery token is reached.
pub fn recover_to_attribute_boundary(parser: &mut Parser<'_>) {
    recover_until(parser, at_attribute_recovery_token);
}

fn recover_until(parser: &mut Parser<'_>, boundary: fn(&Parser<'_>) -> bool) {
    while !boundary(parser) {
        let before = parser.position();
        parser.bump();
        debug_assert!(
            parser.is_eof() || parser.position() > before,
            "recovery must consume a token or reach EOF"
        );
        if parser.position() == before {
            break;
        }
    }
}
