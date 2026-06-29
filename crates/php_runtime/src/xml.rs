//! Bounded XML/DOM/SimpleXML runtime support.
//!
//! This module intentionally implements a strict, local XML subset for the
//! WordPress compatibility slice. It rejects DTDs, processing instructions, and
//! unresolved entities instead of fetching external resources or guessing.

use crate::{
    ArrayKey, ClassEntry, ClassFlags, ObjectRef, PhpArray, PhpString, Value, normalize_class_name,
};

const XML_STORAGE: &str = "__phrust_xml_document";
const XML_NODE_PATH: &str = "__phrust_xml_path";
const XML_READER_EVENTS: &str = "__phrust_xml_reader_events";
const XML_READER_INDEX: &str = "__phrust_xml_reader_index";
const XML_WRITER_BUFFER: &str = "__phrust_xml_writer_buffer";
const XML_WRITER_STACK: &str = "__phrust_xml_writer_stack";
const XML_WRITER_OPEN_TAG: &str = "__phrust_xml_writer_open_tag";

pub const XML_READER_NONE: i64 = 0;
pub const XML_READER_ELEMENT: i64 = 1;
pub const XML_READER_TEXT: i64 = 3;
pub const XML_READER_END_ELEMENT: i64 = 15;

/// Parsed XML document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlDocument {
    pub root: XmlElement,
}

/// Element node in the bounded XML tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlElement {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<XmlNode>,
}

/// Child node in the bounded XML tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XmlNode {
    Element(XmlElement),
    Text(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlReaderEvent {
    pub node_type: i64,
    pub name: String,
    pub value: String,
    pub attributes: Vec<(String, String)>,
}

/// Parses a strict XML document using no external resources.
pub fn parse_xml(input: &str) -> Result<XmlDocument, String> {
    Parser::new(input).parse_document()
}

pub fn serialize_document(document: &XmlDocument) -> String {
    serialize_element(&document.root)
}

pub fn serialize_element(element: &XmlElement) -> String {
    let mut out = String::new();
    out.push('<');
    out.push_str(&element.name);
    for (name, value) in &element.attributes {
        out.push(' ');
        out.push_str(name);
        out.push_str("=\"");
        out.push_str(&escape_text(value, true));
        out.push('"');
    }
    if element.children.is_empty() {
        out.push_str("/>");
        return out;
    }
    out.push('>');
    for child in &element.children {
        match child {
            XmlNode::Element(child) => out.push_str(&serialize_element(child)),
            XmlNode::Text(text) => out.push_str(&escape_text(text, false)),
        }
    }
    out.push_str("</");
    out.push_str(&element.name);
    out.push('>');
    out
}

pub fn element_text(element: &XmlElement) -> String {
    let mut out = String::new();
    collect_text(element, &mut out);
    out
}

pub fn reader_events(document: &XmlDocument) -> Vec<XmlReaderEvent> {
    let mut events = Vec::new();
    push_reader_events(&document.root, &mut events);
    events
}

pub fn empty_internal_class(name: &str) -> ClassEntry {
    ClassEntry {
        name: normalize_class_name(name),
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
    }
}

pub fn new_dom_document() -> ObjectRef {
    ObjectRef::new_with_display_name(&empty_internal_class("DOMDocument"), "DOMDocument")
}

pub fn new_dom_node() -> ObjectRef {
    ObjectRef::new_with_display_name(&empty_internal_class("DOMNode"), "DOMNode")
}

pub fn new_xml_parser() -> ObjectRef {
    ObjectRef::new_with_display_name(&empty_internal_class("XMLParser"), "XMLParser")
}

pub fn new_dom_element(element: &XmlElement) -> ObjectRef {
    let object =
        ObjectRef::new_with_display_name(&empty_internal_class("DOMElement"), "DOMElement");
    object.set_property("nodeName", Value::string(element.name.as_bytes().to_vec()));
    object.set_property("tagName", Value::string(element.name.as_bytes().to_vec()));
    object.set_property(
        "textContent",
        Value::string(element_text(element).into_bytes()),
    );
    object.set_property(
        "nodeValue",
        Value::string(element_text(element).into_bytes()),
    );
    object.set_property(
        XML_STORAGE,
        document_value(&XmlDocument {
            root: element.clone(),
        }),
    );
    object.set_property(XML_NODE_PATH, Value::string(Vec::<u8>::new()));
    object
}

pub fn new_dom_node_list(elements: Vec<XmlElement>) -> ObjectRef {
    let object =
        ObjectRef::new_with_display_name(&empty_internal_class("DOMNodeList"), "DOMNodeList");
    let mut entries = PhpArray::new();
    for (index, element) in elements.into_iter().enumerate() {
        let node = Value::Object(new_dom_element(&element));
        object.set_property(index.to_string(), node.clone());
        entries.append(node);
    }
    object.set_property("length", Value::Int(entries.len() as i64));
    object.set_property("__entries", Value::Array(entries));
    object
}

pub fn new_simplexml_element(element: &XmlElement) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(
        &empty_internal_class("SimpleXMLElement"),
        "SimpleXMLElement",
    );
    object.set_property("__text", Value::string(element_text(element).into_bytes()));
    object.set_property(
        XML_STORAGE,
        document_value(&XmlDocument {
            root: element.clone(),
        }),
    );
    object.set_property(XML_NODE_PATH, Value::string(Vec::<u8>::new()));
    for child in element.children.iter().filter_map(|child| match child {
        XmlNode::Element(element) => Some(element),
        XmlNode::Text(_) => None,
    }) {
        object.set_property(
            child.name.clone(),
            Value::Object(new_simplexml_element(child)),
        );
    }
    object
}

