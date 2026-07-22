use super::*;

fn is_xml_writer(class_name: &str) -> bool {
    normalize_class_name(class_name) == "xmlwriter"
}

fn new_memory_writer() -> php_runtime::api::ObjectRef {
    let object = php_runtime::api::xml::new_xml_writer();
    let _ = php_runtime::api::xml::xml_writer_open_memory(&object);
    object
}

fn open_uri_writer(
    context: &NativeRequestColdState<'_>,
    uri: &str,
) -> Option<php_runtime::api::ObjectRef> {
    let path = normalize_runtime_path(context, uri);
    if !context
        .options
        .runtime_context
        .filesystem
        .allows_path(&path)
    {
        return None;
    }
    let object = php_runtime::api::xml::new_xml_writer();
    let _ = php_runtime::api::xml::xml_writer_open_uri(&object, uri);
    Some(object)
}

pub(in crate::vm::jit_abi) fn construct_native_xml_writer(
    context: &mut NativeRequestColdState<'_>,
    class_name: &str,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    if !is_xml_writer(class_name) {
        return None;
    }
    let result = decode_arguments(context, arguments)
        .and_then(|arguments| {
            expect_arity("XMLWriter::__construct", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::new_xml_writer())
        })
        .and_then(|object| context.encode(Value::Object(object)));
    Some(result)
}

fn flush_writer(
    context: &NativeRequestColdState<'_>,
    object: &php_runtime::api::ObjectRef,
    empty: bool,
) -> Value {
    let Some(uri) = php_runtime::api::xml::xml_writer_uri(object) else {
        return php_runtime::api::xml::xml_writer_flush_memory(object, empty);
    };
    let path = normalize_runtime_path(context, &uri);
    if !context
        .options
        .runtime_context
        .filesystem
        .allows_path(&path)
    {
        return Value::Bool(false);
    }
    let bytes = php_runtime::api::xml::xml_writer_pending_bytes(object);
    if std::fs::write(path, &bytes).is_err() {
        return Value::Bool(false);
    }
    if empty {
        php_runtime::api::xml::xml_writer_clear_buffer(object);
    }
    Value::Int(bytes.len() as i64)
}

fn call_xml_writer_method(
    context: &NativeRequestColdState<'_>,
    object: &php_runtime::api::ObjectRef,
    method: &str,
    arguments: Vec<Value>,
) -> Result<Value, String> {
    match method.to_ascii_lowercase().as_str() {
        "openmemory" => {
            expect_arity("XMLWriter::openMemory", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::xml_writer_open_memory(object))
        }
        "openuri" => {
            expect_arity("XMLWriter::openUri", arguments.len(), 1, 1)?;
            let uri = string_argument("XMLWriter::openUri", arguments[0].clone())?;
            let path = normalize_runtime_path(context, &uri);
            if !context
                .options
                .runtime_context
                .filesystem
                .allows_path(&path)
            {
                return Ok(Value::Bool(false));
            }
            Ok(php_runtime::api::xml::xml_writer_open_uri(object, &uri))
        }
        "startdocument" => {
            expect_arity("XMLWriter::startDocument", arguments.len(), 0, 3)?;
            Ok(php_runtime::api::xml::xml_writer_start_document(object))
        }
        "startelement" => {
            expect_arity("XMLWriter::startElement", arguments.len(), 1, 1)?;
            let name = string_argument("XMLWriter::startElement", arguments[0].clone())?;
            Ok(php_runtime::api::xml::xml_writer_start_element(
                object, &name,
            ))
        }
        "writeattribute" => {
            expect_arity("XMLWriter::writeAttribute", arguments.len(), 2, 2)?;
            let name = string_argument("XMLWriter::writeAttribute", arguments[0].clone())?;
            let value = string_argument("XMLWriter::writeAttribute", arguments[1].clone())?;
            Ok(php_runtime::api::xml::xml_writer_write_attribute(
                object, &name, &value,
            ))
        }
        "text" => {
            expect_arity("XMLWriter::text", arguments.len(), 1, 1)?;
            let value = string_argument("XMLWriter::text", arguments[0].clone())?;
            Ok(php_runtime::api::xml::xml_writer_text(object, &value))
        }
        "writecomment" => {
            expect_arity("XMLWriter::writeComment", arguments.len(), 1, 1)?;
            let value = string_argument("XMLWriter::writeComment", arguments[0].clone())?;
            Ok(php_runtime::api::xml::xml_writer_write_comment(
                object, &value,
            ))
        }
        "writecdata" => {
            expect_arity("XMLWriter::writeCdata", arguments.len(), 1, 1)?;
            let value = string_argument("XMLWriter::writeCdata", arguments[0].clone())?;
            Ok(php_runtime::api::xml::xml_writer_write_cdata(
                object, &value,
            ))
        }
        "writeelement" => {
            expect_arity("XMLWriter::writeElement", arguments.len(), 1, 2)?;
            let name = string_argument("XMLWriter::writeElement", arguments[0].clone())?;
            let value = arguments
                .get(1)
                .cloned()
                .map(|value| string_argument("XMLWriter::writeElement", value))
                .transpose()?;
            Ok(php_runtime::api::xml::xml_writer_write_element(
                object,
                &name,
                value.as_deref(),
            ))
        }
        "endelement" => {
            expect_arity("XMLWriter::endElement", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::xml_writer_end_element(object))
        }
        "enddocument" => {
            expect_arity("XMLWriter::endDocument", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::xml_writer_end_document(object))
        }
        "outputmemory" => {
            expect_arity("XMLWriter::outputMemory", arguments.len(), 0, 1)?;
            let flush = arguments
                .first()
                .map(|value| bool_argument("XMLWriter::outputMemory", value))
                .transpose()?
                .unwrap_or(true);
            Ok(php_runtime::api::xml::xml_writer_output_memory(
                object, flush,
            ))
        }
        "flush" => {
            expect_arity("XMLWriter::flush", arguments.len(), 0, 1)?;
            let empty = arguments
                .first()
                .map(|value| bool_argument("XMLWriter::flush", value))
                .transpose()?
                .unwrap_or(true);
            Ok(flush_writer(context, object, empty))
        }
        method => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method XMLWriter::{method} is not implemented in the native XMLWriter slice"
        )),
    }
}

