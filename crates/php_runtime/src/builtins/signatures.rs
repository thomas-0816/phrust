//! Shared internal builtin signatures.

use super::{BuiltinContext, BuiltinError, RuntimeSourceSpan};
use crate::Value;

/// Result returned by an internal builtin.
pub type BuiltinResult = Result<Value, BuiltinError>;

/// Small internal builtin outcome. Request-boundary state is deliberately not
/// carried here; the VM owns output, diagnostics and services separately.
#[derive(Debug)]
pub enum BuiltinOutcome {
    Return(Value),
    Error(BuiltinError),
}

impl From<BuiltinResult> for BuiltinOutcome {
    fn from(result: BuiltinResult) -> Self {
        match result {
            Ok(value) => Self::Return(value),
            Err(error) => Self::Error(error),
        }
    }
}

/// Internal builtin function signature.
pub type InternalFunction =
    fn(&mut BuiltinContext<'_>, Vec<Value>, RuntimeSourceSpan) -> BuiltinResult;
