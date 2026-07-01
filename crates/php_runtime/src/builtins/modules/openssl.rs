//! OpenSSL-compatible helper builtin slice.

use super::core::{expect_arity, hex_encode, int_arg, string_arg, value_error};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{ArrayKey, PhpArray, PhpString, Value};
use md5::{Digest as Md5Digest, Md5};
use sha1::Sha1;
use sha2::{Sha224, Sha256, Sha384, Sha512};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "openssl_cipher_iv_length",
        builtin_openssl_cipher_iv_length,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_get_cipher_methods",
        builtin_openssl_get_cipher_methods,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_digest",
        builtin_openssl_digest,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_get_md_methods",
        builtin_openssl_get_md_methods,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_random_pseudo_bytes",
        builtin_openssl_random_pseudo_bytes,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_pkey_get_public",
        builtin_openssl_pkey_get_public,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_get_publickey",
        builtin_openssl_pkey_get_public,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_error_string",
        builtin_openssl_error_string,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_verify",
        builtin_openssl_verify,
        BuiltinCompatibility::Php,
    ),
];

const OPENSSL_MD_METHODS: &[&str] = &["md5", "sha1", "sha224", "sha256", "sha384", "sha512"];
const OPENSSL_CIPHER_METHODS: &[&str] = &["aes-128-cbc", "aes-256-cbc"];

pub(in crate::builtins::modules) fn builtin_openssl_random_pseudo_bytes(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin openssl_random_pseudo_bytes expects one or two argument(s)",
        ));
    }
    let length = int_arg("openssl_random_pseudo_bytes", &args[0])?;
    if length < 1 {
        return Err(value_error(
            "openssl_random_pseudo_bytes",
            "length must be greater than 0",
        ));
    }
    let mut bytes = vec![0; length as usize];
    getrandom::fill(&mut bytes).map_err(|error| {
        BuiltinError::new(
            "E_PHP_RUNTIME_OPENSSL_RANDOM_FAILURE",
            format!("openssl_random_pseudo_bytes(): failed to read random bytes: {error}"),
        )
    })?;
    Ok(Value::string(bytes))
}

pub(in crate::builtins::modules) fn builtin_openssl_digest(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin openssl_digest expects two or three argument(s)",
        ));
    }
    let data = string_arg("openssl_digest", &args[0])?;
    let method = string_arg("openssl_digest", &args[1])?.to_string_lossy();
    let raw_output = args
        .get(2)
        .map(crate::convert::to_bool)
        .transpose()
        .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_BUILTIN_TYPE", message))?
        .unwrap_or(false);
    let Some(digest) = digest_bytes(&method, data.as_bytes()) else {
        return Ok(Value::Bool(false));
    };
    Ok(if raw_output {
        Value::string(digest)
    } else {
        Value::string(hex_encode(&digest))
    })
}

pub(in crate::builtins::modules) fn builtin_openssl_get_cipher_methods(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin openssl_get_cipher_methods expects zero or one argument(s)",
        ));
    }
    Ok(string_list(OPENSSL_CIPHER_METHODS))
}

pub(in crate::builtins::modules) fn builtin_openssl_cipher_iv_length(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("openssl_cipher_iv_length", &args, 1)?;
    let method = string_arg("openssl_cipher_iv_length", &args[0])?.to_string_lossy();
    let length = match method.to_ascii_lowercase().as_str() {
        "aes-128-cbc" | "aes-256-cbc" => 16,
        _ => return Ok(Value::Bool(false)),
    };
    Ok(Value::Int(length))
}

pub(in crate::builtins::modules) fn builtin_openssl_get_md_methods(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("openssl_get_md_methods", &args, 0)?;
    let mut array = PhpArray::new();
    for (index, method) in OPENSSL_MD_METHODS.iter().enumerate() {
        array.insert(
            ArrayKey::Int(index as i64),
            Value::String(PhpString::from(*method)),
        );
    }
    Ok(Value::Array(array))
}

fn string_list(values: &[&str]) -> Value {
    let mut array = PhpArray::new();
    for (index, value) in values.iter().enumerate() {
        array.insert(
            ArrayKey::Int(index as i64),
            Value::String(PhpString::from(*value)),
        );
    }
    Value::Array(array)
}

pub(in crate::builtins::modules) fn builtin_openssl_verify(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=4).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin openssl_verify expects three or four argument(s)",
        ));
    }
    let _data = string_arg("openssl_verify", &args[0])?;
    let _signature = string_arg("openssl_verify", &args[1])?;
    let _public_key = string_arg("openssl_verify", &args[2])?;
    if let Some(algorithm) = args.get(3) {
        match algorithm {
            Value::Int(_) => {}
            value => {
                let _ = string_arg("openssl_verify", value)?;
            }
        }
    }
    Ok(Value::Int(-1))
}

pub(in crate::builtins::modules) fn builtin_openssl_pkey_get_public(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("openssl_pkey_get_public", &args, 1)?;
    let _key = string_arg("openssl_pkey_get_public", &args[0])?;
    Ok(Value::Bool(false))
}

pub(in crate::builtins::modules) fn builtin_openssl_error_string(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("openssl_error_string", &args, 0)?;
    Ok(Value::string(
        "OpenSSL public-key verification is not implemented by this runtime",
    ))
}

fn digest_bytes(method: &str, data: &[u8]) -> Option<Vec<u8>> {
    let normalized = method.to_ascii_lowercase().replace('-', "");
    match normalized.as_str() {
        "md5" => Some(Md5::digest(data).to_vec()),
        "sha1" => Some(Sha1::digest(data).to_vec()),
        "sha224" => Some(Sha224::digest(data).to_vec()),
        "sha256" => Some(Sha256::digest(data).to_vec()),
        "sha384" => Some(Sha384::digest(data).to_vec()),
        "sha512" => Some(Sha512::digest(data).to_vec()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputBuffer;

    #[test]
    fn openssl_digest_covers_wordpress_hash_methods() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);

        assert_eq!(
            builtin_openssl_digest(
                &mut context,
                vec![Value::string("abc"), Value::string("sha256")],
                RuntimeSourceSpan::default(),
            )
            .expect("digest"),
            Value::string("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(
            builtin_openssl_digest(
                &mut context,
                vec![Value::string("abc"), Value::string("unknown")],
                RuntimeSourceSpan::default(),
            )
            .expect("unsupported digest"),
            Value::Bool(false)
        );
    }

    #[test]
    fn openssl_md_methods_and_verify_gap_are_explicit() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);

        let Value::Array(methods) =
            builtin_openssl_get_md_methods(&mut context, vec![], RuntimeSourceSpan::default())
                .expect("methods")
        else {
            panic!("expected method array");
        };
        assert!(methods.iter().any(|(_, value)| {
            matches!(value, Value::String(method) if method.as_bytes() == b"sha256")
        }));
        assert_eq!(
            builtin_openssl_verify(
                &mut context,
                vec![
                    Value::string("data"),
                    Value::string("signature"),
                    Value::string("public-key"),
                ],
                RuntimeSourceSpan::default(),
            )
            .expect("verify gap"),
            Value::Int(-1)
        );
    }
}