fn execute_static_factory(
    context: &mut NativeRequestColdState<'_>,
    method: &str,
    arguments: &[i64],
) -> Result<i64, String> {
    let arguments = decode_arguments(context, arguments)?;
    match method.to_ascii_lowercase().as_str() {
        "tomemory" => {
            expect_arity("XMLWriter::toMemory", arguments.len(), 0, 0)?;
            context.encode(Value::Object(new_memory_writer()))
        }
        "touri" => {
            expect_arity("XMLWriter::toUri", arguments.len(), 1, 1)?;
            let uri = string_argument("XMLWriter::toUri", arguments[0].clone())?;
            context.encode(open_uri_writer(context, &uri).map_or(Value::Bool(false), Value::Object))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: static method XMLWriter::{method} is not implemented"
        )),
    }
}

pub(in crate::vm::jit_abi) fn execute_native_xml_writer_instruction(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    match &instruction.kind {
        php_ir::InstructionKind::NewObject {
            display_class_name, ..
        } => construct_native_xml_writer(context, display_class_name, arguments),
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
            if !is_xml_writer(&object.class_name()) {
                return None;
            }
            let result = decode_arguments(context, &arguments[1..])
                .and_then(|arguments| call_xml_writer_method(context, &object, method, arguments))
                .and_then(|value| context.encode(value));
            Some(result)
        }
        php_ir::InstructionKind::CallStaticMethod {
            class_name, method, ..
        } if is_xml_writer(class_name) => Some(execute_static_factory(context, method, arguments)),
        _ => None,
    }
}

pub(in crate::vm::jit_abi) fn execute_native_xml_writer_builtin(
    context: &mut NativeRequestColdState<'_>,
    name: &str,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    if !name.starts_with("xmlwriter_") {
        return None;
    }
    let result = (|| -> Result<i64, String> {
        match name {
            "xmlwriter_open_memory" => {
                expect_arity("xmlwriter_open_memory", arguments.len(), 0, 0)?;
                context.encode(Value::Object(new_memory_writer()))
            }
            "xmlwriter_open_uri" => {
                expect_arity("xmlwriter_open_uri", arguments.len(), 1, 1)?;
                let values = decode_arguments(context, arguments)?;
                let uri = string_argument("xmlwriter_open_uri", values[0].clone())?;
                context.encode(
                    open_uri_writer(context, &uri).map_or(Value::Bool(false), Value::Object),
                )
            }
            _ => {
                let Some((receiver, arguments)) = arguments.split_first() else {
                    return Err(format!("{name} expects an XMLWriter argument"));
                };
                let receiver = match context.decode(*receiver)? {
                    Value::Reference(reference) => reference.get(),
                    value => value,
                };
                let Value::Object(object) = receiver else {
                    return Err(format!("{name} expects an XMLWriter argument"));
                };
                if !is_xml_writer(&object.class_name()) {
                    return Err(format!("{name} expects an XMLWriter argument"));
                }
                let method = name.trim_start_matches("xmlwriter_").replace('_', "");
                let values = decode_arguments(context, arguments)?;
                let value = call_xml_writer_method(context, &object, &method, values)?;
                context.encode(value)
            }
        }
    })();
    Some(result)
}
