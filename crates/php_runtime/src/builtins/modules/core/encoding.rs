use super::haval::haval_digest;
use super::snefru::snefru_digest;
use super::*;
use gost94::{Digest as GostDigest, Gost94CryptoPro, Gost94Test};
use md2::Md2;
use md4::Md4;
use md5::{Digest, Md5};
use murmurs::{murmur3_x64_128, murmur3_x86_32, murmur3_x86_128};
use ripemd::{Ripemd128, Ripemd160, Ripemd256, Ripemd320};
use sha1::Sha1;
use sha2::{Sha224, Sha256, Sha384, Sha512, Sha512_224, Sha512_256};
use sha3::{Sha3_224, Sha3_256, Sha3_384, Sha3_512};
use std::collections::HashSet;
use tiger::{Digest as TigerDigest, Tiger, Tiger4};
use whirlpool::Whirlpool;
use xxhash_rust::{
    xxh3::{
        xxh3_64, xxh3_64_with_secret, xxh3_64_with_seed, xxh3_128, xxh3_128_with_secret,
        xxh3_128_with_seed,
    },
    xxh32::xxh32,
    xxh64::xxh64,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::builtins::modules) struct HashOptions {
    pub seed: Option<u64>,
    pub secret: Option<Vec<u8>>,
}

pub(in crate::builtins::modules) fn parse_hash_options(
    context: &mut BuiltinContext<'_>,
    name: &str,
    algorithm: &str,
    value: Option<&Value>,
    span: RuntimeSourceSpan,
) -> Result<HashOptions, BuiltinError> {
    let Some(value) = value else {
        return Ok(HashOptions::default());
    };
    let Value::Array(array) = deref_value(value) else {
        return Err(argument_type_error(name, "#4 ($options)", "array", value));
    };
    let mut options = HashOptions::default();
    if let Some(seed) = array.get(&ArrayKey::String(PhpString::from("seed"))) {
        options.seed = hash_seed_option(context, name, algorithm, seed, span.clone())?;
    }
    if let Some(secret) = array.get(&ArrayKey::String(PhpString::from("secret"))) {
        if !matches!(deref_value(secret), Value::String(_)) {
            context.php_deprecation(
                "E_PHP_HASH_SECRET_TYPE_DEPRECATED",
                format!(
                    "{name}(): Passing a secret of a type other than string is deprecated because it implicitly converts to a string, potentially hiding bugs"
                ),
                span,
            );
        }
        let secret = string_arg(name, secret)?.as_bytes().to_vec();
        if secret.len() < 136 {
            return Err(BuiltinError::new(
                "E_PHP_RUNTIME_BUILTIN_VALUE",
                format!(
                    "{algorithm}: Secret length must be >= 136 bytes, {} bytes passed",
                    secret.len()
                ),
            ));
        }
        options.secret = Some(secret);
    }
    validate_hash_options(name, algorithm, &options)?;
    Ok(options)
}

fn hash_seed_option(
    context: &mut BuiltinContext<'_>,
    name: &str,
    algorithm: &str,
    value: &Value,
    span: RuntimeSourceSpan,
) -> Result<Option<u64>, BuiltinError> {
    if let Value::Int(seed) = deref_value(value) {
        return Ok(Some(seed as u64));
    }

    let normalized = normalized_hash_algorithm(algorithm);
    let behavior = match normalized.as_deref() {
        Some("murmur3a" | "murmur3c" | "murmur3f" | "xxh32" | "xxh64") => {
            Some(("the same as setting the seed to 0", Some(0)))
        }
        Some("xxh3" | "xxh128") => Some(("ignored", None)),
        _ => None,
    };
    if let Some((message, seed)) = behavior {
        context.php_deprecation(
            "E_PHP_HASH_SEED_TYPE_DEPRECATED",
            format!(
                "{name}(): Passing a seed of a type other than int is deprecated because it is {message}"
            ),
            span,
        );
        return Ok(seed);
    }

    Ok(Some(int_arg(name, value)? as u64))
}

