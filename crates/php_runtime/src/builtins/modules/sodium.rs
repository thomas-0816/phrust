//! Real-crypto sodium MVP for common dependency probes.

#![allow(unsafe_code)]

use super::core::{arity_error, int_arg, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{ReferenceCell, Value};
use base64::{Engine, engine::general_purpose};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use libsodium_sys as sodium_sys;
use std::ffi::{CStr, CString};
use std::sync::OnceLock;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("sodium_add", builtin_sodium_add, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "sodium_bin2hex",
        builtin_sodium_bin2hex,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_generichash",
        builtin_sodium_crypto_generichash,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_generichash_keygen",
        builtin_sodium_crypto_generichash_keygen,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_aead_xchacha20poly1305_ietf_keygen",
        builtin_sodium_crypto_aead_xchacha20poly1305_ietf_keygen,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_aead_xchacha20poly1305_ietf_decrypt",
        builtin_sodium_crypto_aead_xchacha20poly1305_ietf_decrypt,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_aead_xchacha20poly1305_ietf_encrypt",
        builtin_sodium_crypto_aead_xchacha20poly1305_ietf_encrypt,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_auth_keygen",
        builtin_sodium_crypto_auth_keygen,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_box",
        builtin_sodium_crypto_box,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_box_keypair",
        builtin_sodium_crypto_box_keypair,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_box_keypair_from_secretkey_and_publickey",
        builtin_sodium_crypto_box_keypair_from_secretkey_and_publickey,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_box_open",
        builtin_sodium_crypto_box_open,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_box_publickey",
        builtin_sodium_crypto_box_publickey,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_box_publickey_from_secretkey",
        builtin_sodium_crypto_box_publickey_from_secretkey,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_box_seal",
        builtin_sodium_crypto_box_seal,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_box_seal_open",
        builtin_sodium_crypto_box_seal_open,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_box_secretkey",
        builtin_sodium_crypto_box_secretkey,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_box_seed_keypair",
        builtin_sodium_crypto_box_seed_keypair,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_kdf_keygen",
        builtin_sodium_crypto_kdf_keygen,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_kdf_derive_from_key",
        builtin_sodium_crypto_kdf_derive_from_key,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_pwhash",
        builtin_sodium_crypto_pwhash,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_pwhash_str",
        builtin_sodium_crypto_pwhash_str,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_pwhash_str_needs_rehash",
        builtin_sodium_crypto_pwhash_str_needs_rehash,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_pwhash_str_verify",
        builtin_sodium_crypto_pwhash_str_verify,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_pwhash_scryptsalsa208sha256",
        builtin_sodium_crypto_pwhash_scryptsalsa208sha256,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_pwhash_scryptsalsa208sha256_str",
        builtin_sodium_crypto_pwhash_scryptsalsa208sha256_str,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_pwhash_scryptsalsa208sha256_str_verify",
        builtin_sodium_crypto_pwhash_scryptsalsa208sha256_str_verify,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_secretbox",
        builtin_sodium_crypto_secretbox,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_secretbox_keygen",
        builtin_sodium_crypto_secretbox_keygen,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_secretbox_open",
        builtin_sodium_crypto_secretbox_open,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_shorthash_keygen",
        builtin_sodium_crypto_shorthash_keygen,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_sign_detached",
        builtin_sodium_crypto_sign_detached,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_crypto_sign_verify_detached",
        builtin_sodium_crypto_sign_verify_detached,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_hex2bin",
        builtin_sodium_hex2bin,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_increment",
        builtin_sodium_increment,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_memcmp",
        builtin_sodium_memcmp,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_memzero",
        builtin_sodium_memzero,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("sodium_pad", builtin_sodium_pad, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "sodium_unpad",
        builtin_sodium_unpad,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_compare",
        builtin_sodium_compare,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_base642bin",
        builtin_sodium_base642bin,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "sodium_bin2base64",
        builtin_sodium_bin2base64,
        BuiltinCompatibility::Php,
    ),
];

const SODIUM_CRYPTO_GENERICHASH_BYTES: usize = sodium_sys::crypto_generichash_BYTES as usize;
const SODIUM_CRYPTO_GENERICHASH_BYTES_MIN: usize =
    sodium_sys::crypto_generichash_BYTES_MIN as usize;
const SODIUM_CRYPTO_GENERICHASH_BYTES_MAX: usize =
    sodium_sys::crypto_generichash_BYTES_MAX as usize;
const SODIUM_CRYPTO_GENERICHASH_KEYBYTES: usize = sodium_sys::crypto_generichash_KEYBYTES as usize;
const SODIUM_CRYPTO_SIGN_BYTES: usize = sodium_sys::crypto_sign_BYTES as usize;
const SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES: usize = sodium_sys::crypto_sign_PUBLICKEYBYTES as usize;
const SODIUM_CRYPTO_SIGN_SECRETKEYBYTES: usize = sodium_sys::crypto_sign_SECRETKEYBYTES as usize;
const SODIUM_CRYPTO_BOX_PUBLICKEYBYTES: usize = sodium_sys::crypto_box_PUBLICKEYBYTES as usize;
const SODIUM_CRYPTO_BOX_SECRETKEYBYTES: usize = sodium_sys::crypto_box_SECRETKEYBYTES as usize;
const SODIUM_CRYPTO_BOX_KEYPAIRBYTES: usize =
    SODIUM_CRYPTO_BOX_SECRETKEYBYTES + SODIUM_CRYPTO_BOX_PUBLICKEYBYTES;
const SODIUM_BASE64_VARIANT_ORIGINAL: i64 = 1;
const SODIUM_BASE64_VARIANT_ORIGINAL_NO_PADDING: i64 = 3;
const SODIUM_BASE64_VARIANT_URLSAFE: i64 = 5;
const SODIUM_BASE64_VARIANT_URLSAFE_NO_PADDING: i64 = 7;

fn builtin_sodium_crypto_generichash(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error(
            "sodium_crypto_generichash",
            "one to three arguments",
        ));
    }
    let message = string_arg("sodium_crypto_generichash", &args[0])?;
    let key = args
        .get(1)
        .map(|value| string_arg("sodium_crypto_generichash", value))
        .transpose()?;
    let length = args
        .get(2)
        .map(|value| int_arg("sodium_crypto_generichash", value))
        .transpose()?
        .unwrap_or(SODIUM_CRYPTO_GENERICHASH_BYTES as i64);
    if !(SODIUM_CRYPTO_GENERICHASH_BYTES_MIN as i64..=SODIUM_CRYPTO_GENERICHASH_BYTES_MAX as i64)
        .contains(&length)
    {
        return Err(value_error(
            "sodium_crypto_generichash",
            "output length must be between 16 and 64 bytes",
        ));
    }
    let mut params = blake2b_simd::Params::new();
    params.hash_length(length as usize);
    if let Some(key) = key.as_ref()
        && !key.is_empty()
    {
        if key.len() > SODIUM_CRYPTO_GENERICHASH_BYTES_MAX {
            return Err(value_error(
                "sodium_crypto_generichash",
                "key length must be at most 64 bytes",
            ));
        }
        params.key(key.as_bytes());
    }
    Ok(Value::string(
        params.hash(message.as_bytes()).as_bytes().to_vec(),
    ))
}

