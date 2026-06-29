//! XML extension builtins for the bounded runtime slice.

use super::core::{arity_error, int_arg, php_argument_type_name, string_arg};
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
    BuiltinEntry::new(
        "xml_get_error_code",
        builtin_xml_get_error_code,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xml_error_string",
        builtin_xml_error_string,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xml_parser_free",
        builtin_xml_parser_free,
        BuiltinCompatibility::Php,
    ),
];

const XML_ERROR_NONE: i64 = 0;
const XML_ERROR_MISMATCHED_TAG: i64 = 76;
const XML_PARSER_ERROR_CODE: &str = "__phrust_xml_error_code";

fn builtin_xml_parser_create(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("xml_parser_create", "zero or one argument(s)"));
    }
    let object = xml::new_xml_parser();
    object.set_property(XML_PARSER_ERROR_CODE, Value::Int(XML_ERROR_NONE));
    Ok(Value::Object(object))
}

fn builtin_xml_parse(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("xml_parse", "two or three argument(s)"));
    }
    let parser = match &args[0] {
        Value::Object(object) if normalize_class_name(&object.class_name()) == "xmlparser" => {
            object.clone()
        }
        value => {
            return Err(BuiltinError::new(
                "E_PHP_RUNTIME_BUILTIN_TYPE",
                format!(
                    "xml_parse(): Argument #1 ($parser) must be of type XMLParser, {} given",
                    php_argument_type_name(value)
                ),
            ));
        }
    };
    let input = string_arg("xml_parse", &args[1])?;
    let input = std::str::from_utf8(input.as_bytes()).map_err(|_| {
        BuiltinError::new(
            "E_PHP_RUNTIME_XML_UTF8",
            "xml_parse(): input must be valid UTF-8",
        )
    })?;
    let ok = xml::parse_xml(input).is_ok();
    parser.set_property(
        XML_PARSER_ERROR_CODE,
        Value::Int(if ok {
            XML_ERROR_NONE
        } else {
            XML_ERROR_MISMATCHED_TAG
        }),
    );
    Ok(Value::Int(i64::from(ok)))
}

fn builtin_xml_get_error_code(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("xml_get_error_code", "one argument"));
    }
    let parser = match &args[0] {
        Value::Object(object) if normalize_class_name(&object.class_name()) == "xmlparser" => {
            object
        }
        value => {
            return Err(BuiltinError::new(
                "E_PHP_RUNTIME_BUILTIN_TYPE",
                format!(
                    "xml_get_error_code(): Argument #1 ($parser) must be of type XMLParser, {} given",
                    php_argument_type_name(value)
                ),
            ));
        }
    };
    Ok(parser
        .get_property(XML_PARSER_ERROR_CODE)
        .unwrap_or(Value::Int(XML_ERROR_NONE)))
}

fn builtin_xml_error_string(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("xml_error_string", "one argument"));
    }
    let code = int_arg("xml_error_string", &args[0])?;
    let message = match code {
        XML_ERROR_NONE => "No error",
        XML_ERROR_MISMATCHED_TAG => "Mismatched tag",
        _ => "syntax error",
    };
    Ok(Value::string(message.as_bytes().to_vec()))
}

fn builtin_xml_parser_free(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("xml_parser_free", "one argument"));
    }
    match &args[0] {
        Value::Object(object) if normalize_class_name(&object.class_name()) == "xmlparser" => {
            Ok(Value::Bool(true))
        }
        value => Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!(
                "xml_parser_free(): Argument #1 ($parser) must be of type XMLParser, {} given",
                php_argument_type_name(value)
            ),
        )),
    }
}