pub fn simplexml_attributes_object(element: &XmlElement) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(
        &empty_internal_class("SimpleXMLElement"),
        "SimpleXMLElement",
    );
    for (name, value) in &element.attributes {
        object.set_property(name.clone(), Value::string(value.as_bytes().to_vec()));
    }
    object
}

pub fn new_xml_reader() -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&empty_internal_class("XMLReader"), "XMLReader");
    set_xml_reader_current(&object, None);
    object.set_property(XML_READER_EVENTS, Value::Array(PhpArray::new()));
    object.set_property(XML_READER_INDEX, Value::Int(-1));
    object
}

pub fn new_xml_writer() -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&empty_internal_class("XMLWriter"), "XMLWriter");
    object.set_property(XML_WRITER_BUFFER, Value::string(Vec::<u8>::new()));
    object.set_property(XML_WRITER_STACK, Value::Array(PhpArray::new()));
    object.set_property(XML_WRITER_OPEN_TAG, Value::Bool(false));
    object
}

pub fn dom_document_load_xml(object: &ObjectRef, xml: &str) -> Result<Value, String> {
    let document = parse_xml(xml)?;
    object.set_property(XML_STORAGE, document_value(&document));
    object.set_property(
        "documentElement",
        Value::Object(new_dom_element(&document.root)),
    );
    object.set_property(
        "textContent",
        Value::string(element_text(&document.root).into_bytes()),
    );
    Ok(Value::Bool(true))
}

pub fn dom_document_save_xml(object: &ObjectRef) -> Value {
    document_from_object(object)
        .map(|document| Value::string(serialize_document(&document).into_bytes()))
        .unwrap_or_else(|| Value::string(Vec::<u8>::new()))
}

pub fn dom_document_create_element(name: &str, value: Option<&str>) -> Value {
    let mut children = Vec::new();
    if let Some(value) = value
        && !value.is_empty()
    {
        children.push(XmlNode::Text(value.to_owned()));
    }
    Value::Object(new_dom_element(&XmlElement {
        name: name.to_owned(),
        attributes: Vec::new(),
        children,
    }))
}

pub fn dom_document_append_child(object: &ObjectRef, child: &ObjectRef) -> Value {
    let Some(child_element) = element_from_object(child) else {
        return Value::Null;
    };
    let document = XmlDocument {
        root: child_element.clone(),
    };
    object.set_property(XML_STORAGE, document_value(&document));
    object.set_property(
        "documentElement",
        Value::Object(new_dom_element(&child_element)),
    );
    object.set_property(
        "textContent",
        Value::string(element_text(&child_element).into_bytes()),
    );
    Value::Object(new_dom_element(&child_element))
}

