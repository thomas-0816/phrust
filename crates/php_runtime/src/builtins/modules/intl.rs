//! intl builtin stubs.

use crate::Value;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "grapheme_strlen",
        builtin_grapheme_strlen,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "intl_get_error_code",
        builtin_intl_get_error_code,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "normalizer_normalize",
        builtin_normalizer_normalize,
        BuiltinCompatibility::Php,
    ),
];

fn builtin_grapheme_strlen(
    _context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unsupported_intl("grapheme_strlen")
}

fn builtin_intl_get_error_code(
    _context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unsupported_intl("intl_get_error_code")
}

fn builtin_normalizer_normalize(
    _context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unsupported_intl("normalizer_normalize")
}

fn unsupported_intl(name: &'static str) -> BuiltinResult {
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_UNSUPPORTED_INTL",
        format!("{name}(): intl is not implemented; extension_loaded(\"intl\") remains false"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputBuffer;
    use crate::builtins::BuiltinRegistry;

    #[test]
    fn intl_stubs_report_explicit_unsupported_error() {
        let entry = BuiltinRegistry::new()
            .get("intl_get_error_code")
            .expect("intl_get_error_code stub exists");
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let error = (entry.function())(&mut context, vec![], RuntimeSourceSpan::default())
            .expect_err("intl stub must not silently succeed");

        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_UNSUPPORTED_INTL");
        assert!(
            error.message().contains("intl is not implemented"),
            "message should make the stub explicit: {}",
            error.message()
        );
    }
}
