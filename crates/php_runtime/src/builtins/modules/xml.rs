//! XML extension builtins for the bounded runtime slice.

use super::core::{arity_error, int_arg, php_argument_type_name, resolve_runtime_path, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{Value, normalize_class_name, xml};
use std::fs;
use std::io::Write;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "xmlwriter_open_memory",
        builtin_xmlwriter_open_memory,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xmlwriter_open_uri",
        builtin_xmlwriter_open_uri,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xmlwriter_start_document",
        builtin_xmlwriter_start_document,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xmlwriter_start_element",
        builtin_xmlwriter_start_element,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xmlwriter_write_attribute",
        builtin_xmlwriter_write_attribute,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xmlwriter_text",
        builtin_xmlwriter_text,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xmlwriter_write_comment",
        builtin_xmlwriter_write_comment,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xmlwriter_write_cdata",
        builtin_xmlwriter_write_cdata,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xmlwriter_write_element",
        builtin_xmlwriter_write_element,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xmlwriter_end_element",
        builtin_xmlwriter_end_element,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xmlwriter_end_document",
        builtin_xmlwriter_end_document,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xmlwriter_output_memory",
        builtin_xmlwriter_output_memory,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xmlwriter_flush",
        builtin_xmlwriter_flush,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xml_parser_create",
        builtin_xml_parser_create,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("xml_parse", builtin_xml_parse, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "xml_parse_into_struct",
        builtin_xml_parse_into_struct,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xml_set_element_handler",
        builtin_xml_set_element_handler,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xml_set_character_data_handler",
        builtin_xml_set_character_data_handler,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xml_set_default_handler",
        builtin_xml_set_default_handler,
        BuiltinCompatibility::Php,
    ),
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
        "xml_get_current_byte_index",
        builtin_xml_get_current_byte_index,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xml_get_current_line_number",
        builtin_xml_get_current_line_number,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xml_get_current_column_number",
        builtin_xml_get_current_column_number,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xml_parser_create_ns",
        builtin_xml_parser_create_ns,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xml_parser_get_option",
        builtin_xml_parser_get_option,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "xml_parser_set_option",
        builtin_xml_parser_set_option,
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
const XML_OPTION_CASE_FOLDING: i64 = 1;
const XML_OPTION_TARGET_ENCODING: i64 = 2;
const XML_OPTION_SKIP_TAGSTART: i64 = 3;
const XML_OPTION_SKIP_WHITE: i64 = 4;
const XML_OPTION_PARSE_HUGE: i64 = 5;
const XML_PARSER_CASE_FOLDING: &str = "__phrust_xml_case_folding";
const XML_PARSER_TARGET_ENCODING: &str = "__phrust_xml_target_encoding";
const XML_PARSER_SKIP_TAGSTART: &str = "__phrust_xml_skip_tagstart";
const XML_PARSER_SKIP_WHITE: &str = "__phrust_xml_skip_white";
const XML_PARSER_PARSE_HUGE: &str = "__phrust_xml_parse_huge";
const XML_PARSER_CURRENT_BYTE: &str = "__phrust_xml_current_byte";
const XML_PARSER_CURRENT_LINE: &str = "__phrust_xml_current_line";
const XML_PARSER_CURRENT_COLUMN: &str = "__phrust_xml_current_column";

fn builtin_xmlwriter_open_memory(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !args.is_empty() {
        return Err(arity_error("xmlwriter_open_memory", "no arguments"));
    }
    let object = xml::new_xml_writer();
    let _ = xml::xml_writer_open_memory(&object);
    Ok(Value::Object(object))
}

fn builtin_xmlwriter_open_uri(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("xmlwriter_open_uri", "one argument"));
    }
    let uri = string_arg("xmlwriter_open_uri", &args[0])?.to_string_lossy();
    if !write_xmlwriter_uri(context, &uri, &[]) {
        return Ok(Value::Bool(false));
    }
    let object = xml::new_xml_writer();
    let _ = xml::xml_writer_open_uri(&object, &uri);
    Ok(Value::Object(object))
}

