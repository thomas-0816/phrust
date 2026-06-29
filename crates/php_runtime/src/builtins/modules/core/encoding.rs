use super::*;
use md5::{Digest, Md5};
use sha1::Sha1;
use sha2::{Sha256, Sha384, Sha512};

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
        Some("sha256") => Ok(Sha256::digest(input).to_vec()),
        Some("sha384") => Ok(Sha384::digest(input).to_vec()),
        Some("sha512") => Ok(Sha512::digest(input).to_vec()),
        Some("crc32") | Some("crc32b") => Ok(crc32fast::hash(input).to_be_bytes().to_vec()),
        _ => Err(value_error(name, "unsupported hash algorithm")),
    }
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
        "md5" | "sha1" | "crc32" | "crc32b" => Some(normalized),
        "sha256" | "sha384" | "sha512" => Some(normalized),
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

pub(in crate::builtins::modules) fn html_escape(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    for byte in bytes {
        match byte {
            b'&' => output.extend_from_slice(b"&amp;"),
            b'<' => output.extend_from_slice(b"&lt;"),
            b'>' => output.extend_from_slice(b"&gt;"),
            b'"' => output.extend_from_slice(b"&quot;"),
            b'\'' => output.extend_from_slice(b"&#039;"),
            _ => output.push(*byte),
        }
    }
    output
}

pub(in crate::builtins::modules) fn html_decode(text: &str) -> Vec<u8> {
    text.replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .into_bytes()
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
    value: &Value,
    pairs: &mut Vec<String>,
) -> Result<(), BuiltinError> {
    match deref_value(value) {
        Value::Array(array) => {
            for (key, value) in array.iter() {
                let key = match key {
                    ArrayKey::Int(index) => index.to_string(),
                    ArrayKey::String(key) => key.to_string_lossy(),
                };
                let name = prefix
                    .as_ref()
                    .map_or(key.clone(), |prefix| format!("{prefix}[{key}]"));
                build_query_pairs(Some(name), value, pairs)?;
            }
        }
        Value::Null => {}
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
                String::from_utf8_lossy(&url_encode(name.as_bytes(), false)),
                String::from_utf8_lossy(&url_encode(value.as_bytes(), false))
            ));
        }
    }
    Ok(())
}
