//! Hash extension builtins for common integrity and keyed-digest flows.

use super::core::{
    HashOptions, arity_error, conversion_error, deref_value, expect_arity, hash_digest_bytes,
    hash_digest_bytes_with_options, hex_encode, hmac_digest_bytes, int_arg, parse_hash_options,
    read_file_value, resource_arg, string_arg, type_error, value_error,
};
use super::strings::{builtin_hash, builtin_hash_hmac};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult, RuntimeSourceSpan,
};
use crate::{ClassEntry, ClassFlags, ObjectRef, Value, normalize_class_name, to_bool};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("hash", builtin_hash, BuiltinCompatibility::Php),
    BuiltinEntry::new("hash_algos", builtin_hash_algos, BuiltinCompatibility::Php),
    BuiltinEntry::new("hash_copy", builtin_hash_copy, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "hash_equals",
        builtin_hash_equals,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("hash_file", builtin_hash_file, BuiltinCompatibility::Php),
    BuiltinEntry::new("hash_final", builtin_hash_final, BuiltinCompatibility::Php),
    BuiltinEntry::new("hash_hmac", builtin_hash_hmac, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "hash_hmac_algos",
        builtin_hash_hmac_algos,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "hash_hmac_file",
        builtin_hash_hmac_file,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("hash_hkdf", builtin_hash_hkdf, BuiltinCompatibility::Php),
    BuiltinEntry::new("hash_init", builtin_hash_init, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "hash_pbkdf2",
        builtin_hash_pbkdf2,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "hash_update",
        builtin_hash_update,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "hash_update_file",
        builtin_hash_update_file,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "hash_update_stream",
        builtin_hash_update_stream,
        BuiltinCompatibility::Php,
    ),
];

const HASH_ALGOS: &[&str] = &[
    "md2",
    "md4",
    "md5",
    "sha1",
    "sha224",
    "sha256",
    "sha384",
    "sha512/224",
    "sha512/256",
    "sha512",
    "sha3-224",
    "sha3-256",
    "sha3-384",
    "sha3-512",
    "ripemd128",
    "ripemd160",
    "ripemd256",
    "ripemd320",
    "whirlpool",
    "tiger128,3",
    "tiger160,3",
    "tiger192,3",
    "gost",
    "gost-crypto",
    "adler32",
    "crc32",
    "crc32b",
    "crc32c",
    "fnv132",
    "fnv1a32",
    "fnv164",
    "fnv1a64",
    "joaat",
    "murmur3a",
    "murmur3c",
    "murmur3f",
    "xxh32",
    "xxh64",
    "xxh3",
    "xxh128",
];

const HASH_HMAC_ALGOS: &[&str] = &[
    "md2",
    "md4",
    "md5",
    "sha1",
    "sha224",
    "sha256",
    "sha384",
    "sha512/224",
    "sha512/256",
    "sha512",
    "sha3-224",
    "sha3-256",
    "sha3-384",
    "sha3-512",
    "ripemd128",
    "ripemd160",
    "ripemd256",
    "ripemd320",
    "whirlpool",
    "tiger128,3",
    "tiger160,3",
    "tiger192,3",
    "gost",
    "gost-crypto",
];
const HASH_HMAC_FLAG: i64 = 1;
const HASH_CONTEXT_CLASS: &str = "HashContext";
const HASH_CONTEXT_ALGORITHM: &str = "__phrust_hash_algorithm";
const HASH_CONTEXT_FLAGS: &str = "__phrust_hash_flags";
const HASH_CONTEXT_KEY: &str = "__phrust_hash_key";
const HASH_CONTEXT_DATA: &str = "__phrust_hash_data";
const HASH_CONTEXT_FINALIZED: &str = "__phrust_hash_finalized";
const HASH_CONTEXT_SEED: &str = "__phrust_hash_seed";
const HASH_CONTEXT_SECRET: &str = "__phrust_hash_secret";

fn builtin_hash_init(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=4).contains(&args.len()) {
        return Err(arity_error("hash_init", "one to four argument(s)"));
    }
    let algorithm = string_arg("hash_init", &args[0])?.to_string_lossy();
    let flags = args
        .get(1)
        .map(|value| int_arg("hash_init", value))
        .transpose()?
        .unwrap_or(0);
    let key = args
        .get(2)
        .map(|value| string_arg("hash_init", value))
        .transpose()?
        .unwrap_or_default();
    let options = parse_hash_options("hash_init", &algorithm, args.get(3))?;

    if flags & !HASH_HMAC_FLAG != 0 {
        return Err(value_error("hash_init", "unsupported hash flags"));
    }
    if flags & HASH_HMAC_FLAG != 0 && !HASH_HMAC_ALGOS.contains(&algorithm.as_ref()) {
        hmac_digest_bytes("hash_init", &algorithm, key.as_bytes(), b"")?;
    } else {
        hash_digest_bytes_with_options("hash_init", &algorithm, b"", &options)?;
    }

    let object = hash_context_object(
        &algorithm,
        flags,
        key.as_bytes(),
        Vec::new(),
        false,
        &options,
    );
    Ok(Value::Object(object))
}

