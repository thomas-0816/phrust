use super::*;
use md5::{Digest, Md5};
use sha1::Sha1;
use sha2::{Sha224, Sha256, Sha384, Sha512, Sha512_224, Sha512_256};

pub(in crate::builtins::modules) fn format_array_values(
    name: &str,
    argument: &str,
    value: &Value,
) -> Result<Vec<Value>, BuiltinError> {
    let Value::Array(array) = deref_value(value) else {
        return Err(argument_type_error(name, argument, "array", value));
    };
    Ok(array.iter().map(|(_, value)| value.clone()).collect())
}

pub(in crate::builtins::modules) fn hex_encode(bytes: &[u8]) -> Vec<u8> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = Vec::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize]);
        output.push(HEX[(byte & 0x0f) as usize]);
    }
    output
}

pub(in crate::builtins::modules) fn hash_digest_bytes(
    name: &str,
    algorithm: &str,
    input: &[u8],
) -> Result<Vec<u8>, BuiltinError> {
    match normalized_hash_algorithm(algorithm).as_deref() {
        Some("md5") => Ok(Md5::digest(input).to_vec()),
        Some("sha1") => Ok(Sha1::digest(input).to_vec()),
        Some("sha224") => Ok(Sha224::digest(input).to_vec()),
        Some("sha256") => Ok(Sha256::digest(input).to_vec()),
        Some("sha384") => Ok(Sha384::digest(input).to_vec()),
        Some("sha512224") => Ok(Sha512_224::digest(input).to_vec()),
        Some("sha512256") => Ok(Sha512_256::digest(input).to_vec()),
        Some("sha512") => Ok(Sha512::digest(input).to_vec()),
        Some("adler32") => Ok(adler32(input).to_be_bytes().to_vec()),
        Some("crc32") | Some("crc32b") => Ok(crc32fast::hash(input).to_be_bytes().to_vec()),
        _ => Err(value_error(name, "unsupported hash algorithm")),
    }
}

fn adler32(input: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65_521;
    let mut a = 1_u32;
    let mut b = 0_u32;
    for byte in input {
        a = (a + u32::from(*byte)) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }
    (b << 16) | a
}

