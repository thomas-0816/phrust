//! Bounded SOAP platform facade.

use super::core::{arity_error, conversion_error};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult, CurlEasyCollector,
    RuntimeSourceSpan,
};
use crate::convert::to_bool;
use crate::xml::{XmlElement, XmlNode};
use crate::{ArrayKey, PhpArray, PhpString, Value};
use curl::easy::{Easy2, List};
use std::collections::BTreeSet;
use std::fs;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "is_soap_fault",
        builtin_is_soap_fault,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "use_soap_error_handler",
        builtin_use_soap_error_handler,
        BuiltinCompatibility::Php,
    ),
];

fn builtin_is_soap_fault(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("is_soap_fault", "one argument"));
    }

    let is_fault = match &args[0] {
        Value::Object(object) => is_soap_fault_class(&object.class_name()),
        _ => false,
    };
    Ok(Value::Bool(is_fault))
}

fn builtin_use_soap_error_handler(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error(
            "use_soap_error_handler",
            "zero or one argument",
        ));
    }

    let enabled = match args.first() {
        Some(value) => {
            to_bool(value).map_err(|message| conversion_error("use_soap_error_handler", message))?
        }
        None => true,
    };
    let previous = context.soap_state().set_error_handler_enabled(enabled);
    Ok(Value::Bool(previous))
}

fn is_soap_fault_class(class_name: &str) -> bool {
    let class_name = class_name.trim_start_matches('\\');
    class_name.eq_ignore_ascii_case("SoapFault")
        || class_name.eq_ignore_ascii_case("Soap\\SoapFault")
}

/// HTTP response captured by the SOAP transport helper.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SoapHttpResponse {
    pub status: u32,
    pub headers: String,
    pub body: String,
}

/// Common WSDL metadata used by the bounded SOAP client.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SoapWsdlInfo {
    pub target_namespace: Option<String>,
    pub location: Option<String>,
    pub operations: Vec<String>,
}

/// Parsed SOAP body result for simple client calls.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SoapParsedBody {
    Return(Value),
    Fault {
        code: String,
        string: String,
        detail: Option<String>,
    },
}

/// Builds a SOAP 1.1 envelope for non-WSDL and bounded WSDL calls.
pub fn build_soap_envelope(method: &str, namespace: &str, args: &[(String, Value)]) -> String {
    let mut body = String::new();
    body.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    body.push_str(
        r#"<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/"><SOAP-ENV:Body>"#,
    );
    body.push_str("<ns1:");
    body.push_str(&escape_xml_name(method));
    body.push_str(" xmlns:ns1=\"");
    body.push_str(&escape_xml_text(namespace));
    body.push_str("\">");
    for (name, value) in args {
        body.push_str(&soap_value_element(name, value));
    }
    body.push_str("</ns1:");
    body.push_str(&escape_xml_name(method));
    body.push_str("></SOAP-ENV:Body></SOAP-ENV:Envelope>");
    body
}

/// Parses a SOAP response body through the libxml-backed XML backend.
pub fn parse_soap_response(xml: &str) -> Result<SoapParsedBody, String> {
    let document = crate::xml_backend::parse_document(xml)?;
    let Some(body) = find_descendant_by_local_name(&document.root, "Body") else {
        return Err("E_PHP_RUNTIME_SOAP_PARSE_ERROR: missing SOAP Body".to_owned());
    };
    let Some(payload) = body.children.iter().find_map(|node| match node {
        XmlNode::Element(element) => Some(element),
        _ => None,
    }) else {
        return Ok(SoapParsedBody::Return(Value::Null));
    };
    if local_name(&payload.name).eq_ignore_ascii_case("Fault") {
        return Ok(SoapParsedBody::Fault {
            code: child_text(payload, "faultcode").unwrap_or_else(|| "Server".to_owned()),
            string: child_text(payload, "faultstring").unwrap_or_default(),
            detail: child_text(payload, "detail"),
        });
    }

    let result = payload
        .children
        .iter()
        .find_map(|node| match node {
            XmlNode::Element(element) if local_name(&element.name).ends_with("Result") => {
                Some(soap_element_value(element))
            }
            XmlNode::Element(element)
                if local_name(&element.name).eq_ignore_ascii_case("return") =>
            {
                Some(soap_element_value(element))
            }
            _ => None,
        })
        .unwrap_or_else(|| soap_element_value(payload));
    Ok(SoapParsedBody::Return(result))
}