fn validate_hash_options(
    name: &str,
    algorithm: &str,
    options: &HashOptions,
) -> Result<(), BuiltinError> {
    match normalized_hash_algorithm(algorithm).as_deref() {
        Some("xxh3" | "xxh128") if options.seed.is_some() && options.secret.is_some() => {
            Err(BuiltinError::new(
                "E_PHP_RUNTIME_BUILTIN_VALUE",
                format!(
                    "{algorithm}: Only one of seed or secret is to be passed for initialization"
                ),
            ))
        }
        Some("murmur3a" | "murmur3c" | "murmur3f") if options.secret.is_some() => Err(value_error(
            name,
            "hash secret is only supported for xxh3 and xxh128",
        )),
        Some("murmur3a" | "murmur3c" | "murmur3f") => Ok(()),
        Some("xxh3" | "xxh128" | "xxh32" | "xxh64") => Ok(()),
        _ if options.seed.is_some() || options.secret.is_some() => Err(value_error(
            name,
            "hash options are only supported for xxHash algorithms",
        )),
        _ => Ok(()),
    }
}

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
    hash_digest_bytes_with_options(name, algorithm, input, &HashOptions::default())
}

pub(in crate::builtins::modules) fn hash_digest_bytes_with_options(
    name: &str,
    algorithm: &str,
    input: &[u8],
    options: &HashOptions,
) -> Result<Vec<u8>, BuiltinError> {
    let normalized = normalized_hash_algorithm(algorithm);
    if let Some((bits, passes)) = normalized.as_deref().and_then(haval_variant) {
        return Ok(haval_digest(input, bits, passes));
    }

    match normalized.as_deref() {
        Some("md2") => Ok(Md2::digest(input).to_vec()),
        Some("md4") => Ok(Md4::digest(input).to_vec()),
        Some("md5") => Ok(Md5::digest(input).to_vec()),
        Some("sha1") => Ok(Sha1::digest(input).to_vec()),
        Some("sha224") => Ok(Sha224::digest(input).to_vec()),
        Some("sha256") => Ok(Sha256::digest(input).to_vec()),
        Some("sha384") => Ok(Sha384::digest(input).to_vec()),
        Some("sha512224") => Ok(Sha512_224::digest(input).to_vec()),
        Some("sha512256") => Ok(Sha512_256::digest(input).to_vec()),
        Some("sha512") => Ok(Sha512::digest(input).to_vec()),
        Some("sha3224") => Ok(Sha3_224::digest(input).to_vec()),
        Some("sha3256") => Ok(Sha3_256::digest(input).to_vec()),
        Some("sha3384") => Ok(Sha3_384::digest(input).to_vec()),
        Some("sha3512") => Ok(Sha3_512::digest(input).to_vec()),
        Some("ripemd128") => Ok(Ripemd128::digest(input).to_vec()),
        Some("ripemd160") => Ok(Ripemd160::digest(input).to_vec()),
        Some("ripemd256") => Ok(Ripemd256::digest(input).to_vec()),
        Some("ripemd320") => Ok(Ripemd320::digest(input).to_vec()),
        Some("whirlpool") => Ok(Whirlpool::digest(input).to_vec()),
        Some("tiger128,3") => Ok(tiger_digest(input, 16)),
        Some("tiger160,3") => Ok(tiger_digest(input, 20)),
        Some("tiger192,3") => Ok(tiger_digest(input, 24)),
        Some("tiger128,4") => Ok(tiger4_digest(input, 16)),
        Some("tiger160,4") => Ok(tiger4_digest(input, 20)),
        Some("tiger192,4") => Ok(tiger4_digest(input, 24)),
        Some("snefru" | "snefru256") => Ok(snefru_digest(input)),
        Some("gost") => Ok(<Gost94Test as GostDigest>::digest(input).to_vec()),
        Some("gostcrypto") => Ok(<Gost94CryptoPro as GostDigest>::digest(input).to_vec()),
        Some("adler32") => Ok(adler32(input).to_be_bytes().to_vec()),
        Some("crc32") => Ok(crc32_bzip2(input).to_le_bytes().to_vec()),
        Some("crc32b") => Ok(crc32fast::hash(input).to_be_bytes().to_vec()),
        Some("crc32c") => Ok(crc32c(input).to_be_bytes().to_vec()),
        Some("fnv132") => Ok(fnv1_32(input).to_be_bytes().to_vec()),
        Some("fnv1a32") => Ok(fnv1a_32(input).to_be_bytes().to_vec()),
        Some("fnv164") => Ok(fnv1_64(input).to_be_bytes().to_vec()),
        Some("fnv1a64") => Ok(fnv1a_64(input).to_be_bytes().to_vec()),
        Some("joaat") => Ok(joaat(input).to_be_bytes().to_vec()),
        Some("murmur3a") => Ok(murmur3_x86_32(input, options.seed.unwrap_or(0) as u32)
            .to_be_bytes()
            .to_vec()),
        Some("murmur3c") => Ok(murmur3_x86_128(input, options.seed.unwrap_or(0) as u32)
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect()),
        Some("murmur3f") => Ok(murmur3_x64_128(input, options.seed.unwrap_or(0) as u32)
            .into_iter()
            .flat_map(u64::to_be_bytes)
            .collect()),
        Some("xxh32") => Ok(xxh32(input, options.seed.unwrap_or(0) as u32)
            .to_be_bytes()
            .to_vec()),
        Some("xxh64") => Ok(xxh64(input, options.seed.unwrap_or(0))
            .to_be_bytes()
            .to_vec()),
        Some("xxh3") => {
            let digest = if let Some(secret) = &options.secret {
                xxh3_64_with_secret(input, secret)
            } else if let Some(seed) = options.seed {
                xxh3_64_with_seed(input, seed)
            } else {
                xxh3_64(input)
            };
            Ok(digest.to_be_bytes().to_vec())
        }
        Some("xxh128") => {
            let digest = if let Some(secret) = &options.secret {
                xxh3_128_with_secret(input, secret)
            } else if let Some(seed) = options.seed {
                xxh3_128_with_seed(input, seed)
            } else {
                xxh3_128(input)
            };
            Ok(digest.to_be_bytes().to_vec())
        }
        _ => Err(hash_algorithm_value_error(name)),
    }
}