pub fn dom_document_element(object: &ObjectRef) -> Value {
    document_from_object(object)
        .map(|document| Value::Object(new_dom_element(&document.root)))
        .unwrap_or(Value::Null)
}

pub fn dom_document_get_elements_by_tag_name(object: &ObjectRef, name: &str) -> Value {
    document_from_object(object)
        .map(|document| {
            Value::Object(new_dom_node_list(elements_by_tag_name(
                &document.root,
                name,
            )))
        })
        .unwrap_or_else(|| Value::Object(new_dom_node_list(Vec::new())))
}

pub fn dom_element_get_elements_by_tag_name(object: &ObjectRef, name: &str) -> Value {
    element_from_object(object)
        .map(|element| Value::Object(new_dom_node_list(elements_by_tag_name(&element, name))))
        .unwrap_or_else(|| Value::Object(new_dom_node_list(Vec::new())))
}

pub fn dom_element_get_attribute(object: &ObjectRef, name: &str) -> Value {
    element_from_object(object)
        .and_then(|element| {
            element
                .attributes
                .into_iter()
                .find(|(attr, _)| attr == name)
                .map(|(_, value)| Value::string(value.into_bytes()))
        })
        .unwrap_or_else(|| Value::string(Vec::<u8>::new()))
}

pub fn dom_element_set_attribute(object: &ObjectRef, name: &str, value: &str) -> Value {
    let Some(mut element) = element_from_object(object) else {
        return Value::Null;
    };
    if let Some((_, existing)) = element.attributes.iter_mut().find(|(attr, _)| attr == name) {
        *existing = value.to_owned();
    } else {
        element.attributes.push((name.to_owned(), value.to_owned()));
    }
    set_dom_element_object(object, &element);
    Value::Null
}

pub fn dom_element_append_child(object: &ObjectRef, child: &ObjectRef) -> Value {
    let Some(mut element) = element_from_object(object) else {
        return Value::Null;
    };
    let Some(child_element) = element_from_object(child) else {
        return Value::Null;
    };
    element
        .children
        .push(XmlNode::Element(child_element.clone()));
    set_dom_element_object(object, &element);
    Value::Object(new_dom_element(&child_element))
}

pub fn dom_node_list_item(object: &ObjectRef, index: i64) -> Value {
    let Some(Value::Array(entries)) = object.get_property("__entries") else {
        return Value::Null;
    };
    entries
        .get(&ArrayKey::Int(index))
        .cloned()
        .unwrap_or(Value::Null)
}

pub fn simplexml_load_string(xml: &str) -> Result<Value, String> {
    let document = parse_xml(xml)?;
    Ok(Value::Object(new_simplexml_element(&document.root)))
}

pub fn simplexml_as_xml(object: &ObjectRef) -> Value {
    document_from_object(object)
        .map(|document| Value::string(serialize_document(&document).into_bytes()))
        .unwrap_or_else(|| Value::string(Vec::<u8>::new()))
}

pub fn simplexml_text(object: &ObjectRef) -> Value {
    document_from_object(object)
        .map(|document| Value::string(element_text(&document.root).into_bytes()))
        .unwrap_or_else(|| {
            object
                .get_property("__text")
                .unwrap_or(Value::string(Vec::<u8>::new()))
        })
}

pub fn simplexml_attributes(object: &ObjectRef) -> Value {
    document_from_object(object)
        .map(|document| Value::Object(simplexml_attributes_object(&document.root)))
        .unwrap_or(Value::Null)
}

pub fn xml_reader_xml(object: &ObjectRef, xml: &str) -> Result<Value, String> {
    let document = parse_xml(xml)?;
    let events = reader_events(&document);
    object.set_property(XML_READER_EVENTS, reader_events_value(&events));
    object.set_property(XML_READER_INDEX, Value::Int(-1));
    set_xml_reader_current(object, None);
    Ok(Value::Bool(true))
}