fn builtin_xmlwriter_start_document(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=4).contains(&args.len()) {
        return Err(arity_error(
            "xmlwriter_start_document",
            "one to four argument(s)",
        ));
    }
    let writer = xml_writer_arg("xmlwriter_start_document", &args[0])?;
    Ok(xml::xml_writer_start_document(&writer))
}

fn builtin_xmlwriter_start_element(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("xmlwriter_start_element", "two arguments"));
    }
    let writer = xml_writer_arg("xmlwriter_start_element", &args[0])?;
    let name = string_arg("xmlwriter_start_element", &args[1])?;
    Ok(xml::xml_writer_start_element(
        &writer,
        &name.to_string_lossy(),
    ))
}

fn builtin_xmlwriter_write_attribute(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("xmlwriter_write_attribute", "three arguments"));
    }
    let writer = xml_writer_arg("xmlwriter_write_attribute", &args[0])?;
    let name = string_arg("xmlwriter_write_attribute", &args[1])?;
    let value = string_arg("xmlwriter_write_attribute", &args[2])?;
    Ok(xml::xml_writer_write_attribute(
        &writer,
        &name.to_string_lossy(),
        &value.to_string_lossy(),
    ))
}

fn builtin_xmlwriter_text(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("xmlwriter_text", "two arguments"));
    }
    let writer = xml_writer_arg("xmlwriter_text", &args[0])?;
    let value = string_arg("xmlwriter_text", &args[1])?;
    Ok(xml::xml_writer_text(&writer, &value.to_string_lossy()))
}

fn builtin_xmlwriter_write_comment(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("xmlwriter_write_comment", "two arguments"));
    }
    let writer = xml_writer_arg("xmlwriter_write_comment", &args[0])?;
    let value = string_arg("xmlwriter_write_comment", &args[1])?;
    Ok(xml::xml_writer_write_comment(
        &writer,
        &value.to_string_lossy(),
    ))
}

fn builtin_xmlwriter_write_cdata(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("xmlwriter_write_cdata", "two arguments"));
    }
    let writer = xml_writer_arg("xmlwriter_write_cdata", &args[0])?;
    let value = string_arg("xmlwriter_write_cdata", &args[1])?;
    Ok(xml::xml_writer_write_cdata(
        &writer,
        &value.to_string_lossy(),
    ))
}

fn builtin_xmlwriter_write_element(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error(
            "xmlwriter_write_element",
            "two or three arguments",
        ));
    }
    let writer = xml_writer_arg("xmlwriter_write_element", &args[0])?;
    let name = string_arg("xmlwriter_write_element", &args[1])?;
    let value = args
        .get(2)
        .map(string_arg_for_xmlwriter_content)
        .transpose()?;
    Ok(xml::xml_writer_write_element(
        &writer,
        &name.to_string_lossy(),
        value.as_deref(),
    ))
}

fn builtin_xmlwriter_end_element(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("xmlwriter_end_element", "one argument"));
    }
    let writer = xml_writer_arg("xmlwriter_end_element", &args[0])?;
    Ok(xml::xml_writer_end_element(&writer))
}

fn builtin_xmlwriter_end_document(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("xmlwriter_end_document", "one argument"));
    }
    let writer = xml_writer_arg("xmlwriter_end_document", &args[0])?;
    Ok(xml::xml_writer_end_document(&writer))
}

fn builtin_xmlwriter_output_memory(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error(
            "xmlwriter_output_memory",
            "one or two arguments",
        ));
    }
    let writer = xml_writer_arg("xmlwriter_output_memory", &args[0])?;
    let flush = xmlwriter_bool_arg("xmlwriter_output_memory", args.get(1))?.unwrap_or(true);
    Ok(xml::xml_writer_output_memory(&writer, flush))
}

fn builtin_xmlwriter_flush(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error("xmlwriter_flush", "one or two arguments"));
    }
    let writer = xml_writer_arg("xmlwriter_flush", &args[0])?;
    let empty = xmlwriter_bool_arg("xmlwriter_flush", args.get(1))?.unwrap_or(true);
    let Some(uri) = xml::xml_writer_uri(&writer) else {
        return Ok(xml::xml_writer_flush_memory(&writer, empty));
    };
    let bytes = xml::xml_writer_pending_bytes(&writer);
    if write_xmlwriter_uri(context, &uri, &bytes) {
        xml::xml_writer_clear_buffer(&writer);
        Ok(Value::Int(bytes.len() as i64))
    } else {
        Ok(Value::Bool(false))
    }
}

