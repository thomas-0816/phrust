//! mbstring builtin stubs.

use crate::Value;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "mb_detect_encoding",
        builtin_mb_detect_encoding,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("mb_strlen", builtin_mb_strlen, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "mb_strtolower",
        builtin_mb_strtolower,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mb_strtoupper",
        builtin_mb_strtoupper,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("mb_substr", builtin_mb_substr, BuiltinCompatibility::Php),
];

fn builtin_mb_detect_encoding(
    _context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unsupported_mbstring("mb_detect_encoding")
}

fn builtin_mb_strlen(
    _context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unsupported_mbstring("mb_strlen")
}

fn builtin_mb_strtolower(
    _context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unsupported_mbstring("mb_strtolower")
}

fn builtin_mb_strtoupper(
    _context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unsupported_mbstring("mb_strtoupper")
}

fn builtin_mb_substr(
    _context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    unsupported_mbstring("mb_substr")
}

fn unsupported_mbstring(name: &'static str) -> BuiltinResult {
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_UNSUPPORTED_MBSTRING",
        format!(
            "{name}(): mbstring is not implemented; extension_loaded(\"mbstring\") remains false"
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputBuffer;
    use crate::builtins::BuiltinRegistry;

    #[test]
    fn mbstring_stubs_report_explicit_unsupported_error() {
        let entry = BuiltinRegistry::new()
            .get("mb_strlen")
            .expect("mb_strlen stub exists");
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let error = (entry.function())(
            &mut context,
            vec![Value::string("abc")],
            RuntimeSourceSpan::default(),
        )
        .expect_err("mb_strlen must not silently succeed");

        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_UNSUPPORTED_MBSTRING");
        assert!(
            error.message().contains("mbstring is not implemented"),
            "message should make the stub explicit: {}",
            error.message()
        );
    }
}