pub fn xml_reader_read(object: &ObjectRef) -> Value {
    let events = reader_events_from_object(object);
    let next = match object.get_property(XML_READER_INDEX) {
        Some(Value::Int(index)) => index + 1,
        _ => 0,
    };
    if next < 0 || next as usize >= events.len() {
        set_xml_reader_current(object, None);
        return Value::Bool(false);
    }
    object.set_property(XML_READER_INDEX, Value::Int(next));
    set_xml_reader_current(object, events.get(next as usize));
    Value::Bool(true)
}

pub fn xml_reader_get_attribute(object: &ObjectRef, name: &str) -> Value {
    let Some(Value::Int(index)) = object.get_property(XML_READER_INDEX) else {
        return Value::Null;
    };
    let events = reader_events_from_object(object);
    events
        .get(index as usize)
        .and_then(|event| {
            event
                .attributes
                .iter()
                .find(|(attr, _)| attr == name)
                .map(|(_, value)| Value::string(value.as_bytes().to_vec()))
        })
        .unwrap_or(Value::Null)
}

pub fn xml_reader_close(object: &ObjectRef) -> Value {
    object.set_property(XML_READER_EVENTS, Value::Array(PhpArray::new()));
    object.set_property(XML_READER_INDEX, Value::Int(-1));
    set_xml_reader_current(object, None);
    Value::Bool(true)
}

pub fn xml_writer_open_memory(object: &ObjectRef) -> Value {
    object.set_property(XML_WRITER_BUFFER, Value::string(Vec::<u8>::new()));
    object.set_property(XML_WRITER_STACK, Value::Array(PhpArray::new()));
    object.set_property(XML_WRITER_OPEN_TAG, Value::Bool(false));
    Value::Bool(true)
}

