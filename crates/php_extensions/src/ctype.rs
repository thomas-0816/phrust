//! ASCII C-locale ctype extension.

use php_runtime::api::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult, RuntimeSourceSpan, Value,
    value_type_name,
};
use php_runtime::api::{BuiltinError, ExtensionDescriptor, ExtensionModule};

pub(crate) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "ctype_alnum",
        builtin_ctype_alnum,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ctype_alpha",
        builtin_ctype_alpha,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ctype_cntrl",
        builtin_ctype_cntrl,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ctype_digit",
        builtin_ctype_digit,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ctype_graph",
        builtin_ctype_graph,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ctype_lower",
        builtin_ctype_lower,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ctype_print",
        builtin_ctype_print,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ctype_punct",
        builtin_ctype_punct,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ctype_space",
        builtin_ctype_space,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ctype_upper",
        builtin_ctype_upper,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ctype_xdigit",
        builtin_ctype_xdigit,
        BuiltinCompatibility::Php,
    ),
];

pub(crate) struct CtypeExtension;

static DESCRIPTOR: ExtensionDescriptor = ExtensionDescriptor {
    name: crate::generated::CTYPE.name,
    version: crate::generated::CTYPE.version,
    dependencies: crate::generated::CTYPE.dependencies,
    functions: ENTRIES,
    classes: &[],
    constants: &[],
    request_state: None,
    capabilities: crate::generated::CTYPE.capabilities,
    initialize: None,
    shutdown: None,
};

impl ExtensionModule for CtypeExtension {
    fn descriptor(&self) -> &'static ExtensionDescriptor {
        &DESCRIPTOR
    }
}

fn arity_error(name: &str, expected: &str) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_ARITY",
        format!("builtin {name} expects {expected}"),
    )
}

fn deref_value(value: &Value) -> Value {
    match value {
        Value::Reference(cell) => cell.get(),
        value => value.clone(),
    }
}

macro_rules! ctype_builtin {
    ($name:ident, $php_name:literal, $predicate:expr, $allow_digits:literal, $allow_minus:literal) => {
        fn $name(
            context: &mut BuiltinContext<'_>,
            args: Vec<Value>,
            span: RuntimeSourceSpan,
        ) -> BuiltinResult {
            if args.len() != 1 {
                return Err(arity_error($php_name, "one argument"));
            }
            Ok(Value::Bool(ctype_check(
                context,
                $php_name,
                &args[0],
                $predicate,
                $allow_digits,
                $allow_minus,
                span,
            )))
        }
    };
}

fn ctype_check(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: &Value,
    predicate: impl Fn(u8) -> bool,
    allow_digits: bool,
    allow_minus: bool,
    span: RuntimeSourceSpan,
) -> bool {
    match deref_value(value) {
        Value::String(input) => {
            let bytes = input.as_bytes();
            !bytes.is_empty() && bytes.iter().copied().all(predicate)
        }
        Value::Int(code) => {
            ctype_deprecation(context, name, value, span);
            if (0..=255).contains(&code) {
                predicate(code as u8)
            } else if (-128..0).contains(&code) {
                predicate((code + 256) as u8)
            } else if code >= 0 {
                allow_digits
            } else {
                allow_minus
            }
        }
        _ => {
            ctype_deprecation(context, name, value, span);
            false
        }
    }
}

fn ctype_deprecation(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: &Value,
    span: RuntimeSourceSpan,
) {
    context.php_deprecation(
        format!("E_PHP_RUNTIME_{}_NON_STRING_ARG", name.to_ascii_uppercase()),
        format!(
            "{name}(): Argument of type {} will be interpreted as string in the future",
            ctype_argument_type_name(value)
        ),
        span,
    );
}

fn ctype_argument_type_name(value: &Value) -> String {
    match deref_value(value) {
        Value::Object(object) => object.display_name(),
        other => value_type_name(&other).to_owned(),
    }
}