/// Parses common WSDL metadata through the libxml-backed XML backend.
pub fn parse_wsdl(xml: &str) -> Result<SoapWsdlInfo, String> {
    let backend = crate::xml_backend::parse_string(xml)?;
    let target_namespace = empty_to_none(
        backend.xpath_string("string(/*[local-name()='definitions']/@targetNamespace)")?,
    );
    let location = empty_to_none(backend.xpath_string(
        "string((//*[local-name()='service']//*[local-name()='address']/@location)[1])",
    )?);
    let mut operation_names = BTreeSet::new();
    for expression in [
        "//*[local-name()='binding']/*[local-name()='operation']/@name",
        "//*[local-name()='portType']/*[local-name()='operation']/@name",
    ] {
        for name in backend.xpath_string_values(expression)? {
            let name = name.trim();
            if !name.is_empty() {
                operation_names.insert(name.to_owned());
            }
        }
    }
    Ok(SoapWsdlInfo {
        target_namespace,
        location,
        operations: operation_names.into_iter().collect(),
    })
}

/// Loads a WSDL document from a local path, file URL, or HTTP(S) URL.
pub fn load_wsdl(uri: &str) -> Result<String, String> {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        return soap_http_get(uri).map(|response| response.body);
    }
    let path = uri.strip_prefix("file://").unwrap_or(uri);
    fs::read_to_string(path).map_err(|error| {
        format!("E_PHP_RUNTIME_SOAP_WSDL_ERROR: failed to read WSDL {path}: {error}")
    })
}

/// Posts a SOAP envelope through libcurl. External hosts require PHRUST_NET_TESTS=1.
pub fn soap_http_post(
    url: &str,
    body: &str,
    soap_action: Option<&str>,
) -> Result<SoapHttpResponse, String> {
    soap_http_request("POST", url, Some(body), soap_action)
}

fn soap_http_get(url: &str) -> Result<SoapHttpResponse, String> {
    soap_http_request("GET", url, None, None)
}

fn soap_http_request(
    method: &str,
    url: &str,
    body: Option<&str>,
    soap_action: Option<&str>,
) -> Result<SoapHttpResponse, String> {
    if !soap_url_allowed(url) {
        return Err(
            "E_PHP_RUNTIME_SOAP_HTTP_DISABLED: remote SOAP HTTP requests require PHRUST_NET_TESTS=1"
                .to_owned(),
        );
    }

    let mut easy = Easy2::new(CurlEasyCollector::default());
    easy.url(url)
        .map_err(|error| format!("E_PHP_RUNTIME_SOAP_HTTP_ERROR: {error}"))?;
    easy.connect_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| format!("E_PHP_RUNTIME_SOAP_HTTP_ERROR: {error}"))?;
    easy.timeout(std::time::Duration::from_secs(30))
        .map_err(|error| format!("E_PHP_RUNTIME_SOAP_HTTP_ERROR: {error}"))?;
    let mut headers = List::new();
    if method == "POST" {
        easy.post(true)
            .map_err(|error| format!("E_PHP_RUNTIME_SOAP_HTTP_ERROR: {error}"))?;
        easy.post_fields_copy(body.unwrap_or_default().as_bytes())
            .map_err(|error| format!("E_PHP_RUNTIME_SOAP_HTTP_ERROR: {error}"))?;
        headers
            .append("Content-Type: text/xml; charset=utf-8")
            .map_err(|error| format!("E_PHP_RUNTIME_SOAP_HTTP_ERROR: {error}"))?;
        if let Some(action) = soap_action {
            headers
                .append(&format!("SOAPAction: \"{}\"", action.replace('"', "\\\"")))
                .map_err(|error| format!("E_PHP_RUNTIME_SOAP_HTTP_ERROR: {error}"))?;
        }
    }
    easy.http_headers(headers)
        .map_err(|error| format!("E_PHP_RUNTIME_SOAP_HTTP_ERROR: {error}"))?;
    easy.perform()
        .map_err(|error| format!("E_PHP_RUNTIME_SOAP_HTTP_ERROR: {error}"))?;
    let status = easy
        .response_code()
        .map_err(|error| format!("E_PHP_RUNTIME_SOAP_HTTP_ERROR: {error}"))?;
    let collector = easy.get_ref();
    Ok(SoapHttpResponse {
        status,
        headers: String::from_utf8_lossy(&collector.headers).into_owned(),
        body: String::from_utf8_lossy(&collector.body).into_owned(),
    })
}

fn soap_url_allowed(url: &str) -> bool {
    if std::env::var("PHRUST_NET_TESTS").ok().as_deref() == Some("1") {
        return true;
    }
    matches!(
        http_url_host(url).as_deref(),
        Some("127.0.0.1" | "localhost" | "::1")
    )
}