pub fn xml_writer_start_document(object: &ObjectRef) -> Value {
    object.set_property(
        XML_WRITER_BUFFER,
        Value::string(br#"<?xml version="1.0"?>"#.to_vec()),
    );
    Value::Bool(true)
}

pub fn xml_writer_start_element(object: &ObjectRef, name: &str) -> Value {
    close_writer_open_tag(object);
    let mut buffer = writer_buffer(object);
    buffer.push('<');
    buffer.push_str(name);
    object.set_property(XML_WRITER_BUFFER, Value::string(buffer.into_bytes()));
    object.set_property(XML_WRITER_OPEN_TAG, Value::Bool(true));
    let mut stack = writer_stack(object);
    stack.append(Value::string(name.as_bytes().to_vec()));
    object.set_property(XML_WRITER_STACK, Value::Array(stack));
    Value::Bool(true)
}

pub fn xml_writer_write_attribute(object: &ObjectRef, name: &str, value: &str) -> Value {
    if !matches!(
        object.get_property(XML_WRITER_OPEN_TAG),
        Some(Value::Bool(true))
    ) {
        return Value::Bool(false);
    }
    let mut buffer = writer_buffer(object);
    buffer.push(' ');
    buffer.push_str(name);
    buffer.push_str("=\"");
    buffer.push_str(&escape_text(value, true));
    buffer.push('"');
    object.set_property(XML_WRITER_BUFFER, Value::string(buffer.into_bytes()));
    Value::Bool(true)
}

pub fn xml_writer_text(object: &ObjectRef, value: &str) -> Value {
    close_writer_open_tag(object);
    let mut buffer = writer_buffer(object);
    buffer.push_str(&escape_text(value, false));
    object.set_property(XML_WRITER_BUFFER, Value::string(buffer.into_bytes()));
    Value::Bool(true)
}

pub fn xml_writer_end_element(object: &ObjectRef) -> Value {
    let mut stack = writer_stack(object);
    let names = stack
        .iter()
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    let Some(Value::String(name)) = names.last() else {
        return Value::Bool(false);
    };
    let name = name.to_string_lossy();
    let mut buffer = writer_buffer(object);
    if matches!(
        object.get_property(XML_WRITER_OPEN_TAG),
        Some(Value::Bool(true))
    ) {
        buffer.push_str("/>");
        object.set_property(XML_WRITER_OPEN_TAG, Value::Bool(false));
    } else {
        buffer.push_str("</");
        buffer.push_str(&name);
        buffer.push('>');
    }
    stack.remove(&ArrayKey::Int(names.len() as i64 - 1));
    object.set_property(XML_WRITER_STACK, Value::Array(stack));
    object.set_property(XML_WRITER_BUFFER, Value::string(buffer.into_bytes()));
    Value::Bool(true)
}

pub fn xml_writer_end_document(object: &ObjectRef) -> Value {
    while writer_stack(object).iter().next().is_some() {
        let _ = xml_writer_end_element(object);
    }
    Value::Bool(true)
}

pub fn xml_writer_output_memory(object: &ObjectRef) -> Value {
    Value::string(writer_buffer(object).into_bytes())
}

fn collect_text(element: &XmlElement, out: &mut String) {
    for child in &element.children {
        match child {
            XmlNode::Element(child) => collect_text(child, out),
            XmlNode::Text(text) => out.push_str(text),
        }
    }
}

fn set_dom_element_object(object: &ObjectRef, element: &XmlElement) {
    object.set_property("nodeName", Value::string(element.name.as_bytes().to_vec()));
    object.set_property("tagName", Value::string(element.name.as_bytes().to_vec()));
    let text = element_text(element);
    object.set_property("textContent", Value::string(text.as_bytes().to_vec()));
    object.set_property("nodeValue", Value::string(text.into_bytes()));
    object.set_property(
        XML_STORAGE,
        document_value(&XmlDocument {
            root: element.clone(),
        }),
    );
}

fn elements_by_tag_name(element: &XmlElement, name: &str) -> Vec<XmlElement> {
    let mut out = Vec::new();
    collect_elements_by_tag_name(element, name, &mut out);
    out
}

fn collect_elements_by_tag_name(element: &XmlElement, name: &str, out: &mut Vec<XmlElement>) {
    if name == "*" || element.name == name {
        out.push(element.clone());
    }
    for child in &element.children {
        if let XmlNode::Element(child) = child {
            collect_elements_by_tag_name(child, name, out);
        }
    }
}

fn push_reader_events(element: &XmlElement, events: &mut Vec<XmlReaderEvent>) {
    events.push(XmlReaderEvent {
        node_type: XML_READER_ELEMENT,
        name: element.name.clone(),
        value: String::new(),
        attributes: element.attributes.clone(),
    });
    for child in &element.children {
        match child {
            XmlNode::Element(child) => push_reader_events(child, events),
            XmlNode::Text(text) if !text.is_empty() => events.push(XmlReaderEvent {
                node_type: XML_READER_TEXT,
                name: String::new(),
                value: text.clone(),
                attributes: Vec::new(),
            }),
            XmlNode::Text(_) => {}
        }
    }
    events.push(XmlReaderEvent {
        node_type: XML_READER_END_ELEMENT,
        name: element.name.clone(),
        value: String::new(),
        attributes: Vec::new(),
    });
}

fn document_value(document: &XmlDocument) -> Value {
    element_value(&document.root)
}

fn document_from_object(object: &ObjectRef) -> Option<XmlDocument> {
    object
        .get_property(XML_STORAGE)
        .and_then(|value| match value {
            Value::Array(array) => {
                element_from_value(&Value::Array(array)).map(|root| XmlDocument { root })
            }
            _ => None,
        })
}

fn element_from_object(object: &ObjectRef) -> Option<XmlElement> {
    document_from_object(object).map(|document| document.root)
}

fn element_value(element: &XmlElement) -> Value {
    let mut array = PhpArray::new();
    array.insert(
        ArrayKey::String(PhpString::from("name")),
        Value::string(element.name.as_bytes().to_vec()),
    );
    let mut attrs = PhpArray::new();
    for (name, value) in &element.attributes {
        attrs.insert(
            ArrayKey::String(PhpString::from(name.as_str())),
            Value::string(value.as_bytes().to_vec()),
        );
    }
    array.insert(
        ArrayKey::String(PhpString::from("attrs")),
        Value::Array(attrs),
    );
    let mut children = PhpArray::new();
    for child in &element.children {
        children.append(match child {
            XmlNode::Element(element) => element_value(element),
            XmlNode::Text(text) => {
                let mut text_node = PhpArray::new();
                text_node.insert(
                    ArrayKey::String(PhpString::from("text")),
                    Value::string(text.as_bytes().to_vec()),
                );
                Value::Array(text_node)
            }
        });
    }
    array.insert(
        ArrayKey::String(PhpString::from("children")),
        Value::Array(children),
    );
    Value::Array(array)
}

fn element_from_value(value: &Value) -> Option<XmlElement> {
    let Value::Array(array) = value else {
        return None;
    };
    let name = match array.get(&ArrayKey::String(PhpString::from("name")))? {
        Value::String(name) => name.to_string_lossy(),
        _ => return None,
    };
    let mut attributes = Vec::new();
    if let Some(Value::Array(attrs)) = array.get(&ArrayKey::String(PhpString::from("attrs"))) {
        for (key, value) in attrs.iter() {
            let ArrayKey::String(key) = key else {
                continue;
            };
            let Value::String(value) = value else {
                continue;
            };
            attributes.push((key.to_string_lossy(), value.to_string_lossy()));
        }
    }
    let mut children = Vec::new();
    if let Some(Value::Array(child_array)) =
        array.get(&ArrayKey::String(PhpString::from("children")))
    {
        for (_, child) in child_array.iter() {
            if let Some(Value::String(text)) = as_text_node(child) {
                children.push(XmlNode::Text(text.to_string_lossy()));
            } else if let Some(element) = element_from_value(child) {
                children.push(XmlNode::Element(element));
            }
        }
    }
    Some(XmlElement {
        name,
        attributes,
        children,
    })
}

fn as_text_node(value: &Value) -> Option<&Value> {
    let Value::Array(array) = value else {
        return None;
    };
    array.get(&ArrayKey::String(PhpString::from("text")))
}

fn reader_events_value(events: &[XmlReaderEvent]) -> Value {
    let mut array = PhpArray::new();
    for event in events {
        let mut entry = PhpArray::new();
        entry.insert(
            ArrayKey::String(PhpString::from("type")),
            Value::Int(event.node_type),
        );
        entry.insert(
            ArrayKey::String(PhpString::from("name")),
            Value::string(event.name.as_bytes().to_vec()),
        );
        entry.insert(
            ArrayKey::String(PhpString::from("value")),
            Value::string(event.value.as_bytes().to_vec()),
        );
        let mut attrs = PhpArray::new();
        for (name, value) in &event.attributes {
            attrs.insert(
                ArrayKey::String(PhpString::from(name.as_str())),
                Value::string(value.as_bytes().to_vec()),
            );
        }
        entry.insert(
            ArrayKey::String(PhpString::from("attrs")),
            Value::Array(attrs),
        );
        array.append(Value::Array(entry));
    }
    Value::Array(array)
}

fn reader_events_from_object(object: &ObjectRef) -> Vec<XmlReaderEvent> {
    let Some(Value::Array(array)) = object.get_property(XML_READER_EVENTS) else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|(_, value)| {
            let Value::Array(entry) = value else {
                return None;
            };
            let node_type = match entry.get(&ArrayKey::String(PhpString::from("type"))) {
                Some(Value::Int(value)) => *value,
                _ => XML_READER_NONE,
            };
            let name = match entry.get(&ArrayKey::String(PhpString::from("name"))) {
                Some(Value::String(value)) => value.to_string_lossy(),
                _ => String::new(),
            };
            let value = match entry.get(&ArrayKey::String(PhpString::from("value"))) {
                Some(Value::String(value)) => value.to_string_lossy(),
                _ => String::new(),
            };
            let mut attributes = Vec::new();
            if let Some(Value::Array(attrs)) =
                entry.get(&ArrayKey::String(PhpString::from("attrs")))
            {
                for (key, value) in attrs.iter() {
                    if let (ArrayKey::String(key), Value::String(value)) = (key, value) {
                        attributes.push((key.to_string_lossy(), value.to_string_lossy()));
                    }
                }
            }
            Some(XmlReaderEvent {
                node_type,
                name,
                value,
                attributes,
            })
        })
        .collect()
}

