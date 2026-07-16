//! XML/DOM/SimpleXML runtime support.
//!
//! Parsing is routed through the libxml2-backed `xml_backend` module. This file
//! keeps the current PHP wrapper objects and small tree projection used by DOM,
//! SimpleXML, XMLReader, XMLWriter, and ext/xml entry points.

use crate::{
    ArrayKey, ClassEntry, ClassFlags, ObjectRef, PhpArray, PhpString, Value, normalize_class_name,
};
use std::collections::BTreeMap;

const XML_STORAGE: &str = "__phrust_xml_document";
const XML_NODE_PATH: &str = "__phrust_xml_path";
const XML_NODE_KIND: &str = "__phrust_xml_node_kind";
const XML_TEXT_STORAGE: &str = "__phrust_xml_text";
const XML_READER_EVENTS: &str = "__phrust_xml_reader_events";
const XML_READER_INDEX: &str = "__phrust_xml_reader_index";
const XML_READER_ATTRIBUTE_INDEX: &str = "__phrust_xml_reader_attribute_index";
const XML_WRITER_BUFFER: &str = "__phrust_xml_writer_buffer";
const XML_WRITER_STACK: &str = "__phrust_xml_writer_stack";
const XML_WRITER_OPEN_TAG: &str = "__phrust_xml_writer_open_tag";
const XML_WRITER_URI: &str = "__phrust_xml_writer_uri";
const DOM_ENTRIES: &str = "__entries";
pub const XML_PARSER_START_ELEMENT_HANDLER: &str = "__phrust_xml_start_element_handler";
pub const XML_PARSER_END_ELEMENT_HANDLER: &str = "__phrust_xml_end_element_handler";
pub const XML_PARSER_CHARACTER_DATA_HANDLER: &str = "__phrust_xml_character_data_handler";
pub const XML_PARSER_DEFAULT_HANDLER: &str = "__phrust_xml_default_handler";
const SIMPLEXML_ENTRIES: &str = "__entries";
const SIMPLEXML_ENTRY_NAMES: &str = "__entry_names";
const SIMPLEXML_COUNT: &str = "__phrust_simplexml_count";
const SIMPLEXML_NAME: &str = "__phrust_simplexml_name";
const SIMPLEXML_XPATH_ATTRIBUTE: &str = "__phrust_simplexml_xpath_attribute";
const SIMPLEXML_XPATH_NAMESPACES: &str = "__phrust_simplexml_xpath_namespaces";

pub const XML_READER_NONE: i64 = 0;
pub const XML_READER_ELEMENT: i64 = 1;
pub const XML_READER_ATTRIBUTE: i64 = 2;
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
    Comment(String),
    Cdata(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XmlReaderEvent {
    pub node_type: i64,
    pub name: String,
    pub value: String,
    pub attributes: Vec<(String, String)>,
    pub namespace_uri: String,
    pub namespaces: Vec<(String, String)>,
    pub depth: i64,
    pub inner_xml: String,
    pub outer_xml: String,
    pub string_value: String,
}

/// Parses a strict XML document using no external resources.
pub fn parse_xml(input: &str) -> Result<XmlDocument, String> {
    crate::xml_backend::parse_document(input)
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
            XmlNode::Comment(text) => {
                out.push_str("<!--");
                out.push_str(text);
                out.push_str("-->");
            }
            XmlNode::Cdata(text) => {
                out.push_str("<![CDATA[");
                out.push_str(text);
                out.push_str("]]>");
            }
        }
    }
    out.push_str("</");
    out.push_str(&element.name);
    out.push('>');
    out
}

fn serialize_children(element: &XmlElement) -> String {
    let mut out = String::new();
    for child in &element.children {
        match child {
            XmlNode::Element(child) => out.push_str(&serialize_element(child)),
            XmlNode::Text(text) => out.push_str(&escape_text(text, false)),
            XmlNode::Comment(text) => {
                out.push_str("<!--");
                out.push_str(text);
                out.push_str("-->");
            }
            XmlNode::Cdata(text) => {
                out.push_str("<![CDATA[");
                out.push_str(text);
                out.push_str("]]>");
            }
        }
    }
    out
}

pub fn element_text(element: &XmlElement) -> String {
    let mut out = String::new();
    collect_text(element, &mut out);
    out
}

fn simplexml_element_text(element: &XmlElement) -> String {
    let mut out = String::new();
    for child in &element.children {
        match child {
            XmlNode::Text(text) | XmlNode::Cdata(text) => out.push_str(text),
            XmlNode::Element(_) | XmlNode::Comment(_) => {}
        }
    }
    out
}

pub fn reader_events(document: &XmlDocument) -> Vec<XmlReaderEvent> {
    let mut events = Vec::new();
    push_reader_events(&document.root, &mut events);
    events
}

pub fn parse_into_struct_arrays(
    document: &XmlDocument,
    case_folding: bool,
) -> (PhpArray, PhpArray) {
    let mut values = PhpArray::new();
    let mut index = PhpArray::new();
    push_struct_element(&document.root, 1, case_folding, &mut values, &mut index);
    (values, index)
}

fn element_element_children(element: &XmlElement) -> Vec<&XmlElement> {
    element
        .children
        .iter()
        .filter_map(|child| match child {
            XmlNode::Element(element) => Some(element),
            XmlNode::Text(_) | XmlNode::Comment(_) | XmlNode::Cdata(_) => None,
        })
        .collect()
}

fn push_struct_element(
    element: &XmlElement,
    level: i64,
    case_folding: bool,
    values: &mut PhpArray,
    index: &mut PhpArray,
) {
    let tag = struct_tag_name(&element.name, case_folding);
    let has_element_children = element
        .children
        .iter()
        .any(|child| matches!(child, XmlNode::Element(_)));
    if !has_element_children {
        let mut row = struct_base_row(&tag, "complete", level);
        insert_struct_attributes(&mut row, element, case_folding);
        let text = element_text(element);
        if !text.is_empty() {
            row.insert(
                ArrayKey::String(PhpString::from("value")),
                Value::string(text.into_bytes()),
            );
        }
        append_struct_row(values, index, &tag, row);
        return;
    }

    let mut row = struct_base_row(&tag, "open", level);
    insert_struct_attributes(&mut row, element, case_folding);
    let mut leading_text = String::new();
    for child in &element.children {
        match child {
            XmlNode::Element(_) => break,
            XmlNode::Text(text) | XmlNode::Cdata(text) => leading_text.push_str(text),
            XmlNode::Comment(_) => {}
        }
    }
    if !leading_text.is_empty() {
        row.insert(
            ArrayKey::String(PhpString::from("value")),
            Value::string(leading_text.into_bytes()),
        );
    }
    append_struct_row(values, index, &tag, row);

    let mut emitted_element_child = false;
    for child in &element.children {
        match child {
            XmlNode::Element(child) => {
                emitted_element_child = true;
                push_struct_element(child, level + 1, case_folding, values, index);
            }
            XmlNode::Text(text) | XmlNode::Cdata(text)
                if emitted_element_child && !text.is_empty() =>
            {
                let mut row = PhpArray::new();
                row.insert(
                    ArrayKey::String(PhpString::from("tag")),
                    Value::string(tag.as_bytes().to_vec()),
                );
                row.insert(
                    ArrayKey::String(PhpString::from("value")),
                    Value::string(text.as_bytes().to_vec()),
                );
                row.insert(
                    ArrayKey::String(PhpString::from("type")),
                    Value::string(b"cdata".to_vec()),
                );
                row.insert(
                    ArrayKey::String(PhpString::from("level")),
                    Value::Int(level),
                );
                append_struct_row(values, index, &tag, row);
            }
            XmlNode::Text(_) | XmlNode::Cdata(_) | XmlNode::Comment(_) => {}
        }
    }

    append_struct_row(values, index, &tag, struct_base_row(&tag, "close", level));
}

fn struct_base_row(tag: &str, row_type: &str, level: i64) -> PhpArray {
    let mut row = PhpArray::new();
    row.insert(
        ArrayKey::String(PhpString::from("tag")),
        Value::string(tag.as_bytes().to_vec()),
    );
    row.insert(
        ArrayKey::String(PhpString::from("type")),
        Value::string(row_type.as_bytes().to_vec()),
    );
    row.insert(
        ArrayKey::String(PhpString::from("level")),
        Value::Int(level),
    );
    row
}

