//! XML extension builtins for the bounded runtime slice.

use super::core::{arity_error, php_argument_type_name, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{Value, normalize_class_name, xml};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "xml_parser_create",
        builtin_xml_parser_create,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("xml_parse", builtin_xml_parse, BuiltinCompatibility::Php),
];

fn builtin_xml_parser_create(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("xml_parser_create", "zero or one argument(s)"));
    }
    Ok(Value::Object(xml::new_xml_parser()))
}

fn builtin_xml_parse(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("xml_parse", "two or three argument(s)"));
    }
    match &args[0] {
        Value::Object(object) if normalize_class_name(&object.class_name()) == "xmlparser" => {}
        value => {
            return Err(BuiltinError::new(
                "E_PHP_RUNTIME_BUILTIN_TYPE",
                format!(
                    "xml_parse(): Argument #1 ($parser) must be of type XMLParser, {} given",
                    php_argument_type_name(value)
                ),
            ));
        }
    }
    let input = string_arg("xml_parse", &args[1])?;
    let input = std::str::from_utf8(input.as_bytes()).map_err(|_| {
        BuiltinError::new(
            "E_PHP_RUNTIME_XML_UTF8",
            "xml_parse(): input must be valid UTF-8",
        )
    })?;
    Ok(Value::Int(i64::from(xml::parse_xml(input).is_ok())))
}