fn builtin_sodium_crypto_generichash_keygen(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !args.is_empty() {
        return Err(arity_error(
            "sodium_crypto_generichash_keygen",
            "zero arguments",
        ));
    }
    let mut key = vec![0_u8; SODIUM_CRYPTO_GENERICHASH_KEYBYTES];
    sodium_keygen("sodium_crypto_generichash_keygen", &mut key, |ptr| unsafe {
        sodium_sys::crypto_generichash_keygen(ptr)
    })?;
    Ok(Value::string(key))
}

fn builtin_sodium_crypto_secretbox_keygen(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    sodium_no_arg_keygen(
        "sodium_crypto_secretbox_keygen",
        sodium_sys::crypto_secretbox_KEYBYTES as usize,
        |ptr| unsafe { sodium_sys::crypto_secretbox_keygen(ptr) },
        args,
    )
}

fn builtin_sodium_crypto_secretbox(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("sodium_crypto_secretbox", "three arguments"));
    }
    let message = string_arg("sodium_crypto_secretbox", &args[0])?;
    let nonce = string_arg("sodium_crypto_secretbox", &args[1])?;
    let key = string_arg("sodium_crypto_secretbox", &args[2])?;
    sodium_require_len(
        nonce.as_bytes(),
        sodium_sys::crypto_secretbox_NONCEBYTES as usize,
        "nonce",
        "SODIUM_CRYPTO_SECRETBOX_NONCEBYTES",
    )?;
    sodium_require_len(
        key.as_bytes(),
        sodium_sys::crypto_secretbox_KEYBYTES as usize,
        "key",
        "SODIUM_CRYPTO_SECRETBOX_KEYBYTES",
    )?;
    ensure_sodium_initialized()?;
    let mut output = vec![0_u8; message.len() + sodium_sys::crypto_secretbox_MACBYTES as usize];
    let result = unsafe {
        sodium_sys::crypto_secretbox_easy(
            output.as_mut_ptr(),
            message.as_bytes().as_ptr(),
            message.len() as u64,
            nonce.as_bytes().as_ptr(),
            key.as_bytes().as_ptr(),
        )
    };
    if result != 0 {
        return Err(sodium_exception("encryption failed"));
    }
    Ok(Value::string(output))
}

fn builtin_sodium_crypto_secretbox_open(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error(
            "sodium_crypto_secretbox_open",
            "three arguments",
        ));
    }
    let ciphertext = string_arg("sodium_crypto_secretbox_open", &args[0])?;
    let nonce = string_arg("sodium_crypto_secretbox_open", &args[1])?;
    let key = string_arg("sodium_crypto_secretbox_open", &args[2])?;
    sodium_require_len(
        nonce.as_bytes(),
        sodium_sys::crypto_secretbox_NONCEBYTES as usize,
        "nonce",
        "SODIUM_CRYPTO_SECRETBOX_NONCEBYTES",
    )?;
    sodium_require_len(
        key.as_bytes(),
        sodium_sys::crypto_secretbox_KEYBYTES as usize,
        "key",
        "SODIUM_CRYPTO_SECRETBOX_KEYBYTES",
    )?;
    let mac_len = sodium_sys::crypto_secretbox_MACBYTES as usize;
    if ciphertext.len() < mac_len {
        return Ok(Value::Bool(false));
    }
    ensure_sodium_initialized()?;
    let mut output = vec![0_u8; ciphertext.len() - mac_len];
    let result = unsafe {
        sodium_sys::crypto_secretbox_open_easy(
            output.as_mut_ptr(),
            ciphertext.as_bytes().as_ptr(),
            ciphertext.len() as u64,
            nonce.as_bytes().as_ptr(),
            key.as_bytes().as_ptr(),
        )
    };
    if result != 0 {
        return Ok(Value::Bool(false));
    }
    Ok(Value::string(output))
}

fn builtin_sodium_crypto_auth_keygen(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    sodium_no_arg_keygen(
        "sodium_crypto_auth_keygen",
        sodium_sys::crypto_auth_KEYBYTES as usize,
        |ptr| unsafe { sodium_sys::crypto_auth_keygen(ptr) },
        args,
    )
}

fn builtin_sodium_crypto_shorthash_keygen(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    sodium_no_arg_keygen(
        "sodium_crypto_shorthash_keygen",
        sodium_sys::crypto_shorthash_KEYBYTES as usize,
        |ptr| unsafe { sodium_sys::crypto_shorthash_keygen(ptr) },
        args,
    )
}

fn builtin_sodium_crypto_box_keypair(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !args.is_empty() {
        return Err(arity_error("sodium_crypto_box_keypair", "zero arguments"));
    }
    ensure_sodium_initialized()?;
    let mut secret_key = vec![0_u8; SODIUM_CRYPTO_BOX_SECRETKEYBYTES];
    let mut public_key = vec![0_u8; SODIUM_CRYPTO_BOX_PUBLICKEYBYTES];
    let result =
        unsafe { sodium_sys::crypto_box_keypair(public_key.as_mut_ptr(), secret_key.as_mut_ptr()) };
    if result != 0 {
        return Err(sodium_exception("keypair generation failed"));
    }
    Ok(Value::string(sodium_box_keypair(secret_key, public_key)))
}

fn builtin_sodium_crypto_box_seed_keypair(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error(
            "sodium_crypto_box_seed_keypair",
            "one argument",
        ));
    }
    let seed = string_arg("sodium_crypto_box_seed_keypair", &args[0])?;
    sodium_require_len(
        seed.as_bytes(),
        sodium_sys::crypto_box_SEEDBYTES as usize,
        "seed",
        "SODIUM_CRYPTO_BOX_SEEDBYTES",
    )?;
    ensure_sodium_initialized()?;
    let mut secret_key = vec![0_u8; SODIUM_CRYPTO_BOX_SECRETKEYBYTES];
    let mut public_key = vec![0_u8; SODIUM_CRYPTO_BOX_PUBLICKEYBYTES];
    let result = unsafe {
        sodium_sys::crypto_box_seed_keypair(
            public_key.as_mut_ptr(),
            secret_key.as_mut_ptr(),
            seed.as_bytes().as_ptr(),
        )
    };
    if result != 0 {
        return Err(sodium_exception("keypair generation failed"));
    }
    Ok(Value::string(sodium_box_keypair(secret_key, public_key)))
}

