//! Safe libxml2-backed XML parsing helpers for the runtime XML facade.

#![allow(unsafe_code)]

use crate::xml::{XmlDocument, XmlElement, XmlNode};
use libxml::bindings::{xmlAttrPtr, xmlGetLastError, xmlNodeGetContent, xmlResetLastError};
use libxml::error::StructuredError;
use libxml::parser::{Parser, ParserOptions};
#[cfg(test)]
use libxml::tree::SaveOptions;
use libxml::tree::{Document, Node};
use libxml::xpath::Context;
use std::ffi::CStr;
use std::os::raw::{c_char, c_void};

/// A parsed libxml2 document with collected backend diagnostics.
pub struct BackendDocument {
    document: Document,
}

impl BackendDocument {
    /// Serialize the libxml2 document as XML without an XML declaration.
    #[cfg(test)]
    pub fn serialize(&self) -> String {
        self.document.to_string_with_options(SaveOptions {
            no_declaration: true,
            ..SaveOptions::default()
        })
    }

    /// Project the libxml2 document into the existing runtime XML tree.
    pub fn to_runtime_document(&self) -> Result<XmlDocument, String> {
        let root = self
            .document
            .get_root_element()
            .ok_or_else(|| "E_PHP_RUNTIME_XML_PARSE_ERROR: missing document element".to_owned())?;
        Ok(XmlDocument {
            root: node_to_element(&root)?,
        })
    }

    /// Evaluate an XPath expression and return matching node string values.
    pub fn xpath_string_values(&self, expression: &str) -> Result<Vec<String>, String> {
        let context = Context::new(&self.document)
            .map_err(|_| "E_PHP_RUNTIME_XML_XPATH_ERROR: failed to create context".to_owned())?;
        register_document_namespaces(&self.document, &context);
        context
            .evaluate_checked(expression)
            .map(|object| object.get_nodes_as_str())
            .map_err(|error| format!("E_PHP_RUNTIME_XML_XPATH_ERROR: {error}"))
    }

    /// Evaluate an XPath expression and return matching element nodes projected
    /// into the runtime XML tree.
    pub fn xpath_elements(&self, expression: &str) -> Result<Vec<XmlElement>, String> {
        let context = Context::new(&self.document)
            .map_err(|_| "E_PHP_RUNTIME_XML_XPATH_ERROR: failed to create context".to_owned())?;
        register_document_namespaces(&self.document, &context);
        let object = context
            .evaluate_checked(expression)
            .map_err(|error| format!("E_PHP_RUNTIME_XML_XPATH_ERROR: {error}"))?;
        let mut elements = Vec::new();
        for node in object.get_nodes_as_vec() {
            if matches!(
                node.get_type(),
                Some(libxml::tree::nodetype::NodeType::ElementNode)
            ) {
                elements.push(node_to_element(&node)?);
            }
        }
        Ok(elements)
    }

    /// Evaluate an XPath expression and return libxml2's scalar string cast.
    pub fn xpath_string(&self, expression: &str) -> Result<String, String> {
        let context = Context::new(&self.document)
            .map_err(|_| "E_PHP_RUNTIME_XML_XPATH_ERROR: failed to create context".to_owned())?;
        register_document_namespaces(&self.document, &context);
        context
            .evaluate_checked(expression)
            .map(|object| object.to_string())
            .map_err(|error| format!("E_PHP_RUNTIME_XML_XPATH_ERROR: {error}"))
    }
}

fn register_document_namespaces(document: &Document, context: &Context) {
    if let Some(root) = document.get_root_element() {
        register_node_namespaces(&root, context);
    }
}

fn register_node_namespaces(node: &Node, context: &Context) {
    for namespace in node.get_namespace_declarations() {
        let prefix = namespace.get_prefix();
        if !prefix.is_empty() {
            let _ = context.register_namespace(&prefix, &namespace.get_href());
        }
    }
    for child in node.get_child_nodes() {
        if matches!(
            child.get_type(),
            Some(libxml::tree::nodetype::NodeType::ElementNode)
        ) {
            register_node_namespaces(&child, context);
        }
    }
}