pub(in crate::builtins::modules) fn hash_algorithm_value_error(name: &str) -> BuiltinError {
    argument_value_error(name, "#1 ($algo)", "must be a valid hashing algorithm")
}

pub(in crate::builtins::modules) fn hmac_hash_algorithm_value_error(name: &str) -> BuiltinError {
    argument_value_error(
        name,
        "#1 ($algo)",
        "must be a valid cryptographic hashing algorithm",
    )
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

fn crc32_bzip2(input: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in input {
        crc ^= u32::from(*byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    !crc
}

fn crc32c(input: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in input {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0x82f6_3b78
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn fnv1_32(input: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    for byte in input {
        hash = hash.wrapping_mul(0x0100_0193);
        hash ^= u32::from(*byte);
    }
    hash
}

fn fnv1a_32(input: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    for byte in input {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

fn fnv1_64(input: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in input {
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        hash ^= u64::from(*byte);
    }
    hash
}

fn fnv1a_64(input: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in input {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn joaat(input: &[u8]) -> u32 {
    let mut hash = 0_u32;
    for byte in input {
        hash = hash.wrapping_add(u32::from(*byte));
        hash = hash.wrapping_add(hash << 10);
        hash ^= hash >> 6;
    }
    hash = hash.wrapping_add(hash << 3);
    hash ^= hash >> 11;
    hash.wrapping_add(hash << 15)
}

fn tiger_digest(input: &[u8], length: usize) -> Vec<u8> {
    let mut digest = <Tiger as TigerDigest>::digest(input).to_vec();
    digest.truncate(length);
    digest
}

fn tiger4_digest(input: &[u8], length: usize) -> Vec<u8> {
    let mut digest = <Tiger4 as TigerDigest>::digest(input).to_vec();
    digest.truncate(length);
    digest
}

pub(in crate::builtins::modules) fn hmac_digest_bytes(
    name: &str,
    algorithm: &str,
    key: &[u8],
    input: &[u8],
) -> Result<Vec<u8>, BuiltinError> {
    let normalized = normalized_hash_algorithm(algorithm);
    if let Some((bits, passes)) = normalized.as_deref().and_then(haval_variant) {
        return Ok(hmac_with_block(
            if key.len() > 128 {
                haval_digest(key, bits, passes)
            } else {
                key.to_vec()
            },
            input,
            128,
            |bytes| haval_digest(bytes, bits, passes),
        ));
    }

    match normalized.as_deref() {
        Some("md2") => Ok(hmac_with_block(
            if key.len() > 16 {
                Md2::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            16,
            |bytes| Md2::digest(bytes).to_vec(),
        )),
        Some("md4") => Ok(hmac_with_block_64(
            if key.len() > 64 {
                Md4::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            |bytes| Md4::digest(bytes).to_vec(),
        )),
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
        Some("sha3224") => Ok(hmac_with_block(
            if key.len() > 144 {
                Sha3_224::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            144,
            |bytes| Sha3_224::digest(bytes).to_vec(),
        )),
        Some("sha3256") => Ok(hmac_with_block(
            if key.len() > 136 {
                Sha3_256::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            136,
            |bytes| Sha3_256::digest(bytes).to_vec(),
        )),
        Some("sha3384") => Ok(hmac_with_block(
            if key.len() > 104 {
                Sha3_384::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            104,
            |bytes| Sha3_384::digest(bytes).to_vec(),
        )),
        Some("sha3512") => Ok(hmac_with_block(
            if key.len() > 72 {
                Sha3_512::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            72,
            |bytes| Sha3_512::digest(bytes).to_vec(),
        )),
        Some("ripemd128") => Ok(hmac_with_block_64(
            if key.len() > 64 {
                Ripemd128::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            |bytes| Ripemd128::digest(bytes).to_vec(),
        )),
        Some("ripemd160") => Ok(hmac_with_block_64(
            if key.len() > 64 {
                Ripemd160::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            |bytes| Ripemd160::digest(bytes).to_vec(),
        )),
        Some("ripemd256") => Ok(hmac_with_block_64(
            if key.len() > 64 {
                Ripemd256::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            |bytes| Ripemd256::digest(bytes).to_vec(),
        )),
        Some("ripemd320") => Ok(hmac_with_block_64(
            if key.len() > 64 {
                Ripemd320::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            |bytes| Ripemd320::digest(bytes).to_vec(),
        )),
        Some("whirlpool") => Ok(hmac_with_block_64(
            if key.len() > 64 {
                Whirlpool::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            |bytes| Whirlpool::digest(bytes).to_vec(),
        )),
        Some("tiger128,3") => Ok(hmac_with_block_64(
            if key.len() > 64 {
                tiger_digest(key, 16)
            } else {
                key.to_vec()
            },
            input,
            |bytes| tiger_digest(bytes, 16),
        )),
        Some("tiger160,3") => Ok(hmac_with_block_64(
            if key.len() > 64 {
                tiger_digest(key, 20)
            } else {
                key.to_vec()
            },
            input,
            |bytes| tiger_digest(bytes, 20),
        )),
        Some("tiger192,3") => Ok(hmac_with_block_64(
            if key.len() > 64 {
                tiger_digest(key, 24)
            } else {
                key.to_vec()
            },
            input,
            |bytes| tiger_digest(bytes, 24),
        )),
        Some("tiger128,4") => Ok(hmac_with_block_64(
            if key.len() > 64 {
                tiger4_digest(key, 16)
            } else {
                key.to_vec()
            },
            input,
            |bytes| tiger4_digest(bytes, 16),
        )),
        Some("tiger160,4") => Ok(hmac_with_block_64(
            if key.len() > 64 {
                tiger4_digest(key, 20)
            } else {
                key.to_vec()
            },
            input,
            |bytes| tiger4_digest(bytes, 20),
        )),
        Some("tiger192,4") => Ok(hmac_with_block_64(
            if key.len() > 64 {
                tiger4_digest(key, 24)
            } else {
                key.to_vec()
            },
            input,
            |bytes| tiger4_digest(bytes, 24),
        )),
        Some("snefru" | "snefru256") => Ok(hmac_with_block(
            if key.len() > 32 {
                snefru_digest(key)
            } else {
                key.to_vec()
            },
            input,
            32,
            snefru_digest,
        )),
        Some("gost") => Ok(hmac_with_block(
            if key.len() > 32 {
                <Gost94Test as GostDigest>::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            32,
            |bytes| <Gost94Test as GostDigest>::digest(bytes).to_vec(),
        )),
        Some("gostcrypto") => Ok(hmac_with_block(
            if key.len() > 32 {
                <Gost94CryptoPro as GostDigest>::digest(key).to_vec()
            } else {
                key.to_vec()
            },
            input,
            32,
            |bytes| <Gost94CryptoPro as GostDigest>::digest(bytes).to_vec(),
        )),
        _ => Err(hmac_hash_algorithm_value_error(name)),
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
        "md2" | "md4" | "md5" | "sha1" | "adler32" | "crc32" | "crc32b" | "crc32c" => {
            Some(normalized)
        }
        "fnv132" | "fnv1a32" | "fnv164" | "fnv1a64" | "joaat" => Some(normalized),
        "murmur3a" | "murmur3c" | "murmur3f" => Some(normalized),
        "xxh32" | "xxh64" | "xxh3" | "xxh128" => Some(normalized),
        "sha224" | "sha256" | "sha384" | "sha512" => Some(normalized),
        "sha3224" | "sha3256" | "sha3384" | "sha3512" => Some(normalized),
        "ripemd128" | "ripemd160" | "ripemd256" | "ripemd320" => Some(normalized),
        "whirlpool" | "gost" | "gostcrypto" | "snefru" | "snefru256" => Some(normalized),
        "haval128,3" | "haval160,3" | "haval192,3" | "haval224,3" | "haval256,3" | "haval128,4"
        | "haval160,4" | "haval192,4" | "haval224,4" | "haval256,4" | "haval128,5"
        | "haval160,5" | "haval192,5" | "haval224,5" | "haval256,5" => Some(normalized),
        "tiger128,3" | "tiger160,3" | "tiger192,3" | "tiger128,4" | "tiger160,4" | "tiger192,4" => {
            Some(normalized)
        }
        "sha512/224" => Some("sha512224".to_owned()),
        "sha512/256" => Some("sha512256".to_owned()),
        "sha512224" | "sha512256" => Some(normalized),
        _ => None,
    }
}

fn haval_variant(algorithm: &str) -> Option<(usize, usize)> {
    match algorithm {
        "haval128,3" => Some((128, 3)),
        "haval160,3" => Some((160, 3)),
        "haval192,3" => Some((192, 3)),
        "haval224,3" => Some((224, 3)),
        "haval256,3" => Some((256, 3)),
        "haval128,4" => Some((128, 4)),
        "haval160,4" => Some((160, 4)),
        "haval192,4" => Some((192, 4)),
        "haval224,4" => Some((224, 4)),
        "haval256,4" => Some((256, 4)),
        "haval128,5" => Some((128, 5)),
        "haval160,5" => Some((160, 5)),
        "haval192,5" => Some((192, 5)),
        "haval224,5" => Some((224, 5)),
        "haval256,5" => Some((256, 5)),
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
const HTML_SPECIALCHARS: i64 = 0;
const HTML_ENTITIES: i64 = 1;
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
                if let Some(entity_len) = valid_html_entity_len(&bytes[index..], flags) {
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

pub(in crate::builtins::modules) fn htmlentities_escape_with_options(
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
                if let Some(entity_len) = valid_html_entity_len(&bytes[index..], flags) {
                    output.extend_from_slice(&bytes[index..index + entity_len]);
                    index += entity_len;
                    continue;
                }
                output.extend_from_slice(b"&amp;");
                index += 1;
                continue;
            }
            b'&' => {
                output.extend_from_slice(b"&amp;");
                index += 1;
                continue;
            }
            b'<' => {
                output.extend_from_slice(b"&lt;");
                index += 1;
                continue;
            }
            b'>' => {
                output.extend_from_slice(b"&gt;");
                index += 1;
                continue;
            }
            b'"' if flags & 2 != 0 => {
                output.extend_from_slice(b"&quot;");
                index += 1;
                continue;
            }
            b'\'' if flags & 1 != 0 => {
                output.extend_from_slice(html_translation_single_quote_entity(html_document_type(
                    flags,
                )));
                index += 1;
                continue;
            }
            _ => {}
        }

        if let Some(rest) = std::str::from_utf8(&bytes[index..]).ok()
            && let Some(character) = rest.chars().next()
            && character.len_utf8() > 1
            && let Some(entity) = html4_named_entity(character)
        {
            output.extend_from_slice(entity);
            index += character.len_utf8();
            continue;
        }

        output.push(byte);
        index += 1;
    }
    output
}

fn html4_named_entity(character: char) -> Option<&'static [u8]> {
    match character {
        '\u{00a0}' => Some(b"&nbsp;"),
        '\u{00a1}' => Some(b"&iexcl;"),
        '\u{00a2}' => Some(b"&cent;"),
        '\u{00a3}' => Some(b"&pound;"),
        '\u{00a4}' => Some(b"&curren;"),
        '\u{00a5}' => Some(b"&yen;"),
        '\u{00a6}' => Some(b"&brvbar;"),
        '\u{00a7}' => Some(b"&sect;"),
        '\u{00a8}' => Some(b"&uml;"),
        '\u{00a9}' => Some(b"&copy;"),
        '\u{00aa}' => Some(b"&ordf;"),
        '\u{00ab}' => Some(b"&laquo;"),
        '\u{00ac}' => Some(b"&not;"),
        '\u{00ad}' => Some(b"&shy;"),
        '\u{00ae}' => Some(b"&reg;"),
        '\u{00af}' => Some(b"&macr;"),
        '\u{00b0}' => Some(b"&deg;"),
        '\u{00b1}' => Some(b"&plusmn;"),
        '\u{00b2}' => Some(b"&sup2;"),
        '\u{00b3}' => Some(b"&sup3;"),
        '\u{00b4}' => Some(b"&acute;"),
        '\u{00b5}' => Some(b"&micro;"),
        '\u{00b6}' => Some(b"&para;"),
        '\u{00b7}' => Some(b"&middot;"),
        '\u{00b8}' => Some(b"&cedil;"),
        '\u{00b9}' => Some(b"&sup1;"),
        '\u{00ba}' => Some(b"&ordm;"),
        '\u{00bb}' => Some(b"&raquo;"),
        '\u{00bc}' => Some(b"&frac14;"),
        '\u{00bd}' => Some(b"&frac12;"),
        '\u{00be}' => Some(b"&frac34;"),
        '\u{00bf}' => Some(b"&iquest;"),
        '\u{00c0}' => Some(b"&Agrave;"),
        '\u{00c1}' => Some(b"&Aacute;"),
        '\u{00c2}' => Some(b"&Acirc;"),
        '\u{00c3}' => Some(b"&Atilde;"),
        '\u{00c4}' => Some(b"&Auml;"),
        '\u{00c5}' => Some(b"&Aring;"),
        '\u{00c6}' => Some(b"&AElig;"),
        '\u{00c7}' => Some(b"&Ccedil;"),
        '\u{00c8}' => Some(b"&Egrave;"),
        '\u{00c9}' => Some(b"&Eacute;"),
        '\u{00ca}' => Some(b"&Ecirc;"),
        '\u{00cb}' => Some(b"&Euml;"),
        '\u{00cc}' => Some(b"&Igrave;"),
        '\u{00cd}' => Some(b"&Iacute;"),
        '\u{00ce}' => Some(b"&Icirc;"),
        '\u{00cf}' => Some(b"&Iuml;"),
        '\u{00d0}' => Some(b"&ETH;"),
        '\u{00d1}' => Some(b"&Ntilde;"),
        '\u{00d2}' => Some(b"&Ograve;"),
        '\u{00d3}' => Some(b"&Oacute;"),
        '\u{00d4}' => Some(b"&Ocirc;"),
        '\u{00d5}' => Some(b"&Otilde;"),
        '\u{00d6}' => Some(b"&Ouml;"),
        '\u{00d7}' => Some(b"&times;"),
        '\u{00d8}' => Some(b"&Oslash;"),
        '\u{00d9}' => Some(b"&Ugrave;"),
        '\u{00da}' => Some(b"&Uacute;"),
        '\u{00db}' => Some(b"&Ucirc;"),
        '\u{00dc}' => Some(b"&Uuml;"),
        '\u{00dd}' => Some(b"&Yacute;"),
        '\u{00de}' => Some(b"&THORN;"),
        '\u{00df}' => Some(b"&szlig;"),
        '\u{00e0}' => Some(b"&agrave;"),
        '\u{00e1}' => Some(b"&aacute;"),
        '\u{00e2}' => Some(b"&acirc;"),
        '\u{00e3}' => Some(b"&atilde;"),
        '\u{00e4}' => Some(b"&auml;"),
        '\u{00e5}' => Some(b"&aring;"),
        '\u{00e6}' => Some(b"&aelig;"),
        '\u{00e7}' => Some(b"&ccedil;"),
        '\u{00e8}' => Some(b"&egrave;"),
        '\u{00e9}' => Some(b"&eacute;"),
        '\u{00ea}' => Some(b"&ecirc;"),
        '\u{00eb}' => Some(b"&euml;"),
        '\u{00ec}' => Some(b"&igrave;"),
        '\u{00ed}' => Some(b"&iacute;"),
        '\u{00ee}' => Some(b"&icirc;"),
        '\u{00ef}' => Some(b"&iuml;"),
        '\u{00f0}' => Some(b"&eth;"),
        '\u{00f1}' => Some(b"&ntilde;"),
        '\u{00f2}' => Some(b"&ograve;"),
        '\u{00f3}' => Some(b"&oacute;"),
        '\u{00f4}' => Some(b"&ocirc;"),
        '\u{00f5}' => Some(b"&otilde;"),
        '\u{00f6}' => Some(b"&ouml;"),
        '\u{00f7}' => Some(b"&divide;"),
        '\u{00f8}' => Some(b"&oslash;"),
        '\u{00f9}' => Some(b"&ugrave;"),
        '\u{00fa}' => Some(b"&uacute;"),
        '\u{00fb}' => Some(b"&ucirc;"),
        '\u{00fc}' => Some(b"&uuml;"),
        '\u{00fd}' => Some(b"&yacute;"),
        '\u{00fe}' => Some(b"&thorn;"),
        '\u{00ff}' => Some(b"&yuml;"),
        '\u{20ac}' => Some(b"&euro;"),
        _ => None,
    }
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

fn valid_html_entity_len(bytes: &[u8], flags: i64) -> Option<usize> {
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
    if matches!(entity, b"amp" | b"lt" | b"gt" | b"quot" | b"apos")
        || html_document_type(flags) != HtmlDocumentType::Xml1 && is_html4_named_entity(entity)
    {
        return Some(semicolon + 1);
    }
    None
}

fn is_html4_named_entity(name: &[u8]) -> bool {
    (0x00a0..=0x00ff)
        .chain(std::iter::once(0x20ac))
        .filter_map(char::from_u32)
        .filter_map(html4_named_entity)
        .any(|encoded| encoded.get(1..encoded.len() - 1) == Some(name))
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
        } else if flags & 1 != 0
            && (remaining.starts_with(b"&#039;")
                || remaining.starts_with(b"&#x27;")
                || (html_document_type(flags) != HtmlDocumentType::Html401
                    && remaining.starts_with(b"&apos;")))
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

pub(in crate::builtins::modules) fn html_translation_table(
    table: i64,
    flags: i64,
    encoding: Option<&PhpString>,
) -> PhpArray {
    let document_type = html_document_type(flags);
    if table == HTML_SPECIALCHARS || is_basic_only_entity_table(table, document_type, encoding) {
        return basic_html_translation_table(flags, document_type);
    }

    // Full HTML4/HTML5/XHTML translation tables require the generated entity
    // dataset; until that lands, expose the safe core subset instead.
    basic_html_translation_table(flags, document_type)
}

fn is_basic_only_entity_table(
    table: i64,
    document_type: HtmlDocumentType,
    encoding: Option<&PhpString>,
) -> bool {
    table == HTML_ENTITIES
        && (document_type == HtmlDocumentType::Xml1
            || (document_type == HtmlDocumentType::Html5
                && encoding.is_some_and(is_basic_only_html5_encoding)))
}

fn basic_html_translation_table(flags: i64, document_type: HtmlDocumentType) -> PhpArray {
    let mut entries = PhpArray::new();
    insert_translation_entry(&mut entries, b"&", b"&amp;");
    if flags & 1 != 0 {
        insert_translation_entry(
            &mut entries,
            b"'",
            html_translation_single_quote_entity(document_type),
        );
    }
    insert_translation_entry(&mut entries, b">", b"&gt;");
    insert_translation_entry(&mut entries, b"<", b"&lt;");
    if flags & 2 != 0 {
        insert_translation_entry(&mut entries, b"\"", b"&quot;");
    }
    entries
}

fn insert_translation_entry(entries: &mut PhpArray, character: &[u8], entity: &[u8]) {
    entries.insert(
        ArrayKey::String(PhpString::from_bytes(character.to_vec())),
        Value::string(entity.to_vec()),
    );
}

fn html_translation_single_quote_entity(document_type: HtmlDocumentType) -> &'static [u8] {
    match document_type {
        HtmlDocumentType::Html401 => b"&#039;",
        HtmlDocumentType::Xml1 | HtmlDocumentType::Xhtml | HtmlDocumentType::Html5 => b"&apos;",
    }
}

fn is_basic_only_html5_encoding(encoding: &PhpString) -> bool {
    let encoding = encoding.to_string_lossy();
    matches!(
        encoding.to_ascii_lowercase().as_str(),
        "sjis" | "shift_jis" | "shift-jis" | "cp932" | "windows-31j"
    )
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
        } else if flags & 1 != 0
            && (remaining.starts_with(b"&#039;")
                || remaining.starts_with(b"&#x27;")
                || (html_document_type(flags) != HtmlDocumentType::Html401
                    && remaining.starts_with(b"&apos;")))
        {
            Some((b'\'', 6))
        } else if remaining.starts_with(b"&#")
            && let Some((byte, len)) = decode_numeric_special_html_entity(remaining, flags)
        {
            Some((byte, len))
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

fn decode_numeric_special_html_entity(bytes: &[u8], flags: i64) -> Option<(u8, usize)> {
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

    let decoded = match codepoint {
        0x22 if flags & 2 != 0 => b'"',
        0x27 if flags & 1 != 0 => b'\'',
        0x26 => b'&',
        0x3c => b'<',
        0x3e => b'>',
        _ => return None,
    };
    Some((decoded, semicolon + 1))
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
    let mut seen_objects = HashSet::new();
    build_query_pairs_inner(
        prefix,
        numeric_prefix,
        raw_encoding,
        value,
        pairs,
        &mut seen_objects,
    )
}

fn build_query_pairs_inner(
    prefix: Option<String>,
    numeric_prefix: Option<&str>,
    raw_encoding: bool,
    value: &Value,
    pairs: &mut Vec<String>,
    seen_objects: &mut HashSet<u64>,
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
                build_query_pairs_inner(
                    Some(name),
                    numeric_prefix,
                    raw_encoding,
                    value,
                    pairs,
                    seen_objects,
                )?;
            }
        }
        Value::Object(object) => {
            if !seen_objects.insert(object.id()) {
                return Ok(());
            }
            for (key, value) in object.properties_snapshot() {
                if !http_build_query_object_property_is_public(&object, &key) {
                    continue;
                }
                let name = prefix
                    .as_ref()
                    .map_or(key.clone(), |prefix| format!("{prefix}[{key}]"));
                build_query_pairs_inner(
                    Some(name),
                    numeric_prefix,
                    raw_encoding,
                    &value,
                    pairs,
                    seen_objects,
                )?;
            }
            seen_objects.remove(&object.id());
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

fn http_build_query_object_property_is_public(
    object: &crate::ObjectRef,
    storage_name: &str,
) -> bool {
    let label = object.property_debug_label(storage_name);
    !(label.ends_with(":protected") || label.ends_with(":private"))
}