fn builtin_sodium_crypto_box_keypair_from_secretkey_and_publickey(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error(
            "sodium_crypto_box_keypair_from_secretkey_and_publickey",
            "two arguments",
        ));
    }
    let secret_key = string_arg(
        "sodium_crypto_box_keypair_from_secretkey_and_publickey",
        &args[0],
    )?;
    let public_key = string_arg(
        "sodium_crypto_box_keypair_from_secretkey_and_publickey",
        &args[1],
    )?;
    sodium_require_len(
        secret_key.as_bytes(),
        SODIUM_CRYPTO_BOX_SECRETKEYBYTES,
        "secret key",
        "SODIUM_CRYPTO_BOX_SECRETKEYBYTES",
    )?;
    sodium_require_len(
        public_key.as_bytes(),
        SODIUM_CRYPTO_BOX_PUBLICKEYBYTES,
        "public key",
        "SODIUM_CRYPTO_BOX_PUBLICKEYBYTES",
    )?;
    Ok(Value::string(sodium_box_keypair(
        secret_key.as_bytes().to_vec(),
        public_key.as_bytes().to_vec(),
    )))
}

fn builtin_sodium_crypto_box_secretkey(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("sodium_crypto_box_secretkey", "one argument"));
    }
    let keypair = string_arg("sodium_crypto_box_secretkey", &args[0])?;
    let (secret_key, _) = sodium_box_keypair_parts(keypair.as_bytes())?;
    Ok(Value::string(secret_key.to_vec()))
}

fn builtin_sodium_crypto_box_publickey(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("sodium_crypto_box_publickey", "one argument"));
    }
    let keypair = string_arg("sodium_crypto_box_publickey", &args[0])?;
    let (_, public_key) = sodium_box_keypair_parts(keypair.as_bytes())?;
    Ok(Value::string(public_key.to_vec()))
}

fn builtin_sodium_crypto_box_publickey_from_secretkey(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error(
            "sodium_crypto_box_publickey_from_secretkey",
            "one argument",
        ));
    }
    let secret_key = string_arg("sodium_crypto_box_publickey_from_secretkey", &args[0])?;
    sodium_require_len(
        secret_key.as_bytes(),
        SODIUM_CRYPTO_BOX_SECRETKEYBYTES,
        "secret key",
        "SODIUM_CRYPTO_BOX_SECRETKEYBYTES",
    )?;
    ensure_sodium_initialized()?;
    let mut public_key = vec![0_u8; SODIUM_CRYPTO_BOX_PUBLICKEYBYTES];
    let result = unsafe {
        sodium_sys::crypto_scalarmult_base(public_key.as_mut_ptr(), secret_key.as_bytes().as_ptr())
    };
    if result != 0 {
        return Err(sodium_exception("public key derivation failed"));
    }
    Ok(Value::string(public_key))
}

fn builtin_sodium_crypto_box(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("sodium_crypto_box", "three arguments"));
    }
    let message = string_arg("sodium_crypto_box", &args[0])?;
    let nonce = string_arg("sodium_crypto_box", &args[1])?;
    let keypair = string_arg("sodium_crypto_box", &args[2])?;
    sodium_require_len(
        nonce.as_bytes(),
        sodium_sys::crypto_box_NONCEBYTES as usize,
        "nonce",
        "SODIUM_CRYPTO_BOX_NONCEBYTES",
    )?;
    let (secret_key, public_key) = sodium_box_keypair_parts(keypair.as_bytes())?;
    ensure_sodium_initialized()?;
    let mut output = vec![0_u8; message.len() + sodium_sys::crypto_box_MACBYTES as usize];
    let result = unsafe {
        sodium_sys::crypto_box_easy(
            output.as_mut_ptr(),
            message.as_bytes().as_ptr(),
            message.len() as u64,
            nonce.as_bytes().as_ptr(),
            public_key.as_ptr(),
            secret_key.as_ptr(),
        )
    };
    if result != 0 {
        return Err(sodium_exception("encryption failed"));
    }
    Ok(Value::string(output))
}

fn builtin_sodium_crypto_box_open(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("sodium_crypto_box_open", "three arguments"));
    }
    let ciphertext = string_arg("sodium_crypto_box_open", &args[0])?;
    let nonce = string_arg("sodium_crypto_box_open", &args[1])?;
    let keypair = string_arg("sodium_crypto_box_open", &args[2])?;
    sodium_require_len(
        nonce.as_bytes(),
        sodium_sys::crypto_box_NONCEBYTES as usize,
        "nonce",
        "SODIUM_CRYPTO_BOX_NONCEBYTES",
    )?;
    let mac_len = sodium_sys::crypto_box_MACBYTES as usize;
    if ciphertext.len() < mac_len {
        return Ok(Value::Bool(false));
    }
    let (secret_key, public_key) = sodium_box_keypair_parts(keypair.as_bytes())?;
    ensure_sodium_initialized()?;
    let mut output = vec![0_u8; ciphertext.len() - mac_len];
    let result = unsafe {
        sodium_sys::crypto_box_open_easy(
            output.as_mut_ptr(),
            ciphertext.as_bytes().as_ptr(),
            ciphertext.len() as u64,
            nonce.as_bytes().as_ptr(),
            public_key.as_ptr(),
            secret_key.as_ptr(),
        )
    };
    if result != 0 {
        return Ok(Value::Bool(false));
    }
    Ok(Value::string(output))
}

fn builtin_sodium_crypto_box_seal(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("sodium_crypto_box_seal", "two arguments"));
    }
    let message = string_arg("sodium_crypto_box_seal", &args[0])?;
    let public_key = string_arg("sodium_crypto_box_seal", &args[1])?;
    sodium_require_len(
        public_key.as_bytes(),
        SODIUM_CRYPTO_BOX_PUBLICKEYBYTES,
        "public key",
        "SODIUM_CRYPTO_BOX_PUBLICKEYBYTES",
    )?;
    ensure_sodium_initialized()?;
    let mut output = vec![0_u8; message.len() + sodium_sys::crypto_box_SEALBYTES as usize];
    let result = unsafe {
        sodium_sys::crypto_box_seal(
            output.as_mut_ptr(),
            message.as_bytes().as_ptr(),
            message.len() as u64,
            public_key.as_bytes().as_ptr(),
        )
    };
    if result != 0 {
        return Err(sodium_exception("encryption failed"));
    }
    Ok(Value::string(output))
}