fn set_xml_reader_current(object: &ObjectRef, event: Option<&XmlReaderEvent>) {
    if let Some(event) = event {
        object.set_property("nodeType", Value::Int(event.node_type));
        object.set_property("name", Value::string(event.name.as_bytes().to_vec()));
        object.set_property("value", Value::string(event.value.as_bytes().to_vec()));
    } else {
        object.set_property("nodeType", Value::Int(XML_READER_NONE));
        object.set_property("name", Value::string(Vec::<u8>::new()));
        object.set_property("value", Value::string(Vec::<u8>::new()));
    }
}

fn writer_buffer(object: &ObjectRef) -> String {
    match object.get_property(XML_WRITER_BUFFER) {
        Some(Value::String(value)) => value.to_string_lossy(),
        _ => String::new(),
    }
}

fn writer_stack(object: &ObjectRef) -> PhpArray {
    match object.get_property(XML_WRITER_STACK) {
        Some(Value::Array(array)) => array,
        _ => PhpArray::new(),
    }
}

fn close_writer_open_tag(object: &ObjectRef) {
    if matches!(
        object.get_property(XML_WRITER_OPEN_TAG),
        Some(Value::Bool(true))
    ) {
        let mut buffer = writer_buffer(object);
        buffer.push('>');
        object.set_property(XML_WRITER_BUFFER, Value::string(buffer.into_bytes()));
        object.set_property(XML_WRITER_OPEN_TAG, Value::Bool(false));
    }
}