fn builtin_hash_update(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("hash_update", &args, 2)?;
    let object = hash_context_arg("hash_update", &args[0])?;
    let data = string_arg("hash_update", &args[1])?;
    hash_context_append(&object, data.as_bytes())?;
    Ok(Value::Bool(true))
}

fn builtin_hash_update_file(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("hash_update_file", "two or three argument(s)"));
    }
    let object = hash_context_arg("hash_update_file", &args[0])?;
    let path = string_arg("hash_update_file", &args[1])?.to_string_lossy();
    let Value::String(input) = read_file_value(context, "hash_update_file", &path, span)? else {
        return Ok(Value::Bool(false));
    };
    hash_context_append(&object, input.as_bytes())?;
    Ok(Value::Bool(true))
}

fn builtin_hash_update_stream(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error(
            "hash_update_stream",
            "two or three argument(s)",
        ));
    }
    let object = hash_context_arg("hash_update_stream", &args[0])?;
    let Some(resource) = resource_arg(&args[1]) else {
        return Err(type_error("hash_update_stream", "resource", &args[1]));
    };
    let bytes = match args
        .get(2)
        .map(|value| int_arg("hash_update_stream", value))
        .transpose()?
        .unwrap_or(-1)
    {
        length if length < 0 => resource.read_to_end(),
        length => resource.read_bytes(length as usize),
    };
    let bytes = match bytes {
        Ok(bytes) => bytes,
        Err(_) => return Ok(Value::Bool(false)),
    };
    let consumed = bytes.len();
    hash_context_append(&object, &bytes)?;
    Ok(Value::Int(consumed as i64))
}

fn builtin_hash_final(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error("hash_final", "one or two argument(s)"));
    }
    let object = hash_context_arg("hash_final", &args[0])?;
    let binary = args
        .get(1)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("hash_final", message))?
        .unwrap_or(false);
    let algorithm = hash_context_string(&object, HASH_CONTEXT_ALGORITHM)?;
    let flags = hash_context_int(&object, HASH_CONTEXT_FLAGS)?;
    let key = hash_context_string(&object, HASH_CONTEXT_KEY)?;
    let data = hash_context_data(&object)?;
    let options = hash_context_options(&object)?;
    let digest = if flags & HASH_HMAC_FLAG != 0 {
        hmac_digest_bytes("hash_final", &algorithm, key.as_bytes(), &data)?
    } else {
        hash_digest_bytes_with_options("hash_final", &algorithm, &data, &options)?
    };
    object.set_property(HASH_CONTEXT_FINALIZED, Value::Bool(true));
    Ok(hash_output(digest, binary))
}

fn builtin_hash_copy(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("hash_copy", &args, 1)?;
    let object = hash_context_arg("hash_copy", &args[0])?;
    Ok(Value::Object(hash_context_object(
        &hash_context_string(&object, HASH_CONTEXT_ALGORITHM)?,
        hash_context_int(&object, HASH_CONTEXT_FLAGS)?,
        hash_context_string(&object, HASH_CONTEXT_KEY)?.as_bytes(),
        hash_context_data(&object)?,
        false,
        &hash_context_options(&object)?,
    )))
}

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

fn builtin_hash_file(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(arity_error("hash_file", "two to four argument(s)"));
    }
    let algorithm = string_arg("hash_file", &args[0])?.to_string_lossy();
    let path = string_arg("hash_file", &args[1])?.to_string_lossy();
    let binary = args
        .get(2)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("hash_file", message))?
        .unwrap_or(false);
    let options = parse_hash_options("hash_file", &algorithm, args.get(3))?;
    let Value::String(input) = read_file_value(context, "hash_file", &path, span)? else {
        return Ok(Value::Bool(false));
    };
    let digest =
        hash_digest_bytes_with_options("hash_file", &algorithm, input.as_bytes(), &options)?;
    Ok(hash_output(digest, binary))
}

fn builtin_hash_hmac_file(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=4).contains(&args.len()) {
        return Err(arity_error("hash_hmac_file", "three or four argument(s)"));
    }
    let algorithm = string_arg("hash_hmac_file", &args[0])?.to_string_lossy();
    let path = string_arg("hash_hmac_file", &args[1])?.to_string_lossy();
    let key = string_arg("hash_hmac_file", &args[2])?;
    let binary = args
        .get(3)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("hash_hmac_file", message))?
        .unwrap_or(false);
    let Value::String(input) = read_file_value(context, "hash_hmac_file", &path, span)? else {
        return Ok(Value::Bool(false));
    };
    let digest = hmac_digest_bytes(
        "hash_hmac_file",
        &algorithm,
        key.as_bytes(),
        input.as_bytes(),
    )?;
    Ok(hash_output(digest, binary))
}