fn builtin_sodium_crypto_box_seal_open(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("sodium_crypto_box_seal_open", "two arguments"));
    }
    let ciphertext = string_arg("sodium_crypto_box_seal_open", &args[0])?;
    let keypair = string_arg("sodium_crypto_box_seal_open", &args[1])?;
    if ciphertext.len() < sodium_sys::crypto_box_SEALBYTES as usize {
        return Ok(Value::Bool(false));
    }
    let (secret_key, public_key) = sodium_box_keypair_parts(keypair.as_bytes())?;
    ensure_sodium_initialized()?;
    let mut output = vec![0_u8; ciphertext.len() - sodium_sys::crypto_box_SEALBYTES as usize];
    let result = unsafe {
        sodium_sys::crypto_box_seal_open(
            output.as_mut_ptr(),
            ciphertext.as_bytes().as_ptr(),
            ciphertext.len() as u64,
            public_key.as_ptr(),
            secret_key.as_ptr(),
        )
    };
    if result != 0 {
        return Ok(Value::Bool(false));
    }
    Ok(Value::string(output))
}

fn builtin_sodium_crypto_kdf_keygen(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    sodium_no_arg_keygen(
        "sodium_crypto_kdf_keygen",
        sodium_sys::crypto_kdf_KEYBYTES as usize,
        |ptr| unsafe { sodium_sys::crypto_kdf_keygen(ptr) },
        args,
    )
}

fn builtin_sodium_crypto_kdf_derive_from_key(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 4 {
        return Err(arity_error(
            "sodium_crypto_kdf_derive_from_key",
            "four arguments",
        ));
    }
    let length = int_arg("sodium_crypto_kdf_derive_from_key", &args[0])?;
    let subkey_id = int_arg("sodium_crypto_kdf_derive_from_key", &args[1])?;
    let context = string_arg("sodium_crypto_kdf_derive_from_key", &args[2])?;
    let key = string_arg("sodium_crypto_kdf_derive_from_key", &args[3])?;
    let min = sodium_sys::crypto_kdf_BYTES_MIN as i64;
    let max = sodium_sys::crypto_kdf_BYTES_MAX as i64;
    if !(min..=max).contains(&length) {
        return Err(sodium_exception(format!(
            "subkey length must be between {min} and {max} bytes"
        )));
    }
    if subkey_id < 0 {
        return Err(sodium_exception(
            "subkey_id must be greater than or equal to 0",
        ));
    }
    sodium_require_len(
        context.as_bytes(),
        sodium_sys::crypto_kdf_CONTEXTBYTES as usize,
        "context",
        "SODIUM_CRYPTO_KDF_CONTEXTBYTES",
    )?;
    sodium_require_len(
        key.as_bytes(),
        sodium_sys::crypto_kdf_KEYBYTES as usize,
        "key",
        "SODIUM_CRYPTO_KDF_KEYBYTES",
    )?;
    ensure_sodium_initialized()?;
    let mut output = vec![0_u8; length as usize];
    let result = unsafe {
        sodium_sys::crypto_kdf_derive_from_key(
            output.as_mut_ptr(),
            output.len(),
            subkey_id as u64,
            context.as_bytes().as_ptr().cast(),
            key.as_bytes().as_ptr(),
        )
    };
    if result != 0 {
        return Err(sodium_exception("key derivation failed"));
    }
    Ok(Value::string(output))
}

fn builtin_sodium_crypto_aead_xchacha20poly1305_ietf_keygen(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    sodium_no_arg_keygen(
        "sodium_crypto_aead_xchacha20poly1305_ietf_keygen",
        sodium_sys::crypto_aead_xchacha20poly1305_ietf_KEYBYTES as usize,
        |ptr| unsafe { sodium_sys::crypto_aead_xchacha20poly1305_ietf_keygen(ptr) },
        args,
    )
}

fn builtin_sodium_crypto_aead_xchacha20poly1305_ietf_encrypt(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 4 {
        return Err(arity_error(
            "sodium_crypto_aead_xchacha20poly1305_ietf_encrypt",
            "four arguments",
        ));
    }
    let message = string_arg(
        "sodium_crypto_aead_xchacha20poly1305_ietf_encrypt",
        &args[0],
    )?;
    let ad = string_arg(
        "sodium_crypto_aead_xchacha20poly1305_ietf_encrypt",
        &args[1],
    )?;
    let nonce = string_arg(
        "sodium_crypto_aead_xchacha20poly1305_ietf_encrypt",
        &args[2],
    )?;
    let key = string_arg(
        "sodium_crypto_aead_xchacha20poly1305_ietf_encrypt",
        &args[3],
    )?;
    sodium_require_len(
        nonce.as_bytes(),
        sodium_sys::crypto_aead_xchacha20poly1305_ietf_NPUBBYTES as usize,
        "nonce",
        "SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_NPUBBYTES",
    )?;
    sodium_require_len(
        key.as_bytes(),
        sodium_sys::crypto_aead_xchacha20poly1305_ietf_KEYBYTES as usize,
        "key",
        "SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_KEYBYTES",
    )?;
    ensure_sodium_initialized()?;
    let mut output =
        vec![0_u8; message.len() + sodium_sys::crypto_aead_xchacha20poly1305_ietf_ABYTES as usize];
    let mut output_len = 0_u64;
    let result = unsafe {
        sodium_sys::crypto_aead_xchacha20poly1305_ietf_encrypt(
            output.as_mut_ptr(),
            &mut output_len,
            message.as_bytes().as_ptr(),
            message.len() as u64,
            ad.as_bytes().as_ptr(),
            ad.len() as u64,
            std::ptr::null(),
            nonce.as_bytes().as_ptr(),
            key.as_bytes().as_ptr(),
        )
    };
    if result != 0 {
        return Err(sodium_exception("encryption failed"));
    }
    output.truncate(output_len as usize);
    Ok(Value::string(output))
}

fn builtin_sodium_crypto_aead_xchacha20poly1305_ietf_decrypt(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 4 {
        return Err(arity_error(
            "sodium_crypto_aead_xchacha20poly1305_ietf_decrypt",
            "four arguments",
        ));
    }
    let ciphertext = string_arg(
        "sodium_crypto_aead_xchacha20poly1305_ietf_decrypt",
        &args[0],
    )?;
    let ad = string_arg(
        "sodium_crypto_aead_xchacha20poly1305_ietf_decrypt",
        &args[1],
    )?;
    let nonce = string_arg(
        "sodium_crypto_aead_xchacha20poly1305_ietf_decrypt",
        &args[2],
    )?;
    let key = string_arg(
        "sodium_crypto_aead_xchacha20poly1305_ietf_decrypt",
        &args[3],
    )?;
    sodium_require_len(
        nonce.as_bytes(),
        sodium_sys::crypto_aead_xchacha20poly1305_ietf_NPUBBYTES as usize,
        "nonce",
        "SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_NPUBBYTES",
    )?;
    sodium_require_len(
        key.as_bytes(),
        sodium_sys::crypto_aead_xchacha20poly1305_ietf_KEYBYTES as usize,
        "key",
        "SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_KEYBYTES",
    )?;
    let mac_len = sodium_sys::crypto_aead_xchacha20poly1305_ietf_ABYTES as usize;
    if ciphertext.len() < mac_len {
        return Ok(Value::Bool(false));
    }
    ensure_sodium_initialized()?;
    let mut output = vec![0_u8; ciphertext.len() - mac_len];
    let mut output_len = 0_u64;
    let result = unsafe {
        sodium_sys::crypto_aead_xchacha20poly1305_ietf_decrypt(
            output.as_mut_ptr(),
            &mut output_len,
            std::ptr::null_mut(),
            ciphertext.as_bytes().as_ptr(),
            ciphertext.len() as u64,
            ad.as_bytes().as_ptr(),
            ad.len() as u64,
            nonce.as_bytes().as_ptr(),
            key.as_bytes().as_ptr(),
        )
    };
    if result != 0 {
        return Ok(Value::Bool(false));
    }
    output.truncate(output_len as usize);
    Ok(Value::string(output))
}

