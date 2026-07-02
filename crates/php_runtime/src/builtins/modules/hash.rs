//! Hash extension builtins used by WordPress integrity and nonce flows.

use super::core::{expect_arity, string_arg};
use super::strings::{builtin_hash, builtin_hash_hmac};
use crate::Value;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult, RuntimeSourceSpan,
};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("hash", builtin_hash, BuiltinCompatibility::Php),
    BuiltinEntry::new("hash_algos", builtin_hash_algos, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "hash_equals",
        builtin_hash_equals,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("hash_hmac", builtin_hash_hmac, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "hash_hmac_algos",
        builtin_hash_hmac_algos,
        BuiltinCompatibility::Php,
    ),
];

const HASH_ALGOS: &[&str] = &[
    "md5", "sha1", "sha256", "sha384", "sha512", "crc32", "crc32b",
];

const HASH_HMAC_ALGOS: &[&str] = &["md5", "sha1", "sha256", "sha384", "sha512"];

fn builtin_hash_algos(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("hash_algos", &args, 0)?;
    Ok(Value::packed_array(
        HASH_ALGOS.iter().copied().map(Value::string).collect(),
    ))
}

fn builtin_hash_hmac_algos(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("hash_hmac_algos", &args, 0)?;
    Ok(Value::packed_array(
        HASH_HMAC_ALGOS.iter().copied().map(Value::string).collect(),
    ))
}

fn builtin_hash_equals(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("hash_equals", &args, 2)?;
    let known = string_arg("hash_equals", &args[0])?;
    let user = string_arg("hash_equals", &args[1])?;
    if known.len() != user.len() {
        return Ok(Value::Bool(false));
    }
    let diff = known
        .as_bytes()
        .iter()
        .zip(user.as_bytes())
        .fold(0_u8, |acc, (left, right)| acc | (left ^ right));
    Ok(Value::Bool(diff == 0))
}

#[cfg(test)]
mod tests {
    use super::super::core::hex_encode;
    use super::*;
    use crate::{OutputBuffer, builtins::BuiltinContext};

    fn call(name: &str, args: Vec<Value>) -> Value {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        ENTRIES
            .iter()
            .find(|entry| entry.name() == name)
            .expect("entry")
            .function()(&mut context, args, RuntimeSourceSpan::default())
        .expect("builtin succeeds")
    }

    #[test]
    fn hash_algos_exposes_wp_integrity_algorithms() {
        let Value::Array(algos) = call("hash_algos", vec![]) else {
            panic!("expected array");
        };
        let values = algos
            .iter()
            .map(|(_, value)| value.to_string())
            .collect::<Vec<_>>();
        assert!(values.iter().any(|value| value.contains("sha256")));
        assert!(values.iter().any(|value| value.contains("crc32")));

        let Value::Array(hmac_algos) = call("hash_hmac_algos", vec![]) else {
            panic!("expected HMAC algorithm array");
        };
        let hmac_values = hmac_algos
            .iter()
            .map(|(_, value)| value.to_string())
            .collect::<Vec<_>>();
        for algorithm in ["md5", "sha1", "sha256", "sha384", "sha512"] {
            assert!(
                hmac_values.iter().any(|value| value.contains(algorithm)),
                "missing HMAC algorithm {algorithm}"
            );
        }
        assert!(!hmac_values.iter().any(|value| value.contains("crc32")));

        assert_eq!(
            call(
                "hash_equals",
                vec![Value::string("same"), Value::string("same")]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call(
                "hash_equals",
                vec![
                    Value::string(hex_encode(b"ab")),
                    Value::string(hex_encode(b"ac"))
                ]
            ),
            Value::Bool(false)
        );
    }
}
