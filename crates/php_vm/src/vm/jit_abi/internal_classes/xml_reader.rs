use super::*;

fn is_xml_reader(class_name: &str) -> bool {
    normalize_class_name(class_name) == "xmlreader"
}

pub(in crate::vm::jit_abi) fn construct_native_xml_reader(
    context: &mut NativeExecutionContext<'_>,
    class_name: &str,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    if !is_xml_reader(class_name) {
        return None;
    }
    let result = decode_arguments(context, arguments)
        .and_then(|arguments| {
            expect_arity("XMLReader::__construct", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::new_xml_reader())
        })
        .and_then(|object| context.encode(Value::Object(object)));
    Some(result)
}

fn call_xml_reader_method(
    context: &NativeExecutionContext<'_>,
    object: &php_runtime::api::ObjectRef,
    method: &str,
    arguments: Vec<Value>,
) -> Result<Value, String> {
    match method.to_ascii_lowercase().as_str() {
        "xml" => {
            expect_arity("XMLReader::XML", arguments.len(), 1, 1)?;
            let xml = string_argument("XMLReader::XML", arguments[0].clone())?;
            php_runtime::api::xml::xml_reader_xml(object, &xml)
        }
        "open" => {
            expect_arity("XMLReader::open", arguments.len(), 1, 3)?;
            let path = string_argument("XMLReader::open", arguments[0].clone())?;
            let Some(source) = read_runtime_file(context, &path)? else {
                return Ok(Value::Bool(false));
            };
            php_runtime::api::xml::xml_reader_xml(object, &source)
        }
        "read" => {
            expect_arity("XMLReader::read", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::xml_reader_read(object))
        }
        "next" => {
            expect_arity("XMLReader::next", arguments.len(), 0, 1)?;
            let name = arguments
                .into_iter()
                .next()
                .map(|value| string_argument("XMLReader::next", value))
                .transpose()?;
            Ok(php_runtime::api::xml::xml_reader_next(
                object,
                name.as_deref(),
            ))
        }
        "getattribute" => {
            expect_arity("XMLReader::getAttribute", arguments.len(), 1, 1)?;
            let name = string_argument("XMLReader::getAttribute", arguments[0].clone())?;
            Ok(php_runtime::api::xml::xml_reader_get_attribute(
                object, &name,
            ))
        }
        "getattributeno" => {
            expect_arity("XMLReader::getAttributeNo", arguments.len(), 1, 1)?;
            let index = int_argument("XMLReader::getAttributeNo", &arguments[0])?;
            Ok(php_runtime::api::xml::xml_reader_get_attribute_no(
                object, index,
            ))
        }
        "lookupnamespace" => {
            expect_arity("XMLReader::lookupNamespace", arguments.len(), 1, 1)?;
            let prefix = string_argument("XMLReader::lookupNamespace", arguments[0].clone())?;
            Ok(php_runtime::api::xml::xml_reader_lookup_namespace(
                object, &prefix,
            ))
        }
        "movetoattribute" => {
            expect_arity("XMLReader::moveToAttribute", arguments.len(), 1, 1)?;
            let name = string_argument("XMLReader::moveToAttribute", arguments[0].clone())?;
            Ok(php_runtime::api::xml::xml_reader_move_to_attribute(
                object, &name,
            ))
        }
        "movetoattributeno" => {
            expect_arity("XMLReader::moveToAttributeNo", arguments.len(), 1, 1)?;
            let index = int_argument("XMLReader::moveToAttributeNo", &arguments[0])?;
            Ok(php_runtime::api::xml::xml_reader_move_to_attribute_no(
                object, index,
            ))
        }
        "movetofirstattribute" => {
            expect_arity("XMLReader::moveToFirstAttribute", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::xml_reader_move_to_first_attribute(
                object,
            ))
        }
        "movetonextattribute" => {
            expect_arity("XMLReader::moveToNextAttribute", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::xml_reader_move_to_next_attribute(
                object,
            ))
        }
        "movetoelement" => {
            expect_arity("XMLReader::moveToElement", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::xml_reader_move_to_element(object))
        }
        "readstring" => {
            expect_arity("XMLReader::readString", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::xml_reader_read_string(object))
        }
        "readinnerxml" => {
            expect_arity("XMLReader::readInnerXml", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::xml_reader_read_inner_xml(object))
        }
        "readouterxml" => {
            expect_arity("XMLReader::readOuterXml", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::xml_reader_read_outer_xml(object))
        }
        "expand" => {
            expect_arity("XMLReader::expand", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::xml_reader_expand(object))
        }
        "close" => {
            expect_arity("XMLReader::close", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::xml_reader_close(object))
        }
        method => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method XMLReader::{method} is not implemented in the native XMLReader slice"
        )),
    }
}

pub(in crate::vm::jit_abi) fn execute_native_xml_reader_instruction(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    match &instruction.kind {
        php_ir::InstructionKind::NewObject {
            display_class_name, ..
        } => construct_native_xml_reader(context, display_class_name, arguments),
        php_ir::InstructionKind::CallMethod { method, .. } => {
            let receiver = arguments.first().copied()?;
            let receiver = match context.decode(receiver) {
                Ok(Value::Reference(reference)) => reference.get(),
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let Value::Object(object) = receiver else {
                return None;
            };
            if !is_xml_reader(&object.class_name()) {
                return None;
            }
            let result = decode_arguments(context, &arguments[1..])
                .and_then(|arguments| call_xml_reader_method(context, &object, method, arguments))
                .and_then(|value| context.encode(value));
            Some(result)
        }
        _ => None,
    }
}

pub(in crate::vm::jit_abi) fn xml_reader_class_constant(
    class_name: &str,
    constant: &str,
) -> Option<Value> {
    if !is_xml_reader(class_name) {
        return None;
    }
    let value = match constant.to_ascii_uppercase().as_str() {
        "NONE" => php_runtime::api::xml::XML_READER_NONE,
        "ELEMENT" => php_runtime::api::xml::XML_READER_ELEMENT,
        "ATTRIBUTE" => php_runtime::api::xml::XML_READER_ATTRIBUTE,
        "TEXT" => php_runtime::api::xml::XML_READER_TEXT,
        "END_ELEMENT" => php_runtime::api::xml::XML_READER_END_ELEMENT,
        _ => return None,
    };
    Some(Value::Int(value))
}