fn builtin_sodium_crypto_pwhash(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 5 || args.len() > 6 {
        return Err(arity_error("sodium_crypto_pwhash", "five or six arguments"));
    }
    let length = int_arg("sodium_crypto_pwhash", &args[0])?;
    let password = string_arg("sodium_crypto_pwhash", &args[1])?;
    let salt = string_arg("sodium_crypto_pwhash", &args[2])?;
    let opslimit = int_arg("sodium_crypto_pwhash", &args[3])?;
    let memlimit = int_arg("sodium_crypto_pwhash", &args[4])?;
    let algo = args
        .get(5)
        .map(|value| int_arg("sodium_crypto_pwhash", value))
        .transpose()?
        .unwrap_or(sodium_sys::crypto_pwhash_ALG_DEFAULT as i64);
    let min = sodium_sys::crypto_pwhash_BYTES_MIN as i64;
    let max = unsafe { sodium_sys::crypto_pwhash_bytes_max() as i64 };
    if !(min..=max).contains(&length) {
        return Err(sodium_exception(format!(
            "output length must be between {min} and {max} bytes"
        )));
    }
    sodium_require_len(
        salt.as_bytes(),
        sodium_sys::crypto_pwhash_SALTBYTES as usize,
        "salt",
        "SODIUM_CRYPTO_PWHASH_SALTBYTES",
    )?;
    let opslimit = sodium_nonnegative_u64(opslimit, "opslimit")?;
    let memlimit = sodium_nonnegative_usize(memlimit, "memlimit")?;
    let algo = i32::try_from(algo).map_err(|_| sodium_exception("unsupported algorithm"))?;
    ensure_sodium_initialized()?;
    let mut output = vec![0_u8; length as usize];
    let result = unsafe {
        sodium_sys::crypto_pwhash(
            output.as_mut_ptr(),
            output.len() as u64,
            password.as_bytes().as_ptr().cast(),
            password.len() as u64,
            salt.as_bytes().as_ptr(),
            opslimit,
            memlimit,
            algo,
        )
    };
    if result != 0 {
        return Err(sodium_exception("password hashing failed"));
    }
    Ok(Value::string(output))
}

fn builtin_sodium_crypto_pwhash_str(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("sodium_crypto_pwhash_str", "three arguments"));
    }
    let password = string_arg("sodium_crypto_pwhash_str", &args[0])?;
    let opslimit =
        sodium_nonnegative_u64(int_arg("sodium_crypto_pwhash_str", &args[1])?, "opslimit")?;
    let memlimit =
        sodium_nonnegative_usize(int_arg("sodium_crypto_pwhash_str", &args[2])?, "memlimit")?;
    ensure_sodium_initialized()?;
    let mut output = vec![0_i8; sodium_sys::crypto_pwhash_STRBYTES as usize];
    let result = unsafe {
        sodium_sys::crypto_pwhash_str(
            output.as_mut_ptr(),
            password.as_bytes().as_ptr().cast(),
            password.len() as u64,
            opslimit,
            memlimit,
        )
    };
    if result != 0 {
        return Err(sodium_exception("password hashing failed"));
    }
    let hash = unsafe { CStr::from_ptr(output.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    Ok(Value::string(hash))
}

fn builtin_sodium_crypto_pwhash_str_verify(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error(
            "sodium_crypto_pwhash_str_verify",
            "two arguments",
        ));
    }
    let hash = string_arg("sodium_crypto_pwhash_str_verify", &args[0])?;
    let password = string_arg("sodium_crypto_pwhash_str_verify", &args[1])?;
    let Ok(hash) = CString::new(hash.as_bytes()) else {
        return Ok(Value::Bool(false));
    };
    ensure_sodium_initialized()?;
    let result = unsafe {
        sodium_sys::crypto_pwhash_str_verify(
            hash.as_ptr(),
            password.as_bytes().as_ptr().cast(),
            password.len() as u64,
        )
    };
    Ok(Value::Bool(result == 0))
}

fn builtin_sodium_crypto_pwhash_str_needs_rehash(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error(
            "sodium_crypto_pwhash_str_needs_rehash",
            "three arguments",
        ));
    }
    let hash = string_arg("sodium_crypto_pwhash_str_needs_rehash", &args[0])?;
    let opslimit = sodium_nonnegative_u64(
        int_arg("sodium_crypto_pwhash_str_needs_rehash", &args[1])?,
        "opslimit",
    )?;
    let memlimit = sodium_nonnegative_usize(
        int_arg("sodium_crypto_pwhash_str_needs_rehash", &args[2])?,
        "memlimit",
    )?;
    let Ok(hash) = CString::new(hash.as_bytes()) else {
        return Ok(Value::Bool(true));
    };
    ensure_sodium_initialized()?;
    let result =
        unsafe { sodium_sys::crypto_pwhash_str_needs_rehash(hash.as_ptr(), opslimit, memlimit) };
    Ok(Value::Bool(result != 0))
}