fn xml_writer_arg(function: &str, value: &Value) -> Result<crate::ObjectRef, BuiltinError> {
    match value {
        Value::Object(object) if normalize_class_name(&object.class_name()) == "xmlwriter" => {
            Ok(object.clone())
        }
        value => Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!(
                "{function}(): Argument #1 ($writer) must be of type XMLWriter, {} given",
                php_argument_type_name(value)
            ),
        )),
    }
}

fn string_arg_for_xmlwriter_content(value: &Value) -> Result<String, BuiltinError> {
    if matches!(value, Value::Null) {
        return Ok(String::new());
    }
    Ok(string_arg("xmlwriter_write_element", value)?.to_string_lossy())
}

fn xmlwriter_bool_arg(function: &str, value: Option<&Value>) -> Result<Option<bool>, BuiltinError> {
    value
        .map(|value| {
            crate::convert::to_bool(value).map_err(|message| {
                BuiltinError::new(
                    "E_PHP_RUNTIME_BUILTIN_TYPE",
                    format!("{function}(): {message}"),
                )
            })
        })
        .transpose()
}

fn write_xmlwriter_uri(context: &BuiltinContext<'_>, uri: &str, bytes: &[u8]) -> bool {
    let resolved = resolve_runtime_path(context, uri);
    if !context.filesystem_capabilities().allows_path(&resolved) {
        return false;
    }
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&resolved)
        .and_then(|mut file| file.write_all(bytes))
        .is_ok()
}

fn builtin_xml_parser_create(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("xml_parser_create", "zero or one argument(s)"));
    }
    Ok(new_xml_parser())
}

fn builtin_xml_parser_create_ns(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 2 {
        return Err(arity_error(
            "xml_parser_create_ns",
            "zero, one, or two argument(s)",
        ));
    }
    Ok(new_xml_parser())
}

fn new_xml_parser() -> Value {
    let object = xml::new_xml_parser();
    object.set_property(XML_PARSER_ERROR_CODE, Value::Int(XML_ERROR_NONE));
    object.set_property(XML_PARSER_CASE_FOLDING, Value::Bool(true));
    object.set_property(XML_PARSER_TARGET_ENCODING, Value::string("UTF-8"));
    object.set_property(XML_PARSER_SKIP_TAGSTART, Value::Int(0));
    object.set_property(XML_PARSER_SKIP_WHITE, Value::Bool(false));
    set_current_position(&object, "");
    Value::Object(object)
}

fn builtin_xml_parse(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
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
    set_current_position(&parser, input);
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

fn builtin_xml_parse_into_struct(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=4).contains(&args.len()) {
        return Err(arity_error(
            "xml_parse_into_struct",
            "three or four argument(s)",
        ));
    }
    let parser = xml_parser_arg("xml_parse_into_struct", &args[0])?;
    let input = string_arg("xml_parse_into_struct", &args[1])?;
    let input = std::str::from_utf8(input.as_bytes()).map_err(|_| {
        BuiltinError::new(
            "E_PHP_RUNTIME_XML_UTF8",
            "xml_parse_into_struct(): input must be valid UTF-8",
        )
    })?;
    let values = reference_arg("xml_parse_into_struct", 3, "values", &args[2])?;
    let index = args
        .get(3)
        .map(|arg| reference_arg("xml_parse_into_struct", 4, "index", arg))
        .transpose()?;
    set_current_position(&parser, input);
    let document = match xml::parse_xml(input) {
        Ok(document) => {
            parser.set_property(XML_PARSER_ERROR_CODE, Value::Int(XML_ERROR_NONE));
            document
        }
        Err(_) => {
            parser.set_property(XML_PARSER_ERROR_CODE, Value::Int(XML_ERROR_MISMATCHED_TAG));
            values.set(Value::Array(crate::PhpArray::new()));
            if let Some(index) = index {
                index.set(Value::Array(crate::PhpArray::new()));
            }
            return Ok(Value::Int(0));
        }
    };
    let case_folding = match parser
        .get_property(XML_PARSER_CASE_FOLDING)
        .unwrap_or(Value::Bool(true))
    {
        Value::Bool(enabled) => enabled,
        value => crate::convert::to_bool(&value)
            .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_BUILTIN_TYPE", message))?,
    };
    let (struct_values, struct_index) = xml::parse_into_struct_arrays(&document, case_folding);
    values.set(Value::Array(struct_values));
    if let Some(index) = index {
        index.set(Value::Array(struct_index));
    }
    Ok(Value::Int(1))
}

