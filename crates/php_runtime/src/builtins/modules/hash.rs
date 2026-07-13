//! Hash extension builtins for common integrity and keyed-digest flows.

use super::core::{
    HashOptions, argument_type_error, argument_value_error, arity_error, conversion_error,
    deref_value, expect_arity, hash_digest_bytes, hash_digest_bytes_with_options, hex_encode,
    hmac_digest_bytes, hmac_hash_algorithm_value_error, int_arg, nullable_string_arg,
    parse_hash_options, read_file_value, resource_arg, string_arg, type_error, value_error,
};
use super::strings::{builtin_hash, builtin_hash_hmac};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{
    ClassEntry, ClassFlags, ClassMethodEntry, ClassMethodFlags, ObjectRef, Value,
    normalize_class_name, to_bool,
};

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
    BuiltinEntry::new("mhash", builtin_mhash, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "mhash_count",
        builtin_mhash_count,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mhash_get_block_size",
        builtin_mhash_get_block_size,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mhash_get_hash_name",
        builtin_mhash_get_hash_name,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mhash_keygen_s2k",
        builtin_mhash_keygen_s2k,
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
    "tiger128,4",
    "tiger160,4",
    "tiger192,4",
    "snefru",
    "snefru256",
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
    "haval128,3",
    "haval160,3",
    "haval192,3",
    "haval224,3",
    "haval256,3",
    "haval128,4",
    "haval160,4",
    "haval192,4",
    "haval224,4",
    "haval256,4",
    "haval128,5",
    "haval160,5",
    "haval192,5",
    "haval224,5",
    "haval256,5",
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
    "tiger128,4",
    "tiger160,4",
    "tiger192,4",
    "snefru",
    "snefru256",
    "gost",
    "gost-crypto",
    "haval128,3",
    "haval160,3",
    "haval192,3",
    "haval224,3",
    "haval256,3",
    "haval128,4",
    "haval160,4",
    "haval192,4",
    "haval224,4",
    "haval256,4",
    "haval128,5",
    "haval160,5",
    "haval192,5",
    "haval224,5",
    "haval256,5",
];

pub(in crate::builtins) fn hash_algorithm_exists(algorithm: &str) -> bool {
    HASH_ALGOS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(algorithm))
}
const HASH_HMAC_FLAG: i64 = 1;
const HASH_CONTEXT_CLASS: &str = "HashContext";
const HASH_CONTEXT_ALGORITHM: &str = "__phrust_hash_algorithm";
const HASH_CONTEXT_FLAGS: &str = "__phrust_hash_flags";
const HASH_CONTEXT_KEY: &str = "__phrust_hash_key";
const HASH_CONTEXT_DATA: &str = "__phrust_hash_data";
const HASH_CONTEXT_FINALIZED: &str = "__phrust_hash_finalized";
const HASH_CONTEXT_SEED: &str = "__phrust_hash_seed";
const HASH_CONTEXT_SECRET: &str = "__phrust_hash_secret";

#[derive(Clone, Copy, Debug)]
struct MhashAlgorithm {
    mhash_name: Option<&'static str>,
    hash_name: Option<&'static str>,
    value: i64,
}