fn insert_struct_attributes(row: &mut PhpArray, element: &XmlElement, case_folding: bool) {
    if element.attributes.is_empty() {
        return;
    }
    let mut attributes = PhpArray::new();
    for (name, value) in &element.attributes {
        let name = struct_tag_name(name, case_folding);
        attributes.insert(
            ArrayKey::String(PhpString::from(name.as_str())),
            Value::string(value.as_bytes().to_vec()),
        );
    }
    row.insert(
        ArrayKey::String(PhpString::from("attributes")),
        Value::Array(attributes),
    );
}

fn append_struct_row(values: &mut PhpArray, index: &mut PhpArray, tag: &str, row: PhpArray) {
    let position = values.len() as i64;
    values.append(Value::Array(row));
    let key = ArrayKey::String(PhpString::from(tag));
    let mut bucket = match index.get(&key) {
        Some(Value::Array(array)) => array.clone(),
        _ => PhpArray::new(),
    };
    bucket.append(Value::Int(position));
    index.insert(key, Value::Array(bucket));
}

fn struct_tag_name(name: &str, case_folding: bool) -> String {
    if case_folding {
        name.to_ascii_uppercase()
    } else {
        name.to_owned()
    }
}

pub fn empty_internal_class(name: &str) -> ClassEntry {
    ClassEntry {
        name: normalize_class_name(name).into(),
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
    let object =
        ObjectRef::new_with_display_name(&empty_internal_class("DOMDocument"), "DOMDocument");
    object.set_property("nodeName", Value::string("#document"));
    object.set_property("nodeValue", Value::Null);
    object.set_property("textContent", Value::string(Vec::<u8>::new()));
    object.set_property("documentElement", Value::Null);
    object.set_property("firstChild", Value::Null);
    object.set_property("lastChild", Value::Null);
    object.set_property(
        "childNodes",
        Value::Object(new_dom_node_list_values(Vec::new())),
    );
    object.set_property("parentNode", Value::Null);
    object.set_property("ownerDocument", Value::Null);
    object
}

pub fn new_dom_node() -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&empty_internal_class("DOMNode"), "DOMNode");
    object.set_property("parentNode", Value::Null);
    object.set_property("ownerDocument", Value::Null);
    object
}

pub fn new_dom_text(text: &str) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&empty_internal_class("DOMText"), "DOMText");
    object.set_property("nodeName", Value::string("#text"));
    object.set_property("nodeValue", Value::string(text.as_bytes().to_vec()));
    object.set_property("textContent", Value::string(text.as_bytes().to_vec()));
    object.set_property("data", Value::string(text.as_bytes().to_vec()));
    object.set_property(XML_NODE_KIND, Value::string(b"text".to_vec()));
    object.set_property(XML_TEXT_STORAGE, Value::string(text.as_bytes().to_vec()));
    object.set_property("parentNode", Value::Null);
    object.set_property("ownerDocument", Value::Null);
    object
}

pub fn new_dom_comment(text: &str) -> ObjectRef {
    let object =
        ObjectRef::new_with_display_name(&empty_internal_class("DOMComment"), "DOMComment");
    object.set_property("nodeName", Value::string("#comment"));
    object.set_property("nodeValue", Value::string(text.as_bytes().to_vec()));
    object.set_property("textContent", Value::string(text.as_bytes().to_vec()));
    object.set_property("data", Value::string(text.as_bytes().to_vec()));
    object.set_property(XML_NODE_KIND, Value::string(b"comment".to_vec()));
    object.set_property(XML_TEXT_STORAGE, Value::string(text.as_bytes().to_vec()));
    object.set_property("parentNode", Value::Null);
    object.set_property("ownerDocument", Value::Null);
    object
}

pub fn new_dom_cdata_section(text: &str) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(
        &empty_internal_class("DOMCdataSection"),
        "DOMCdataSection",
    );
    object.set_property("nodeName", Value::string("#cdata-section"));
    object.set_property("nodeValue", Value::string(text.as_bytes().to_vec()));
    object.set_property("textContent", Value::string(text.as_bytes().to_vec()));
    object.set_property("data", Value::string(text.as_bytes().to_vec()));
    object.set_property(XML_NODE_KIND, Value::string(b"cdata".to_vec()));
    object.set_property(XML_TEXT_STORAGE, Value::string(text.as_bytes().to_vec()));
    object.set_property("parentNode", Value::Null);
    object.set_property("ownerDocument", Value::Null);
    object
}

pub fn new_dom_attr(name: &str, value: &str) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&empty_internal_class("DOMAttr"), "DOMAttr");
    object.set_property("name", Value::string(name.as_bytes().to_vec()));
    object.set_property("value", Value::string(value.as_bytes().to_vec()));
    object.set_property("nodeName", Value::string(name.as_bytes().to_vec()));
    object.set_property("nodeValue", Value::string(value.as_bytes().to_vec()));
    object.set_property("textContent", Value::string(value.as_bytes().to_vec()));
    object.set_property("parentNode", Value::Null);
    object.set_property("ownerDocument", Value::Null);
    object
}

pub fn new_dom_xpath(document: &ObjectRef) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&empty_internal_class("DOMXPath"), "DOMXPath");
    if let Some(document) = document_from_object(document) {
        object.set_property(XML_STORAGE, document_value(&document));
    }
    object
}

pub fn new_xml_parser() -> ObjectRef {
    ObjectRef::new_with_display_name(&empty_internal_class("XMLParser"), "XMLParser")
}

pub fn new_dom_element(element: &XmlElement) -> ObjectRef {
    let object =
        ObjectRef::new_with_display_name(&empty_internal_class("DOMElement"), "DOMElement");
    set_dom_element_object(&object, element);
    object
}

pub fn new_dom_named_node_map(attributes: &[(String, String)]) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(
        &empty_internal_class("DOMNamedNodeMap"),
        "DOMNamedNodeMap",
    );
    let mut entries = PhpArray::new();
    for (index, (name, value)) in attributes
        .iter()
        .filter(|(name, _)| !is_namespace_declaration_name(name))
        .enumerate()
    {
        let attribute = Value::Object(new_dom_attr(name, value));
        object.set_property(index.to_string(), attribute.clone());
        object.set_property(name.clone(), attribute.clone());
        entries.insert(ArrayKey::Int(index as i64), attribute);
    }
    object.set_property("length", Value::Int(entries.len() as i64));
    object.set_property(DOM_ENTRIES, Value::Array(entries));
    object
}

fn new_dom_node_from_xml_node(node: &XmlNode) -> Value {
    match node {
        XmlNode::Element(element) => Value::Object(new_dom_element(element)),
        XmlNode::Text(text) => Value::Object(new_dom_text(text)),
        XmlNode::Comment(text) => Value::Object(new_dom_comment(text)),
        XmlNode::Cdata(text) => Value::Object(new_dom_cdata_section(text)),
    }
}

fn is_namespace_declaration_name(name: &str) -> bool {
    name == "xmlns" || name.starts_with("xmlns:")
}

fn new_dom_node_list_values(values: Vec<Value>) -> ObjectRef {
    let object =
        ObjectRef::new_with_display_name(&empty_internal_class("DOMNodeList"), "DOMNodeList");
    let mut entries = PhpArray::new();
    for (index, node) in values.into_iter().enumerate() {
        object.set_property(index.to_string(), node.clone());
        entries.insert(ArrayKey::Int(index as i64), node);
    }
    object.set_property("length", Value::Int(entries.len() as i64));
    object.set_property(DOM_ENTRIES, Value::Array(entries));
    object
}

pub fn new_dom_node_list(elements: Vec<XmlElement>) -> ObjectRef {
    new_dom_node_list_values(
        elements
            .into_iter()
            .map(|element| Value::Object(new_dom_element(&element)))
            .collect(),
    )
}

fn set_dom_element_base_properties(object: &ObjectRef, element: &XmlElement) {
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
    object.set_property(
        "attributes",
        Value::Object(new_dom_named_node_map(&element.attributes)),
    );
    let child_values: Vec<Value> = element
        .children
        .iter()
        .map(new_dom_node_from_xml_node)
        .collect();
    object.set_property(
        "firstChild",
        child_values.first().cloned().unwrap_or(Value::Null),
    );
    object.set_property(
        "lastChild",
        child_values.last().cloned().unwrap_or(Value::Null),
    );
    object.set_property(
        "childNodes",
        Value::Object(new_dom_node_list_values(child_values)),
    );
    object.set_property("parentNode", Value::Null);
    object.set_property("ownerDocument", Value::Null);
}