fn builtin_xml_set_element_handler(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("xml_set_element_handler", "three arguments"));
    }
    let parser = xml_parser_arg("xml_set_element_handler", &args[0])?;
    let start = xml_handler_arg("xml_set_element_handler", 2, "start_handler", &args[1])?;
    let end = xml_handler_arg("xml_set_element_handler", 3, "end_handler", &args[2])?;
    parser.set_property(xml::XML_PARSER_START_ELEMENT_HANDLER, start);
    parser.set_property(xml::XML_PARSER_END_ELEMENT_HANDLER, end);
    Ok(Value::Bool(true))
}

fn reference_arg(
    function: &str,
    position: usize,
    param: &str,
    value: &Value,
) -> Result<crate::ReferenceCell, BuiltinError> {
    match value {
        Value::Reference(cell) => Ok(cell.clone()),
        value => Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!(
                "{function}(): Argument #{position} (${param}) must be passed by reference, {} given",
                php_argument_type_name(value)
            ),
        )),
    }
}

fn builtin_xml_set_character_data_handler(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error(
            "xml_set_character_data_handler",
            "two arguments",
        ));
    }
    let parser = xml_parser_arg("xml_set_character_data_handler", &args[0])?;
    let handler = xml_handler_arg("xml_set_character_data_handler", 2, "handler", &args[1])?;
    parser.set_property(xml::XML_PARSER_CHARACTER_DATA_HANDLER, handler);
    Ok(Value::Bool(true))
}

fn builtin_xml_set_default_handler(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("xml_set_default_handler", "two arguments"));
    }
    let parser = xml_parser_arg("xml_set_default_handler", &args[0])?;
    let handler = xml_handler_arg("xml_set_default_handler", 2, "handler", &args[1])?;
    parser.set_property(xml::XML_PARSER_DEFAULT_HANDLER, handler);
    Ok(Value::Bool(true))
}

fn xml_handler_arg(
    function: &str,
    position: usize,
    param: &str,
    value: &Value,
) -> Result<Value, BuiltinError> {
    match value {
        Value::Null
        | Value::String(_)
        | Value::Array(_)
        | Value::Object(_)
        | Value::Callable(_) => Ok(value.clone()),
        value => Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!(
                "{function}(): Argument #{position} (${param}) must be of type callable|string|null, {} given",
                php_argument_type_name(value)
            ),
        )),
    }
}

fn set_current_position(parser: &crate::ObjectRef, input: &str) {
    parser.set_property(XML_PARSER_CURRENT_BYTE, Value::Int(input.len() as i64));
    let line = input.bytes().filter(|byte| *byte == b'\n').count() as i64 + 1;
    let column = input
        .rsplit_once('\n')
        .map(|(_, tail)| tail.len() as i64 + 1)
        .unwrap_or(input.len() as i64 + 1);
    parser.set_property(XML_PARSER_CURRENT_LINE, Value::Int(line));
    parser.set_property(XML_PARSER_CURRENT_COLUMN, Value::Int(column));
}

fn builtin_xml_get_error_code(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
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
    args: crate::builtins::BuiltinArgs,
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

fn builtin_xml_get_current_byte_index(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    xml_current_position("xml_get_current_byte_index", args, XML_PARSER_CURRENT_BYTE)
}

fn builtin_xml_get_current_line_number(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    xml_current_position("xml_get_current_line_number", args, XML_PARSER_CURRENT_LINE)
}

fn builtin_xml_get_current_column_number(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    xml_current_position(
        "xml_get_current_column_number",
        args,
        XML_PARSER_CURRENT_COLUMN,
    )
}

fn xml_current_position(name: &str, args: crate::builtins::BuiltinArgs, property: &str) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error(name, "one argument"));
    }
    let parser = xml_parser_arg(name, &args[0])?;
    Ok(parser.get_property(property).unwrap_or(Value::Int(0)))
}