fn builtin_hash_pbkdf2(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(4..=6).contains(&args.len()) {
        return Err(arity_error("hash_pbkdf2", "four to six argument(s)"));
    }
    let algorithm = string_arg("hash_pbkdf2", &args[0])?.to_string_lossy();
    ensure_hmac_algorithm("hash_pbkdf2", &algorithm)?;
    let password = string_arg("hash_pbkdf2", &args[1])?;
    let salt = string_arg("hash_pbkdf2", &args[2])?;
    let iterations = int_arg("hash_pbkdf2", &args[3])?;
    if iterations <= 0 {
        return Err(value_error(
            "hash_pbkdf2",
            "iterations must be greater than 0",
        ));
    }
    let length = args
        .get(4)
        .map(|value| int_arg("hash_pbkdf2", value))
        .transpose()?
        .unwrap_or(0);
    if length < 0 {
        return Err(value_error(
            "hash_pbkdf2",
            "length must be greater than or equal to 0",
        ));
    }
    let binary = args
        .get(5)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("hash_pbkdf2", message))?
        .unwrap_or(false);
    let digest_len = hash_digest_len(&algorithm)?;
    let raw_length = if length == 0 {
        digest_len
    } else if binary {
        length as usize
    } else {
        (length as usize).div_ceil(2)
    };
    let digest = pbkdf2_bytes(
        &algorithm,
        password.as_bytes(),
        salt.as_bytes(),
        iterations as usize,
        raw_length,
    )?;
    if binary {
        Ok(Value::string(digest))
    } else {
        let mut hex = hex_encode(&digest);
        if length > 0 {
            hex.truncate(length as usize);
        }
        Ok(Value::string(hex))
    }
}

fn builtin_hash_hkdf(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=5).contains(&args.len()) {
        return Err(arity_error("hash_hkdf", "two to five argument(s)"));
    }
    let algorithm = string_arg("hash_hkdf", &args[0])?.to_string_lossy();
    ensure_hmac_algorithm("hash_hkdf", &algorithm)?;
    let key = string_arg("hash_hkdf", &args[1])?;
    let digest_len = hash_digest_len(&algorithm)?;
    let length = args
        .get(2)
        .map(|value| int_arg("hash_hkdf", value))
        .transpose()?
        .unwrap_or(0);
    if length < 0 {
        return Err(value_error(
            "hash_hkdf",
            "length must be greater than or equal to 0",
        ));
    }
    let length = if length == 0 {
        digest_len
    } else {
        length as usize
    };
    if length > 255 * digest_len {
        return Err(value_error("hash_hkdf", "length is too large"));
    }
    let info = args
        .get(3)
        .map(|value| string_arg("hash_hkdf", value))
        .transpose()?
        .unwrap_or_default();
    let salt = args
        .get(4)
        .map(|value| string_arg("hash_hkdf", value))
        .transpose()?
        .map_or_else(|| vec![0; digest_len], |value| value.as_bytes().to_vec());
    Ok(Value::string(hkdf_bytes(
        &algorithm,
        key.as_bytes(),
        length,
        info.as_bytes(),
        &salt,
    )?))
}

fn hash_output(digest: Vec<u8>, binary: bool) -> Value {
    if binary {
        Value::string(digest)
    } else {
        Value::string(hex_encode(&digest))
    }
}

fn hash_context_object(
    algorithm: &str,
    flags: i64,
    key: &[u8],
    data: Vec<u8>,
    finalized: bool,
    options: &HashOptions,
) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&hash_context_class(), HASH_CONTEXT_CLASS);
    object.set_property(HASH_CONTEXT_ALGORITHM, Value::string(algorithm));
    object.set_property(HASH_CONTEXT_FLAGS, Value::Int(flags));
    object.set_property(HASH_CONTEXT_KEY, Value::string(key));
    object.set_property(HASH_CONTEXT_DATA, Value::string(data));
    object.set_property(HASH_CONTEXT_FINALIZED, Value::Bool(finalized));
    object.set_property(
        HASH_CONTEXT_SEED,
        options
            .seed
            .map(|seed| Value::Int(seed as i64))
            .unwrap_or(Value::Null),
    );
    object.set_property(
        HASH_CONTEXT_SECRET,
        options
            .secret
            .as_ref()
            .map(|secret| Value::string(secret.clone()))
            .unwrap_or(Value::Null),
    );
    object
}

fn hash_context_arg(name: &str, value: &Value) -> Result<ObjectRef, crate::builtins::BuiltinError> {
    let Value::Object(object) = deref_value(value) else {
        return Err(type_error(name, "HashContext", value));
    };
    if normalize_class_name(&object.class_name()) != normalize_class_name(HASH_CONTEXT_CLASS)
        || !matches!(
            object.get_property(HASH_CONTEXT_FINALIZED),
            Some(Value::Bool(false))
        )
    {
        return Err(type_error(name, "valid, non-finalized HashContext", value));
    }
    Ok(object)
}

fn hash_context_string(
    object: &ObjectRef,
    property: &str,
) -> Result<String, crate::builtins::BuiltinError> {
    let Some(Value::String(value)) = object.get_property(property) else {
        return Err(value_error("hash", "invalid HashContext state"));
    };
    Ok(value.to_string_lossy())
}