fn builtin_sodium_crypto_pwhash_scryptsalsa208sha256(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 5 {
        return Err(arity_error(
            "sodium_crypto_pwhash_scryptsalsa208sha256",
            "five arguments",
        ));
    }
    let length = int_arg("sodium_crypto_pwhash_scryptsalsa208sha256", &args[0])?;
    let password = string_arg("sodium_crypto_pwhash_scryptsalsa208sha256", &args[1])?;
    let salt = string_arg("sodium_crypto_pwhash_scryptsalsa208sha256", &args[2])?;
    let opslimit = int_arg("sodium_crypto_pwhash_scryptsalsa208sha256", &args[3])?;
    let memlimit = int_arg("sodium_crypto_pwhash_scryptsalsa208sha256", &args[4])?;
    let min = sodium_sys::crypto_pwhash_scryptsalsa208sha256_BYTES_MIN as i64;
    let max = unsafe { sodium_sys::crypto_pwhash_scryptsalsa208sha256_bytes_max() as i64 };
    if !(min..=max).contains(&length) {
        return Err(sodium_exception(format!(
            "output length must be between {min} and {max} bytes"
        )));
    }
    sodium_require_len(
        salt.as_bytes(),
        sodium_sys::crypto_pwhash_scryptsalsa208sha256_SALTBYTES as usize,
        "salt",
        "SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_SALTBYTES",
    )?;
    let opslimit = sodium_nonnegative_u64(opslimit, "opslimit")?;
    let memlimit = sodium_nonnegative_usize(memlimit, "memlimit")?;
    ensure_sodium_initialized()?;
    let mut output = vec![0_u8; length as usize];
    let result = unsafe {
        sodium_sys::crypto_pwhash_scryptsalsa208sha256(
            output.as_mut_ptr(),
            output.len() as u64,
            password.as_bytes().as_ptr().cast(),
            password.len() as u64,
            salt.as_bytes().as_ptr(),
            opslimit,
            memlimit,
        )
    };
    if result != 0 {
        return Err(sodium_exception("password hashing failed"));
    }
    Ok(Value::string(output))
}

fn builtin_sodium_crypto_pwhash_scryptsalsa208sha256_str(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error(
            "sodium_crypto_pwhash_scryptsalsa208sha256_str",
            "three arguments",
        ));
    }
    let password = string_arg("sodium_crypto_pwhash_scryptsalsa208sha256_str", &args[0])?;
    let opslimit = sodium_nonnegative_u64(
        int_arg("sodium_crypto_pwhash_scryptsalsa208sha256_str", &args[1])?,
        "opslimit",
    )?;
    let memlimit = sodium_nonnegative_usize(
        int_arg("sodium_crypto_pwhash_scryptsalsa208sha256_str", &args[2])?,
        "memlimit",
    )?;
    ensure_sodium_initialized()?;
    let mut output = vec![0_i8; sodium_sys::crypto_pwhash_scryptsalsa208sha256_STRBYTES as usize];
    let result = unsafe {
        sodium_sys::crypto_pwhash_scryptsalsa208sha256_str(
            output.as_mut_ptr(),
            password.as_bytes().as_ptr().cast(),
            password.len() as u64,
            opslimit,
            memlimit,
        )
    };
    if result != 0 {
        return Err(sodium_exception("password hashing failed"));
    }
    let hash = unsafe { CStr::from_ptr(output.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    Ok(Value::string(hash))
}

fn builtin_sodium_crypto_pwhash_scryptsalsa208sha256_str_verify(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error(
            "sodium_crypto_pwhash_scryptsalsa208sha256_str_verify",
            "two arguments",
        ));
    }
    let hash = string_arg(
        "sodium_crypto_pwhash_scryptsalsa208sha256_str_verify",
        &args[0],
    )?;
    let password = string_arg(
        "sodium_crypto_pwhash_scryptsalsa208sha256_str_verify",
        &args[1],
    )?;
    let Ok(hash) = CString::new(hash.as_bytes()) else {
        return Ok(Value::Bool(false));
    };
    ensure_sodium_initialized()?;
    let result = unsafe {
        sodium_sys::crypto_pwhash_scryptsalsa208sha256_str_verify(
            hash.as_ptr(),
            password.as_bytes().as_ptr().cast(),
            password.len() as u64,
        )
    };
    Ok(Value::Bool(result == 0))
}

fn builtin_sodium_crypto_sign_verify_detached(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error(
            "sodium_crypto_sign_verify_detached",
            "three arguments",
        ));
    }
    let signature = string_arg("sodium_crypto_sign_verify_detached", &args[0])?;
    let message = string_arg("sodium_crypto_sign_verify_detached", &args[1])?;
    let public_key = string_arg("sodium_crypto_sign_verify_detached", &args[2])?;
    if signature.len() != SODIUM_CRYPTO_SIGN_BYTES
        || public_key.len() != SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES
    {
        return Ok(Value::Bool(false));
    }
    let Ok(verifying_key) = VerifyingKey::from_bytes(public_key.as_bytes().try_into().unwrap())
    else {
        return Ok(Value::Bool(false));
    };
    let Ok(signature) = Signature::from_slice(signature.as_bytes()) else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool(
        verifying_key.verify(message.as_bytes(), &signature).is_ok(),
    ))
}

fn builtin_sodium_crypto_sign_detached(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("sodium_crypto_sign_detached", "two arguments"));
    }
    let message = string_arg("sodium_crypto_sign_detached", &args[0])?;
    let secret_key = string_arg("sodium_crypto_sign_detached", &args[1])?;
    if secret_key.len() != SODIUM_CRYPTO_SIGN_SECRETKEYBYTES {
        return Err(value_error(
            "sodium_crypto_sign_detached",
            "secret key must be 64 bytes",
        ));
    }
    let seed: &[u8; 32] = secret_key.as_bytes()[..32].try_into().unwrap();
    let signing_key = SigningKey::from_bytes(seed);
    Ok(Value::string(
        signing_key.sign(message.as_bytes()).to_bytes().to_vec(),
    ))
}

fn builtin_sodium_bin2hex(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("sodium_bin2hex", "one argument"));
    }
    let input = string_arg("sodium_bin2hex", &args[0])?;
    Ok(Value::string(hex_encode(input.as_bytes())))
}

fn builtin_sodium_hex2bin(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("sodium_hex2bin", "one or two arguments"));
    }
    let input = string_arg("sodium_hex2bin", &args[0])?;
    let ignore = args
        .get(1)
        .map(|value| string_arg("sodium_hex2bin", value))
        .transpose()?;
    let bytes = input
        .as_bytes()
        .iter()
        .copied()
        .filter(|byte| {
            ignore
                .as_ref()
                .is_some_and(|ignore| ignore.as_bytes().contains(byte))
                || !byte.is_ascii_whitespace()
        })
        .filter(|byte| {
            !ignore
                .as_ref()
                .is_some_and(|ignore| ignore.as_bytes().contains(byte))
        })
        .collect::<Vec<_>>();
    hex_decode(&bytes)
        .map(Value::string)
        .ok_or_else(|| value_error("sodium_hex2bin", "input must be hexadecimal"))
}

fn builtin_sodium_bin2base64(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("sodium_bin2base64", "two arguments"));
    }
    let input = string_arg("sodium_bin2base64", &args[0])?;
    let variant = int_arg("sodium_bin2base64", &args[1])?;
    Ok(Value::string(
        base64_engine(variant)?.encode(input.as_bytes()),
    ))
}