ctype_builtin!(
    builtin_ctype_alnum,
    "ctype_alnum",
    |byte: u8| byte.is_ascii_alphanumeric(),
    true,
    false
);
ctype_builtin!(
    builtin_ctype_alpha,
    "ctype_alpha",
    |byte: u8| byte.is_ascii_alphabetic(),
    false,
    false
);
ctype_builtin!(
    builtin_ctype_cntrl,
    "ctype_cntrl",
    |byte: u8| byte.is_ascii_control(),
    false,
    false
);
ctype_builtin!(
    builtin_ctype_digit,
    "ctype_digit",
    |byte: u8| byte.is_ascii_digit(),
    true,
    false
);
ctype_builtin!(
    builtin_ctype_graph,
    "ctype_graph",
    |byte: u8| byte.is_ascii_graphic(),
    true,
    true
);
ctype_builtin!(
    builtin_ctype_lower,
    "ctype_lower",
    |byte: u8| byte.is_ascii_lowercase(),
    false,
    false
);
ctype_builtin!(
    builtin_ctype_print,
    "ctype_print",
    |byte: u8| byte.is_ascii_graphic() || byte == b' ',
    true,
    true
);
ctype_builtin!(
    builtin_ctype_punct,
    "ctype_punct",
    |byte: u8| byte.is_ascii_punctuation(),
    false,
    false
);
ctype_builtin!(
    builtin_ctype_space,
    "ctype_space",
    |byte: u8| matches!(byte, b'\t' | b'\n' | 0x0B | 0x0C | b'\r' | b' '),
    false,
    false
);
ctype_builtin!(
    builtin_ctype_upper,
    "ctype_upper",
    |byte: u8| byte.is_ascii_uppercase(),
    false,
    false
);
ctype_builtin!(
    builtin_ctype_xdigit,
    "ctype_xdigit",
    |byte: u8| byte.is_ascii_hexdigit(),
    true,
    false
);

#[cfg(test)]
mod tests {
    use super::*;
    use php_runtime::api::{ClassEntry, ClassFlags, ObjectRef, OutputBuffer, PhpString};

    fn call(name: &str, value: Value) -> (Value, Vec<String>) {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let result =
            ENTRIES
                .iter()
                .find(|entry| entry.name() == name)
                .expect("ctype entry")
                .function()(&mut context, vec![value], RuntimeSourceSpan::default())
            .expect("ctype succeeds");
        let diagnostics = context
            .take_diagnostics()
            .into_iter()
            .map(|diagnostic| diagnostic.id().to_owned())
            .collect();
        (result, diagnostics)
    }

    #[test]
    fn ctype_strings_are_ascii_byte_classified() {
        assert_eq!(
            call("ctype_digit", Value::String(PhpString::from("0123"))).0,
            Value::Bool(true)
        );
        assert_eq!(
            call("ctype_digit", Value::String(PhpString::from(""))).0,
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "ctype_alpha",
                Value::String(PhpString::from_bytes(vec![b'a', 0xE9]))
            )
            .0,
            Value::Bool(false)
        );
        assert_eq!(
            call(
                "ctype_space",
                Value::String(PhpString::from("\u{0b}\u{0c}"))
            )
            .0,
            Value::Bool(true)
        );
    }

    #[test]
    fn ctype_ints_use_legacy_codepoint_fallbacks() {
        assert_eq!(call("ctype_digit", Value::Int(48)).0, Value::Bool(true));
        assert_eq!(call("ctype_digit", Value::Int(65)).0, Value::Bool(false));
        assert_eq!(call("ctype_upper", Value::Int(-65)).0, Value::Bool(false));
        assert_eq!(call("ctype_digit", Value::Int(256)).0, Value::Bool(true));
        assert_eq!(call("ctype_xdigit", Value::Int(256)).0, Value::Bool(true));
        assert_eq!(call("ctype_alpha", Value::Int(256)).0, Value::Bool(false));
        assert_eq!(call("ctype_graph", Value::Int(-129)).0, Value::Bool(true));
        assert_eq!(call("ctype_digit", Value::Int(-129)).0, Value::Bool(false));
    }

    #[test]
    fn ctype_non_strings_deprecate_and_return_false() {
        let (result, diagnostics) = call("ctype_digit", Value::Bool(true));
        assert_eq!(result, Value::Bool(false));
        assert_eq!(
            diagnostics,
            vec!["E_PHP_RUNTIME_CTYPE_DIGIT_NON_STRING_ARG"]
        );

        let (result, diagnostics) = call("ctype_digit", Value::Null);
        assert_eq!(result, Value::Bool(false));
        assert_eq!(
            diagnostics,
            vec!["E_PHP_RUNTIME_CTYPE_DIGIT_NON_STRING_ARG"]
        );
    }

    #[test]
    fn ctype_object_deprecation_names_class() {
        let class = ClassEntry {
            name: "classa".to_owned().into(),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        };
        assert_eq!(
            ctype_argument_type_name(&Value::Object(ObjectRef::new_with_display_name(
                &class, "classA",
            ))),
            "classA"
        );
    }
}