fn http_url_host(url: &str) -> Option<String> {
    let without_scheme = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;
    let authority = without_scheme.split('/').next().unwrap_or(without_scheme);
    let host = authority
        .strip_prefix('[')
        .and_then(|rest| rest.split(']').next())
        .unwrap_or_else(|| authority.split(':').next().unwrap_or(authority));
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

fn soap_value_element(name: &str, value: &Value) -> String {
    let name = escape_xml_name(name);
    match value {
        Value::Array(array) => {
            let mut out = String::new();
            out.push('<');
            out.push_str(&name);
            out.push('>');
            for (key, value) in array.iter() {
                out.push_str(&soap_value_element(&array_key_name(&key), value));
            }
            out.push_str("</");
            out.push_str(&name);
            out.push('>');
            out
        }
        Value::Bool(value) => format!("<{name}>{}</{name}>", if *value { "true" } else { "false" }),
        Value::Int(value) => format!("<{name}>{value}</{name}>"),
        Value::Float(value) => format!("<{name}>{}</{name}>", value.to_f64()),
        Value::Null => format!("<{name}/>"),
        Value::String(value) => format!(
            "<{name}>{}</{name}>",
            escape_xml_text(&value.to_string_lossy())
        ),
        Value::Reference(cell) => soap_value_element(&name, &cell.borrow()),
        _ => format!(
            "<{name}>{}</{name}>",
            escape_xml_text(&format!("{value:?}"))
        ),
    }
}

fn array_key_name(key: &ArrayKey) -> String {
    match key {
        ArrayKey::Int(index) => format!("param{index}"),
        ArrayKey::String(name) => name.to_string_lossy(),
    }
}

fn soap_element_value(element: &XmlElement) -> Value {
    let child_elements = element
        .children
        .iter()
        .filter_map(|node| match node {
            XmlNode::Element(child) => Some(child),
            _ => None,
        })
        .collect::<Vec<_>>();
    if child_elements.is_empty() {
        return Value::String(PhpString::from(element_text(element).as_str()));
    }
    let mut array = PhpArray::new();
    for child in child_elements {
        array.insert(
            ArrayKey::String(PhpString::from(local_name(&child.name))),
            soap_element_value(child),
        );
    }
    Value::Array(array)
}

fn find_descendant_by_local_name<'a>(
    element: &'a XmlElement,
    name: &str,
) -> Option<&'a XmlElement> {
    if local_name(&element.name).eq_ignore_ascii_case(name) {
        return Some(element);
    }
    element.children.iter().find_map(|node| match node {
        XmlNode::Element(child) => find_descendant_by_local_name(child, name),
        _ => None,
    })
}

fn child_text(element: &XmlElement, name: &str) -> Option<String> {
    element.children.iter().find_map(|node| match node {
        XmlNode::Element(child) if local_name(&child.name).eq_ignore_ascii_case(name) => {
            Some(element_text(child))
        }
        _ => None,
    })
}

fn element_text(element: &XmlElement) -> String {
    let mut out = String::new();
    for child in &element.children {
        match child {
            XmlNode::Text(text) | XmlNode::Cdata(text) => out.push_str(text),
            XmlNode::Element(child) => out.push_str(&element_text(child)),
            XmlNode::Comment(_) => {}
        }
    }
    out
}

fn local_name(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn empty_to_none(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn escape_xml_text(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn escape_xml_name(input: &str) -> String {
    let mut out = String::new();
    for (index, ch) in input.chars().enumerate() {
        let allowed = ch == '_' || ch == '-' || ch == '.' || ch.is_ascii_alphanumeric();
        if !allowed || (index == 0 && (ch == '-' || ch == '.' || ch.is_ascii_digit())) {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    if out.is_empty() {
        "method".to_owned()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soap_response_parses_return_value_with_xml_backend() {
        let parsed = parse_soap_response(
            r#"<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/"><SOAP-ENV:Body><m:EchoResponse xmlns:m="urn:test"><return>ok</return></m:EchoResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>"#,
        )
        .unwrap();

        assert_eq!(parsed, SoapParsedBody::Return(Value::string("ok")));
    }

    #[test]
    fn wsdl_parser_extracts_common_metadata() {
        let wsdl = r#"<definitions targetNamespace="urn:test" xmlns="http://schemas.xmlsoap.org/wsdl/" xmlns:soap="http://schemas.xmlsoap.org/wsdl/soap/"><portType name="DemoPort"><operation name="echo"/></portType><binding name="DemoBinding" type="DemoPort"><operation name="echo"><soap:operation soapAction="urn:test#echo"/></operation></binding><service name="Demo"><port name="DemoPort" binding="DemoBinding"><soap:address location="http://127.0.0.1:18081/soap"/></port></service></definitions>"#;

        let info = parse_wsdl(wsdl).unwrap();

        assert_eq!(info.target_namespace.as_deref(), Some("urn:test"));
        assert_eq!(
            info.location.as_deref(),
            Some("http://127.0.0.1:18081/soap")
        );
        assert_eq!(info.operations, vec!["echo"]);
    }
}