/// Parse a string through libxml2 and return the current runtime XML tree.
pub fn parse_document(input: &str) -> Result<XmlDocument, String> {
    parse_string(input)?.to_runtime_document()
}

/// Parse a string through libxml2 with external network access disabled.
pub fn parse_string(input: &str) -> Result<BackendDocument, String> {
    reset_last_error();
    Parser::default()
        .parse_string_with_options(input.as_bytes(), secure_parse_options())
        .map(|document| BackendDocument { document })
        .map_err(|error| format_parse_error(&error.to_string()))
}

fn secure_parse_options() -> ParserOptions<'static> {
    ParserOptions {
        recover: false,
        no_error: true,
        no_warning: true,
        no_net: true,
        ..ParserOptions::default()
    }
}

fn reset_last_error() {
    unsafe { xmlResetLastError() };
}

fn format_parse_error(fallback: &str) -> String {
    match last_error_message() {
        Some(message) if !message.trim().is_empty() => {
            format!("E_PHP_RUNTIME_XML_PARSE_ERROR: {}", message.trim())
        }
        _ => format!("E_PHP_RUNTIME_XML_PARSE_ERROR: {fallback}"),
    }
}

fn last_error_message() -> Option<String> {
    let error = unsafe { xmlGetLastError() };
    if error.is_null() {
        None
    } else {
        unsafe { StructuredError::from_raw(error) }.message
    }
}

fn node_to_element(node: &Node) -> Result<XmlElement, String> {
    let name = qualified_node_name(node);
    if name.is_empty() {
        return Err("E_PHP_RUNTIME_XML_PARSE_ERROR: element has no name".to_owned());
    }

    let mut attributes = namespace_declarations(node);
    attributes.extend(node_attributes(node));

    let mut children = Vec::new();
    for child in node.get_child_nodes() {
        match child.get_type() {
            Some(libxml::tree::nodetype::NodeType::ElementNode) => {
                children.push(XmlNode::Element(node_to_element(&child)?));
            }
            Some(libxml::tree::nodetype::NodeType::TextNode) => {
                let text = child.get_content();
                if !text.is_empty() {
                    children.push(XmlNode::Text(text));
                }
            }
            Some(libxml::tree::nodetype::NodeType::CDataSectionNode) => {
                children.push(XmlNode::Cdata(child.get_content()));
            }
            Some(libxml::tree::nodetype::NodeType::CommentNode) => {
                children.push(XmlNode::Comment(child.get_content()));
            }
            Some(libxml::tree::nodetype::NodeType::EntityRefNode) => {
                return Err(format!(
                    "E_PHP_RUNTIME_XML_UNRESOLVED_ENTITY: {}",
                    child.get_name()
                ));
            }
            _ => {}
        }
    }

    Ok(XmlElement {
        name,
        attributes,
        children,
    })
}

fn qualified_node_name(node: &Node) -> String {
    let local = node.get_name();
    match node.get_namespace() {
        Some(namespace) => {
            let prefix = namespace.get_prefix();
            if prefix.is_empty() {
                local
            } else {
                format!("{prefix}:{local}")
            }
        }
        None => local,
    }
}

fn namespace_declarations(node: &Node) -> Vec<(String, String)> {
    node.get_namespace_declarations()
        .into_iter()
        .map(|namespace| {
            let prefix = namespace.get_prefix();
            let name = if prefix.is_empty() {
                "xmlns".to_owned()
            } else {
                format!("xmlns:{prefix}")
            };
            (name, namespace.get_href())
        })
        .collect()
}

fn node_attributes(node: &Node) -> Vec<(String, String)> {
    let mut attributes = Vec::new();
    let mut current = first_property(node);
    while !current.is_null() {
        if let Some(name) = attr_name(current) {
            attributes.push((name, attr_value(current)));
        }
        current = next_property(current);
    }
    attributes
}

fn first_property(node: &Node) -> xmlAttrPtr {
    let node_ptr = node.node_ptr();
    if node_ptr.is_null() {
        return std::ptr::null_mut();
    }
    unsafe { (*node_ptr).properties }
}

fn next_property(attr: xmlAttrPtr) -> xmlAttrPtr {
    if attr.is_null() {
        return std::ptr::null_mut();
    }
    unsafe { (*attr).next }
}