pub fn new_simplexml_element(element: &XmlElement) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(
        &empty_internal_class("SimpleXMLElement"),
        "SimpleXMLElement",
    );
    object.set_property(
        SIMPLEXML_NAME,
        Value::string(element.name.as_bytes().to_vec()),
    );
    object.set_property(
        SIMPLEXML_COUNT,
        Value::Int(element_element_children(element).len() as i64),
    );
    object.set_property(
        "__text",
        Value::string(simplexml_element_text(element).into_bytes()),
    );
    object.set_property(
        XML_STORAGE,
        document_value(&XmlDocument {
            root: element.clone(),
        }),
    );
    object.set_property(XML_NODE_PATH, Value::string(Vec::<u8>::new()));
    let mut children_by_name: BTreeMap<String, Vec<ObjectRef>> = BTreeMap::new();
    for child in element.children.iter().filter_map(|child| match child {
        XmlNode::Element(element) => Some(element),
        XmlNode::Text(_) | XmlNode::Comment(_) | XmlNode::Cdata(_) => None,
    }) {
        children_by_name
            .entry(child.name.clone())
            .or_default()
            .push(new_simplexml_element(child));
    }
    for (name, children) in children_by_name {
        object.set_property(
            name.clone(),
            Value::Object(new_simplexml_element_list(&name, &children)),
        );
    }
    object
}

fn new_simplexml_element_list(name: &str, elements: &[ObjectRef]) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(
        &empty_internal_class("SimpleXMLElement"),
        "SimpleXMLElement",
    );
    let visible_name = if elements.is_empty() { "" } else { name };
    object.set_property(
        SIMPLEXML_NAME,
        Value::string(visible_name.as_bytes().to_vec()),
    );
    object.set_property(SIMPLEXML_COUNT, Value::Int(elements.len() as i64));
    object.set_property("__text", Value::string(Vec::<u8>::new()));
    let mut entries = PhpArray::new();
    let mut entry_names = PhpArray::new();
    for (index, element) in elements.iter().enumerate() {
        let value = Value::Object(element.clone());
        object.set_property(index.to_string(), value.clone());
        entries.insert(ArrayKey::Int(index as i64), value);
        entry_names.insert(
            ArrayKey::Int(index as i64),
            Value::string(name.as_bytes().to_vec()),
        );
    }
    if let Some(first) = elements.first() {
        for (property, value) in first.properties_snapshot() {
            if property.starts_with("__") || property.contains(':') {
                continue;
            }
            object.set_property(property, value);
        }
    }
    object.set_property(SIMPLEXML_ENTRIES, Value::Array(entries));
    object.set_property(SIMPLEXML_ENTRY_NAMES, Value::Array(entry_names));
    object
}

fn new_simplexml_children_list(children: &[XmlNode]) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(
        &empty_internal_class("SimpleXMLElement"),
        "SimpleXMLElement",
    );
    object.set_property(SIMPLEXML_NAME, Value::string(Vec::<u8>::new()));
    object.set_property("__text", Value::string(Vec::<u8>::new()));
    let mut entries = PhpArray::new();
    let mut entry_names = PhpArray::new();
    let mut children_by_name: BTreeMap<String, Vec<ObjectRef>> = BTreeMap::new();
    for (index, child) in children
        .iter()
        .filter_map(|child| match child {
            XmlNode::Element(element) => Some(element),
            XmlNode::Text(_) | XmlNode::Comment(_) | XmlNode::Cdata(_) => None,
        })
        .enumerate()
    {
        let child_object = new_simplexml_element(child);
        let value = Value::Object(child_object.clone());
        object.set_property(index.to_string(), value.clone());
        entries.insert(ArrayKey::Int(index as i64), value);
        entry_names.insert(
            ArrayKey::Int(index as i64),
            Value::string(child.name.as_bytes().to_vec()),
        );
        children_by_name
            .entry(child.name.clone())
            .or_default()
            .push(child_object);
    }
    for (name, children) in children_by_name {
        object.set_property(
            name.clone(),
            Value::Object(new_simplexml_element_list(&name, &children)),
        );
    }
    object.set_property(SIMPLEXML_COUNT, Value::Int(entries.len() as i64));
    object.set_property(SIMPLEXML_ENTRIES, Value::Array(entries));
    object.set_property(SIMPLEXML_ENTRY_NAMES, Value::Array(entry_names));
    object
}

pub fn simplexml_attributes_object(element: &XmlElement) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(
        &empty_internal_class("SimpleXMLElement"),
        "SimpleXMLElement",
    );
    let first_name = element
        .attributes
        .first()
        .map(|(name, _)| name.as_str())
        .unwrap_or_default();
    let first_value = element
        .attributes
        .first()
        .map(|(_, value)| value.as_str())
        .unwrap_or_default();
    object.set_property(
        SIMPLEXML_NAME,
        Value::string(first_name.as_bytes().to_vec()),
    );
    object.set_property(SIMPLEXML_COUNT, Value::Int(element.attributes.len() as i64));
    object.set_property("__text", Value::string(first_value.as_bytes().to_vec()));
    let mut entries = PhpArray::new();
    let mut entry_names = PhpArray::new();
    for (index, (name, value)) in element.attributes.iter().enumerate() {
        let attribute = new_simplexml_attribute(name, value);
        let attribute_value = Value::Object(attribute);
        object.set_property(name.clone(), attribute_value.clone());
        object.set_property(index.to_string(), attribute_value.clone());
        entries.insert(ArrayKey::Int(index as i64), attribute_value);
        entry_names.insert(
            ArrayKey::Int(index as i64),
            Value::string(name.as_bytes().to_vec()),
        );
    }
    object.set_property(SIMPLEXML_ENTRIES, Value::Array(entries));
    object.set_property(SIMPLEXML_ENTRY_NAMES, Value::Array(entry_names));
    object
}

fn new_simplexml_attribute(name: &str, value: &str) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(
        &empty_internal_class("SimpleXMLElement"),
        "SimpleXMLElement",
    );
    object.set_property(SIMPLEXML_NAME, Value::string(name.as_bytes().to_vec()));
    object.set_property(SIMPLEXML_COUNT, Value::Int(0));
    object.set_property("__text", Value::string(value.as_bytes().to_vec()));
    object
}

fn new_simplexml_xpath_attribute(name: &str, value: &str) -> ObjectRef {
    let object = new_simplexml_attribute(name, value);
    object.set_property(SIMPLEXML_COUNT, Value::Int(1));
    object.set_property(SIMPLEXML_XPATH_ATTRIBUTE, Value::Bool(true));
    object
}

pub fn new_xml_reader() -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&empty_internal_class("XMLReader"), "XMLReader");
    set_xml_reader_current(&object, None);
    object.set_property(XML_READER_EVENTS, Value::Array(PhpArray::new()));
    object.set_property(XML_READER_INDEX, Value::Int(-1));
    object.set_property(XML_READER_ATTRIBUTE_INDEX, Value::Int(-1));
    object
}

pub fn new_xml_writer() -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&empty_internal_class("XMLWriter"), "XMLWriter");
    object.set_property(XML_WRITER_BUFFER, Value::string(Vec::<u8>::new()));
    object.set_property(XML_WRITER_STACK, Value::Array(PhpArray::new()));
    object.set_property(XML_WRITER_OPEN_TAG, Value::Bool(false));
    object.set_property(XML_WRITER_URI, Value::Null);
    object
}