fn builtin_xml_parser_get_option(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("xml_parser_get_option", "two arguments"));
    }
    let parser = xml_parser_arg("xml_parser_get_option", &args[0])?;
    let option = int_arg("xml_parser_get_option", &args[1])?;
    Ok(match option {
        XML_OPTION_CASE_FOLDING => parser
            .get_property(XML_PARSER_CASE_FOLDING)
            .unwrap_or(Value::Bool(true)),
        XML_OPTION_TARGET_ENCODING => parser
            .get_property(XML_PARSER_TARGET_ENCODING)
            .unwrap_or_else(|| Value::string("UTF-8")),
        XML_OPTION_SKIP_TAGSTART => parser
            .get_property(XML_PARSER_SKIP_TAGSTART)
            .unwrap_or(Value::Int(0)),
        XML_OPTION_SKIP_WHITE => parser
            .get_property(XML_PARSER_SKIP_WHITE)
            .unwrap_or(Value::Bool(false)),
        XML_OPTION_PARSE_HUGE => parser
            .get_property(XML_PARSER_PARSE_HUGE)
            .unwrap_or(Value::Bool(false)),
        _ => Value::Bool(false),
    })
}

fn builtin_xml_parser_set_option(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("xml_parser_set_option", "three arguments"));
    }
    let parser = xml_parser_arg("xml_parser_set_option", &args[0])?;
    let option = int_arg("xml_parser_set_option", &args[1])?;
    match option {
        XML_OPTION_CASE_FOLDING => {
            let enabled = crate::convert::to_bool(&args[2])
                .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_BUILTIN_TYPE", message))?;
            parser.set_property(XML_PARSER_CASE_FOLDING, Value::Bool(enabled));
        }
        XML_OPTION_TARGET_ENCODING => {
            let encoding = string_arg("xml_parser_set_option", &args[2])?;
            parser.set_property(XML_PARSER_TARGET_ENCODING, Value::String(encoding));
        }
        XML_OPTION_SKIP_TAGSTART => {
            let offset = int_arg("xml_parser_set_option", &args[2])?;
            if offset < 0 {
                return Ok(Value::Bool(false));
            }
            parser.set_property(XML_PARSER_SKIP_TAGSTART, Value::Int(offset));
        }
        XML_OPTION_SKIP_WHITE => {
            let enabled = crate::convert::to_bool(&args[2])
                .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_BUILTIN_TYPE", message))?;
            parser.set_property(XML_PARSER_SKIP_WHITE, Value::Bool(enabled));
        }
        XML_OPTION_PARSE_HUGE => {
            let enabled = crate::convert::to_bool(&args[2])
                .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_BUILTIN_TYPE", message))?;
            parser.set_property(XML_PARSER_PARSE_HUGE, Value::Bool(enabled));
        }
        _ => return Ok(Value::Bool(false)),
    }
    Ok(Value::Bool(true))
}

fn builtin_xml_parser_free(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("xml_parser_free", "one argument"));
    }
    match &args[0] {
        Value::Object(object) if normalize_class_name(&object.class_name()) == "xmlparser" => {
            context.php_deprecation(
                "E_PHP_RUNTIME_XML_PARSER_FREE_DEPRECATED",
                "Function xml_parser_free() is deprecated since 8.5, as it has no effect since PHP 8.0",
                span,
            );
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

fn xml_parser_arg(name: &str, value: &Value) -> Result<crate::ObjectRef, BuiltinError> {
    match value {
        Value::Object(object) if normalize_class_name(&object.class_name()) == "xmlparser" => {
            Ok(object.clone())
        }
        value => Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!(
                "{name}(): Argument #1 ($parser) must be of type XMLParser, {} given",
                php_argument_type_name(value)
            ),
        )),
    }
}