pub(in crate::builtins::modules) fn hmac_digest_bytes(
    name: &str,
    algorithm: &str,
    key: &[u8],
    input: &[u8],
) -> Result<Vec<u8>, BuiltinError> {
    match normalized_hash_algorithm(algorithm).as_deref() {
        Some("md5") => Ok(hmac_with_block_64(
            if key.len() > 64 {
                Md5::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            |bytes| Md5::digest(bytes).to_vec(),
        )),
        Some("sha1") => Ok(hmac_with_block_64(
            if key.len() > 64 {
                Sha1::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            |bytes| Sha1::digest(bytes).to_vec(),
        )),
        Some("sha224") => Ok(hmac_with_block(
            if key.len() > 64 {
                Sha224::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            64,
            |bytes| Sha224::digest(bytes).to_vec(),
        )),
        Some("sha256") => Ok(hmac_with_block(
            if key.len() > 64 {
                Sha256::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            64,
            |bytes| Sha256::digest(bytes).to_vec(),
        )),
        Some("sha384") => Ok(hmac_with_block(
            if key.len() > 128 {
                Sha384::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            128,
            |bytes| Sha384::digest(bytes).to_vec(),
        )),
        Some("sha512224") => Ok(hmac_with_block(
            if key.len() > 128 {
                Sha512_224::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            128,
            |bytes| Sha512_224::digest(bytes).to_vec(),
        )),
        Some("sha512256") => Ok(hmac_with_block(
            if key.len() > 128 {
                Sha512_256::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            128,
            |bytes| Sha512_256::digest(bytes).to_vec(),
        )),
        Some("sha512") => Ok(hmac_with_block(
            if key.len() > 128 {
                Sha512::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            128,
            |bytes| Sha512::digest(bytes).to_vec(),
        )),
        _ => Err(value_error(name, "unsupported hash algorithm")),
    }
}

pub(in crate::builtins::modules) fn hmac_with_block_64(
    key: Vec<u8>,
    input: &[u8],
    digest: impl Fn(&[u8]) -> Vec<u8>,
) -> Vec<u8> {
    hmac_with_block(key, input, 64, digest)
}

pub(in crate::builtins::modules) fn hmac_with_block(
    mut key: Vec<u8>,
    input: &[u8],
    block_size: usize,
    digest: impl Fn(&[u8]) -> Vec<u8>,
) -> Vec<u8> {
    key.resize(block_size, 0);
    let outer_pad = key.iter().map(|byte| byte ^ 0x5c).collect::<Vec<_>>();
    let mut inner = key.iter().map(|byte| byte ^ 0x36).collect::<Vec<_>>();
    inner.extend_from_slice(input);
    let inner_digest = digest(&inner);
    let mut outer = outer_pad;
    outer.extend_from_slice(&inner_digest);
    digest(&outer)
}

pub(in crate::builtins::modules) fn normalized_hash_algorithm(algorithm: &str) -> Option<String> {
    let normalized = algorithm.to_ascii_lowercase().replace('-', "");
    match normalized.as_str() {
        "md5" | "sha1" | "adler32" | "crc32" | "crc32b" => Some(normalized),
        "sha224" | "sha256" | "sha384" | "sha512" => Some(normalized),
        "sha512/224" => Some("sha512224".to_owned()),
        "sha512/256" => Some("sha512256".to_owned()),
        "sha512224" | "sha512256" => Some(normalized),
        _ => None,
    }
}

pub(in crate::builtins::modules) fn hex_decode(bytes: &[u8]) -> Option<Vec<u8>> {
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let mut output = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        output.push((hex_nibble(chunk[0])? << 4) | hex_nibble(chunk[1])?);
    }
    Some(output)
}

pub(in crate::builtins::modules) fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Engine default for `htmlspecialchars`-family flags: escape both quote
/// kinds (`ENT_QUOTES`); substitution/doctype bits are not modeled byte-wise.
pub(in crate::builtins::modules) const HTML_ESCAPE_DEFAULT_FLAGS: i64 = 3;
pub(in crate::builtins::modules) const PHP_QUERY_RFC3986: i64 = 2;
const ENT_XML1: i64 = 16;
const ENT_XHTML: i64 = 32;
const ENT_HTML5: i64 = 48;

pub(in crate::builtins::modules) fn html_escape_with_options(
    bytes: &[u8],
    flags: i64,
    double_encode: bool,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(html_escaped_capacity(bytes, flags));
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'&' if !double_encode => {
                if let Some(entity_len) = valid_html_entity_len(&bytes[index..]) {
                    output.extend_from_slice(&bytes[index..index + entity_len]);
                    index += entity_len;
                    continue;
                }
                output.extend_from_slice(b"&amp;");
            }
            b'&' => output.extend_from_slice(b"&amp;"),
            b'<' => output.extend_from_slice(b"&lt;"),
            b'>' => output.extend_from_slice(b"&gt;"),
            b'"' if flags & 2 != 0 => output.extend_from_slice(b"&quot;"),
            b'\'' if flags & 1 != 0 => output.extend_from_slice(b"&#039;"),
            _ => output.push(byte),
        }
        index += 1;
    }
    output
}

/// Exact escaped length for `double_encode` output; an upper bound when
/// existing entities are passed through (a literal `&` never expands past
/// `&amp;`).
fn html_escaped_capacity(bytes: &[u8], flags: i64) -> usize {
    bytes
        .iter()
        .map(|byte| match byte {
            b'&' => 5,
            b'<' | b'>' => 4,
            b'"' if flags & 2 != 0 => 6,
            b'\'' if flags & 1 != 0 => 6,
            _ => 1,
        })
        .sum()
}

fn valid_html_entity_len(bytes: &[u8]) -> Option<usize> {
    debug_assert_eq!(bytes.first(), Some(&b'&'));
    let semicolon = php_source::byte_kernel::find_byte(bytes, b';')?;
    if semicolon < 3 {
        return None;
    }
    let entity = &bytes[1..semicolon];
    if let Some(decimal) = entity.strip_prefix(b"#")
        && !decimal.is_empty()
        && php_source::byte_kernel::all_ascii_digits(decimal)
    {
        return Some(semicolon + 1);
    }
    if let Some(hex) = entity
        .strip_prefix(b"#x")
        .or_else(|| entity.strip_prefix(b"#X"))
        && !hex.is_empty()
        && hex.iter().all(u8::is_ascii_hexdigit)
    {
        return Some(semicolon + 1);
    }
    if matches!(entity, b"amp" | b"lt" | b"gt" | b"quot" | b"apos") {
        return Some(semicolon + 1);
    }
    None
}

pub(in crate::builtins::modules) fn html_entity_decode_with_flags(
    text: &str,
    flags: i64,
) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let remaining = &bytes[index..];
        let decoded = if remaining.starts_with(b"&lt;") {
            Some((b"<".as_slice(), 4))
        } else if remaining.starts_with(b"&gt;") {
            Some((b">".as_slice(), 4))
        } else if remaining.starts_with(b"&amp;") {
            Some((b"&".as_slice(), 5))
        } else if flags & 2 != 0 && remaining.starts_with(b"&quot;") {
            Some((b"\"".as_slice(), 6))
        } else if flags & 1 != 0 && remaining.starts_with(b"&#039;") {
            Some((b"'".as_slice(), 6))
        } else if flags & 1 != 0 && remaining.starts_with(b"&#x27;") {
            Some((b"'".as_slice(), 6))
        } else if flags & 1 != 0
            && html_document_type(flags) != HtmlDocumentType::Html401
            && remaining.starts_with(b"&apos;")
        {
            Some((b"'".as_slice(), 6))
        } else if remaining.starts_with(b"&#")
            && let Some((decoded, len)) = decode_numeric_html_entity(remaining, flags)
        {
            let mut buffer = [0_u8; 4];
            output.extend_from_slice(decoded.encode_utf8(&mut buffer).as_bytes());
            index += len;
            continue;
        } else {
            None
        };
        if let Some((entity, len)) = decoded {
            output.extend_from_slice(entity);
            index += len;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    output
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HtmlDocumentType {
    Html401,
    Xml1,
    Xhtml,
    Html5,
}

fn html_document_type(flags: i64) -> HtmlDocumentType {
    match flags & ENT_HTML5 {
        ENT_XML1 => HtmlDocumentType::Xml1,
        ENT_XHTML => HtmlDocumentType::Xhtml,
        ENT_HTML5 => HtmlDocumentType::Html5,
        _ => HtmlDocumentType::Html401,
    }
}

fn decode_numeric_html_entity(bytes: &[u8], flags: i64) -> Option<(char, usize)> {
    debug_assert_eq!(bytes.first(), Some(&b'&'));
    let semicolon = php_source::byte_kernel::find_byte(bytes, b';')?;
    let entity = &bytes[1..semicolon];
    let codepoint = if let Some(decimal) = entity.strip_prefix(b"#")
        && !decimal.is_empty()
        && php_source::byte_kernel::all_ascii_digits(decimal)
    {
        parse_entity_codepoint(decimal, 10)?
    } else if let Some(hex) = entity
        .strip_prefix(b"#x")
        .or_else(|| entity.strip_prefix(b"#X"))
        && !hex.is_empty()
        && hex.iter().all(u8::is_ascii_hexdigit)
    {
        parse_entity_codepoint(hex, 16)?
    } else {
        return None;
    };
    if codepoint == 0x27 && flags & 1 == 0 {
        return None;
    }
    let document_type = html_document_type(flags);
    if !html_entity_codepoint_allowed(codepoint, document_type) {
        return None;
    }
    Some((char::from_u32(codepoint)?, semicolon + 1))
}

fn parse_entity_codepoint(bytes: &[u8], radix: u32) -> Option<u32> {
    let mut value = 0_u32;
    for byte in bytes {
        value = value
            .checked_mul(radix)?
            .checked_add((*byte as char).to_digit(radix)?)?;
    }
    Some(value)
}

fn html_entity_codepoint_allowed(codepoint: u32, document_type: HtmlDocumentType) -> bool {
    if codepoint > 0x10ffff || (0xd800..=0xdfff).contains(&codepoint) {
        return false;
    }
    match document_type {
        HtmlDocumentType::Html401 => {
            matches!(codepoint, 0x09 | 0x0a | 0x0d)
                || (0x20..=0x7e).contains(&codepoint)
                || codepoint >= 0xa0
        }
        HtmlDocumentType::Xml1 | HtmlDocumentType::Xhtml => {
            matches!(codepoint, 0x09 | 0x0a | 0x0d)
                || (0x20..=0xd7ff).contains(&codepoint)
                || (0xe000..=0xfffd).contains(&codepoint)
                || (0x10000..=0x10ffff).contains(&codepoint)
        }
        HtmlDocumentType::Html5 => {
            matches!(codepoint, 0x09 | 0x0a | 0x0c)
                || (0x20..=0x7e).contains(&codepoint)
                || (codepoint >= 0xa0 && !is_html5_noncharacter(codepoint))
        }
    }
}

fn is_html5_noncharacter(codepoint: u32) -> bool {
    (0xfdd0..=0xfdef).contains(&codepoint) || matches!(codepoint & 0xffff, 0xfffe | 0xffff)
}

pub(in crate::builtins::modules) fn htmlspecialchars_decode_with_flags(
    text: &str,
    flags: i64,
) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let remaining = &bytes[index..];
        let decoded = if remaining.starts_with(b"&lt;") {
            Some((b'<', 4))
        } else if remaining.starts_with(b"&gt;") {
            Some((b'>', 4))
        } else if remaining.starts_with(b"&amp;") {
            Some((b'&', 5))
        } else if flags & 2 != 0 && remaining.starts_with(b"&quot;") {
            Some((b'"', 6))
        } else if flags & 1 != 0 && remaining.starts_with(b"&#039;") {
            Some((b'\'', 6))
        } else if flags & 1 != 0 && remaining.starts_with(b"&#x27;") {
            Some((b'\'', 6))
        } else {
            None
        };
        if let Some((byte, len)) = decoded {
            output.push(byte);
            index += len;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    output
}

pub(in crate::builtins::modules) fn url_encode(bytes: &[u8], raw: bool) -> Vec<u8> {
    let mut output = Vec::new();
    for byte in bytes {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_')
            || (!raw && *byte == b'.')
            || (raw && matches!(byte, b'.' | b'~'))
        {
            output.push(*byte);
        } else if !raw && *byte == b' ' {
            output.push(b'+');
        } else {
            output.extend_from_slice(format!("%{byte:02X}").as_bytes());
        }
    }
    output
}

pub(in crate::builtins::modules) fn url_decode(bytes: &[u8], raw: bool) -> Vec<u8> {
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) =
                (hex_nibble(bytes[index + 1]), hex_nibble(bytes[index + 2]))
        {
            output.push((high << 4) | low);
            index += 3;
        } else {
            output.push(if !raw && bytes[index] == b'+' {
                b' '
            } else {
                bytes[index]
            });
            index += 1;
        }
    }
    output
}

pub(in crate::builtins::modules) fn build_query_pairs(
    prefix: Option<String>,
    numeric_prefix: Option<&str>,
    raw_encoding: bool,
    value: &Value,
    pairs: &mut Vec<String>,
) -> Result<(), BuiltinError> {
    match deref_value(value) {
        Value::Array(array) => {
            for (key, value) in array.iter() {
                let key = match key {
                    ArrayKey::Int(index) => match (prefix.as_ref(), numeric_prefix) {
                        (None, Some(numeric_prefix)) => format!("{numeric_prefix}{index}"),
                        _ => index.to_string(),
                    },
                    ArrayKey::String(key) => key.to_string_lossy(),
                };
                let name = prefix
                    .as_ref()
                    .map_or(key.clone(), |prefix| format!("{prefix}[{key}]"));
                build_query_pairs(Some(name), numeric_prefix, raw_encoding, value, pairs)?;
            }
        }
        Value::Null | Value::Resource(_) => {}
        scalar => {
            let Some(name) = prefix else {
                return Ok(());
            };
            let value = match scalar {
                Value::Bool(true) => crate::PhpString::from_test_str("1"),
                Value::Bool(false) => crate::PhpString::from_test_str("0"),
                other => string_arg("http_build_query", &other)?,
            };
            pairs.push(format!(
                "{}={}",
                String::from_utf8_lossy(&url_encode(name.as_bytes(), raw_encoding)),
                String::from_utf8_lossy(&url_encode(value.as_bytes(), raw_encoding))
            ));
        }
    }
    Ok(())
}