const MHASH_ALGOS: &[MhashAlgorithm] = &[
    MhashAlgorithm {
        mhash_name: Some("CRC32"),
        hash_name: Some("crc32"),
        value: 0,
    },
    MhashAlgorithm {
        mhash_name: Some("MD5"),
        hash_name: Some("md5"),
        value: 1,
    },
    MhashAlgorithm {
        mhash_name: Some("SHA1"),
        hash_name: Some("sha1"),
        value: 2,
    },
    MhashAlgorithm {
        mhash_name: Some("HAVAL256"),
        hash_name: Some("haval256,3"),
        value: 3,
    },
    MhashAlgorithm {
        mhash_name: None,
        hash_name: None,
        value: 4,
    },
    MhashAlgorithm {
        mhash_name: Some("RIPEMD160"),
        hash_name: Some("ripemd160"),
        value: 5,
    },
    MhashAlgorithm {
        mhash_name: None,
        hash_name: None,
        value: 6,
    },
    MhashAlgorithm {
        mhash_name: Some("TIGER"),
        hash_name: Some("tiger192,3"),
        value: 7,
    },
    MhashAlgorithm {
        mhash_name: Some("GOST"),
        hash_name: Some("gost"),
        value: 8,
    },
    MhashAlgorithm {
        mhash_name: Some("CRC32B"),
        hash_name: Some("crc32b"),
        value: 9,
    },
    MhashAlgorithm {
        mhash_name: Some("HAVAL224"),
        hash_name: Some("haval224,3"),
        value: 10,
    },
    MhashAlgorithm {
        mhash_name: Some("HAVAL192"),
        hash_name: Some("haval192,3"),
        value: 11,
    },
    MhashAlgorithm {
        mhash_name: Some("HAVAL160"),
        hash_name: Some("haval160,3"),
        value: 12,
    },
    MhashAlgorithm {
        mhash_name: Some("HAVAL128"),
        hash_name: Some("haval128,3"),
        value: 13,
    },
    MhashAlgorithm {
        mhash_name: Some("TIGER128"),
        hash_name: Some("tiger128,3"),
        value: 14,
    },
    MhashAlgorithm {
        mhash_name: Some("TIGER160"),
        hash_name: Some("tiger160,3"),
        value: 15,
    },
    MhashAlgorithm {
        mhash_name: Some("MD4"),
        hash_name: Some("md4"),
        value: 16,
    },
    MhashAlgorithm {
        mhash_name: Some("SHA256"),
        hash_name: Some("sha256"),
        value: 17,
    },
    MhashAlgorithm {
        mhash_name: Some("ADLER32"),
        hash_name: Some("adler32"),
        value: 18,
    },
    MhashAlgorithm {
        mhash_name: Some("SHA224"),
        hash_name: Some("sha224"),
        value: 19,
    },
    MhashAlgorithm {
        mhash_name: Some("SHA512"),
        hash_name: Some("sha512"),
        value: 20,
    },
    MhashAlgorithm {
        mhash_name: Some("SHA384"),
        hash_name: Some("sha384"),
        value: 21,
    },
    MhashAlgorithm {
        mhash_name: Some("WHIRLPOOL"),
        hash_name: Some("whirlpool"),
        value: 22,
    },
    MhashAlgorithm {
        mhash_name: Some("RIPEMD128"),
        hash_name: Some("ripemd128"),
        value: 23,
    },
    MhashAlgorithm {
        mhash_name: Some("RIPEMD256"),
        hash_name: Some("ripemd256"),
        value: 24,
    },
    MhashAlgorithm {
        mhash_name: Some("RIPEMD320"),
        hash_name: Some("ripemd320"),
        value: 25,
    },
    MhashAlgorithm {
        mhash_name: None,
        hash_name: None,
        value: 26,
    },
    MhashAlgorithm {
        mhash_name: Some("SNEFRU256"),
        hash_name: Some("snefru256"),
        value: 27,
    },
    MhashAlgorithm {
        mhash_name: Some("MD2"),
        hash_name: Some("md2"),
        value: 28,
    },
    MhashAlgorithm {
        mhash_name: Some("FNV132"),
        hash_name: Some("fnv132"),
        value: 29,
    },
    MhashAlgorithm {
        mhash_name: Some("FNV1A32"),
        hash_name: Some("fnv1a32"),
        value: 30,
    },
    MhashAlgorithm {
        mhash_name: Some("FNV164"),
        hash_name: Some("fnv164"),
        value: 31,
    },
    MhashAlgorithm {
        mhash_name: Some("FNV1A64"),
        hash_name: Some("fnv1a64"),
        value: 32,
    },
    MhashAlgorithm {
        mhash_name: Some("JOAAT"),
        hash_name: Some("joaat"),
        value: 33,
    },
    MhashAlgorithm {
        mhash_name: Some("CRC32C"),
        hash_name: Some("crc32c"),
        value: 34,
    },
    MhashAlgorithm {
        mhash_name: Some("MURMUR3A"),
        hash_name: Some("murmur3a"),
        value: 35,
    },
    MhashAlgorithm {
        mhash_name: Some("MURMUR3C"),
        hash_name: Some("murmur3c"),
        value: 36,
    },
    MhashAlgorithm {
        mhash_name: Some("MURMUR3F"),
        hash_name: Some("murmur3f"),
        value: 37,
    },
    MhashAlgorithm {
        mhash_name: Some("XXH32"),
        hash_name: Some("xxh32"),
        value: 38,
    },
    MhashAlgorithm {
        mhash_name: Some("XXH64"),
        hash_name: Some("xxh64"),
        value: 39,
    },
    MhashAlgorithm {
        mhash_name: Some("XXH3"),
        hash_name: Some("xxh3"),
        value: 40,
    },
    MhashAlgorithm {
        mhash_name: Some("XXH128"),
        hash_name: Some("xxh128"),
        value: 41,
    },
];