fn hash_context_data(object: &ObjectRef) -> Result<Vec<u8>, crate::builtins::BuiltinError> {
    let Some(Value::String(value)) = object.get_property(HASH_CONTEXT_DATA) else {
        return Err(value_error("hash", "invalid HashContext state"));
    };
    Ok(value.as_bytes().to_vec())
}

fn hash_context_options(object: &ObjectRef) -> Result<HashOptions, crate::builtins::BuiltinError> {
    let seed = match object.get_property(HASH_CONTEXT_SEED) {
        Some(Value::Int(seed)) => Some(seed as u64),
        Some(Value::Null) | None => None,
        _ => return Err(value_error("hash", "invalid HashContext state")),
    };
    let secret = match object.get_property(HASH_CONTEXT_SECRET) {
        Some(Value::String(secret)) => Some(secret.as_bytes().to_vec()),
        Some(Value::Null) | None => None,
        _ => return Err(value_error("hash", "invalid HashContext state")),
    };
    Ok(HashOptions { seed, secret })
}

fn hash_context_append(
    object: &ObjectRef,
    chunk: &[u8],
) -> Result<(), crate::builtins::BuiltinError> {
    let mut bytes = hash_context_data(object)?;
    bytes.extend_from_slice(chunk);
    object.set_property(HASH_CONTEXT_DATA, Value::string(bytes));
    Ok(())
}

fn ensure_hmac_algorithm(name: &str, algorithm: &str) -> Result<(), crate::builtins::BuiltinError> {
    if HASH_HMAC_ALGOS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(algorithm))
    {
        Ok(())
    } else {
        Err(value_error(name, "unsupported hash algorithm"))
    }
}

fn hash_digest_len(algorithm: &str) -> Result<usize, crate::builtins::BuiltinError> {
    Ok(hash_digest_bytes("hash", algorithm, b"")?.len())
}

fn pbkdf2_bytes(
    algorithm: &str,
    password: &[u8],
    salt: &[u8],
    iterations: usize,
    length: usize,
) -> Result<Vec<u8>, crate::builtins::BuiltinError> {
    let digest_len = hash_digest_len(algorithm)?;
    let mut output = Vec::with_capacity(length);
    let blocks = length.div_ceil(digest_len);
    for block_index in 1..=blocks {
        let mut block_salt = Vec::with_capacity(salt.len() + 4);
        block_salt.extend_from_slice(salt);
        block_salt.extend_from_slice(&(block_index as u32).to_be_bytes());
        let mut u = hmac_digest_bytes("hash_pbkdf2", algorithm, password, &block_salt)?;
        let mut t = u.clone();
        for _ in 1..iterations {
            u = hmac_digest_bytes("hash_pbkdf2", algorithm, password, &u)?;
            for (left, right) in t.iter_mut().zip(&u) {
                *left ^= right;
            }
        }
        output.extend_from_slice(&t);
    }
    output.truncate(length);
    Ok(output)
}

fn hkdf_bytes(
    algorithm: &str,
    key: &[u8],
    length: usize,
    info: &[u8],
    salt: &[u8],
) -> Result<Vec<u8>, crate::builtins::BuiltinError> {
    let prk = hmac_digest_bytes("hash_hkdf", algorithm, salt, key)?;
    let digest_len = hash_digest_len(algorithm)?;
    let mut output = Vec::with_capacity(length);
    let mut previous = Vec::new();
    for counter in 1..=length.div_ceil(digest_len) {
        let mut input = previous;
        input.extend_from_slice(info);
        input.push(counter as u8);
        previous = hmac_digest_bytes("hash_hkdf", algorithm, &prk, &input)?;
        output.extend_from_slice(&previous);
    }
    output.truncate(length);
    Ok(output)
}

fn hash_context_int(
    object: &ObjectRef,
    property: &str,
) -> Result<i64, crate::builtins::BuiltinError> {
    let Some(Value::Int(value)) = object.get_property(property) else {
        return Err(value_error("hash", "invalid HashContext state"));
    };
    Ok(value)
}

