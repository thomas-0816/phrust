use super::*;

const DOM_COLLECTION_ENTRIES: &str = "__entries";

fn is_dom_runtime_class(class_name: &str) -> bool {
    matches!(
        normalize_class_name(class_name).as_str(),
        "domdocument"
            | "domnode"
            | "domelement"
            | "domattr"
            | "domtext"
            | "domcomment"
            | "domcdatasection"
            | "domnodelist"
            | "domnamednodemap"
            | "domxpath"
    )
}

fn construct_dom_object(
    class_name: &str,
    arguments: Vec<Value>,
) -> Result<php_runtime::api::ObjectRef, String> {
    match normalize_class_name(class_name).as_str() {
        "domdocument" => {
            expect_arity("DOMDocument::__construct", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::new_dom_document())
        }
        "domnode" => {
            expect_arity("DOMNode::__construct", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::new_dom_node())
        }
        "domtext" => {
            expect_arity("DOMText::__construct", arguments.len(), 0, 1)?;
            let value = arguments
                .into_iter()
                .next()
                .map(|value| string_argument("DOMText::__construct", value))
                .transpose()?
                .unwrap_or_default();
            Ok(php_runtime::api::xml::new_dom_text(&value))
        }
        "domcomment" => {
            expect_arity("DOMComment::__construct", arguments.len(), 0, 1)?;
            let value = arguments
                .into_iter()
                .next()
                .map(|value| string_argument("DOMComment::__construct", value))
                .transpose()?
                .unwrap_or_default();
            Ok(php_runtime::api::xml::new_dom_comment(&value))
        }
        "domcdatasection" => {
            expect_arity("DOMCdataSection::__construct", arguments.len(), 0, 1)?;
            let value = arguments
                .into_iter()
                .next()
                .map(|value| string_argument("DOMCdataSection::__construct", value))
                .transpose()?
                .unwrap_or_default();
            Ok(php_runtime::api::xml::new_dom_cdata_section(&value))
        }
        "domattr" => {
            expect_arity("DOMAttr::__construct", arguments.len(), 1, 2)?;
            let name = string_argument("DOMAttr::__construct", arguments[0].clone())?;
            let value = arguments
                .get(1)
                .cloned()
                .map(|value| string_argument("DOMAttr::__construct", value))
                .transpose()?
                .unwrap_or_default();
            Ok(php_runtime::api::xml::new_dom_attr(&name, &value))
        }
        "domelement" => {
            expect_arity("DOMElement::__construct", arguments.len(), 1, 1)?;
            let name = string_argument("DOMElement::__construct", arguments[0].clone())?;
            Ok(php_runtime::api::xml::new_dom_element(
                &php_runtime::api::xml::XmlElement {
                    name,
                    attributes: Vec::new(),
                    children: Vec::new(),
                },
            ))
        }
        "domnodelist" => {
            expect_arity("DOMNodeList::__construct", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::new_dom_node_list(Vec::new()))
        }
        "domnamednodemap" => {
            expect_arity("DOMNamedNodeMap::__construct", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::new_dom_named_node_map(&[]))
        }
        "domxpath" => {
            expect_arity("DOMXPath::__construct", arguments.len(), 1, 1)?;
            let document = object_argument("DOMXPath::__construct", &arguments[0])?;
            if !document.class_name().eq_ignore_ascii_case("DOMDocument") {
                return Err(
                    "E_PHP_VM_DOM_TYPE_ERROR: DOMXPath::__construct expects DOMDocument".to_owned(),
                );
            }
            Ok(php_runtime::api::xml::new_dom_xpath(&document))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_CLASS: class {class_name} is not a native DOM class"
        )),
    }
}

pub(in crate::vm::jit_abi) fn construct_native_dom_class(
    context: &mut NativeExecutionContext<'_>,
    class_name: &str,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    if !is_dom_runtime_class(class_name) {
        return None;
    }
    let result = decode_arguments(context, arguments)
        .and_then(|arguments| construct_dom_object(class_name, arguments))
        .and_then(|object| context.encode(Value::Object(object)));
    Some(result)
}