fn builtin_hash_init(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
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
        .map(|value| {
            nullable_string_arg(
                context,
                "hash_init",
                value,
                "#3 ($key)",
                "string",
                span.clone(),
            )
        })
        .transpose()?
        .unwrap_or_default();
    let options = parse_hash_options(context, "hash_init", &algorithm, args.get(3), span)?;

    if flags & !HASH_HMAC_FLAG != 0 {
        return Err(value_error("hash_init", "unsupported hash flags"));
    }
    if flags & HASH_HMAC_FLAG != 0 {
        if !HASH_HMAC_ALGOS
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(&algorithm))
        {
            hash_digest_bytes_with_options("hash_init", &algorithm, b"", &options)?;
            return Err(argument_value_error(
                "hash_init",
                "#1 ($algo)",
                "must be a cryptographic hashing algorithm if HMAC is requested",
            ));
        }
        if key.as_bytes().is_empty() {
            return Err(argument_value_error(
                "hash_init",
                "#3 ($key)",
                "must not be empty when HMAC is requested",
            ));
        }
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
    let known = hash_equals_string_arg("#1 ($known_string)", &args[0])?;
    let user = hash_equals_string_arg("#2 ($user_string)", &args[1])?;
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

fn hash_equals_string_arg(argument: &str, value: &Value) -> Result<crate::PhpString, BuiltinError> {
    match deref_value(value) {
        Value::String(value) => Ok(value),
        other => Err(argument_type_error(
            "hash_equals",
            argument,
            "string",
            &other,
        )),
    }
}

fn builtin_mhash(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("mhash", "two or three argument(s)"));
    }
    emit_mhash_deprecation(context, "mhash", span);
    let Some(algorithm) = mhash_algorithm_from_value(int_arg("mhash", &args[0])?) else {
        return Ok(Value::Bool(false));
    };
    let Some(hash_name) = algorithm.hash_name else {
        return Ok(Value::Bool(false));
    };
    let data = string_arg("mhash", &args[1])?;
    let digest = match args.get(2) {
        Some(Value::Null) | None => hash_digest_bytes("mhash", hash_name, data.as_bytes())?,
        Some(key) => {
            let key = string_arg("mhash", key)?;
            hmac_digest_bytes("mhash", hash_name, key.as_bytes(), data.as_bytes())?
        }
    };
    Ok(Value::string(digest))
}

fn builtin_mhash_get_hash_name(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mhash_get_hash_name", &args, 1)?;
    emit_mhash_deprecation(context, "mhash_get_hash_name", span);
    let Some(algorithm) = mhash_algorithm_from_value(int_arg("mhash_get_hash_name", &args[0])?)
    else {
        return Ok(Value::Bool(false));
    };
    Ok(algorithm
        .mhash_name
        .map(Value::string)
        .unwrap_or(Value::Bool(false)))
}

fn builtin_mhash_count(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mhash_count", &args, 0)?;
    emit_mhash_deprecation(context, "mhash_count", span);
    Ok(Value::Int((MHASH_ALGOS.len() - 1) as i64))
}

fn builtin_mhash_get_block_size(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mhash_get_block_size", &args, 1)?;
    emit_mhash_deprecation(context, "mhash_get_block_size", span);
    let Some(algorithm) = mhash_algorithm_from_value(int_arg("mhash_get_block_size", &args[0])?)
    else {
        return Ok(Value::Bool(false));
    };
    let Some(hash_name) = algorithm.hash_name else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Int(hash_digest_len(hash_name)? as i64))
}