fn builtin_sodium_base642bin(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error("sodium_base642bin", "two or three arguments"));
    }
    let input = string_arg("sodium_base642bin", &args[0])?;
    let variant = int_arg("sodium_base642bin", &args[1])?;
    let ignore = args
        .get(2)
        .map(|value| string_arg("sodium_base642bin", value))
        .transpose()?;
    let input_bytes = if let Some(ignore) = ignore.as_ref() {
        input
            .as_bytes()
            .iter()
            .copied()
            .filter(|byte| !ignore.as_bytes().contains(byte))
            .collect::<Vec<_>>()
    } else {
        input.as_bytes().to_vec()
    };
    base64_engine(variant)?
        .decode(input_bytes)
        .map(Value::string)
        .map_err(|_| value_error("sodium_base642bin", "input must be valid base64"))
}

fn builtin_sodium_memzero(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("sodium_memzero", "one argument"));
    }
    let (cell, mut bytes) = sodium_mutable_string_arg(&args[0], "a PHP string is required")?;
    ensure_sodium_initialized()?;
    unsafe {
        sodium_sys::sodium_memzero(bytes.as_mut_ptr().cast(), bytes.len());
    }
    if let Some(cell) = cell {
        cell.set(Value::Null);
    }
    Ok(Value::Null)
}

fn builtin_sodium_memcmp(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("sodium_memcmp", "two arguments"));
    }
    let left = sodium_strict_string_arg(&args[0], "both parameters must be strings")?;
    let right = sodium_strict_string_arg(&args[1], "both parameters must be strings")?;
    if left.len() != right.len() {
        return Err(sodium_exception("both strings must be of the same length"));
    }
    ensure_sodium_initialized()?;
    let result = unsafe {
        sodium_sys::sodium_memcmp(left.as_ptr().cast(), right.as_ptr().cast(), left.len())
    };
    Ok(Value::Int(i64::from(result)))
}

fn builtin_sodium_compare(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("sodium_compare", "two arguments"));
    }
    let left = sodium_strict_string_arg(&args[0], "both parameters must be strings")?;
    let right = sodium_strict_string_arg(&args[1], "both parameters must be strings")?;
    if left.len() != right.len() {
        return Err(sodium_exception("both strings must be of the same length"));
    }
    ensure_sodium_initialized()?;
    let result = unsafe { sodium_sys::sodium_compare(left.as_ptr(), right.as_ptr(), left.len()) };
    Ok(Value::Int(i64::from(result)))
}

fn builtin_sodium_increment(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("sodium_increment", "one argument"));
    }
    let (cell, mut bytes) = sodium_mutable_string_arg(&args[0], "a PHP string is required")?;
    ensure_sodium_initialized()?;
    unsafe {
        sodium_sys::sodium_increment(bytes.as_mut_ptr(), bytes.len());
    }
    if let Some(cell) = cell {
        cell.set(Value::string(bytes));
    }
    Ok(Value::Null)
}

fn builtin_sodium_add(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("sodium_add", "two arguments"));
    }
    let (cell, mut left) = sodium_mutable_string_arg(&args[0], "PHP strings are required")?;
    let right = sodium_strict_string_arg(&args[1], "PHP strings are required")?;
    if left.len() != right.len() {
        return Err(sodium_exception("both strings must be of the same length"));
    }
    ensure_sodium_initialized()?;
    unsafe {
        sodium_sys::sodium_add(left.as_mut_ptr(), right.as_ptr(), left.len());
    }
    if let Some(cell) = cell {
        cell.set(Value::string(left));
    }
    Ok(Value::Null)
}

fn builtin_sodium_pad(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("sodium_pad", "two arguments"));
    }
    let input = string_arg("sodium_pad", &args[0])?;
    let block_size = sodium_block_size("sodium_pad", &args[1])?;
    ensure_sodium_initialized()?;
    let mut padded = vec![0_u8; ((input.len() / block_size) + 1) * block_size];
    padded[..input.len()].copy_from_slice(input.as_bytes());
    let mut padded_len = 0_usize;
    let result = unsafe {
        sodium_sys::sodium_pad(
            &mut padded_len,
            padded.as_mut_ptr(),
            input.len(),
            block_size,
            padded.len(),
        )
    };
    if result != 0 {
        return Err(sodium_exception("padding failed"));
    }
    padded.truncate(padded_len);
    Ok(Value::string(padded))
}

fn builtin_sodium_unpad(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("sodium_unpad", "two arguments"));
    }
    let input = string_arg("sodium_unpad", &args[0])?;
    let block_size = sodium_block_size("sodium_unpad", &args[1])?;
    ensure_sodium_initialized()?;
    let mut unpadded_len = 0_usize;
    let result = unsafe {
        sodium_sys::sodium_unpad(
            &mut unpadded_len,
            input.as_bytes().as_ptr(),
            input.len(),
            block_size,
        )
    };
    if result != 0 {
        return Err(sodium_exception("invalid padding"));
    }
    Ok(Value::string(input.as_bytes()[..unpadded_len].to_vec()))
}

fn base64_engine(variant: i64) -> Result<&'static general_purpose::GeneralPurpose, BuiltinError> {
    match variant {
        SODIUM_BASE64_VARIANT_ORIGINAL => Ok(&general_purpose::STANDARD),
        SODIUM_BASE64_VARIANT_ORIGINAL_NO_PADDING => Ok(&general_purpose::STANDARD_NO_PAD),
        SODIUM_BASE64_VARIANT_URLSAFE => Ok(&general_purpose::URL_SAFE),
        SODIUM_BASE64_VARIANT_URLSAFE_NO_PADDING => Ok(&general_purpose::URL_SAFE_NO_PAD),
        _ => Err(value_error("sodium_base64", "unsupported base64 variant")),
    }
}

fn sodium_no_arg_keygen(
    name: &str,
    length: usize,
    keygen: impl FnOnce(*mut u8),
    args: crate::builtins::BuiltinArgs,
) -> BuiltinResult {
    if !args.is_empty() {
        return Err(arity_error(name, "zero arguments"));
    }
    let mut key = vec![0_u8; length];
    sodium_keygen(name, &mut key, keygen)?;
    Ok(Value::string(key))
}

fn sodium_keygen(name: &str, output: &mut [u8], keygen: impl FnOnce(*mut u8)) -> BuiltinResult {
    ensure_sodium_initialized()?;
    keygen(output.as_mut_ptr());
    if output.is_empty() {
        return Err(sodium_exception(format!("{name} failed")));
    }
    Ok(Value::Null)
}

fn sodium_mutable_string_arg(
    value: &Value,
    message: &'static str,
) -> Result<(Option<ReferenceCell>, Vec<u8>), BuiltinError> {
    match value {
        Value::String(string) => Ok((None, string.as_bytes().to_vec())),
        Value::Reference(cell) => match cell.get() {
            Value::String(string) => Ok((Some(cell.clone()), string.as_bytes().to_vec())),
            _ => Err(sodium_exception(message)),
        },
        _ => Err(sodium_exception(message)),
    }
}