fn escape_text(value: &str, attribute: bool) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            '\'' if attribute => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn parse_document(mut self) -> Result<XmlDocument, String> {
        self.skip_ws();
        if self.starts_with("<?xml") {
            self.consume_until("?>")?;
            self.skip_ws();
        }
        if self.starts_with("<!") {
            return Err(
                "E_PHP_RUNTIME_XML_UNSUPPORTED_DECLARATION: DTD and declarations are unsupported"
                    .to_owned(),
            );
        }
        let root = self.parse_element()?;
        self.skip_ws();
        if !self.eof() {
            return Err(
                "E_PHP_RUNTIME_XML_TRAILING_CONTENT: XML document has trailing content".to_owned(),
            );
        }
        Ok(XmlDocument { root })
    }

    fn parse_element(&mut self) -> Result<XmlElement, String> {
        self.expect("<")?;
        if self.starts_with("/") {
            return Err("E_PHP_RUNTIME_XML_UNEXPECTED_CLOSE: unexpected closing tag".to_owned());
        }
        if self.starts_with("!") || self.starts_with("?") {
            return Err(
                "E_PHP_RUNTIME_XML_UNSUPPORTED_DECLARATION: declarations are unsupported"
                    .to_owned(),
            );
        }
        let name = self.parse_name()?;
        let mut attributes = Vec::new();
        loop {
            self.skip_ws();
            if self.starts_with("/>") {
                self.pos += 2;
                return Ok(XmlElement {
                    name,
                    attributes,
                    children: Vec::new(),
                });
            }
            if self.starts_with(">") {
                self.pos += 1;
                break;
            }
            let attr = self.parse_name()?;
            self.skip_ws();
            self.expect("=")?;
            self.skip_ws();
            let value = self.parse_quoted_value()?;
            attributes.push((attr, value));
        }
        let mut children = Vec::new();
        loop {
            if self.eof() {
                return Err(format!(
                    "E_PHP_RUNTIME_XML_UNCLOSED_ELEMENT: element {name} is not closed"
                ));
            }
            if self.starts_with("</") {
                self.pos += 2;
                let close = self.parse_name()?;
                self.skip_ws();
                self.expect(">")?;
                if close != name {
                    return Err(format!(
                        "E_PHP_RUNTIME_XML_MISMATCHED_CLOSE: expected </{name}> got </{close}>"
                    ));
                }
                return Ok(XmlElement {
                    name,
                    attributes,
                    children,
                });
            }
            if self.starts_with("<") {
                children.push(XmlNode::Element(self.parse_element()?));
            } else {
                let text = self.parse_text()?;
                if !text.is_empty() {
                    children.push(XmlNode::Text(text));
                }
            }
        }
    }

    fn parse_name(&mut self) -> Result<String, String> {
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':' | '.') {
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err("E_PHP_RUNTIME_XML_EXPECTED_NAME: expected XML name".to_owned());
        }
        Ok(self.input[start..self.pos].to_owned())
    }

    fn parse_quoted_value(&mut self) -> Result<String, String> {
        let Some(quote) = self.peek_char() else {
            return Err("E_PHP_RUNTIME_XML_EXPECTED_QUOTE: expected quoted attribute".to_owned());
        };
        if quote != '"' && quote != '\'' {
            return Err("E_PHP_RUNTIME_XML_EXPECTED_QUOTE: expected quoted attribute".to_owned());
        }
        self.pos += 1;
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch == quote {
                let raw = &self.input[start..self.pos];
                self.pos += 1;
                return decode_entities(raw);
            }
            self.pos += ch.len_utf8();
        }
        Err("E_PHP_RUNTIME_XML_UNCLOSED_ATTRIBUTE: attribute is not closed".to_owned())
    }

    fn parse_text(&mut self) -> Result<String, String> {
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch == '<' {
                break;
            }
            self.pos += ch.len_utf8();
        }
        decode_entities(&self.input[start..self.pos])
    }

    fn consume_until(&mut self, marker: &str) -> Result<(), String> {
        let Some(index) = self.input[self.pos..].find(marker) else {
            return Err(
                "E_PHP_RUNTIME_XML_UNCLOSED_DECLARATION: XML declaration is not closed".to_owned(),
            );
        };
        self.pos += index + marker.len();
        Ok(())
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() {
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, token: &str) -> Result<(), String> {
        if self.starts_with(token) {
            self.pos += token.len();
            Ok(())
        } else {
            Err(format!(
                "E_PHP_RUNTIME_XML_EXPECTED_TOKEN: expected {token}"
            ))
        }
    }

    fn starts_with(&self, token: &str) -> bool {
        self.input[self.pos..].starts_with(token)
    }

    fn eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }
}