fn builtin_mhash_keygen_s2k(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mhash_keygen_s2k", &args, 4)?;
    emit_mhash_deprecation(context, "mhash_keygen_s2k", span);
    let Some(algorithm) = mhash_algorithm_from_value(int_arg("mhash_keygen_s2k", &args[0])?) else {
        return Ok(Value::Bool(false));
    };
    let Some(hash_name) = algorithm.hash_name else {
        return Ok(Value::Bool(false));
    };
    let password = string_arg("mhash_keygen_s2k", &args[1])?;
    let salt = string_arg("mhash_keygen_s2k", &args[2])?;
    let bytes = int_arg("mhash_keygen_s2k", &args[3])?;
    if bytes <= 0 {
        return Err(argument_value_error(
            "mhash_keygen_s2k",
            "#4 ($length)",
            "must be a greater than 0",
        ));
    }
    let mut padded_salt = [0_u8; 8];
    let salt_bytes = salt.as_bytes();
    let copy_len = salt_bytes.len().min(padded_salt.len());
    padded_salt[..copy_len].copy_from_slice(&salt_bytes[..copy_len]);

    let digest_len = hash_digest_len(hash_name)?;
    let blocks = (bytes as usize).div_ceil(digest_len);
    let mut key = Vec::with_capacity(blocks * digest_len);
    for block_index in 0..blocks {
        let mut data =
            Vec::with_capacity(block_index + padded_salt.len() + password.as_bytes().len());
        data.extend(std::iter::repeat_n(0_u8, block_index));
        data.extend_from_slice(&padded_salt);
        data.extend_from_slice(password.as_bytes());
        key.extend_from_slice(&hash_digest_bytes("mhash_keygen_s2k", hash_name, &data)?);
    }
    key.truncate(bytes as usize);
    Ok(Value::string(key))
}

fn emit_mhash_deprecation(
    context: &mut BuiltinContext<'_>,
    function: &str,
    span: RuntimeSourceSpan,
) {
    context.php_deprecation(
        "E_PHP_MHASH_FUNCTION_DEPRECATED",
        format!("Function {function}() is deprecated since 8.1"),
        span,
    );
}

fn mhash_algorithm_from_value(value: i64) -> Option<MhashAlgorithm> {
    let index = usize::try_from(value).ok()?;
    MHASH_ALGOS
        .get(index)
        .copied()
        .filter(|algorithm| algorithm.value == value)
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
    reject_null_byte_filename("hash_file", "#2 ($filename)", &path)?;
    let binary = args
        .get(2)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("hash_file", message))?
        .unwrap_or(false);
    let options = parse_hash_options(context, "hash_file", &algorithm, args.get(3), span.clone())?;
    hash_digest_bytes_with_options("hash_file", &algorithm, b"", &options)?;
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
    reject_null_byte_filename("hash_hmac_file", "#2 ($filename)", &path)?;
    let key = string_arg("hash_hmac_file", &args[2])?;
    let binary = args
        .get(3)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("hash_hmac_file", message))?
        .unwrap_or(false);
    hmac_digest_bytes("hash_hmac_file", &algorithm, key.as_bytes(), b"")?;
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