fn sodium_strict_string_arg(value: &Value, message: &'static str) -> Result<Vec<u8>, BuiltinError> {
    match value {
        Value::String(string) => Ok(string.as_bytes().to_vec()),
        Value::Reference(cell) => match cell.get() {
            Value::String(string) => Ok(string.as_bytes().to_vec()),
            _ => Err(sodium_exception(message)),
        },
        _ => Err(sodium_exception(message)),
    }
}

fn sodium_block_size(name: &str, value: &Value) -> Result<usize, BuiltinError> {
    let block_size = int_arg(name, value)?;
    if block_size <= 0 {
        return Err(sodium_exception("block size must be greater than 0"));
    }
    usize::try_from(block_size).map_err(|_| sodium_exception("block size is too large"))
}

fn sodium_require_len(
    bytes: &[u8],
    expected: usize,
    label: &str,
    constant: &str,
) -> Result<(), BuiltinError> {
    if bytes.len() == expected {
        return Ok(());
    }
    Err(sodium_exception(format!(
        "{label} must be {constant} bytes long"
    )))
}

fn sodium_nonnegative_u64(value: i64, label: &str) -> Result<u64, BuiltinError> {
    u64::try_from(value).map_err(|_| sodium_exception(format!("{label} must be non-negative")))
}

fn sodium_nonnegative_usize(value: i64, label: &str) -> Result<usize, BuiltinError> {
    usize::try_from(value).map_err(|_| sodium_exception(format!("{label} must be non-negative")))
}

fn sodium_box_keypair(mut secret_key: Vec<u8>, public_key: Vec<u8>) -> Vec<u8> {
    secret_key.reserve(public_key.len());
    secret_key.extend_from_slice(&public_key);
    secret_key
}

fn sodium_box_keypair_parts(keypair: &[u8]) -> Result<(&[u8], &[u8]), BuiltinError> {
    sodium_require_len(
        keypair,
        SODIUM_CRYPTO_BOX_KEYPAIRBYTES,
        "keypair",
        "SODIUM_CRYPTO_BOX_KEYPAIRBYTES",
    )?;
    Ok(keypair.split_at(SODIUM_CRYPTO_BOX_SECRETKEYBYTES))
}

fn ensure_sodium_initialized() -> Result<(), BuiltinError> {
    static INIT: OnceLock<Result<(), String>> = OnceLock::new();
    match INIT.get_or_init(|| {
        let status = unsafe { sodium_sys::sodium_init() };
        if status < 0 {
            Err("libsodium initialization failed".to_owned())
        } else {
            Ok(())
        }
    }) {
        Ok(()) => Ok(()),
        Err(message) => Err(sodium_exception(message.clone())),
    }
}

#[allow(dead_code)]
fn sodium_version_string() -> Result<String, BuiltinError> {
    ensure_sodium_initialized()?;
    let ptr = unsafe { sodium_sys::sodium_version_string() };
    if ptr.is_null() {
        return Err(sodium_exception("libsodium version unavailable"));
    }
    Ok(unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned())
}

fn hex_encode(bytes: &[u8]) -> Vec<u8> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = Vec::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize]);
        output.push(HEX[(byte & 0x0f) as usize]);
    }
    output
}

fn hex_decode(bytes: &[u8]) -> Option<Vec<u8>> {
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let mut output = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        output.push(hex_value(pair[0])? << 4 | hex_value(pair[1])?);
    }
    Some(output)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn value_error(name: &str, message: impl Into<String>) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_VALUE",
        format!("{name}(): {}", message.into()),
    )
}

fn sodium_exception(message: impl Into<String>) -> BuiltinError {
    BuiltinError::new("E_PHP_RUNTIME_SODIUM_EXCEPTION", message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputBuffer;

    fn call(name: &str, args: crate::builtins::BuiltinArgs, context: &mut BuiltinContext<'_>) -> BuiltinResult {
        ENTRIES
            .iter()
            .find(|entry| entry.name() == name)
            .expect("sodium entry")
            .function()(context, args, RuntimeSourceSpan::default())
    }

    #[test]
    fn sodium_utils_mutate_references_through_libsodium() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);

        let value = ReferenceCell::new(Value::string(vec![
            0xff, 0xff, 0x80, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
        ]));
        call(
            "sodium_increment",
            vec![Value::Reference(value.clone())],
            &mut context,
        )
        .expect("increment succeeds");
        assert_eq!(
            value.get(),
            Value::string(vec![
                0x00, 0x00, 0x81, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
            ])
        );

        call(
            "sodium_add",
            vec![
                Value::Reference(value.clone()),
                Value::string(vec![
                    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0xfa, 0xfb, 0xfc,
                ]),
            ],
            &mut context,
        )
        .expect("add succeeds");
        assert_eq!(
            value.get(),
            Value::string(vec![
                0x01, 0x02, 0x84, 0x05, 0x07, 0x09, 0x0b, 0x0d, 0x00, 0x03, 0x05,
            ])
        );

        let secret = ReferenceCell::new(Value::string("abc"));
        call(
            "sodium_memzero",
            vec![Value::Reference(secret.clone())],
            &mut context,
        )
        .expect("memzero succeeds");
        assert_eq!(secret.get(), Value::Null);
    }

    #[test]
    fn sodium_keygens_and_padding_use_backend_constants() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);

        let Value::String(key) = call("sodium_crypto_secretbox_keygen", vec![], &mut context)
            .expect("secretbox keygen succeeds")
        else {
            panic!("expected key string");
        };
        assert_eq!(key.len(), sodium_sys::crypto_secretbox_KEYBYTES as usize);

        let Value::String(padded) = call(
            "sodium_pad",
            vec![Value::string("xyz"), Value::Int(16)],
            &mut context,
        )
        .expect("pad succeeds") else {
            panic!("expected padded string");
        };
        assert_eq!(
            padded.as_bytes(),
            &[b'x', b'y', b'z', 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        let unpadded = call(
            "sodium_unpad",
            vec![Value::String(padded), Value::Int(16)],
            &mut context,
        )
        .expect("unpad succeeds");
        assert_eq!(unpadded, Value::string("xyz"));
    }

    #[test]
    fn sodium_invalid_mutable_string_uses_sodium_exception_id() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let error = call("sodium_increment", vec![Value::Int(123)], &mut context)
            .expect_err("invalid value fails");
        assert_eq!(error.diagnostic_id(), "E_PHP_RUNTIME_SODIUM_EXCEPTION");
        assert_eq!(error.message(), "a PHP string is required");
    }
}