fn hash_context_class() -> ClassEntry {
    ClassEntry {
        name: normalize_class_name(HASH_CONTEXT_CLASS),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: ClassFlags {
            is_final: true,
            ..ClassFlags::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ArrayKey, FilesystemCapabilities, OutputBuffer, PhpArray, PhpString,
        builtins::BuiltinContext,
    };

    fn call(name: &str, args: Vec<Value>) -> Value {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        call_with_context(name, args, &mut context)
    }

    fn call_with_context(name: &str, args: Vec<Value>, context: &mut BuiltinContext<'_>) -> Value {
        ENTRIES
            .iter()
            .find(|entry| entry.name() == name)
            .expect("entry")
            .function()(context, args, RuntimeSourceSpan::default())
        .expect("builtin succeeds")
    }

    #[test]
    fn hash_algos_exposes_common_integrity_algorithms() {
        let Value::Array(algos) = call("hash_algos", vec![]) else {
            panic!("expected array");
        };
        let values = algos
            .iter()
            .map(|(_, value)| value.to_string())
            .collect::<Vec<_>>();
        assert!(values.iter().any(|value| value.contains("sha256")));
        assert!(values.iter().any(|value| value.contains("adler32")));
        assert!(values.iter().any(|value| value.contains("crc32")));
        assert!(values.iter().any(|value| value.contains("murmur3a")));
        assert!(values.iter().any(|value| value.contains("murmur3c")));
        assert!(values.iter().any(|value| value.contains("murmur3f")));
        assert!(values.iter().any(|value| value.contains("tiger128,3")));
        assert!(values.iter().any(|value| value.contains("tiger160,3")));
        assert!(values.iter().any(|value| value.contains("tiger192,3")));
        assert!(values.iter().any(|value| value.contains("gost")));
        assert!(values.iter().any(|value| value.contains("gost-crypto")));
        assert!(values.iter().any(|value| value.contains("xxh32")));
        assert!(values.iter().any(|value| value.contains("xxh64")));
        assert!(values.iter().any(|value| value.contains("xxh3")));
        assert!(values.iter().any(|value| value.contains("xxh128")));

        let Value::Array(hmac_algos) = call("hash_hmac_algos", vec![]) else {
            panic!("expected HMAC algorithm array");
        };
        let hmac_values = hmac_algos
            .iter()
            .map(|(_, value)| value.to_string())
            .collect::<Vec<_>>();
        for algorithm in [
            "md2",
            "md4",
            "md5",
            "sha1",
            "sha224",
            "sha256",
            "sha384",
            "sha512/224",
            "sha512/256",
            "sha512",
            "sha3-224",
            "sha3-256",
            "sha3-384",
            "sha3-512",
            "ripemd128",
            "ripemd160",
            "ripemd256",
            "ripemd320",
            "whirlpool",
            "tiger128,3",
            "tiger160,3",
            "tiger192,3",
            "gost",
            "gost-crypto",
        ] {
            assert!(
                hmac_values.iter().any(|value| value.contains(algorithm)),
                "missing HMAC algorithm {algorithm}"
            );
        }
        assert!(!hmac_values.iter().any(|value| value.contains("crc32")));
        assert!(!hmac_values.iter().any(|value| value.contains("murmur3")));

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

    #[test]
    fn hash_file_and_hmac_file_read_allowed_paths() {
        let root = std::env::temp_dir().join(format!("phrust-hash-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create tempdir");
        let path = root.join("payload.txt");
        std::fs::write(&path, b"data").expect("write payload");
        let capabilities = FilesystemCapabilities::none().with_allowed_roots(vec![root.clone()]);
        let mut output = OutputBuffer::default();
        let mut context =
            BuiltinContext::with_runtime(&mut output, root.clone(), capabilities, None);

        assert_eq!(
            call_with_context(
                "hash_file",
                vec![Value::string("sha256"), Value::string("payload.txt")],
                &mut context,
            ),
            Value::string("3a6eb0790f39ac87c94f3856b2dd2c5d110e6811602261a9a923d3bb23adc8b7")
        );
        assert_eq!(
            call_with_context(
                "hash_hmac_file",
                vec![
                    Value::string("sha256"),
                    Value::string("payload.txt"),
                    Value::string("key")
                ],
                &mut context,
            ),
            Value::string("5031fe3d989c6d1537a013fa6e739da23463fdaec3b70137d828e36ace221bd0")
        );
        std::fs::remove_dir_all(&root).expect("remove tempdir");
    }

    #[test]
    fn hash_supports_adler32_vectors() {
        assert_eq!(
            call("hash", vec![Value::string("adler32"), Value::string("")]),
            Value::string("00000001")
        );
        assert_eq!(
            call("hash", vec![Value::string("adler32"), Value::string("abc")]),
            Value::string("024d0127")
        );
        assert_eq!(
            call(
                "hash",
                vec![
                    Value::string("adler32"),
                    Value::string("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789")
                ]
            ),
            Value::string("8adb150c")
        );
    }

    #[test]
    fn hash_supports_crc_fnv_and_joaat_vectors() {
        assert_eq!(
            call("hash", vec![Value::string("crc32"), Value::string("abc")]),
            Value::string("73bb8c64")
        );
        assert_eq!(
            call("hash", vec![Value::string("crc32b"), Value::string("abc")]),
            Value::string("352441c2")
        );
        assert_eq!(
            call("hash", vec![Value::string("crc32c"), Value::string("abc")]),
            Value::string("364b3fb7")
        );
        assert_eq!(
            call("hash", vec![Value::string("fnv132"), Value::string("")]),
            Value::string("811c9dc5")
        );
        assert_eq!(
            call(
                "hash",
                vec![Value::string("fnv132"), Value::string("foobar")]
            ),
            Value::string("31f0b262")
        );
        assert_eq!(
            call("hash", vec![Value::string("fnv1a32"), Value::string("l")]),
            Value::string("e90c310b")
        );
        assert_eq!(
            call("hash", vec![Value::string("fnv164"), Value::string("")]),
            Value::string("cbf29ce484222325")
        );
        assert_eq!(
            call(
                "hash",
                vec![Value::string("fnv164"), Value::string("foobar")]
            ),
            Value::string("340d8765a4dda9c2")
        );
        assert_eq!(
            call("hash", vec![Value::string("fnv1a64"), Value::string("9")]),
            Value::string("af63b44c8601a894")
        );
        assert_eq!(
            call(
                "hash",
                vec![Value::string("joaat"), Value::string("hello world")]
            ),
            Value::string("3e4a5a57")
        );
        assert_eq!(
            call("hash", vec![Value::string("joaat"), Value::string("a")]),
            Value::string("ca2e9442")
        );
    }

    #[test]
    fn hash_supports_sha3_vectors_and_hmac() {
        assert_eq!(
            call("hash", vec![Value::string("sha3-224"), Value::string("")]),
            Value::string("6b4e03423667dbb73b6e15454f0eb1abd4597f9a1b078e3f5b5a6bc7")
        );
        assert_eq!(
            call("hash", vec![Value::string("sha3-256"), Value::string("")]),
            Value::string("a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a")
        );
        assert_eq!(
            call("hash", vec![Value::string("sha3-384"), Value::string("")]),
            Value::string(
                "0c63a75b845e4f7d01107d852e4c2485c51a50aaaa94fc61995e71bbee983a2ac3713831264adb47fb6bd1e058d5f004"
            )
        );
        assert_eq!(
            call("hash", vec![Value::string("sha3-512"), Value::string("")]),
            Value::string(
                "a69f73cca23a9ac5c8b567dc185a756e97c982164fe25859e0d1dcc1475c80a615b2123af1f5f94c11e3e9402c3ac558f500199d95b6d3e301758586281dcd26"
            )
        );
        assert_eq!(
            call(
                "hash_hmac",
                vec![
                    Value::string("sha3-256"),
                    Value::string("payload"),
                    Value::string("key")
                ]
            ),
            Value::string("fe8fae51320c42433aa330c1eec41e63cc9c6d307c2eb4f2dd6cf09f4a5e812b")
        );
    }

    #[test]
    fn hash_supports_ripemd_vectors_and_hmac() {
        assert_eq!(
            call("hash", vec![Value::string("ripemd128"), Value::string("")]),
            Value::string("cdf26213a150dc3ecb610f18f6b38b46")
        );
        assert_eq!(
            call("hash", vec![Value::string("ripemd160"), Value::string("")]),
            Value::string("9c1185a5c5e9fc54612808977ee8f548b2258d31")
        );
        assert_eq!(
            call("hash", vec![Value::string("ripemd256"), Value::string("")]),
            Value::string("02ba4c4e5f8ecd1877fc52d64d30e37a2d9774fb1e5d026380ae0168e3c5522d")
        );
        assert_eq!(
            call("hash", vec![Value::string("ripemd320"), Value::string("")]),
            Value::string(
                "22d65d5661536cdc75c1fdf5c6de7b41b9f27325ebc61e8557177d705a0ec880151c3a32a00899b8"
            )
        );
        assert_eq!(
            call(
                "hash_hmac",
                vec![
                    Value::string("ripemd160"),
                    Value::string("payload"),
                    Value::string("key")
                ]
            ),
            Value::string("b89bad7ab6f5ada8ab77f806aa7cbdb58cf053fc")
        );
    }

    #[test]
    fn hash_supports_md2_md4_vectors_and_hmac() {
        assert_eq!(
            call("hash", vec![Value::string("md2"), Value::string("")]),
            Value::string("8350e5a3e24c153df2275c9f80692773")
        );
        assert_eq!(
            call("hash", vec![Value::string("md2"), Value::string("abc")]),
            Value::string("da853b0d3f88d99b30283a69e6ded6bb")
        );
        assert_eq!(
            call("hash", vec![Value::string("md4"), Value::string("")]),
            Value::string("31d6cfe0d16ae931b73c59d7e0c089c0")
        );
        assert_eq!(
            call("hash", vec![Value::string("md4"), Value::string("abc")]),
            Value::string("a448017aaf21d8525fc10ae87aa6729d")
        );
        assert_eq!(
            call(
                "hash_hmac",
                vec![
                    Value::string("md2"),
                    Value::string("payload"),
                    Value::string("key")
                ]
            ),
            Value::string("6ed0e7a3502c41954afc993fcc87c735")
        );
        assert_eq!(
            call(
                "hash_hmac",
                vec![
                    Value::string("md4"),
                    Value::string("payload"),
                    Value::string("key")
                ]
            ),
            Value::string("09c186d814e523835f522cd8de87b965")
        );
    }

    #[test]
    fn hash_supports_murmur3_vectors() {
        assert_eq!(
            call("hash", vec![Value::string("murmur3a"), Value::string("")]),
            Value::string("00000000")
        );
        assert_eq!(
            call(
                "hash",
                vec![Value::string("murmur3a"), Value::string("foo")]
            ),
            Value::string("f6a5c420")
        );
        assert_eq!(
            call(
                "hash",
                vec![
                    Value::string("murmur3c"),
                    Value::string("Two hashes meet in a bar")
                ]
            ),
            Value::string("8036c2707453c6f37348142be7eaf75c")
        );
        assert_eq!(
            call(
                "hash",
                vec![Value::string("murmur3c"), Value::string("hash me!")]
            ),
            Value::string("c7009299985a5627a9280372a9280372")
        );
        assert_eq!(
            call(
                "hash",
                vec![
                    Value::string("murmur3f"),
                    Value::string("Two hashes meet in a bar")
                ]
            ),
            Value::string("40256ed26fa6ece7785092ed33c8b659")
        );
        assert_eq!(
            call(
                "hash",
                vec![Value::string("murmur3f"), Value::string("hash me!")]
            ),
            Value::string("c43668294e89db0ba5772846e5804467")
        );
    }

    #[test]
    fn hash_supports_murmur3_seed_vectors() {
        for (algorithm, seed, expected) in [
            ("murmur3f", 42, "95855f9be0db784a5c37e878c4a4dcee"),
            ("murmur3c", 106, "f64c9eb40287fa686575163893e283b2"),
            ("murmur3a", 2345, "7f7ec59b"),
        ] {
            let mut options = PhpArray::new();
            options.insert(ArrayKey::String(PhpString::from("seed")), Value::Int(seed));
            let options = Value::Array(options);
            let input = Value::string("Two hashes meet in a bar.");

            assert_eq!(
                call(
                    "hash",
                    vec![
                        Value::string(algorithm),
                        input.clone(),
                        Value::Bool(false),
                        options.clone(),
                    ],
                ),
                Value::string(expected)
            );

            let context = call(
                "hash_init",
                vec![
                    Value::string(algorithm),
                    Value::Int(0),
                    Value::string(""),
                    options.clone(),
                ],
            );
            for chunk in ["Two", " hashes", " meet", " in", " a", " bar."] {
                call("hash_update", vec![context.clone(), Value::string(chunk)]);
            }
            assert_eq!(call("hash_final", vec![context]), Value::string(expected));
        }
    }

    #[test]
    fn hash_supports_xxhash_seed_vectors() {
        let mut options = PhpArray::new();
        options.insert(ArrayKey::String(PhpString::from("seed")), Value::Int(42));
        let options = Value::Array(options);
        let input = Value::string("Lorem ipsum dolor sit amet, consectetur adipiscing elit.");

        for (algorithm, expected) in [
            ("xxh32", "3d0cc7e5"),
            ("xxh64", "9c9aa071b5d22a15"),
            ("xxh3", "366409913c16b70d"),
            ("xxh128", "f87856a7589354e92aeca886c71ed7fb"),
        ] {
            assert_eq!(
                call(
                    "hash",
                    vec![
                        Value::string(algorithm),
                        input.clone(),
                        Value::Bool(false),
                        options.clone(),
                    ],
                ),
                Value::string(expected)
            );

            let context = call(
                "hash_init",
                vec![
                    Value::string(algorithm),
                    Value::Int(0),
                    Value::string(""),
                    options.clone(),
                ],
            );
            call(
                "hash_update",
                vec![
                    context.clone(),
                    Value::string("Lorem ipsum dolor sit amet,"),
                ],
            );
            call(
                "hash_update",
                vec![
                    context.clone(),
                    Value::string(" consectetur adipiscing elit."),
                ],
            );
            assert_eq!(call("hash_final", vec![context]), Value::string(expected));
        }
    }

    #[test]
    fn hash_supports_tiger3_vectors_and_hmac() {
        for (algorithm, digest, hmac) in [
            (
                "tiger128,3",
                "2aab1484e8c158f2bfb8c5ff41b57a52",
                "6a1402befd9f3a1b82416fb4bd49f178",
            ),
            (
                "tiger160,3",
                "2aab1484e8c158f2bfb8c5ff41b57a525129131c",
                "afe24c5aaf426f9527df0655f8a8f5661c421420",
            ),
            (
                "tiger192,3",
                "2aab1484e8c158f2bfb8c5ff41b57a525129131c957b5f93",
                "2612c8b754b60c65610dc5837c3fbce4cd2d846e35884800",
            ),
        ] {
            assert_eq!(
                call("hash", vec![Value::string(algorithm), Value::string("abc")]),
                Value::string(digest)
            );
            assert_eq!(
                call(
                    "hash_hmac",
                    vec![
                        Value::string(algorithm),
                        Value::string("payload"),
                        Value::string("key"),
                    ],
                ),
                Value::string(hmac)
            );
        }
    }

    #[test]
    fn hash_supports_whirlpool_vectors_and_hmac() {
        assert_eq!(
            call("hash", vec![Value::string("whirlpool"), Value::string("")]),
            Value::string(
                "19fa61d75522a4669b44e39c1d2e1726c530232130d407f89afee0964997f7a7\
                 3e83be698b288febcf88e3e03c4f0757ea8964e59b63d93708b138cc42a66eb3"
                    .replace(' ', "")
            )
        );
        assert_eq!(
            call(
                "hash",
                vec![Value::string("whirlpool"), Value::string("abc")]
            ),
            Value::string(
                "4e2448a4c6f486bb16b6562c73b4020bf3043e3a731bce721ae1b303d97e6d4c\
                 7181eebdb6c57e277d0e34957114cbd6c797fc9d95d8b582d225292076d4eef5"
                    .replace(' ', "")
            )
        );
        assert_eq!(
            call(
                "hash_hmac",
                vec![
                    Value::string("whirlpool"),
                    Value::string("payload"),
                    Value::string("key")
                ]
            ),
            Value::string(
                "6f2491caf3e6d9f854bc818ec2aa5b2783fca804377192ef59c4402357468195\
                 7c77af7aee6156ec00b34673ae7ef1090ff7874c822eca828d030c4a15681b66"
                    .replace(' ', "")
            )
        );
    }

    #[test]
    fn hash_supports_gost_vectors_and_hmac() {
        assert_eq!(
            call("hash", vec![Value::string("gost"), Value::string("")]),
            Value::string("ce85b99cc46752fffee35cab9a7b0278abb4c2d2055cff685af4912c49490f8d")
        );
        assert_eq!(
            call(
                "hash",
                vec![
                    Value::string("gost"),
                    Value::string("The quick brown fox jumps over the lazy dog")
                ]
            ),
            Value::string("77b7fa410c9ac58a25f49bca7d0468c9296529315eaca76bd1a10f376d1f4294")
        );
        assert_eq!(
            call(
                "hash",
                vec![Value::string("gost-crypto"), Value::string("")]
            ),
            Value::string("981e5f3ca30c841487830f84fb433e13ac1101569b9c13584ac483234cd656c0")
        );
        assert_eq!(
            call(
                "hash",
                vec![
                    Value::string("gost-crypto"),
                    Value::string("The quick brown fox jumps over the lazy dog")
                ]
            ),
            Value::string("9004294a361a508c586fe53d1f1b02746765e71b765472786e4770d565830a76")
        );
        assert_eq!(
            call(
                "hash_hmac",
                vec![
                    Value::string("gost"),
                    Value::string("payload"),
                    Value::string("key")
                ]
            ),
            Value::string("ec9938d7c9445db93969926b616da3a7ccef4b616782599b5577db73261de99c")
        );
        assert_eq!(
            call(
                "hash_hmac",
                vec![
                    Value::string("gost-crypto"),
                    Value::string("payload"),
                    Value::string("key")
                ]
            ),
            Value::string("55d662833d81f4fbc02c98634f873c0cd9e2426da2981a3b1f942bd41179e979")
        );
    }

    #[test]
    fn hash_context_update_copy_and_finalize_match_incremental_digest() {
        let context = call("hash_init", vec![Value::string("sha256")]);
        let Value::Object(handle) = context.clone() else {
            panic!("expected HashContext");
        };
        assert_eq!(handle.display_name(), HASH_CONTEXT_CLASS);
        assert_eq!(
            call("hash_update", vec![context.clone(), Value::string("ab")]),
            Value::Bool(true)
        );
        let copy = call("hash_copy", vec![context.clone()]);
        assert_eq!(
            call("hash_update", vec![context.clone(), Value::string("c")]),
            Value::Bool(true)
        );
        assert_eq!(
            call("hash_final", vec![context.clone()]),
            Value::string("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert!(call_with_error("hash_update", vec![context, Value::string("x")]).is_err());
        assert_eq!(
            call("hash_final", vec![copy]),
            Value::string("fb8e20fc2e4c3f248c60c39bd652f3c1347298bb977b8b4d5903b85055620603")
        );
    }

    #[test]
    fn hash_context_supports_hmac_and_binary_output() {
        let context = call(
            "hash_init",
            vec![
                Value::string("sha256"),
                Value::Int(HASH_HMAC_FLAG),
                Value::string("key"),
            ],
        );
        assert_eq!(
            call("hash_update", vec![context.clone(), Value::string("data")]),
            Value::Bool(true)
        );
        assert_eq!(
            call("hash_final", vec![context]),
            Value::string("5031fe3d989c6d1537a013fa6e739da23463fdaec3b70137d828e36ace221bd0")
        );

        let binary_context = call("hash_init", vec![Value::string("md5")]);
        assert_eq!(
            call(
                "hash_update",
                vec![binary_context.clone(), Value::string("data")]
            ),
            Value::Bool(true)
        );
        assert_eq!(
            call("hash_final", vec![binary_context, Value::Bool(true)]),
            Value::string(vec![
                0x8d, 0x77, 0x7f, 0x38, 0x5d, 0x3d, 0xfe, 0xc8, 0x81, 0x5d, 0x20, 0xf7, 0x49, 0x60,
                0x26, 0xdc,
            ])
        );
    }

    fn call_with_error(name: &str, args: Vec<Value>) -> BuiltinResult {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        ENTRIES
            .iter()
            .find(|entry| entry.name() == name)
            .expect("entry")
            .function()(&mut context, args, RuntimeSourceSpan::default())
    }
}