fn reject_null_byte_filename(
    name: &str,
    argument: &str,
    path: &str,
) -> Result<(), crate::builtins::BuiltinError> {
    if path.as_bytes().contains(&0) {
        Err(argument_value_error(
            name,
            argument,
            "must not contain any null bytes",
        ))
    } else {
        Ok(())
    }
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
        return Err(argument_value_error(
            "hash_pbkdf2",
            "#4 ($iterations)",
            "must be greater than 0",
        ));
    }
    let length = args
        .get(4)
        .map(|value| int_arg("hash_pbkdf2", value))
        .transpose()?
        .unwrap_or(0);
    if length < 0 {
        return Err(argument_value_error(
            "hash_pbkdf2",
            "#5 ($length)",
            "must be greater than or equal to 0",
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
    if key.as_bytes().is_empty() {
        return Err(argument_value_error(
            "hash_hkdf",
            "#2 ($key)",
            "must not be empty",
        ));
    }
    let digest_len = hash_digest_len(&algorithm)?;
    let length = args
        .get(2)
        .map(|value| int_arg("hash_hkdf", value))
        .transpose()?
        .unwrap_or(0);
    if length < 0 {
        return Err(argument_value_error(
            "hash_hkdf",
            "#3 ($length)",
            "must be greater than or equal to 0",
        ));
    }
    let length = if length == 0 {
        digest_len
    } else {
        length as usize
    };
    if length > 255 * digest_len {
        return Err(argument_value_error(
            "hash_hkdf",
            "#3 ($length)",
            &format!("must be less than or equal to {}", 255 * digest_len),
        ));
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
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            format!("{name}(): Argument #1 ($context) must be a valid, non-finalized HashContext"),
        ));
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
        Err(hmac_hash_algorithm_value_error(name))
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
        name: normalize_class_name(HASH_CONTEXT_CLASS).into(),
        parent: None,
        interfaces: Vec::new(),
        methods: vec![ClassMethodEntry {
            name: "__construct".to_owned(),
            origin_class: normalize_class_name(HASH_CONTEXT_CLASS),
            function_id: 0,
            flags: ClassMethodFlags {
                is_private: true,
                ..ClassMethodFlags::default()
            },
            attributes: Vec::new(),
        }],
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

    fn call_result(name: &str, args: Vec<Value>) -> Result<Value, crate::builtins::BuiltinError> {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        ENTRIES
            .iter()
            .find(|entry| entry.name() == name)
            .expect("entry")
            .function()(&mut context, args, RuntimeSourceSpan::default())
    }

    fn call_with_context(name: &str, args: Vec<Value>, context: &mut BuiltinContext<'_>) -> Value {
        call_with_context_result(name, args, context).expect("builtin succeeds")
    }

    fn call_with_context_result(
        name: &str,
        args: Vec<Value>,
        context: &mut BuiltinContext<'_>,
    ) -> Result<Value, crate::builtins::BuiltinError> {
        ENTRIES
            .iter()
            .find(|entry| entry.name() == name)
            .expect("entry")
            .function()(context, args, RuntimeSourceSpan::default())
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
        assert!(values.iter().any(|value| value.contains("tiger128,4")));
        assert!(values.iter().any(|value| value.contains("tiger160,4")));
        assert!(values.iter().any(|value| value.contains("tiger192,4")));
        assert!(values.iter().any(|value| value.contains("snefru")));
        assert!(values.iter().any(|value| value.contains("snefru256")));
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
            "tiger128,4",
            "tiger160,4",
            "tiger192,4",
            "snefru",
            "snefru256",
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
    fn mhash_compatibility_maps_legacy_ids_and_keygen() {
        assert_eq!(call("mhash_count", vec![]), Value::Int(41));
        assert_eq!(
            call("mhash_get_hash_name", vec![Value::Int(1)]),
            Value::string("MD5")
        );
        assert_eq!(
            call("mhash_get_hash_name", vec![Value::Int(4)]),
            Value::Bool(false)
        );
        assert_eq!(
            call("mhash_get_block_size", vec![Value::Int(3)]),
            Value::Int(32)
        );
        assert_eq!(
            call("mhash_get_block_size", vec![Value::Int(4)]),
            Value::Bool(false)
        );

        let Value::String(md5) = call("mhash", vec![Value::Int(1), Value::string("test")]) else {
            panic!("expected raw md5 digest");
        };
        assert_eq!(
            hex_encode(md5.as_bytes()),
            b"098f6bcd4621d373cade4e832627b4f6"
        );

        let Value::String(hmac) = call(
            "mhash",
            vec![
                Value::Int(1),
                Value::string("test"),
                Value::string("secret"),
            ],
        ) else {
            panic!("expected raw md5 hmac digest");
        };
        assert_eq!(
            hex_encode(hmac.as_bytes()),
            b"63d6baf65df6bdee8f32b332e0930669"
        );

        let Value::String(key) = call(
            "mhash_keygen_s2k",
            vec![
                Value::Int(1),
                Value::string("password"),
                Value::string("salt"),
                Value::Int(24),
            ],
        ) else {
            panic!("expected generated key bytes");
        };
        assert_eq!(
            hex_encode(key.as_bytes()),
            b"626cab462d0ec4eacc809ffc36d716cedab253a24d387745"
        );
    }

    #[test]
    fn hash_invalid_algorithm_errors_match_php() {
        let hash = call_result("hash", vec![Value::string("foo"), Value::string("")])
            .expect_err("invalid hash algorithm is rejected");
        assert_eq!(hash.diagnostic_id(), "E_PHP_RUNTIME_BUILTIN_VALUE");
        assert_eq!(
            hash.message(),
            "hash(): Argument #1 ($algo) must be a valid hashing algorithm"
        );

        let hash_init = call_result("hash_init", vec![Value::string("foo")])
            .expect_err("invalid hash_init algorithm is rejected");
        assert_eq!(
            hash_init.message(),
            "hash_init(): Argument #1 ($algo) must be a valid hashing algorithm"
        );

        let hash_init_non_crypto_hmac = call_result(
            "hash_init",
            vec![
                Value::string("crc32"),
                Value::Int(HASH_HMAC_FLAG),
                Value::string("key"),
            ],
        )
        .expect_err("non-cryptographic HMAC hash_init algorithm is rejected");
        assert_eq!(
            hash_init_non_crypto_hmac.message(),
            "hash_init(): Argument #1 ($algo) must be a cryptographic hashing algorithm if HMAC is requested"
        );

        let hash_init_empty_key = call_result(
            "hash_init",
            vec![
                Value::string("md5"),
                Value::Int(HASH_HMAC_FLAG),
                Value::string(""),
            ],
        )
        .expect_err("empty HMAC hash_init key is rejected");
        assert_eq!(
            hash_init_empty_key.message(),
            "hash_init(): Argument #3 ($key) must not be empty when HMAC is requested"
        );

        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let hash_init_null_key = call_with_context_result(
            "hash_init",
            vec![
                Value::string("md5"),
                Value::Int(HASH_HMAC_FLAG),
                Value::Null,
            ],
            &mut context,
        )
        .expect_err("null HMAC hash_init key is rejected after deprecation");
        assert_eq!(
            hash_init_null_key.message(),
            "hash_init(): Argument #3 ($key) must not be empty when HMAC is requested"
        );
        let diagnostics = context.take_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message(),
            "hash_init(): Passing null to parameter #3 ($key) of type string is deprecated"
        );

        let hmac = call_result(
            "hash_hmac",
            vec![
                Value::string("foo"),
                Value::string(""),
                Value::string("key"),
            ],
        )
        .expect_err("invalid HMAC algorithm is rejected");
        assert_eq!(
            hmac.message(),
            "hash_hmac(): Argument #1 ($algo) must be a valid cryptographic hashing algorithm"
        );

        let hmac_file = call_result(
            "hash_hmac_file",
            vec![
                Value::string("foo"),
                Value::string("missing.txt"),
                Value::string("key"),
            ],
        )
        .expect_err("invalid hash_hmac_file algorithm is rejected before file access");
        assert_eq!(
            hmac_file.message(),
            "hash_hmac_file(): Argument #1 ($algo) must be a valid cryptographic hashing algorithm"
        );

        let hmac_file_null_path = call_result(
            "hash_hmac_file",
            vec![
                Value::string("md5"),
                Value::string(b"bad\0path".to_vec()),
                Value::string("key"),
            ],
        )
        .expect_err("hash_hmac_file rejects null bytes in filename");
        assert_eq!(
            hmac_file_null_path.message(),
            "hash_hmac_file(): Argument #2 ($filename) must not contain any null bytes"
        );

        let pbkdf2 = call_result(
            "hash_pbkdf2",
            vec![
                Value::string("foo"),
                Value::string("password"),
                Value::string("salt"),
                Value::Int(1),
            ],
        )
        .expect_err("invalid PBKDF2 algorithm is rejected");
        assert_eq!(
            pbkdf2.message(),
            "hash_pbkdf2(): Argument #1 ($algo) must be a valid cryptographic hashing algorithm"
        );

        let pbkdf2_iterations = call_result(
            "hash_pbkdf2",
            vec![
                Value::string("md5"),
                Value::string("password"),
                Value::string("salt"),
                Value::Int(0),
            ],
        )
        .expect_err("invalid PBKDF2 iteration count is rejected");
        assert_eq!(
            pbkdf2_iterations.message(),
            "hash_pbkdf2(): Argument #4 ($iterations) must be greater than 0"
        );

        let pbkdf2_length = call_result(
            "hash_pbkdf2",
            vec![
                Value::string("md5"),
                Value::string("password"),
                Value::string("salt"),
                Value::Int(1),
                Value::Int(-1),
            ],
        )
        .expect_err("invalid PBKDF2 length is rejected");
        assert_eq!(
            pbkdf2_length.message(),
            "hash_pbkdf2(): Argument #5 ($length) must be greater than or equal to 0"
        );

        let hkdf_key = call_result("hash_hkdf", vec![Value::string("sha1"), Value::string("")])
            .expect_err("empty HKDF key is rejected");
        assert_eq!(
            hkdf_key.message(),
            "hash_hkdf(): Argument #2 ($key) must not be empty"
        );

        let hkdf_length = call_result(
            "hash_hkdf",
            vec![
                Value::string("sha1"),
                Value::string("input key material"),
                Value::Int(-1),
            ],
        )
        .expect_err("invalid HKDF length is rejected");
        assert_eq!(
            hkdf_length.message(),
            "hash_hkdf(): Argument #3 ($length) must be greater than or equal to 0"
        );

        let hkdf_max_length = call_result(
            "hash_hkdf",
            vec![
                Value::string("sha1"),
                Value::string("input key material"),
                Value::Int(5101),
            ],
        )
        .expect_err("oversized HKDF length is rejected");
        assert_eq!(
            hkdf_max_length.message(),
            "hash_hkdf(): Argument #3 ($length) must be less than or equal to 5100"
        );
    }

    #[test]
    fn hash_equals_requires_strict_string_arguments() {
        let first = call_result("hash_equals", vec![Value::Int(123), Value::string("NaN")])
            .expect_err("non-string known string is rejected");
        assert_eq!(first.diagnostic_id(), "E_PHP_RUNTIME_BUILTIN_TYPE");
        assert_eq!(
            first.message(),
            "hash_equals(): Argument #1 ($known_string) must be of type string, int given"
        );

        let second = call_result("hash_equals", vec![Value::string("NaN"), Value::Int(123)])
            .expect_err("non-string user string is rejected");
        assert_eq!(second.diagnostic_id(), "E_PHP_RUNTIME_BUILTIN_TYPE");
        assert_eq!(
            second.message(),
            "hash_equals(): Argument #2 ($user_string) must be of type string, int given"
        );

        let null = call_result("hash_equals", vec![Value::Null, Value::string("")])
            .expect_err("null known string is rejected");
        assert_eq!(
            null.message(),
            "hash_equals(): Argument #1 ($known_string) must be of type string, null given"
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
    fn hash_seed_and_secret_option_type_deprecations_match_php() {
        let input = Value::string("Lorem ipsum dolor sit amet, consectetur adipiscing elit.");

        let mut murmur_options = PhpArray::new();
        murmur_options.insert(
            ArrayKey::String(PhpString::from("seed")),
            Value::string("42"),
        );
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        assert_eq!(
            call_with_context(
                "hash",
                vec![
                    Value::string("murmur3a"),
                    input.clone(),
                    Value::Bool(false),
                    Value::Array(murmur_options),
                ],
                &mut context,
            ),
            call("hash", vec![Value::string("murmur3a"), input.clone()])
        );
        let diagnostics = context.take_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message(),
            "hash(): Passing a seed of a type other than int is deprecated because it is the same as setting the seed to 0"
        );

        let mut xxh3_options = PhpArray::new();
        xxh3_options.insert(
            ArrayKey::String(PhpString::from("seed")),
            Value::string("42"),
        );
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        assert_eq!(
            call_with_context(
                "hash",
                vec![
                    Value::string("xxh3"),
                    input.clone(),
                    Value::Bool(false),
                    Value::Array(xxh3_options),
                ],
                &mut context,
            ),
            call("hash", vec![Value::string("xxh3"), input])
        );
        let diagnostics = context.take_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message(),
            "hash(): Passing a seed of a type other than int is deprecated because it is ignored"
        );

        let mut secret_options = PhpArray::new();
        secret_options.insert(ArrayKey::String(PhpString::from("secret")), Value::Int(4));
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let error = call_with_context_result(
            "hash_init",
            vec![
                Value::string("xxh3"),
                Value::Int(0),
                Value::string(""),
                Value::Array(secret_options),
            ],
            &mut context,
        )
        .expect_err("short converted secret is rejected after deprecation");
        assert_eq!(
            error.message(),
            "xxh3: Secret length must be >= 136 bytes, 1 bytes passed"
        );
        let diagnostics = context.take_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message(),
            "hash_init(): Passing a secret of a type other than string is deprecated because it implicitly converts to a string, potentially hiding bugs"
        );
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
    fn hash_supports_tiger4_vectors_and_hmac() {
        for (algorithm, digest, hmac) in [
            (
                "tiger128,4",
                "538883c8fc5f28250299018e66bdf4fd",
                "8a398c914ecc1837438befc56ce98f7c",
            ),
            (
                "tiger160,4",
                "538883c8fc5f28250299018e66bdf4fdb5ef7b65",
                "04939206562668d7e2ba5414d63327cd609c13ed",
            ),
            (
                "tiger192,4",
                "538883c8fc5f28250299018e66bdf4fdb5ef7b65f2e91753",
                "5980cb8fc54fd79616ba13298e56846210013fb0d11a45e5",
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
    fn hash_supports_snefru_vectors_and_hmac() {
        for algorithm in ["snefru", "snefru256"] {
            assert_eq!(
                call("hash", vec![Value::string(algorithm), Value::string("")]),
                Value::string("8617f366566a011837f4fb4ba5bedea2b892f3ed8b894023d16ae344b2be5881")
            );
            assert_eq!(
                call("hash", vec![Value::string(algorithm), Value::string("abc")]),
                Value::string("7d033205647a2af3dc8339f6cb25643c33ebc622d32979c4b612b02c4903031b")
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
                Value::string("4069aae0bfcf515ae3dcf53c79ebf2f7742ea2298a4339c328634fc381c3914f")
            );
        }
    }

    #[test]
    fn hash_supports_haval_vectors_context_and_hmac() {
        for (algorithm, digest, hmac, binary_len) in [
            (
                "haval128,3",
                "9e40ed883fb63e985d299b40cda2b8f2",
                "bbd14162d2cd743c07848aa132c3bac0",
                16,
            ),
            (
                "haval160,4",
                "77aca22f5b12cc09010afc9c0797308638b1cb9b",
                "a494b4743b2419732652d44c3aa08b389c6114a3",
                20,
            ),
            (
                "haval256,5",
                "976cd6254c337969e5913b158392a2921af16fca51f5601d486e0a9de01156e7",
                "390c347d78b2c2a9467915e168eae0816121bd10ef56b97d69cc57f876435a84",
                32,
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
            let Value::String(binary) = call(
                "hash",
                vec![
                    Value::string(algorithm),
                    Value::string("abc"),
                    Value::Bool(true),
                ],
            ) else {
                panic!("expected binary hash string");
            };
            assert_eq!(binary.as_bytes().len(), binary_len);
        }

        let context = call("hash_init", vec![Value::string("haval224,5")]);
        assert_eq!(
            call("hash_update", vec![context.clone(), Value::string("ab")]),
            Value::Bool(true)
        );
        assert_eq!(
            call("hash_update", vec![context.clone(), Value::string("c")]),
            Value::Bool(true)
        );
        assert_eq!(
            call("hash_final", vec![context]),
            Value::string("8081027a500147c512e5f1055986674d746d92af4841abeb89da64ad")
        );
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
        let finalized_error = call_with_error("hash_update", vec![context, Value::string("x")])
            .expect_err("finalized HashContext is rejected");
        assert_eq!(
            finalized_error.diagnostic_id(),
            "E_PHP_RUNTIME_BUILTIN_TYPE"
        );
        assert_eq!(
            finalized_error.message(),
            "hash_update(): Argument #1 ($context) must be a valid, non-finalized HashContext"
        );
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