fn attr_name(attr: xmlAttrPtr) -> Option<String> {
    if attr.is_null() {
        return None;
    }

    let name = unsafe { xml_char_to_string((*attr).name)? };
    let namespace = unsafe { (*attr).ns };
    if namespace.is_null() {
        return Some(name);
    }

    match unsafe { xml_char_to_string((*namespace).prefix) } {
        Some(prefix) if !prefix.is_empty() => Some(format!("{prefix}:{name}")),
        _ => Some(name),
    }
}

fn attr_value(attr: xmlAttrPtr) -> String {
    let content_ptr = unsafe { xmlNodeGetContent(attr as libxml::bindings::xmlNodePtr) };
    if content_ptr.is_null() {
        return String::new();
    }
    let value = unsafe { CStr::from_ptr(content_ptr as *const c_char) }
        .to_string_lossy()
        .into_owned();
    free_xml_buffer(content_ptr as *mut c_void);
    value
}

unsafe fn xml_char_to_string(ptr: *const libxml::bindings::xmlChar) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    Some(
        unsafe { CStr::from_ptr(ptr as *const c_char) }
            .to_string_lossy()
            .into_owned(),
    )
}

fn free_xml_buffer(ptr: *mut c_void) {
    let xml_free = unsafe { libxml::bindings::xmlFree };
    unsafe {
        if let Some(xml_free) = xml_free {
            xml_free(ptr);
        } else {
            libc::free(ptr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comments_cdata_namespaces_and_attributes() {
        let document = parse_document(
            r#"<?xml version="1.0"?><root xmlns:h="urn:h" id="7"><!--c--><h:item a="b"><![CDATA[A & B]]></h:item></root>"#,
        )
        .expect("libxml2 parse");

        assert_eq!(document.root.name, "root");
        assert!(
            document
                .root
                .attributes
                .contains(&("id".to_owned(), "7".to_owned()))
        );
        assert!(
            document
                .root
                .attributes
                .contains(&("xmlns:h".to_owned(), "urn:h".to_owned()))
        );
        assert!(matches!(document.root.children[0], XmlNode::Comment(_)));
        let XmlNode::Element(child) = &document.root.children[1] else {
            panic!("expected namespaced child element");
        };
        assert_eq!(child.name, "h:item");
        assert_eq!(child.children, vec![XmlNode::Cdata("A & B".to_owned())]);
    }

    #[test]
    fn preserves_attribute_order() {
        let document = parse_document(r#"<root id="7" code="x" z:a="b" xmlns:z="urn:z"/>"#)
            .expect("libxml2 parse");

        assert_eq!(
            document.root.attributes,
            vec![
                ("xmlns:z".to_owned(), "urn:z".to_owned()),
                ("id".to_owned(), "7".to_owned()),
                ("code".to_owned(), "x".to_owned()),
                ("z:a".to_owned(), "b".to_owned()),
            ]
        );
    }

    #[test]
    fn rejects_malformed_xml() {
        let error = parse_document("<root><child></root>").expect_err("must reject malformed XML");
        assert!(error.contains("E_PHP_RUNTIME_XML_PARSE_ERROR"));
    }

    #[test]
    fn rejects_unresolved_entities_without_external_loading() {
        let error =
            parse_document("<!DOCTYPE root SYSTEM \"http://example.invalid/x\"><root>&xxe;</root>")
                .expect_err("unresolved entity must fail closed");
        assert!(
            error.contains("E_PHP_RUNTIME_XML_PARSE_ERROR")
                || error.contains("E_PHP_RUNTIME_XML_UNRESOLVED_ENTITY")
        );
    }

    #[test]
    fn serializes_and_evaluates_xpath() {
        let backend =
            parse_string("<root><item>A</item><item>B</item></root>").expect("backend document");
        assert_eq!(
            backend.serialize(),
            "<root><item>A</item><item>B</item></root>\n"
        );
        assert_eq!(
            backend.xpath_string_values("//item").expect("xpath values"),
            vec!["A".to_owned(), "B".to_owned()]
        );
    }
}
