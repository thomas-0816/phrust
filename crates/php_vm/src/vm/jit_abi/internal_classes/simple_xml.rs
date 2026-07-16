use super::*;

fn is_simple_xml(class_name: &str) -> bool {
    normalize_class_name(class_name) == "simplexmlelement"
}

pub(in crate::vm::jit_abi) fn construct_native_simple_xml(
    context: &mut NativeExecutionContext<'_>,
    class_name: &str,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    if !is_simple_xml(class_name) {
        return None;
    }
    let result = decode_arguments(context, arguments)
        .and_then(|arguments| {
            expect_arity("SimpleXMLElement::__construct", arguments.len(), 1, 1)?;
            let xml = string_argument("SimpleXMLElement::__construct", arguments[0].clone())?;
            match php_runtime::api::xml::simplexml_load_string(&xml)? {
                Value::Object(object) => Ok(object),
                _ => Err(
                    "E_PHP_VM_XML_INTERNAL: SimpleXMLElement constructor returned no object"
                        .to_owned(),
                ),
            }
        })
        .and_then(|object| context.encode(Value::Object(object)));
    Some(result)
}

fn write_xml_file(context: &NativeExecutionContext<'_>, path: &str, bytes: &[u8]) -> Value {
    let path = normalize_runtime_path(context, path);
    if !context
        .options
        .runtime_context
        .filesystem
        .allows_path(&path)
    {
        return Value::Bool(false);
    }
    Value::Bool(std::fs::write(path, bytes).is_ok())
}

fn call_simple_xml_method(
    context: &NativeExecutionContext<'_>,
    object: &php_runtime::api::ObjectRef,
    method: &str,
    arguments: Vec<Value>,
) -> Result<Value, String> {
    match method.to_ascii_lowercase().as_str() {
        "asxml" | "savexml" => {
            let display = if method.eq_ignore_ascii_case("savexml") {
                "SimpleXMLElement::saveXML"
            } else {
                "SimpleXMLElement::asXML"
            };
            expect_arity(display, arguments.len(), 0, 1)?;
            let xml = php_runtime::api::xml::simplexml_as_xml(object);
            let Some(path) = arguments.first() else {
                return Ok(xml);
            };
            if matches!(path, Value::Null) {
                return Ok(xml);
            }
            let path = string_argument(display, path.clone())?;
            let Value::String(bytes) = xml else {
                return Ok(Value::Bool(false));
            };
            Ok(write_xml_file(context, &path, bytes.as_bytes()))
        }
        "attributes" => {
            expect_arity("SimpleXMLElement::attributes", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::simplexml_attributes(object))
        }
        "children" => {
            expect_arity("SimpleXMLElement::children", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::simplexml_children(object))
        }
        "getname" => {
            expect_arity("SimpleXMLElement::getName", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::simplexml_get_name(object))
        }
        "registerxpathnamespace" => {
            expect_arity(
                "SimpleXMLElement::registerXPathNamespace",
                arguments.len(),
                2,
                2,
            )?;
            let prefix = string_argument(
                "SimpleXMLElement::registerXPathNamespace",
                arguments[0].clone(),
            )?;
            let namespace = string_argument(
                "SimpleXMLElement::registerXPathNamespace",
                arguments[1].clone(),
            )?;
            Ok(php_runtime::api::xml::simplexml_register_xpath_namespace(
                object, &prefix, &namespace,
            ))
        }
        "xpath" => {
            expect_arity("SimpleXMLElement::xpath", arguments.len(), 1, 1)?;
            let expression = string_argument("SimpleXMLElement::xpath", arguments[0].clone())?;
            Ok(php_runtime::api::xml::simplexml_xpath(object, &expression))
        }
        "__tostring" => {
            expect_arity("SimpleXMLElement::__toString", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::simplexml_text(object))
        }
        method => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: SimpleXMLElement::{method} is not implemented"
        )),
    }
}

pub(in crate::vm::jit_abi) fn execute_native_simple_xml_instruction(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    match &instruction.kind {
        php_ir::InstructionKind::NewObject {
            display_class_name, ..
        } => construct_native_simple_xml(context, display_class_name, arguments),
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
            if !is_simple_xml(&object.class_name()) {
                return None;
            }
            let result = decode_arguments(context, &arguments[1..])
                .and_then(|arguments| call_simple_xml_method(context, &object, method, arguments))
                .and_then(|value| context.encode(value));
            Some(result)
        }
        _ => None,
    }
}

pub(in crate::vm::jit_abi) fn native_simple_xml_property(
    object: &php_runtime::api::ObjectRef,
    property: &str,
) -> Option<Value> {
    is_simple_xml(&object.class_name())
        .then(|| php_runtime::api::xml::simplexml_property(object, property))
}

pub(in crate::vm::jit_abi) fn native_simple_xml_dimension(
    object: &php_runtime::api::ObjectRef,
    key: &php_runtime::api::ArrayKey,
) -> Option<Value> {
    is_simple_xml(&object.class_name())
        .then(|| php_runtime::api::xml::simplexml_dimension(object, key))
}

pub(in crate::vm::jit_abi) fn native_simple_xml_text(
    object: &php_runtime::api::ObjectRef,
) -> Option<Value> {
    is_simple_xml(&object.class_name()).then(|| php_runtime::api::xml::simplexml_text(object))
}

pub(in crate::vm::jit_abi) fn native_simple_xml_count(
    object: &php_runtime::api::ObjectRef,
) -> Option<i64> {
    if !is_simple_xml(&object.class_name()) {
        return None;
    }
    match object.get_property(php_runtime::api::xml::simplexml_count_property()) {
        Some(Value::Int(count)) => Some(count),
        _ => Some(0),
    }
}

pub(in crate::vm::jit_abi) fn native_simple_xml_empty(
    object: &php_runtime::api::ObjectRef,
) -> Option<bool> {
    is_simple_xml(&object.class_name())
        .then(|| php_runtime::api::xml::simplexml_empty_access(object))
}

pub(in crate::vm::jit_abi) fn native_simple_xml_entries(
    object: &php_runtime::api::ObjectRef,
) -> Option<Vec<(Value, Value)>> {
    is_simple_xml(&object.class_name())
        .then(|| php_runtime::api::xml::simplexml_iteration_entries(object))
}