fn dom_load_file(
    context: &NativeExecutionContext<'_>,
    object: &php_runtime::api::ObjectRef,
    path: &str,
) -> Result<Value, String> {
    let path = normalize_runtime_path(context, path);
    if !context
        .options
        .runtime_context
        .filesystem
        .allows_path(&path)
    {
        return Ok(Value::Bool(false));
    }
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(_) => return Ok(Value::Bool(false)),
    };
    php_runtime::api::xml::dom_document_load_xml(object, &source)
}

fn dom_save_file(
    context: &NativeExecutionContext<'_>,
    object: &php_runtime::api::ObjectRef,
    path: &str,
) -> Result<Value, String> {
    let path = normalize_runtime_path(context, path);
    if !context
        .options
        .runtime_context
        .filesystem
        .allows_path(&path)
    {
        return Ok(Value::Bool(false));
    }
    let Value::String(bytes) = php_runtime::api::xml::dom_document_save_xml(object) else {
        return Ok(Value::Bool(false));
    };
    if std::fs::write(&path, bytes.as_bytes()).is_err() {
        return Ok(Value::Bool(false));
    }
    Ok(Value::Int(bytes.len() as i64))
}

fn call_dom_method(
    context: &NativeExecutionContext<'_>,
    object: &php_runtime::api::ObjectRef,
    method: &str,
    arguments: Vec<Value>,
) -> Result<Value, String> {
    let class_name = normalize_class_name(&object.class_name());
    let method = method.to_ascii_lowercase();
    match (class_name.as_str(), method.as_str()) {
        ("domdocument", "loadxml") => {
            expect_arity("DOMDocument::loadXML", arguments.len(), 1, 1)?;
            let xml = string_argument("DOMDocument::loadXML", arguments[0].clone())?;
            php_runtime::api::xml::dom_document_load_xml(object, &xml)
        }
        ("domdocument", "load") => {
            expect_arity("DOMDocument::load", arguments.len(), 1, 1)?;
            let path = string_argument("DOMDocument::load", arguments[0].clone())?;
            dom_load_file(context, object, &path)
        }
        ("domdocument", "savexml") => {
            expect_arity("DOMDocument::saveXML", arguments.len(), 0, 0)?;
            Ok(php_runtime::api::xml::dom_document_save_xml(object))
        }
        ("domdocument", "save") => {
            expect_arity("DOMDocument::save", arguments.len(), 1, 1)?;
            let path = string_argument("DOMDocument::save", arguments[0].clone())?;
            dom_save_file(context, object, &path)
        }
        ("domdocument", "createelement") => {
            expect_arity("DOMDocument::createElement", arguments.len(), 1, 2)?;
            let name = string_argument("DOMDocument::createElement", arguments[0].clone())?;
            let value = arguments
                .get(1)
                .cloned()
                .map(|value| string_argument("DOMDocument::createElement", value))
                .transpose()?;
            Ok(php_runtime::api::xml::dom_document_create_element(
                &name,
                value.as_deref(),
            ))
        }
        ("domdocument", "createtextnode") => {
            expect_arity("DOMDocument::createTextNode", arguments.len(), 1, 1)?;
            let value = string_argument("DOMDocument::createTextNode", arguments[0].clone())?;
            Ok(php_runtime::api::xml::dom_document_create_text_node(&value))
        }
        ("domdocument", "createcomment") => {
            expect_arity("DOMDocument::createComment", arguments.len(), 1, 1)?;
            let value = string_argument("DOMDocument::createComment", arguments[0].clone())?;
            Ok(php_runtime::api::xml::dom_document_create_comment(&value))
        }
        ("domdocument", "createcdatasection") => {
            expect_arity("DOMDocument::createCDATASection", arguments.len(), 1, 1)?;
            let value = string_argument("DOMDocument::createCDATASection", arguments[0].clone())?;
            Ok(php_runtime::api::xml::dom_document_create_cdata_section(
                &value,
            ))
        }
        ("domdocument", "createattribute") => {
            expect_arity("DOMDocument::createAttribute", arguments.len(), 1, 1)?;
            let name = string_argument("DOMDocument::createAttribute", arguments[0].clone())?;
            Ok(php_runtime::api::xml::dom_document_create_attribute(&name))
        }
        ("domdocument", "appendchild") => {
            expect_arity("DOMDocument::appendChild", arguments.len(), 1, 1)?;
            let child = object_argument("DOMDocument::appendChild", &arguments[0])?;
            Ok(php_runtime::api::xml::dom_document_append_child(
                object, &child,
            ))
        }
        ("domdocument", "getelementsbytagname") => {
            expect_arity("DOMDocument::getElementsByTagName", arguments.len(), 1, 1)?;
            let name = string_argument("DOMDocument::getElementsByTagName", arguments[0].clone())?;
            Ok(php_runtime::api::xml::dom_document_get_elements_by_tag_name(object, &name))
        }
        ("domdocument", "getelementsbytagnamens") => {
            expect_arity("DOMDocument::getElementsByTagNameNS", arguments.len(), 2, 2)?;
            let namespace =
                string_argument("DOMDocument::getElementsByTagNameNS", arguments[0].clone())?;
            let local =
                string_argument("DOMDocument::getElementsByTagNameNS", arguments[1].clone())?;
            Ok(
                php_runtime::api::xml::dom_document_get_elements_by_tag_name_ns(
                    object, &namespace, &local,
                ),
            )
        }
        ("domelement", "getelementsbytagname") => {
            expect_arity("DOMElement::getElementsByTagName", arguments.len(), 1, 1)?;
            let name = string_argument("DOMElement::getElementsByTagName", arguments[0].clone())?;
            Ok(php_runtime::api::xml::dom_element_get_elements_by_tag_name(
                object, &name,
            ))
        }
        ("domelement", "getelementsbytagnamens") => {
            expect_arity("DOMElement::getElementsByTagNameNS", arguments.len(), 2, 2)?;
            let namespace =
                string_argument("DOMElement::getElementsByTagNameNS", arguments[0].clone())?;
            let local =
                string_argument("DOMElement::getElementsByTagNameNS", arguments[1].clone())?;
            Ok(
                php_runtime::api::xml::dom_element_get_elements_by_tag_name_ns(
                    object, &namespace, &local,
                ),
            )
        }
        ("domelement", "getattribute") => {
            expect_arity("DOMElement::getAttribute", arguments.len(), 1, 1)?;
            let name = string_argument("DOMElement::getAttribute", arguments[0].clone())?;
            Ok(php_runtime::api::xml::dom_element_get_attribute(
                object, &name,
            ))
        }
        ("domelement", "hasattribute") => {
            expect_arity("DOMElement::hasAttribute", arguments.len(), 1, 1)?;
            let name = string_argument("DOMElement::hasAttribute", arguments[0].clone())?;
            Ok(php_runtime::api::xml::dom_element_has_attribute(
                object, &name,
            ))
        }
        ("domelement", "getattributenode") => {
            expect_arity("DOMElement::getAttributeNode", arguments.len(), 1, 1)?;
            let name = string_argument("DOMElement::getAttributeNode", arguments[0].clone())?;
            Ok(php_runtime::api::xml::dom_element_get_attribute_node(
                object, &name,
            ))
        }
        ("domelement", "setattribute") => {
            expect_arity("DOMElement::setAttribute", arguments.len(), 2, 2)?;
            let name = string_argument("DOMElement::setAttribute", arguments[0].clone())?;
            let value = string_argument("DOMElement::setAttribute", arguments[1].clone())?;
            Ok(php_runtime::api::xml::dom_element_set_attribute(
                object, &name, &value,
            ))
        }
        ("domelement", "removeattribute") => {
            expect_arity("DOMElement::removeAttribute", arguments.len(), 1, 1)?;
            let name = string_argument("DOMElement::removeAttribute", arguments[0].clone())?;
            Ok(php_runtime::api::xml::dom_element_remove_attribute(
                object, &name,
            ))
        }
        ("domelement", "setattributenode") => {
            expect_arity("DOMElement::setAttributeNode", arguments.len(), 1, 1)?;
            let attribute = object_argument("DOMElement::setAttributeNode", &arguments[0])?;
            Ok(php_runtime::api::xml::dom_element_set_attribute_node(
                object, &attribute,
            ))
        }
        ("domelement", "appendchild") => {
            expect_arity("DOMElement::appendChild", arguments.len(), 1, 1)?;
            let child = object_argument("DOMElement::appendChild", &arguments[0])?;
            Ok(php_runtime::api::xml::dom_element_append_child(
                object, &child,
            ))
        }
        ("domnodelist", "item") => {
            expect_arity("DOMNodeList::item", arguments.len(), 1, 1)?;
            let index = int_argument("DOMNodeList::item", &arguments[0])?;
            Ok(php_runtime::api::xml::dom_node_list_item(object, index))
        }
        ("domnodelist", "count") => {
            expect_arity("DOMNodeList::count", arguments.len(), 0, 0)?;
            Ok(Value::Int(
                native_dom_collection_entries(object).map_or(0, |entries| entries.len() as i64),
            ))
        }
        ("domnamednodemap", "item") => {
            expect_arity("DOMNamedNodeMap::item", arguments.len(), 1, 1)?;
            let index = int_argument("DOMNamedNodeMap::item", &arguments[0])?;
            Ok(php_runtime::api::xml::dom_named_node_map_item(
                object, index,
            ))
        }
        ("domnamednodemap", "getnameditem") => {
            expect_arity("DOMNamedNodeMap::getNamedItem", arguments.len(), 1, 1)?;
            let name = string_argument("DOMNamedNodeMap::getNamedItem", arguments[0].clone())?;
            Ok(php_runtime::api::xml::dom_named_node_map_get_named_item(
                object, &name,
            ))
        }
        ("domnamednodemap", "count") => {
            expect_arity("DOMNamedNodeMap::count", arguments.len(), 0, 0)?;
            Ok(Value::Int(
                native_dom_collection_entries(object).map_or(0, |entries| entries.len() as i64),
            ))
        }
        ("domxpath", "query") => {
            expect_arity("DOMXPath::query", arguments.len(), 1, 1)?;
            let expression = string_argument("DOMXPath::query", arguments[0].clone())?;
            Ok(php_runtime::api::xml::dom_xpath_query(object, &expression))
        }
        ("domxpath", "evaluate") => {
            expect_arity("DOMXPath::evaluate", arguments.len(), 1, 1)?;
            let expression = string_argument("DOMXPath::evaluate", arguments[0].clone())?;
            Ok(php_runtime::api::xml::dom_xpath_evaluate(
                object,
                &expression,
            ))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method {}::{method} is not implemented in the native DOM slice",
            object.display_name()
        )),
    }
}

pub(in crate::vm::jit_abi) fn execute_native_dom_instruction(
    context: &mut NativeExecutionContext<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    match &instruction.kind {
        php_ir::InstructionKind::NewObject {
            display_class_name, ..
        } => construct_native_dom_class(context, display_class_name, arguments),
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
            if !is_dom_runtime_class(&object.class_name()) {
                return None;
            }
            let result = decode_arguments(context, &arguments[1..])
                .and_then(|arguments| call_dom_method(context, &object, method, arguments))
                .and_then(|value| context.encode(value));
            Some(result)
        }
        _ => None,
    }
}

pub(in crate::vm::jit_abi) fn native_dom_collection_entries(
    object: &php_runtime::api::ObjectRef,
) -> Option<php_runtime::api::PhpArray> {
    if !matches!(
        normalize_class_name(&object.class_name()).as_str(),
        "domnodelist" | "domnamednodemap"
    ) {
        return None;
    }
    match object.get_property(DOM_COLLECTION_ENTRIES) {
        Some(Value::Array(entries)) => Some(entries),
        _ => None,
    }
}