fn decode_entities(raw: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(index) = rest.find('&') {
        out.push_str(&rest[..index]);
        let after = &rest[index + 1..];
        let Some(end) = after.find(';') else {
            return Err("E_PHP_RUNTIME_XML_UNCLOSED_ENTITY: entity is not closed".to_owned());
        };
        let entity = &after[..end];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {
                return Err(format!(
                    "E_PHP_RUNTIME_XML_UNSUPPORTED_ENTITY: entity &{entity}; is unsupported"
                ));
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_serializes_nested_xml() {
        let document =
            parse_xml(r#"<root id="7"><child>A &amp; B</child></root>"#).expect("valid XML");
        assert_eq!(document.root.name, "root");
        assert_eq!(
            document.root.attributes[0],
            ("id".to_owned(), "7".to_owned())
        );
        assert_eq!(element_text(&document.root), "A & B");
        assert_eq!(
            serialize_document(&document),
            r#"<root id="7"><child>A &amp; B</child></root>"#
        );
    }

    #[test]
    fn rejects_unsupported_entities() {
        let error = parse_xml("<root>&xxe;</root>").expect_err("entity must fail");
        assert!(error.contains("E_PHP_RUNTIME_XML_UNSUPPORTED_ENTITY"));
    }

    #[test]
    fn reader_events_include_element_text_and_close() {
        let document = parse_xml("<root><child a=\"b\">text</child></root>").expect("valid XML");
        let events = reader_events(&document);
        assert_eq!(events[0].node_type, XML_READER_ELEMENT);
        assert_eq!(events[1].name, "child");
        assert_eq!(events[2].value, "text");
        assert_eq!(events[3].node_type, XML_READER_END_ELEMENT);
    }
}