pub fn dom_document_load_xml(object: &ObjectRef, xml: &str) -> Result<Value, String> {
    let document = parse_xml(xml)?;
    set_dom_document_object(object, &document);
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

pub fn dom_document_create_text_node(value: &str) -> Value {
    Value::Object(new_dom_text(value))
}

pub fn dom_document_create_comment(value: &str) -> Value {
    Value::Object(new_dom_comment(value))
}

pub fn dom_document_create_cdata_section(value: &str) -> Value {
    Value::Object(new_dom_cdata_section(value))
}

pub fn dom_document_create_attribute(name: &str) -> Value {
    Value::Object(new_dom_attr(name, ""))
}

pub fn dom_document_append_child(object: &ObjectRef, child: &ObjectRef) -> Value {
    let Some(child_element) = element_from_object(child) else {
        return Value::Null;
    };
    let document = XmlDocument {
        root: child_element.clone(),
    };
    set_dom_document_object(object, &document);
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

pub fn dom_document_get_elements_by_tag_name_ns(
    object: &ObjectRef,
    namespace_uri: &str,
    local_name: &str,
) -> Value {
    document_from_object(object)
        .map(|document| {
            Value::Object(new_dom_node_list(elements_by_tag_name_ns(
                &document.root,
                namespace_uri,
                local_name,
            )))
        })
        .unwrap_or_else(|| Value::Object(new_dom_node_list(Vec::new())))
}

pub fn dom_element_get_elements_by_tag_name(object: &ObjectRef, name: &str) -> Value {
    element_from_object(object)
        .map(|element| Value::Object(new_dom_node_list(elements_by_tag_name(&element, name))))
        .unwrap_or_else(|| Value::Object(new_dom_node_list(Vec::new())))
}

pub fn dom_element_get_elements_by_tag_name_ns(
    object: &ObjectRef,
    namespace_uri: &str,
    local_name: &str,
) -> Value {
    element_from_object(object)
        .map(|element| {
            Value::Object(new_dom_node_list(elements_by_tag_name_ns(
                &element,
                namespace_uri,
                local_name,
            )))
        })
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

pub fn dom_element_has_attribute(object: &ObjectRef, name: &str) -> Value {
    Value::Bool(
        element_from_object(object)
            .is_some_and(|element| element.attributes.iter().any(|(attr, _)| attr == name)),
    )
}

pub fn dom_element_get_attribute_node(object: &ObjectRef, name: &str) -> Value {
    element_from_object(object)
        .and_then(|element| {
            element
                .attributes
                .into_iter()
                .find(|(attr, _)| attr == name)
                .map(|(attr, value)| Value::Object(new_dom_attr(&attr, &value)))
        })
        .unwrap_or(Value::Null)
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

pub fn dom_element_remove_attribute(object: &ObjectRef, name: &str) -> Value {
    let Some(mut element) = element_from_object(object) else {
        return Value::Null;
    };
    element.attributes.retain(|(attr, _)| attr != name);
    set_dom_element_object(object, &element);
    Value::Null
}

pub fn dom_element_set_attribute_node(object: &ObjectRef, attribute: &ObjectRef) -> Value {
    let name = match attribute.get_property("name") {
        Some(Value::String(name)) => name.to_string_lossy(),
        _ => return Value::Null,
    };
    let value = match attribute.get_property("value") {
        Some(Value::String(value)) => value.to_string_lossy(),
        _ => String::new(),
    };
    dom_element_set_attribute(object, &name, &value);
    Value::Object(attribute.clone())
}

pub fn dom_element_append_child(object: &ObjectRef, child: &ObjectRef) -> Value {
    let Some(mut element) = element_from_object(object) else {
        return Value::Null;
    };
    if let Some(child_element) = element_from_object(child) {
        element
            .children
            .push(XmlNode::Element(child_element.clone()));
        set_dom_element_object(object, &element);
        return Value::Object(new_dom_element(&child_element));
    }
    let Some(text) = text_from_object(child) else {
        return Value::Null;
    };
    let node = match node_kind_from_object(child).as_deref() {
        Some("comment") => XmlNode::Comment(text.clone()),
        Some("cdata") => XmlNode::Cdata(text.clone()),
        _ => XmlNode::Text(text.clone()),
    };
    element.children.push(node);
    set_dom_element_object(object, &element);
    Value::Object(child.clone())
}

pub fn dom_node_list_item(object: &ObjectRef, index: i64) -> Value {
    let Some(Value::Array(entries)) = object.get_property(DOM_ENTRIES) else {
        return Value::Null;
    };
    entries
        .get(&ArrayKey::Int(index))
        .cloned()
        .unwrap_or(Value::Null)
}

pub fn dom_named_node_map_item(object: &ObjectRef, index: i64) -> Value {
    dom_node_list_item(object, index)
}

pub fn dom_named_node_map_get_named_item(object: &ObjectRef, name: &str) -> Value {
    object.get_property(name).unwrap_or(Value::Null)
}

pub fn dom_xpath_query(object: &ObjectRef, expression: &str) -> Value {
    let Some(document) = document_from_object(object) else {
        return Value::Object(new_dom_node_list(Vec::new()));
    };
    crate::xml_backend::parse_string(&serialize_document(&document))
        .and_then(|document| document.xpath_elements(expression))
        .map(|elements| Value::Object(new_dom_node_list(elements)))
        .unwrap_or_else(|_| Value::Bool(false))
}

pub fn dom_xpath_evaluate(object: &ObjectRef, expression: &str) -> Value {
    let Some(document) = document_from_object(object) else {
        return Value::Bool(false);
    };
    crate::xml_backend::parse_string(&serialize_document(&document))
        .and_then(|document| document.xpath_string(expression))
        .map(|value| Value::string(value.into_bytes()))
        .unwrap_or_else(|_| Value::Bool(false))
}

pub fn simplexml_load_string(xml: &str) -> Result<Value, String> {
    let document = parse_xml(xml)?;
    Ok(Value::Object(new_simplexml_element(&document.root)))
}

pub fn simplexml_import_dom(object: &ObjectRef) -> Value {
    element_from_object(object)
        .map(|element| Value::Object(new_simplexml_element(&element)))
        .unwrap_or(Value::Bool(false))
}

pub fn dom_import_simplexml(object: &ObjectRef) -> Value {
    simplexml_context_element(object)
        .map(|element| Value::Object(new_dom_element(&element)))
        .unwrap_or(Value::Bool(false))
}

pub fn simplexml_as_xml(object: &ObjectRef) -> Value {
    if simplexml_is_xpath_attribute(object) {
        let name = match simplexml_get_name(object) {
            Value::String(name) => name.to_string_lossy(),
            _ => String::new(),
        };
        let value = match simplexml_text(object) {
            Value::String(value) => value.to_string_lossy(),
            _ => String::new(),
        };
        return Value::string(format!(" {name}=\"{}\"", escape_text(&value, true)).into_bytes());
    }
    document_from_object(object)
        .map(|document| Value::string(serialize_document(&document).into_bytes()))
        .or_else(|| {
            first_simplexml_entry(object).and_then(|entry| match simplexml_as_xml(&entry) {
                Value::String(value) => Some(Value::String(value)),
                _ => None,
            })
        })
        .unwrap_or_else(|| Value::string(Vec::<u8>::new()))
}

pub fn simplexml_text(object: &ObjectRef) -> Value {
    document_from_object(object)
        .map(|document| Value::string(simplexml_element_text(&document.root).into_bytes()))
        .or_else(|| {
            first_simplexml_entry(object).and_then(|entry| match simplexml_text(&entry) {
                Value::String(value) => Some(Value::String(value)),
                _ => None,
            })
        })
        .unwrap_or_else(|| {
            object
                .get_property("__text")
                .unwrap_or(Value::string(Vec::<u8>::new()))
        })
}

pub fn simplexml_attributes(object: &ObjectRef) -> Value {
    document_from_object(object)
        .map(|document| Value::Object(simplexml_attributes_object(&document.root)))
        .or_else(|| first_simplexml_entry(object).map(|entry| simplexml_attributes(&entry)))
        .unwrap_or(Value::Null)
}

pub fn simplexml_children(object: &ObjectRef) -> Value {
    document_from_object(object)
        .map(|document| Value::Object(new_simplexml_children_list(&document.root.children)))
        .or_else(|| first_simplexml_entry(object).map(|entry| simplexml_children(&entry)))
        .unwrap_or_else(|| Value::Object(object.clone()))
}

pub fn simplexml_get_name(object: &ObjectRef) -> Value {
    object
        .get_property(SIMPLEXML_NAME)
        .unwrap_or_else(|| Value::string(Vec::<u8>::new()))
}

pub fn simplexml_property(object: &ObjectRef, property: &str) -> Value {
    object
        .get_property(property)
        .unwrap_or_else(|| Value::Object(new_simplexml_element_list(property, &[])))
}

pub fn simplexml_dimension(object: &ObjectRef, key: &ArrayKey) -> Value {
    match key {
        ArrayKey::Int(index) => simplexml_numeric_dimension(object, *index),
        ArrayKey::String(name) => simplexml_string_dimension(object, &name.to_string_lossy()),
    }
}

pub fn simplexml_register_xpath_namespace(
    object: &ObjectRef,
    prefix: &str,
    namespace: &str,
) -> Value {
    let mut namespaces = match object.get_property(SIMPLEXML_XPATH_NAMESPACES) {
        Some(Value::Array(namespaces)) => namespaces,
        _ => PhpArray::new(),
    };
    namespaces.insert(
        ArrayKey::String(PhpString::from(prefix.as_bytes().to_vec())),
        Value::string(namespace.as_bytes().to_vec()),
    );
    object.set_property(SIMPLEXML_XPATH_NAMESPACES, Value::Array(namespaces));
    Value::Bool(true)
}

pub fn simplexml_xpath(object: &ObjectRef, expression: &str) -> Value {
    let Some(context) = simplexml_context_element(object) else {
        return Value::Array(PhpArray::new());
    };
    let Some(path) = parse_simplexml_xpath(expression) else {
        return Value::Bool(false);
    };
    simplexml_xpath_value(&context, &path)
}

pub fn simplexml_empty_access(object: &ObjectRef) -> bool {
    if let Some(entry) = first_simplexml_entry(object) {
        return simplexml_empty_access(&entry);
    }
    if let Some(document) = document_from_object(object) {
        let text = simplexml_element_text(&document.root);
        return element_element_children(&document.root).is_empty()
            && (text.is_empty() || text.as_bytes() == b"0");
    }
    match object.get_property("__text") {
        Some(Value::String(text)) => text.is_empty() || text.as_bytes() == b"0",
        Some(Value::Uninitialized | Value::Null) | None => true,
        Some(value) => matches!(value, Value::Bool(false) | Value::Int(0)),
    }
}

fn simplexml_numeric_dimension(object: &ObjectRef, index: i64) -> Value {
    if index < 0 {
        return Value::Null;
    }
    if index == 0 && simplexml_is_xpath_attribute(object) {
        return Value::Object(object.clone());
    }
    if let Some(Value::Array(entries)) = object.get_property(SIMPLEXML_ENTRIES) {
        return entries
            .get(&ArrayKey::Int(index))
            .cloned()
            .unwrap_or(Value::Null);
    }
    if index == 0 && document_from_object(object).is_some() {
        return Value::Object(object.clone());
    }
    Value::Null
}

fn simplexml_string_dimension(object: &ObjectRef, name: &str) -> Value {
    if simplexml_entries_are_attributes(object) {
        return object.get_property(name).unwrap_or(Value::Null);
    }
    document_from_object(object)
        .and_then(|document| {
            document
                .root
                .attributes
                .into_iter()
                .find(|(attr, _)| attr == name)
                .map(|(attr, value)| Value::Object(new_simplexml_attribute(&attr, &value)))
        })
        .or_else(|| {
            first_simplexml_entry(object).and_then(|entry| {
                match simplexml_dimension(
                    &entry,
                    &ArrayKey::String(PhpString::from(name.as_bytes().to_vec())),
                ) {
                    Value::Null => None,
                    value => Some(value),
                }
            })
        })
        .unwrap_or(Value::Null)
}

fn simplexml_entries_are_attributes(object: &ObjectRef) -> bool {
    first_simplexml_entry(object).is_some_and(|entry| document_from_object(&entry).is_none())
}

fn simplexml_is_xpath_attribute(object: &ObjectRef) -> bool {
    matches!(
        object.get_property(SIMPLEXML_XPATH_ATTRIBUTE),
        Some(Value::Bool(true))
    )
}

fn simplexml_context_element(object: &ObjectRef) -> Option<XmlElement> {
    document_from_object(object)
        .map(|document| document.root)
        .or_else(|| {
            first_simplexml_entry(object).and_then(|entry| simplexml_context_element(&entry))
        })
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SimpleXmlXPath {
    absolute: bool,
    steps: Vec<SimpleXmlXPathStep>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SimpleXmlXPathStep {
    descendant: bool,
    axis: SimpleXmlXPathAxis,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SimpleXmlXPathAxis {
    Element(String),
    Attribute(String),
}

fn parse_simplexml_xpath(expression: &str) -> Option<SimpleXmlXPath> {
    let mut expression = expression.trim();
    if expression.is_empty()
        || expression.contains('[')
        || expression.contains(']')
        || expression.contains('(')
        || expression.contains(')')
        || expression.contains('|')
    {
        return None;
    }
    if let Some(rest) = expression.strip_prefix("./") {
        expression = rest;
    }
    let absolute = expression.starts_with('/') && !expression.starts_with("//");
    if absolute {
        expression = &expression[1..];
    }

    let mut steps = Vec::new();
    let mut rest = expression;
    while !rest.is_empty() {
        let mut descendant = false;
        if let Some(next) = rest.strip_prefix("//") {
            descendant = true;
            rest = next;
        } else if let Some(next) = rest.strip_prefix('/') {
            rest = next;
        }
        if rest.is_empty() {
            return None;
        }
        if let Some(next) = rest.strip_prefix("./") {
            rest = next;
        }
        let (segment, next) = rest
            .split_once('/')
            .map(|(segment, next)| (segment, Some(next)))
            .unwrap_or((rest, None));
        if segment.is_empty() || segment == "." {
            return None;
        }
        let axis = if let Some(name) = segment.strip_prefix('@') {
            if name.is_empty() || !simplexml_xpath_name_is_supported(name) {
                return None;
            }
            SimpleXmlXPathAxis::Attribute(name.to_owned())
        } else {
            if !simplexml_xpath_name_is_supported(segment) {
                return None;
            }
            SimpleXmlXPathAxis::Element(segment.to_owned())
        };
        steps.push(SimpleXmlXPathStep { descendant, axis });
        rest = next.unwrap_or_default();
    }

    Some(SimpleXmlXPath { absolute, steps })
}

fn simplexml_xpath_name_is_supported(name: &str) -> bool {
    name == "*"
        || name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':'))
}

fn simplexml_xpath_value(context: &XmlElement, path: &SimpleXmlXPath) -> Value {
    let mut current = vec![context.clone()];
    let mut attributes = Vec::new();
    for (index, step) in path.steps.iter().enumerate() {
        match &step.axis {
            SimpleXmlXPathAxis::Element(name) => {
                if path.absolute && index == 0 && !step.descendant {
                    current.retain(|element| simplexml_xpath_name_matches(&element.name, name));
                } else if step.descendant {
                    let mut next = Vec::new();
                    for element in &current {
                        collect_simplexml_xpath_descendants(element, name, &mut next);
                    }
                    current = next;
                } else {
                    let mut next = Vec::new();
                    for element in &current {
                        for child in element_element_children(element) {
                            if simplexml_xpath_name_matches(&child.name, name) {
                                next.push(child.clone());
                            }
                        }
                    }
                    current = next;
                }
            }
            SimpleXmlXPathAxis::Attribute(name) => {
                if index + 1 != path.steps.len() {
                    return Value::Array(PhpArray::new());
                }
                attributes.clear();
                for element in &current {
                    for (attr, value) in &element.attributes {
                        if simplexml_xpath_name_matches(attr, name) {
                            attributes.push((attr.clone(), value.clone()));
                        }
                    }
                }
            }
        }
    }

    let mut array = PhpArray::new();
    if path
        .steps
        .last()
        .is_some_and(|step| matches!(step.axis, SimpleXmlXPathAxis::Attribute(_)))
    {
        for (name, value) in attributes {
            array.append(Value::Object(new_simplexml_xpath_attribute(&name, &value)));
        }
    } else {
        for element in current {
            array.append(Value::Object(new_simplexml_element(&element)));
        }
    }
    Value::Array(array)
}

fn collect_simplexml_xpath_descendants(
    element: &XmlElement,
    name: &str,
    out: &mut Vec<XmlElement>,
) {
    if simplexml_xpath_name_matches(&element.name, name) {
        out.push(element.clone());
    }
    for child in element_element_children(element) {
        collect_simplexml_xpath_descendants(child, name, out);
    }
}

fn simplexml_xpath_name_matches(candidate: &str, pattern: &str) -> bool {
    pattern == "*" || candidate == pattern
}

pub fn simplexml_count_property() -> &'static str {
    SIMPLEXML_COUNT
}

/// Returns the PHP-visible key/value sequence used by `foreach` on a
/// `SimpleXMLElement`. The vector deliberately preserves duplicate child names.
#[must_use]
pub fn simplexml_iteration_entries(object: &ObjectRef) -> Vec<(Value, Value)> {
    let iterable = if matches!(
        object.get_property(SIMPLEXML_ENTRIES),
        Some(Value::Array(_))
    ) {
        object.clone()
    } else {
        match simplexml_children(object) {
            Value::Object(children) => children,
            _ => return Vec::new(),
        }
    };
    let Some(Value::Array(entries)) = iterable.get_property(SIMPLEXML_ENTRIES) else {
        return Vec::new();
    };
    let names = match iterable.get_property(SIMPLEXML_ENTRY_NAMES) {
        Some(Value::Array(names)) => Some(names),
        _ => None,
    };
    entries
        .iter()
        .map(|(key, value)| {
            let visible_key = names
                .as_ref()
                .and_then(|names| names.get(&key))
                .cloned()
                .unwrap_or_else(|| match key {
                    ArrayKey::Int(key) => Value::Int(key),
                    ArrayKey::String(key) => Value::String(key),
                });
            (visible_key, value.clone())
        })
        .collect()
}

pub fn xml_reader_xml(object: &ObjectRef, xml: &str) -> Result<Value, String> {
    let document = parse_xml(xml)?;
    let events = reader_events(&document);
    object.set_property(XML_READER_EVENTS, reader_events_value(&events));
    object.set_property(XML_READER_INDEX, Value::Int(-1));
    object.set_property(XML_READER_ATTRIBUTE_INDEX, Value::Int(-1));
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
        object.set_property(XML_READER_INDEX, Value::Int(events.len() as i64));
        set_xml_reader_current(object, None);
        return Value::Bool(false);
    }
    object.set_property(XML_READER_INDEX, Value::Int(next));
    object.set_property(XML_READER_ATTRIBUTE_INDEX, Value::Int(-1));
    set_xml_reader_current(object, events.get(next as usize));
    Value::Bool(true)
}

pub fn xml_reader_next(object: &ObjectRef, name: Option<&str>) -> Value {
    let events = reader_events_from_object(object);
    let current_index = match object.get_property(XML_READER_INDEX) {
        Some(Value::Int(index)) => index,
        _ => -1,
    };
    let min_depth = events
        .get(current_index.max(0) as usize)
        .filter(|event| current_index >= 0 && event.node_type == XML_READER_ELEMENT)
        .map(|event| event.depth)
        .unwrap_or(i64::MAX);

    let mut index = current_index + 1;
    while index >= 0 && (index as usize) < events.len() {
        let event = &events[index as usize];
        if event.node_type == XML_READER_ELEMENT
            && event.depth <= min_depth
            && name.is_none_or(|name| event.name == name)
        {
            object.set_property(XML_READER_INDEX, Value::Int(index));
            object.set_property(XML_READER_ATTRIBUTE_INDEX, Value::Int(-1));
            set_xml_reader_current(object, Some(event));
            return Value::Bool(true);
        }
        index += 1;
    }

    object.set_property(XML_READER_INDEX, Value::Int(events.len() as i64));
    set_xml_reader_current(object, None);
    Value::Bool(false)
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

pub fn xml_reader_get_attribute_no(object: &ObjectRef, index: i64) -> Value {
    if index < 0 {
        return Value::Null;
    }
    let Some(Value::Int(current)) = object.get_property(XML_READER_INDEX) else {
        return Value::Null;
    };
    let events = reader_events_from_object(object);
    events
        .get(current as usize)
        .and_then(|event| event.attributes.get(index as usize))
        .map(|(_, value)| Value::string(value.as_bytes().to_vec()))
        .unwrap_or(Value::Null)
}

pub fn xml_reader_lookup_namespace(object: &ObjectRef, prefix: &str) -> Value {
    current_reader_event(object)
        .and_then(|event| {
            event
                .namespaces
                .iter()
                .find(|(candidate, _)| candidate == prefix)
                .map(|(_, uri)| Value::string(uri.as_bytes().to_vec()))
        })
        .unwrap_or(Value::Null)
}

pub fn xml_reader_move_to_attribute(object: &ObjectRef, name: &str) -> Value {
    let Some(event) = current_reader_event(object) else {
        return Value::Bool(false);
    };
    let Some(index) = event
        .attributes
        .iter()
        .position(|(attr_name, _)| attr_name == name)
    else {
        return Value::Bool(false);
    };
    set_xml_reader_attribute_current(object, &event, index)
}

pub fn xml_reader_move_to_attribute_no(object: &ObjectRef, index: i64) -> Value {
    if index < 0 {
        return Value::Bool(false);
    }
    let Some(event) = current_reader_event(object) else {
        return Value::Bool(false);
    };
    if index as usize >= event.attributes.len() {
        return Value::Bool(false);
    }
    set_xml_reader_attribute_current(object, &event, index as usize)
}

pub fn xml_reader_move_to_first_attribute(object: &ObjectRef) -> Value {
    xml_reader_move_to_attribute_no(object, 0)
}

pub fn xml_reader_move_to_next_attribute(object: &ObjectRef) -> Value {
    let next = match object.get_property(XML_READER_ATTRIBUTE_INDEX) {
        Some(Value::Int(index)) if index >= 0 => index + 1,
        _ => 0,
    };
    xml_reader_move_to_attribute_no(object, next)
}

pub fn xml_reader_move_to_element(object: &ObjectRef) -> Value {
    if !matches!(
        object.get_property(XML_READER_ATTRIBUTE_INDEX),
        Some(Value::Int(index)) if index >= 0
    ) {
        return Value::Bool(false);
    }
    let Some(event) = current_reader_event(object) else {
        return Value::Bool(false);
    };
    object.set_property(XML_READER_ATTRIBUTE_INDEX, Value::Int(-1));
    set_xml_reader_current(object, Some(&event));
    Value::Bool(true)
}

pub fn xml_reader_read_string(object: &ObjectRef) -> Value {
    if let Some((_, value)) = current_reader_attribute(object) {
        return Value::string(value.into_bytes());
    }
    current_reader_event(object)
        .map(|event| Value::string(event.string_value.into_bytes()))
        .unwrap_or_else(|| Value::string(Vec::<u8>::new()))
}

pub fn xml_reader_read_inner_xml(object: &ObjectRef) -> Value {
    current_reader_event(object)
        .map(|event| Value::string(event.inner_xml.into_bytes()))
        .unwrap_or_else(|| Value::string(Vec::<u8>::new()))
}

pub fn xml_reader_read_outer_xml(object: &ObjectRef) -> Value {
    current_reader_event(object)
        .map(|event| Value::string(event.outer_xml.into_bytes()))
        .unwrap_or_else(|| Value::string(Vec::<u8>::new()))
}

pub fn xml_reader_expand(object: &ObjectRef) -> Value {
    if matches!(
        object.get_property(XML_READER_ATTRIBUTE_INDEX),
        Some(Value::Int(index)) if index >= 0
    ) {
        return Value::Bool(false);
    }
    let Some(event) = current_reader_event(object) else {
        return Value::Bool(false);
    };
    if event.node_type != XML_READER_ELEMENT {
        return Value::Bool(false);
    }
    parse_xml(&event.outer_xml)
        .map(|document| Value::Object(new_dom_element(&document.root)))
        .unwrap_or(Value::Bool(false))
}

pub fn xml_reader_close(object: &ObjectRef) -> Value {
    object.set_property(XML_READER_EVENTS, Value::Array(PhpArray::new()));
    object.set_property(XML_READER_INDEX, Value::Int(-1));
    object.set_property(XML_READER_ATTRIBUTE_INDEX, Value::Int(-1));
    set_xml_reader_current(object, None);
    Value::Bool(true)
}

pub fn xml_writer_open_memory(object: &ObjectRef) -> Value {
    object.set_property(XML_WRITER_BUFFER, Value::string(Vec::<u8>::new()));
    object.set_property(XML_WRITER_STACK, Value::Array(PhpArray::new()));
    object.set_property(XML_WRITER_OPEN_TAG, Value::Bool(false));
    object.set_property(XML_WRITER_URI, Value::Null);
    Value::Bool(true)
}

pub fn xml_writer_open_uri(object: &ObjectRef, uri: &str) -> Value {
    object.set_property(XML_WRITER_BUFFER, Value::string(Vec::<u8>::new()));
    object.set_property(XML_WRITER_STACK, Value::Array(PhpArray::new()));
    object.set_property(XML_WRITER_OPEN_TAG, Value::Bool(false));
    object.set_property(XML_WRITER_URI, Value::string(uri.as_bytes().to_vec()));
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

pub fn xml_writer_write_comment(object: &ObjectRef, value: &str) -> Value {
    close_writer_open_tag(object);
    let mut buffer = writer_buffer(object);
    buffer.push_str("<!--");
    buffer.push_str(value);
    buffer.push_str("-->");
    object.set_property(XML_WRITER_BUFFER, Value::string(buffer.into_bytes()));
    Value::Bool(true)
}

pub fn xml_writer_write_cdata(object: &ObjectRef, value: &str) -> Value {
    close_writer_open_tag(object);
    let mut buffer = writer_buffer(object);
    buffer.push_str("<![CDATA[");
    buffer.push_str(value);
    buffer.push_str("]]>");
    object.set_property(XML_WRITER_BUFFER, Value::string(buffer.into_bytes()));
    Value::Bool(true)
}

pub fn xml_writer_write_element(object: &ObjectRef, name: &str, value: Option<&str>) -> Value {
    if !matches!(xml_writer_start_element(object, name), Value::Bool(true)) {
        return Value::Bool(false);
    }
    if let Some(value) = value
        && !matches!(xml_writer_text(object, value), Value::Bool(true))
    {
        return Value::Bool(false);
    }
    xml_writer_end_element(object)
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

pub fn xml_writer_output_memory(object: &ObjectRef, flush: bool) -> Value {
    let buffer = writer_buffer(object);
    if flush {
        object.set_property(XML_WRITER_BUFFER, Value::string(Vec::<u8>::new()));
    }
    Value::string(buffer.into_bytes())
}

pub fn xml_writer_flush_memory(object: &ObjectRef, empty: bool) -> Value {
    xml_writer_output_memory(object, empty)
}

pub fn xml_writer_uri(object: &ObjectRef) -> Option<String> {
    match object.get_property(XML_WRITER_URI) {
        Some(Value::String(value)) if !value.as_bytes().is_empty() => Some(value.to_string_lossy()),
        _ => None,
    }
}

pub fn xml_writer_pending_bytes(object: &ObjectRef) -> Vec<u8> {
    close_writer_open_tag(object);
    writer_buffer(object).into_bytes()
}

pub fn xml_writer_clear_buffer(object: &ObjectRef) {
    object.set_property(XML_WRITER_BUFFER, Value::string(Vec::<u8>::new()));
}

fn collect_text(element: &XmlElement, out: &mut String) {
    for child in &element.children {
        match child {
            XmlNode::Element(child) => collect_text(child, out),
            XmlNode::Text(text) => out.push_str(text),
            XmlNode::Cdata(text) => out.push_str(text),
            XmlNode::Comment(_) => {}
        }
    }
}

fn set_dom_element_object(object: &ObjectRef, element: &XmlElement) {
    set_dom_element_base_properties(object, element);
}

fn set_dom_document_object(object: &ObjectRef, document: &XmlDocument) {
    let document_element = Value::Object(new_dom_element(&document.root));
    object.set_property(XML_STORAGE, document_value(document));
    object.set_property("documentElement", document_element.clone());
    object.set_property("firstChild", document_element.clone());
    object.set_property("lastChild", document_element.clone());
    object.set_property(
        "childNodes",
        Value::Object(new_dom_node_list_values(vec![document_element])),
    );
    object.set_property(
        "textContent",
        Value::string(element_text(&document.root).into_bytes()),
    );
    object.set_property("nodeName", Value::string("#document"));
    object.set_property("nodeValue", Value::Null);
    object.set_property("parentNode", Value::Null);
    object.set_property("ownerDocument", Value::Null);
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

fn elements_by_tag_name_ns(
    element: &XmlElement,
    namespace_uri: &str,
    local_name: &str,
) -> Vec<XmlElement> {
    let mut out = Vec::new();
    collect_elements_by_tag_name_ns(
        element,
        namespace_uri,
        local_name,
        &BTreeMap::new(),
        &mut out,
    );
    out
}

fn collect_elements_by_tag_name_ns(
    element: &XmlElement,
    namespace_uri: &str,
    local_name: &str,
    inherited_namespaces: &BTreeMap<String, String>,
    out: &mut Vec<XmlElement>,
) {
    let namespaces = element_namespace_context(element, inherited_namespaces);
    let element_namespace = namespace_uri_for_name(&element.name, &namespaces, false);
    let element_local = local_name_for_qualified_name(&element.name);
    if (namespace_uri == "*" || element_namespace == namespace_uri)
        && (local_name == "*" || element_local == local_name)
    {
        out.push(element.clone());
    }
    for child in &element.children {
        if let XmlNode::Element(child) = child {
            collect_elements_by_tag_name_ns(child, namespace_uri, local_name, &namespaces, out);
        }
    }
}

fn local_name_for_qualified_name(name: &str) -> &str {
    name.rsplit_once(':')
        .map(|(_, local)| local)
        .unwrap_or(name)
}

fn push_reader_events(element: &XmlElement, events: &mut Vec<XmlReaderEvent>) {
    push_reader_events_with_depth(element, events, 0, &BTreeMap::new());
}

fn push_reader_events_with_depth(
    element: &XmlElement,
    events: &mut Vec<XmlReaderEvent>,
    depth: i64,
    inherited_namespaces: &BTreeMap<String, String>,
) {
    let namespaces = element_namespace_context(element, inherited_namespaces);
    let namespace_uri = namespace_uri_for_name(&element.name, &namespaces, false);
    let namespaces_vec = namespace_vec(&namespaces);
    let inner_xml = serialize_children(element);
    let outer_xml = serialize_element(element);
    let string_value = element_text(element);
    events.push(XmlReaderEvent {
        node_type: XML_READER_ELEMENT,
        name: element.name.clone(),
        value: String::new(),
        attributes: element.attributes.clone(),
        namespace_uri: namespace_uri.clone(),
        namespaces: namespaces_vec.clone(),
        depth,
        inner_xml,
        outer_xml,
        string_value,
    });
    for child in &element.children {
        match child {
            XmlNode::Element(child) => {
                push_reader_events_with_depth(child, events, depth + 1, &namespaces)
            }
            XmlNode::Text(text) if !text.is_empty() => events.push(XmlReaderEvent {
                node_type: XML_READER_TEXT,
                name: "#text".to_owned(),
                value: text.clone(),
                attributes: Vec::new(),
                namespace_uri: String::new(),
                namespaces: namespaces_vec.clone(),
                depth: depth + 1,
                inner_xml: String::new(),
                outer_xml: escape_text(text, false),
                string_value: text.clone(),
            }),
            XmlNode::Text(_) | XmlNode::Comment(_) | XmlNode::Cdata(_) => {}
        }
    }
    events.push(XmlReaderEvent {
        node_type: XML_READER_END_ELEMENT,
        name: element.name.clone(),
        value: String::new(),
        attributes: element.attributes.clone(),
        namespace_uri,
        namespaces: namespaces_vec,
        depth,
        inner_xml: String::new(),
        outer_xml: String::new(),
        string_value: String::new(),
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

fn first_simplexml_entry(object: &ObjectRef) -> Option<ObjectRef> {
    let Some(Value::Array(entries)) = object.get_property(SIMPLEXML_ENTRIES) else {
        return None;
    };
    entries.iter().find_map(|(_, value)| match value {
        Value::Object(object) => Some(object.clone()),
        _ => None,
    })
}

fn element_from_object(object: &ObjectRef) -> Option<XmlElement> {
    document_from_object(object).map(|document| document.root)
}

fn text_from_object(object: &ObjectRef) -> Option<String> {
    if !matches!(
        normalize_class_name(&object.class_name()).as_str(),
        "domtext" | "domcomment" | "domcdatasection"
    ) {
        return None;
    }
    object
        .get_property(XML_TEXT_STORAGE)
        .and_then(|value| match value {
            Value::String(text) => Some(text.to_string_lossy()),
            _ => None,
        })
}

fn node_kind_from_object(object: &ObjectRef) -> Option<String> {
    object
        .get_property(XML_NODE_KIND)
        .and_then(|value| match value {
            Value::String(kind) => Some(kind.to_string_lossy()),
            _ => None,
        })
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
            XmlNode::Comment(text) => {
                let mut comment_node = PhpArray::new();
                comment_node.insert(
                    ArrayKey::String(PhpString::from("comment")),
                    Value::string(text.as_bytes().to_vec()),
                );
                Value::Array(comment_node)
            }
            XmlNode::Cdata(text) => {
                let mut cdata_node = PhpArray::new();
                cdata_node.insert(
                    ArrayKey::String(PhpString::from("cdata")),
                    Value::string(text.as_bytes().to_vec()),
                );
                Value::Array(cdata_node)
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
            } else if let Some(Value::String(text)) = as_comment_node(child) {
                children.push(XmlNode::Comment(text.to_string_lossy()));
            } else if let Some(Value::String(text)) = as_cdata_node(child) {
                children.push(XmlNode::Cdata(text.to_string_lossy()));
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

fn as_comment_node(value: &Value) -> Option<&Value> {
    let Value::Array(array) = value else {
        return None;
    };
    array.get(&ArrayKey::String(PhpString::from("comment")))
}

fn as_cdata_node(value: &Value) -> Option<&Value> {
    let Value::Array(array) = value else {
        return None;
    };
    array.get(&ArrayKey::String(PhpString::from("cdata")))
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
        entry.insert(
            ArrayKey::String(PhpString::from("namespace_uri")),
            Value::string(event.namespace_uri.as_bytes().to_vec()),
        );
        entry.insert(
            ArrayKey::String(PhpString::from("depth")),
            Value::Int(event.depth),
        );
        entry.insert(
            ArrayKey::String(PhpString::from("inner_xml")),
            Value::string(event.inner_xml.as_bytes().to_vec()),
        );
        entry.insert(
            ArrayKey::String(PhpString::from("outer_xml")),
            Value::string(event.outer_xml.as_bytes().to_vec()),
        );
        entry.insert(
            ArrayKey::String(PhpString::from("string_value")),
            Value::string(event.string_value.as_bytes().to_vec()),
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
        let mut namespaces = PhpArray::new();
        for (prefix, uri) in &event.namespaces {
            namespaces.insert(
                ArrayKey::String(PhpString::from(prefix.as_str())),
                Value::string(uri.as_bytes().to_vec()),
            );
        }
        entry.insert(
            ArrayKey::String(PhpString::from("namespaces")),
            Value::Array(namespaces),
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
            let namespace_uri = match entry.get(&ArrayKey::String(PhpString::from("namespace_uri")))
            {
                Some(Value::String(value)) => value.to_string_lossy(),
                _ => String::new(),
            };
            let depth = match entry.get(&ArrayKey::String(PhpString::from("depth"))) {
                Some(Value::Int(value)) => *value,
                _ => 0,
            };
            let inner_xml = match entry.get(&ArrayKey::String(PhpString::from("inner_xml"))) {
                Some(Value::String(value)) => value.to_string_lossy(),
                _ => String::new(),
            };
            let outer_xml = match entry.get(&ArrayKey::String(PhpString::from("outer_xml"))) {
                Some(Value::String(value)) => value.to_string_lossy(),
                _ => String::new(),
            };
            let string_value = match entry.get(&ArrayKey::String(PhpString::from("string_value"))) {
                Some(Value::String(value)) => value.to_string_lossy(),
                _ => value.clone(),
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
            let mut namespaces = Vec::new();
            if let Some(Value::Array(namespace_array)) =
                entry.get(&ArrayKey::String(PhpString::from("namespaces")))
            {
                for (key, value) in namespace_array.iter() {
                    if let (ArrayKey::String(key), Value::String(value)) = (key, value) {
                        namespaces.push((key.to_string_lossy(), value.to_string_lossy()));
                    }
                }
            }
            Some(XmlReaderEvent {
                node_type,
                name,
                value,
                attributes,
                namespace_uri,
                namespaces,
                depth,
                inner_xml,
                outer_xml,
                string_value,
            })
        })
        .collect()
}

fn current_reader_event(object: &ObjectRef) -> Option<XmlReaderEvent> {
    let Some(Value::Int(index)) = object.get_property(XML_READER_INDEX) else {
        return None;
    };
    if index < 0 {
        return None;
    }
    reader_events_from_object(object)
        .get(index as usize)
        .cloned()
}

fn current_reader_attribute(object: &ObjectRef) -> Option<(String, String)> {
    let Some(Value::Int(index)) = object.get_property(XML_READER_ATTRIBUTE_INDEX) else {
        return None;
    };
    if index < 0 {
        return None;
    }
    current_reader_event(object).and_then(|event| event.attributes.get(index as usize).cloned())
}

fn set_xml_reader_current(object: &ObjectRef, event: Option<&XmlReaderEvent>) {
    if let Some(event) = event {
        object.set_property(XML_READER_ATTRIBUTE_INDEX, Value::Int(-1));
        object.set_property("nodeType", Value::Int(event.node_type));
        object.set_property("name", Value::string(event.name.as_bytes().to_vec()));
        object.set_property("value", Value::string(event.value.as_bytes().to_vec()));
        object.set_property(
            "namespaceURI",
            Value::string(event.namespace_uri.as_bytes().to_vec()),
        );
        object.set_property("depth", Value::Int(event.depth));
        object.set_property("attributeCount", Value::Int(event.attributes.len() as i64));
        object.set_property("hasAttributes", Value::Bool(!event.attributes.is_empty()));
        object.set_property("hasValue", Value::Bool(!event.value.is_empty()));
        let (prefix, local_name) = split_xml_name(&event.name);
        object.set_property("localName", Value::string(local_name.as_bytes().to_vec()));
        object.set_property("prefix", Value::string(prefix.as_bytes().to_vec()));
    } else {
        object.set_property("nodeType", Value::Int(XML_READER_NONE));
        object.set_property("name", Value::string(Vec::<u8>::new()));
        object.set_property("value", Value::string(Vec::<u8>::new()));
        object.set_property("namespaceURI", Value::string(Vec::<u8>::new()));
        object.set_property("depth", Value::Int(0));
        object.set_property("attributeCount", Value::Int(0));
        object.set_property("hasAttributes", Value::Bool(false));
        object.set_property("hasValue", Value::Bool(false));
        object.set_property("localName", Value::string(Vec::<u8>::new()));
        object.set_property("prefix", Value::string(Vec::<u8>::new()));
    }
}

fn set_xml_reader_attribute_current(
    object: &ObjectRef,
    event: &XmlReaderEvent,
    index: usize,
) -> Value {
    let Some((name, value)) = event.attributes.get(index) else {
        return Value::Bool(false);
    };
    let (prefix, local_name) = split_xml_name(name);
    let namespace_uri = namespace_uri_for_attribute(name, &event.namespaces);
    object.set_property(XML_READER_ATTRIBUTE_INDEX, Value::Int(index as i64));
    object.set_property("nodeType", Value::Int(XML_READER_ATTRIBUTE));
    object.set_property("name", Value::string(name.as_bytes().to_vec()));
    object.set_property("localName", Value::string(local_name.as_bytes().to_vec()));
    object.set_property("prefix", Value::string(prefix.as_bytes().to_vec()));
    object.set_property(
        "namespaceURI",
        Value::string(namespace_uri.as_bytes().to_vec()),
    );
    object.set_property("value", Value::string(value.as_bytes().to_vec()));
    object.set_property("depth", Value::Int(event.depth + 1));
    object.set_property("attributeCount", Value::Int(0));
    object.set_property("hasAttributes", Value::Bool(false));
    object.set_property("hasValue", Value::Bool(true));
    Value::Bool(true)
}

fn split_xml_name(name: &str) -> (String, String) {
    if let Some((prefix, local)) = name.split_once(':') {
        (prefix.to_string(), local.to_string())
    } else {
        (String::new(), name.to_string())
    }
}

fn element_namespace_context(
    element: &XmlElement,
    inherited: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut namespaces = inherited.clone();
    for (name, value) in &element.attributes {
        if name == "xmlns" {
            namespaces.insert(String::new(), value.clone());
        } else if let Some(prefix) = name.strip_prefix("xmlns:") {
            namespaces.insert(prefix.to_owned(), value.clone());
        }
    }
    namespaces
}

fn namespace_vec(namespaces: &BTreeMap<String, String>) -> Vec<(String, String)> {
    namespaces
        .iter()
        .map(|(prefix, uri)| (prefix.clone(), uri.clone()))
        .collect()
}

fn namespace_uri_for_name(
    name: &str,
    namespaces: &BTreeMap<String, String>,
    is_attribute: bool,
) -> String {
    let (prefix, _) = split_xml_name(name);
    if prefix.is_empty() {
        if is_attribute {
            String::new()
        } else {
            namespaces.get("").cloned().unwrap_or_default()
        }
    } else {
        namespaces.get(&prefix).cloned().unwrap_or_default()
    }
}

fn namespace_uri_for_attribute(name: &str, namespaces: &[(String, String)]) -> String {
    if name == "xmlns" || name.starts_with("xmlns:") {
        return "http://www.w3.org/2000/xmlns/".to_owned();
    }
    let (prefix, _) = split_xml_name(name);
    if prefix.is_empty() {
        return String::new();
    }
    namespaces
        .iter()
        .find(|(candidate, _)| candidate == &prefix)
        .map(|(_, uri)| uri.clone())
        .unwrap_or_default()
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
        assert!(error.contains("E_PHP_RUNTIME_XML_PARSE_ERROR"));
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

    #[test]
    fn simplexml_iteration_preserves_duplicate_child_names() {
        let Value::Object(root) = simplexml_load_string("<root><a>A</a><b>B</b><a>C</a></root>")
            .expect("valid SimpleXML")
        else {
            panic!("SimpleXML root must be an object");
        };
        let entries = simplexml_iteration_entries(&root);
        let names = entries
            .iter()
            .map(|(name, _)| match name {
                Value::String(name) => name.to_string_lossy(),
                other => panic!("unexpected SimpleXML iteration key: {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(names, ["a", "b", "a"]);
    }
}
