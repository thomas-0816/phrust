//! OpenSSL-compatible helper builtin slice.

use super::core::{
    assign_reference_arg, deref_value, expect_arity, hex_encode, int_arg, read_file_value,
    string_arg, value_error,
};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{ArrayKey, PhpArray, PhpString, Value};
use ::openssl::hash::{MessageDigest, hash};
use ::openssl::pkey::{PKey, Private, Public};
use ::openssl::rsa::Rsa;
use ::openssl::sign::{Signer, Verifier};
use ::openssl::symm::{Cipher, Crypter, Mode};
use ::openssl::x509::{X509, X509NameRef};
use base64::{Engine, engine::general_purpose};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "openssl_cipher_iv_length",
        builtin_openssl_cipher_iv_length,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_cipher_key_length",
        builtin_openssl_cipher_key_length,
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
        "openssl_decrypt",
        builtin_openssl_decrypt,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_encrypt",
        builtin_openssl_encrypt,
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
        "openssl_pkey_get_private",
        builtin_openssl_pkey_get_private,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_get_privatekey",
        builtin_openssl_pkey_get_private,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_pkey_new",
        builtin_openssl_pkey_new,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_pkey_export",
        builtin_openssl_pkey_export,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_pkey_get_details",
        builtin_openssl_pkey_get_details,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_sign",
        builtin_openssl_sign,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_x509_read",
        builtin_openssl_x509_read,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_x509_parse",
        builtin_openssl_x509_parse,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_x509_check_private_key",
        builtin_openssl_x509_check_private_key,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "openssl_x509_verify",
        builtin_openssl_x509_verify,
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

const OPENSSL_MD_METHODS: &[&str] = &[
    "md5",
    "sha1",
    "sha224",
    "sha256",
    "sha384",
    "sha512",
    "ripemd160",
    "sha3-224",
    "sha3-256",
    "sha3-384",
    "sha3-512",
];
const OPENSSL_CIPHER_METHODS: &[&str] = &[
    "aes-128-cbc",
    "aes-192-cbc",
    "aes-256-cbc",
    "aes-128-ctr",
    "aes-192-ctr",
    "aes-256-ctr",
    "aes-128-gcm",
    "aes-192-gcm",
    "aes-256-gcm",
];
const OPENSSL_ALGO_MD5: i64 = 2;
const OPENSSL_ALGO_SHA1: i64 = 1;
const OPENSSL_ALGO_SHA224: i64 = 6;
const OPENSSL_ALGO_SHA256: i64 = 7;
const OPENSSL_ALGO_SHA384: i64 = 8;
const OPENSSL_ALGO_SHA512: i64 = 9;
const OPENSSL_RAW_DATA: i64 = 1;
const OPENSSL_ZERO_PADDING: i64 = 2;
const OPENSSL_DONT_ZERO_PAD_KEY: i64 = 4;
const OPENSSL_KEYTYPE_RSA: i64 = 0;

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
    assign_reference_arg(args.get(1), Value::Bool(true));
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
    Ok(
        match cipher_for_method(&method).and_then(|cipher| cipher.cipher.iv_len()) {
            Some(length) => Value::Int(length as i64),
            None => Value::Bool(false),
        },
    )
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

pub(in crate::builtins::modules) fn builtin_openssl_encrypt(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=8).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin openssl_encrypt expects three to eight argument(s)",
        ));
    }
    let data = string_arg("openssl_encrypt", &args[0])?;
    let method = string_arg("openssl_encrypt", &args[1])?.to_string_lossy();
    let passphrase = string_arg("openssl_encrypt", &args[2])?;
    let options = args
        .get(3)
        .map(|value| int_arg("openssl_encrypt", value))
        .transpose()?
        .unwrap_or(0);
    let iv = args
        .get(4)
        .map(|value| string_arg("openssl_encrypt", value))
        .transpose()?;
    let tag_argument = args.get(5);
    let aad = args
        .get(6)
        .map(|value| string_arg("openssl_encrypt", value))
        .transpose()?;
    let tag_length = args
        .get(7)
        .map(|value| int_arg("openssl_encrypt", value))
        .transpose()?
        .unwrap_or(16);
    let Some(cipher) = cipher_for_method(&method) else {
        queue_openssl_warning_and_error(
            context,
            "openssl_encrypt",
            "Unknown cipher algorithm",
            _span,
        );
        return Ok(Value::Bool(false));
    };
    if options & OPENSSL_DONT_ZERO_PAD_KEY != 0 && !cipher.aead {
        queue_openssl_warning_and_error(
            context,
            "openssl_encrypt",
            "Key length cannot be set for the cipher algorithm",
            _span,
        );
        return Ok(Value::Bool(false));
    }
    if cipher.aead && !(1..=16).contains(&tag_length) {
        queue_openssl_warning_and_error(
            context,
            "openssl_encrypt",
            "Tag length cannot be less than 1 or greater than 16",
            _span,
        );
        return Ok(Value::Bool(false));
    }
    let encrypted = match openssl_crypt(
        "openssl_encrypt",
        cipher,
        Mode::Encrypt,
        data.as_bytes(),
        passphrase.as_bytes(),
        iv.as_ref().map(|value| value.as_bytes()).unwrap_or(&[]),
        options & OPENSSL_ZERO_PADDING == 0,
        aad.as_ref().map(|value| value.as_bytes()).unwrap_or(&[]),
        None,
        tag_length as usize,
    ) {
        Ok(Some(encrypted)) => encrypted,
        Ok(None) => {
            queue_openssl_error(context, "openssl_encrypt", "Cipher operation failed");
            return Ok(Value::Bool(false));
        }
        Err(error) => {
            queue_openssl_error(context, "openssl_encrypt", error.message());
            return Ok(Value::Bool(false));
        }
    };
    if let Some(tag) = encrypted.tag {
        assign_reference_arg(tag_argument, Value::string(tag));
    } else if tag_argument.is_some() && !cipher.aead {
        assign_reference_arg(tag_argument, Value::Null);
    }
    Ok(if options & OPENSSL_RAW_DATA != 0 {
        Value::string(encrypted.output)
    } else {
        Value::string(general_purpose::STANDARD.encode(encrypted.output))
    })
}

pub(in crate::builtins::modules) fn builtin_openssl_decrypt(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=7).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin openssl_decrypt expects three to seven argument(s)",
        ));
    }
    let data = string_arg("openssl_decrypt", &args[0])?;
    let method = string_arg("openssl_decrypt", &args[1])?.to_string_lossy();
    let passphrase = string_arg("openssl_decrypt", &args[2])?;
    let options = args
        .get(3)
        .map(|value| int_arg("openssl_decrypt", value))
        .transpose()?
        .unwrap_or(0);
    let iv = args
        .get(4)
        .map(|value| string_arg("openssl_decrypt", value))
        .transpose()?;
    let tag = args
        .get(5)
        .map(|value| string_arg("openssl_decrypt", value))
        .transpose()?;
    let aad = args
        .get(6)
        .map(|value| string_arg("openssl_decrypt", value))
        .transpose()?;
    let Some(cipher) = cipher_for_method(&method) else {
        queue_openssl_warning_and_error(
            context,
            "openssl_decrypt",
            "Unknown cipher algorithm",
            _span,
        );
        return Ok(Value::Bool(false));
    };
    if options & OPENSSL_DONT_ZERO_PAD_KEY != 0 && !cipher.aead {
        queue_openssl_warning_and_error(
            context,
            "openssl_decrypt",
            "Key length cannot be set for the cipher algorithm",
            _span,
        );
        return Ok(Value::Bool(false));
    }
    if cipher.aead
        && tag
            .as_ref()
            .map(|value| value.as_bytes().is_empty())
            .unwrap_or(true)
    {
        queue_openssl_warning_and_error(
            context,
            "openssl_decrypt",
            "A tag is required when using AEAD cipher mode",
            _span,
        );
        return Ok(Value::Bool(false));
    }
    let input = if options & OPENSSL_RAW_DATA != 0 {
        data.as_bytes().to_vec()
    } else {
        match general_purpose::STANDARD.decode(data.as_bytes()) {
            Ok(decoded) => decoded,
            Err(_) => {
                queue_openssl_error(context, "openssl_decrypt", "Bad base64 input");
                return Ok(Value::Bool(false));
            }
        }
    };
    let decrypted = match openssl_crypt(
        "openssl_decrypt",
        cipher,
        Mode::Decrypt,
        &input,
        passphrase.as_bytes(),
        iv.as_ref().map(|value| value.as_bytes()).unwrap_or(&[]),
        options & OPENSSL_ZERO_PADDING == 0,
        aad.as_ref().map(|value| value.as_bytes()).unwrap_or(&[]),
        tag.as_ref().map(|value| value.as_bytes()),
        16,
    ) {
        Ok(Some(decrypted)) => decrypted,
        Ok(None) => {
            queue_openssl_error(context, "openssl_decrypt", "Bad decrypt");
            return Ok(Value::Bool(false));
        }
        Err(error) => {
            queue_openssl_error(context, "openssl_decrypt", error.message());
            return Ok(Value::Bool(false));
        }
    };
    Ok(Value::string(decrypted.output))
}

fn queue_openssl_error(context: &mut BuiltinContext<'_>, function: &str, message: impl AsRef<str>) {
    context.push_openssl_error(format!("{function}(): {}", message.as_ref()));
}

fn queue_openssl_warning_and_error(
    context: &mut BuiltinContext<'_>,
    function: &str,
    message: impl AsRef<str>,
    span: RuntimeSourceSpan,
) {
    let message = message.as_ref();
    context.php_warning(
        "E_PHP_RUNTIME_OPENSSL",
        format!("{function}(): {message}"),
        span,
    );
    queue_openssl_error(context, function, message);
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

#[derive(Clone, Copy)]
struct CipherInfo {
    cipher: Cipher,
    aead: bool,
}

fn cipher_for_method(method: &str) -> Option<CipherInfo> {
    let (cipher, aead) = match method.to_ascii_lowercase().as_str() {
        "aes-128-cbc" | "aes128" => (Cipher::aes_128_cbc(), false),
        "aes-192-cbc" => (Cipher::aes_192_cbc(), false),
        "aes-256-cbc" => (Cipher::aes_256_cbc(), false),
        "aes-128-ctr" => (Cipher::aes_128_ctr(), false),
        "aes-192-ctr" => (Cipher::aes_192_ctr(), false),
        "aes-256-ctr" => (Cipher::aes_256_ctr(), false),
        "aes-128-gcm" => (Cipher::aes_128_gcm(), true),
        "aes-192-gcm" => (Cipher::aes_192_gcm(), true),
        "aes-256-gcm" => (Cipher::aes_256_gcm(), true),
        _ => return None,
    };
    Some(CipherInfo { cipher, aead })
}

pub(in crate::builtins::modules) fn builtin_openssl_cipher_key_length(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("openssl_cipher_key_length", &args, 1)?;
    let method = string_arg("openssl_cipher_key_length", &args[0])?.to_string_lossy();
    Ok(match cipher_for_method(&method) {
        Some(cipher) => Value::Int(cipher.cipher.key_len() as i64),
        None => Value::Bool(false),
    })
}

struct CryptResult {
    output: Vec<u8>,
    tag: Option<Vec<u8>>,
}

#[allow(clippy::too_many_arguments)]
fn openssl_crypt(
    name: &str,
    cipher: CipherInfo,
    mode: Mode,
    input: &[u8],
    passphrase: &[u8],
    iv: &[u8],
    pkcs_padding: bool,
    aad: &[u8],
    tag: Option<&[u8]>,
    tag_length: usize,
) -> Result<Option<CryptResult>, BuiltinError> {
    let key = normalized_cipher_input(passphrase, cipher.cipher.key_len());
    let Some(iv_len) = cipher.cipher.iv_len() else {
        return Err(value_error(name, "cipher requires an IV length"));
    };
    let iv = normalized_cipher_input(iv, iv_len);
    let mut crypter = Crypter::new(cipher.cipher, mode, &key, Some(&iv)).map_err(|error| {
        BuiltinError::new(
            "E_PHP_RUNTIME_OPENSSL_CIPHER",
            format!("{name}(): failed to initialize cipher: {error}"),
        )
    })?;
    crypter.pad(pkcs_padding);
    if cipher.aead {
        if let Some(tag) = tag {
            crypter.set_tag(tag).map_err(|error| {
                BuiltinError::new(
                    "E_PHP_RUNTIME_OPENSSL_CIPHER",
                    format!("{name}(): failed to set authentication tag: {error}"),
                )
            })?;
        }
        if !aad.is_empty() {
            crypter.aad_update(aad).map_err(|error| {
                BuiltinError::new(
                    "E_PHP_RUNTIME_OPENSSL_CIPHER",
                    format!("{name}(): cipher AAD update failed: {error}"),
                )
            })?;
        }
    }
    let mut output = vec![0_u8; input.len() + cipher.cipher.block_size()];
    let mut count = crypter.update(input, &mut output).map_err(|error| {
        BuiltinError::new(
            "E_PHP_RUNTIME_OPENSSL_CIPHER",
            format!("{name}(): cipher update failed: {error}"),
        )
    })?;
    count += match crypter.finalize(&mut output[count..]) {
        Ok(count) => count,
        Err(_error) if matches!(mode, Mode::Decrypt) => {
            return Ok(None);
        }
        Err(error) => {
            return Err(BuiltinError::new(
                "E_PHP_RUNTIME_OPENSSL_CIPHER",
                format!("{name}(): cipher finalize failed: {error}"),
            ));
        }
    };
    output.truncate(count);
    let tag = if cipher.aead && matches!(mode, Mode::Encrypt) {
        let mut tag = vec![0_u8; tag_length];
        crypter.get_tag(&mut tag).map_err(|error| {
            BuiltinError::new(
                "E_PHP_RUNTIME_OPENSSL_CIPHER",
                format!("{name}(): failed to read authentication tag: {error}"),
            )
        })?;
        Some(tag)
    } else {
        None
    };
    Ok(Some(CryptResult { output, tag }))
}

fn normalized_cipher_input(input: &[u8], length: usize) -> Vec<u8> {
    let mut output = vec![0_u8; length];
    let count = input.len().min(length);
    output[..count].copy_from_slice(&input[..count]);
    output
}

pub(in crate::builtins::modules) fn builtin_openssl_verify(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=5).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin openssl_verify expects three to five argument(s)",
        ));
    }
    let data = string_arg("openssl_verify", &args[0])?;
    let signature = string_arg("openssl_verify", &args[1])?;
    let public_key = string_arg("openssl_verify", &args[2])?;
    let Some(digest) = message_digest_for_verify(context, args.get(3))? else {
        return Ok(Value::Int(-1));
    };
    if let Some(padding) = args.get(4) {
        let padding = int_arg("openssl_verify", padding)?;
        if padding != 0 {
            queue_openssl_error(
                context,
                "openssl_verify",
                "Signature padding modes are not implemented by this runtime",
            );
            return Ok(Value::Int(-1));
        }
    }
    let Some(public_key) =
        public_key_for_verify(context, public_key.to_string_lossy().as_ref(), span)?
    else {
        return Ok(Value::Bool(false));
    };
    let mut verifier = match Verifier::new(digest, &public_key) {
        Ok(verifier) => verifier,
        Err(error) => {
            queue_openssl_error(context, "openssl_verify", error.to_string());
            return Ok(Value::Int(-1));
        }
    };
    if let Err(error) = verifier.update(data.as_bytes()) {
        queue_openssl_error(context, "openssl_verify", error.to_string());
        return Ok(Value::Int(-1));
    }
    match verifier.verify(signature.as_bytes()) {
        Ok(true) => Ok(Value::Int(1)),
        Ok(false) => Ok(Value::Int(0)),
        Err(error) => {
            queue_openssl_error(context, "openssl_verify", error.to_string());
            Ok(Value::Int(-1))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_openssl_pkey_get_public(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("openssl_pkey_get_public", &args, 1)?;
    match public_key_pem_from_value(context, "openssl_pkey_get_public", &args[0], span)? {
        Some(public_key) => Ok(Value::string(public_key)),
        None => {
            queue_openssl_error(
                context,
                "openssl_pkey_get_public",
                "Unable to load public key",
            );
            Ok(Value::Bool(false))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_openssl_pkey_get_private(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin openssl_pkey_get_private expects one or two argument(s)",
        ));
    }
    match private_key_from_value(context, "openssl_pkey_get_private", &args[0], span)? {
        Some(private_key) => match private_key.private_key_to_pem_pkcs8() {
            Ok(pem) => Ok(Value::string(pem)),
            Err(error) => {
                queue_openssl_error(
                    context,
                    "openssl_pkey_get_private",
                    format!("Unable to export private key: {error}"),
                );
                Ok(Value::Bool(false))
            }
        },
        None => {
            queue_openssl_error(
                context,
                "openssl_pkey_get_private",
                "Unable to load private key",
            );
            Ok(Value::Bool(false))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_openssl_pkey_new(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin openssl_pkey_new expects zero or one argument(s)",
        ));
    }
    let bits = openssl_pkey_option_int(args.first(), "private_key_bits").unwrap_or(2048);
    let key_type =
        openssl_pkey_option_int(args.first(), "private_key_type").unwrap_or(OPENSSL_KEYTYPE_RSA);
    if key_type != OPENSSL_KEYTYPE_RSA {
        queue_openssl_error(
            context,
            "openssl_pkey_new",
            "Only RSA key generation is supported",
        );
        return Ok(Value::Bool(false));
    }
    if bits < 512 || bits > u32::MAX as i64 {
        queue_openssl_error(context, "openssl_pkey_new", "Invalid RSA key size");
        return Ok(Value::Bool(false));
    }
    let rsa = match Rsa::generate(bits as u32) {
        Ok(rsa) => rsa,
        Err(error) => {
            queue_openssl_error(
                context,
                "openssl_pkey_new",
                format!("RSA key generation failed: {error}"),
            );
            return Ok(Value::Bool(false));
        }
    };
    let private_key = match PKey::from_rsa(rsa) {
        Ok(private_key) => private_key,
        Err(error) => {
            queue_openssl_error(
                context,
                "openssl_pkey_new",
                format!("RSA key conversion failed: {error}"),
            );
            return Ok(Value::Bool(false));
        }
    };
    match private_key.private_key_to_pem_pkcs8() {
        Ok(pem) => Ok(Value::string(pem)),
        Err(error) => {
            queue_openssl_error(
                context,
                "openssl_pkey_new",
                format!("Unable to export generated private key: {error}"),
            );
            Ok(Value::Bool(false))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_openssl_pkey_export(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin openssl_pkey_export expects two to four argument(s)",
        ));
    }
    let Some(private_key) = private_key_from_value(context, "openssl_pkey_export", &args[0], span)?
    else {
        queue_openssl_error(
            context,
            "openssl_pkey_export",
            "Cannot get key from parameter 1",
        );
        return Ok(Value::Bool(false));
    };
    let passphrase = args
        .get(2)
        .map(|value| string_arg("openssl_pkey_export", value))
        .transpose()?;
    let exported = if let Some(passphrase) = passphrase.filter(|value| !value.as_bytes().is_empty())
    {
        private_key
            .private_key_to_pem_pkcs8_passphrase(Cipher::aes_128_cbc(), passphrase.as_bytes())
    } else {
        private_key.private_key_to_pem_pkcs8()
    };
    match exported {
        Ok(pem) => {
            assign_reference_arg(args.get(1), Value::string(pem));
            Ok(Value::Bool(true))
        }
        Err(error) => {
            queue_openssl_error(
                context,
                "openssl_pkey_export",
                format!("Unable to export private key: {error}"),
            );
            Ok(Value::Bool(false))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_openssl_pkey_get_details(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("openssl_pkey_get_details", &args, 1)?;
    let public_key =
        public_key_from_value(context, "openssl_pkey_get_details", &args[0], span.clone())?;
    let Some(public_key) = public_key else {
        queue_openssl_error(
            context,
            "openssl_pkey_get_details",
            "Cannot get key from parameter 1",
        );
        return Ok(Value::Bool(false));
    };
    let public_pem = match public_key.public_key_to_pem() {
        Ok(pem) => pem,
        Err(error) => {
            queue_openssl_error(
                context,
                "openssl_pkey_get_details",
                format!("Unable to export public key: {error}"),
            );
            return Ok(Value::Bool(false));
        }
    };
    let mut details = PhpArray::new();
    details.insert(
        ArrayKey::String(PhpString::from("bits")),
        Value::Int(public_key.bits() as i64),
    );
    details.insert(
        ArrayKey::String(PhpString::from("key")),
        Value::string(public_pem),
    );
    details.insert(
        ArrayKey::String(PhpString::from("type")),
        Value::Int(OPENSSL_KEYTYPE_RSA),
    );
    Ok(Value::Array(details))
}

pub(in crate::builtins::modules) fn builtin_openssl_sign(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=4).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin openssl_sign expects three or four argument(s)",
        ));
    }
    let data = string_arg("openssl_sign", &args[0])?;
    let Some(digest) = message_digest_for_function(context, "openssl_sign", args.get(3))? else {
        return Ok(Value::Bool(false));
    };
    let Some(private_key) = private_key_from_value(context, "openssl_sign", &args[2], span)? else {
        queue_openssl_error(
            context,
            "openssl_sign",
            "Supplied key param cannot be coerced into a private key",
        );
        return Ok(Value::Bool(false));
    };
    let mut signer = match Signer::new(digest, &private_key) {
        Ok(signer) => signer,
        Err(error) => {
            queue_openssl_error(context, "openssl_sign", error.to_string());
            return Ok(Value::Bool(false));
        }
    };
    if let Err(error) = signer.update(data.as_bytes()) {
        queue_openssl_error(context, "openssl_sign", error.to_string());
        return Ok(Value::Bool(false));
    }
    match signer.sign_to_vec() {
        Ok(signature) => {
            assign_reference_arg(args.get(1), Value::string(signature));
            Ok(Value::Bool(true))
        }
        Err(error) => {
            queue_openssl_error(context, "openssl_sign", error.to_string());
            Ok(Value::Bool(false))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_openssl_x509_read(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("openssl_x509_read", &args, 1)?;
    match x509_from_value(context, "openssl_x509_read", &args[0], span)? {
        Some(certificate) => match certificate.to_pem() {
            Ok(pem) => Ok(Value::string(pem)),
            Err(error) => {
                queue_openssl_error(
                    context,
                    "openssl_x509_read",
                    format!("Unable to export certificate: {error}"),
                );
                Ok(Value::Bool(false))
            }
        },
        None => {
            queue_openssl_error(context, "openssl_x509_read", "Unable to load certificate");
            Ok(Value::Bool(false))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_openssl_x509_parse(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin openssl_x509_parse expects one or two argument(s)",
        ));
    }
    let Some(certificate) = x509_from_value(context, "openssl_x509_parse", &args[0], span)? else {
        queue_openssl_error(context, "openssl_x509_parse", "Unable to load certificate");
        return Ok(Value::Bool(false));
    };
    let mut parsed = PhpArray::new();
    parsed.insert(
        ArrayKey::String(PhpString::from("subject")),
        Value::Array(x509_name_array(certificate.subject_name())),
    );
    parsed.insert(
        ArrayKey::String(PhpString::from("issuer")),
        Value::Array(x509_name_array(certificate.issuer_name())),
    );
    if let Ok(serial) = certificate
        .serial_number()
        .to_bn()
        .and_then(|bn| bn.to_hex_str())
    {
        parsed.insert(
            ArrayKey::String(PhpString::from("serialNumberHex")),
            Value::string(serial.to_string()),
        );
    }
    parsed.insert(
        ArrayKey::String(PhpString::from("validFrom")),
        Value::string(certificate.not_before().to_string()),
    );
    parsed.insert(
        ArrayKey::String(PhpString::from("validTo")),
        Value::string(certificate.not_after().to_string()),
    );
    Ok(Value::Array(parsed))
}

pub(in crate::builtins::modules) fn builtin_openssl_x509_check_private_key(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("openssl_x509_check_private_key", &args, 2)?;
    let Some(certificate) = x509_from_value(
        context,
        "openssl_x509_check_private_key",
        &args[0],
        span.clone(),
    )?
    else {
        queue_openssl_error(
            context,
            "openssl_x509_check_private_key",
            "Unable to load certificate",
        );
        return Ok(Value::Bool(false));
    };
    let Some(private_key) =
        private_key_from_value(context, "openssl_x509_check_private_key", &args[1], span)?
    else {
        queue_openssl_error(
            context,
            "openssl_x509_check_private_key",
            "Unable to load private key",
        );
        return Ok(Value::Bool(false));
    };
    let cert_public = certificate
        .public_key()
        .and_then(|key| key.public_key_to_pem());
    let private_public = private_key.public_key_to_pem();
    Ok(Value::Bool(matches!(
        (cert_public, private_public),
        (Ok(left), Ok(right)) if left == right
    )))
}

pub(in crate::builtins::modules) fn builtin_openssl_x509_verify(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("openssl_x509_verify", &args, 2)?;
    let Some(certificate) =
        x509_from_value(context, "openssl_x509_verify", &args[0], span.clone())?
    else {
        queue_openssl_error(context, "openssl_x509_verify", "Unable to load certificate");
        return Ok(Value::Int(-1));
    };
    let Some(public_key) = public_key_from_value(context, "openssl_x509_verify", &args[1], span)?
    else {
        queue_openssl_error(context, "openssl_x509_verify", "Unable to load public key");
        return Ok(Value::Int(-1));
    };
    match certificate.verify(&public_key) {
        Ok(true) => Ok(Value::Int(1)),
        Ok(false) => Ok(Value::Int(0)),
        Err(error) => {
            queue_openssl_error(context, "openssl_x509_verify", error.to_string());
            Ok(Value::Int(-1))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_openssl_error_string(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("openssl_error_string", &args, 0)?;
    Ok(context
        .pop_openssl_error()
        .map(Value::string)
        .unwrap_or(Value::Bool(false)))
}

fn digest_bytes(method: &str, data: &[u8]) -> Option<Vec<u8>> {
    hash(message_digest_for_name(method)?, data)
        .ok()
        .map(|digest| digest.to_vec())
}

fn message_digest_for_verify(
    context: &mut BuiltinContext<'_>,
    algorithm: Option<&Value>,
) -> Result<Option<MessageDigest>, BuiltinError> {
    message_digest_for_function(context, "openssl_verify", algorithm)
}

fn message_digest_for_function(
    context: &mut BuiltinContext<'_>,
    function: &str,
    algorithm: Option<&Value>,
) -> Result<Option<MessageDigest>, BuiltinError> {
    let digest = match algorithm.map(deref_value) {
        None => Some(MessageDigest::sha1()),
        Some(Value::Int(OPENSSL_ALGO_MD5)) => Some(MessageDigest::md5()),
        Some(Value::Int(OPENSSL_ALGO_SHA1)) => Some(MessageDigest::sha1()),
        Some(Value::Int(OPENSSL_ALGO_SHA224)) => Some(MessageDigest::sha224()),
        Some(Value::Int(OPENSSL_ALGO_SHA256)) => Some(MessageDigest::sha256()),
        Some(Value::Int(OPENSSL_ALGO_SHA384)) => Some(MessageDigest::sha384()),
        Some(Value::Int(OPENSSL_ALGO_SHA512)) => Some(MessageDigest::sha512()),
        Some(Value::Int(_)) => None,
        Some(value) => {
            let algorithm = string_arg(function, &value)?.to_string_lossy();
            message_digest_for_name(&algorithm)
        }
    };
    if digest.is_none() {
        queue_openssl_error(context, function, "Unknown digest algorithm");
    }
    Ok(digest)
}

fn message_digest_for_name(name: &str) -> Option<MessageDigest> {
    MessageDigest::from_name(name).or_else(|| {
        let normalized = name.to_ascii_lowercase().replace('-', "");
        MessageDigest::from_name(&normalized)
    })
}

fn public_key_for_verify(
    context: &mut BuiltinContext<'_>,
    key: &str,
    span: RuntimeSourceSpan,
) -> Result<Option<PKey<Public>>, BuiltinError> {
    let key_bytes = if let Some(path) = key.strip_prefix("file://") {
        match read_file_value(context, "openssl_verify", path, span.clone())? {
            Value::String(bytes) => bytes.as_bytes().to_vec(),
            _ => return Ok(None),
        }
    } else {
        key.as_bytes().to_vec()
    };
    if let Ok(public_key) = PKey::public_key_from_pem(&key_bytes) {
        return Ok(Some(public_key));
    }
    if let Ok(certificate) = X509::from_pem(&key_bytes)
        && let Ok(public_key) = certificate.public_key()
    {
        return Ok(Some(public_key));
    }
    context.php_warning(
        "E_PHP_RUNTIME_OPENSSL_KEY",
        "openssl_verify(): Supplied key param cannot be coerced into a public key",
        span,
    );
    Ok(None)
}

fn openssl_input_bytes(
    context: &mut BuiltinContext<'_>,
    function: &str,
    value: &Value,
    span: RuntimeSourceSpan,
) -> Result<Option<Vec<u8>>, BuiltinError> {
    let input = string_arg(function, value)?;
    let input = input.to_string_lossy();
    if let Some(path) = input.strip_prefix("file://") {
        return match read_file_value(context, function, path, span)? {
            Value::String(bytes) => Ok(Some(bytes.as_bytes().to_vec())),
            _ => Ok(None),
        };
    }
    Ok(Some(input.into_bytes()))
}

fn private_key_from_value(
    context: &mut BuiltinContext<'_>,
    function: &str,
    value: &Value,
    span: RuntimeSourceSpan,
) -> Result<Option<PKey<Private>>, BuiltinError> {
    let (key_value, passphrase) = match deref_value(value) {
        Value::Array(array) => {
            let key = array
                .get(&ArrayKey::Int(0))
                .cloned()
                .unwrap_or(Value::Bool(false));
            let passphrase = array
                .get(&ArrayKey::Int(1))
                .map(|value| string_arg(function, value))
                .transpose()?;
            (key, passphrase)
        }
        value => (value, None),
    };
    let Some(bytes) = openssl_input_bytes(context, function, &key_value, span)? else {
        return Ok(None);
    };
    if let Some(passphrase) = passphrase
        && let Ok(key) = PKey::private_key_from_pem_passphrase(&bytes, passphrase.as_bytes())
    {
        return Ok(Some(key));
    }
    Ok(PKey::private_key_from_pem(&bytes).ok())
}

fn public_key_from_value(
    context: &mut BuiltinContext<'_>,
    function: &str,
    value: &Value,
    span: RuntimeSourceSpan,
) -> Result<Option<PKey<Public>>, BuiltinError> {
    let Some(bytes) = openssl_input_bytes(context, function, value, span)? else {
        return Ok(None);
    };
    if let Ok(public_key) = PKey::public_key_from_pem(&bytes) {
        return Ok(Some(public_key));
    }
    if let Ok(private_key) = PKey::private_key_from_pem(&bytes)
        && let Ok(public_pem) = private_key.public_key_to_pem()
        && let Ok(public_key) = PKey::public_key_from_pem(&public_pem)
    {
        return Ok(Some(public_key));
    }
    if let Ok(certificate) = X509::from_pem(&bytes)
        && let Ok(public_key) = certificate.public_key()
    {
        return Ok(Some(public_key));
    }
    Ok(None)
}

fn public_key_pem_from_value(
    context: &mut BuiltinContext<'_>,
    function: &str,
    value: &Value,
    span: RuntimeSourceSpan,
) -> Result<Option<Vec<u8>>, BuiltinError> {
    Ok(public_key_from_value(context, function, value, span)?
        .and_then(|public_key| public_key.public_key_to_pem().ok()))
}

fn x509_from_value(
    context: &mut BuiltinContext<'_>,
    function: &str,
    value: &Value,
    span: RuntimeSourceSpan,
) -> Result<Option<X509>, BuiltinError> {
    let Some(bytes) = openssl_input_bytes(context, function, value, span)? else {
        return Ok(None);
    };
    Ok(X509::from_pem(&bytes)
        .ok()
        .filter(x509_has_rfc5280_time_encoding))
}

fn x509_has_rfc5280_time_encoding(certificate: &X509) -> bool {
    certificate
        .to_der()
        .is_ok_and(|der| der_has_rfc5280_time_encoding(&der))
}

fn der_has_rfc5280_time_encoding(der: &[u8]) -> bool {
    fn validate_items(mut input: &[u8]) -> Option<bool> {
        while !input.is_empty() {
            let tag = *input.first()?;
            input = &input[1..];
            let first_length = *input.first()?;
            input = &input[1..];
            let length = if first_length & 0x80 == 0 {
                usize::from(first_length)
            } else {
                let octets = usize::from(first_length & 0x7f);
                if octets == 0 || octets > std::mem::size_of::<usize>() || input.len() < octets {
                    return None;
                }
                let mut length = 0_usize;
                for byte in &input[..octets] {
                    length = length.checked_mul(256)?.checked_add(usize::from(*byte))?;
                }
                input = &input[octets..];
                length
            };
            let (value, rest) = input.split_at_checked(length)?;
            input = rest;

            match tag {
                // RFC 5280 requires seconds and a trailing Z in certificate
                // UTCTime and GeneralizedTime values.
                0x17 if value.len() != 13
                    || !value[..12].iter().all(u8::is_ascii_digit)
                    || value[12] != b'Z' =>
                {
                    return Some(false);
                }
                0x18 if value.len() != 15
                    || !value[..14].iter().all(u8::is_ascii_digit)
                    || value[14] != b'Z' =>
                {
                    return Some(false);
                }
                tag if tag & 0x20 != 0 && !validate_items(value)? => return Some(false),
                _ => {}
            }
        }
        Some(true)
    }

    validate_items(der).unwrap_or(false)
}

fn openssl_pkey_option_int(options: Option<&Value>, name: &str) -> Option<i64> {
    let Some(Value::Array(options)) = options.map(deref_value) else {
        return None;
    };
    options
        .get(&ArrayKey::String(PhpString::from(name)))
        .and_then(|value| int_arg("openssl_pkey_new", value).ok())
}

fn x509_name_array(name: &X509NameRef) -> PhpArray {
    let mut array = PhpArray::new();
    for entry in name.entries() {
        let Ok(short_name) = entry.object().nid().short_name() else {
            continue;
        };
        let Ok(data) = entry.data().to_string() else {
            continue;
        };
        array.insert(
            ArrayKey::String(PhpString::from(short_name)),
            Value::string(data),
        );
    }
    array
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OutputBuffer, ReferenceCell};
    use ::openssl::asn1::{Asn1Integer, Asn1Time};
    use ::openssl::bn::BigNum;
    use ::openssl::x509::{X509Builder, X509NameBuilder};

    #[test]
    fn openssl_digest_covers_common_hash_methods() {
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
    fn openssl_random_pseudo_bytes_sets_strong_result_reference() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let strong_result = ReferenceCell::new(Value::Null);

        let bytes = builtin_openssl_random_pseudo_bytes(
            &mut context,
            vec![Value::Int(8), Value::Reference(strong_result.clone())],
            RuntimeSourceSpan::default(),
        )
        .expect("random bytes");

        let Value::String(bytes) = bytes else {
            panic!("expected random byte string");
        };
        assert_eq!(bytes.as_bytes().len(), 8);
        assert_eq!(strong_result.get(), Value::Bool(true));
    }

    #[test]
    fn openssl_md_methods_and_verify_rsa_sha256() {
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
        let signature = general_purpose::STANDARD
            .decode(concat!(
                "HonyonljJhIXsVVzuSVTSJlOBAsBQpvkXx24d5jmyETYEBFSZBbcJkJJAq5fD1GX",
                "V+tcY3UEH0rt2+l9WPdTAFnykcfiEiRfyQ4VuS4pGDvuyRv/K0qIIv8XPfY4+jwef",
                "68g9gp+6GItQzCAeG67hVq/qVfC7tUmNsBkxlHo2kQ="
            ))
            .expect("base64 signature");
        let public_key = concat!(
            "-----BEGIN PUBLIC KEY-----\n",
            "MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDLXp6PkCtbpV+P1gwFQWH6Ez0U\n",
            "83uEmS8IGnpeI8Fk8rY/vHOZzZZaxRCw+loyc342qCDIQheMOCNm5Fkevz06q757\n",
            "/oooiLR3yryYGKiKG1IZIiplmtsC95oKrzUSKk60wuI1mbgpMUP5LKi/Tvxes5Pm\n",
            "kUtXfimz2qgkeUcPpQIDAQAB\n",
            "-----END PUBLIC KEY-----\n",
        );
        assert_eq!(
            builtin_openssl_verify(
                &mut context,
                vec![
                    Value::string("data"),
                    Value::string(signature.clone()),
                    Value::string(public_key),
                    Value::Int(OPENSSL_ALGO_SHA256),
                ],
                RuntimeSourceSpan::default(),
            )
            .expect("verify valid signature"),
            Value::Int(1)
        );
        assert_eq!(
            builtin_openssl_verify(
                &mut context,
                vec![
                    Value::string("wrong"),
                    Value::string(signature),
                    Value::string(public_key),
                    Value::Int(OPENSSL_ALGO_SHA256),
                ],
                RuntimeSourceSpan::default(),
            )
            .expect("verify invalid signature"),
            Value::Int(0)
        );
        assert_eq!(
            builtin_openssl_error_string(&mut context, vec![], RuntimeSourceSpan::default())
                .expect("drained queue"),
            Value::Bool(false)
        );
    }

    #[test]
    fn openssl_aes_cbc_encrypt_decrypt_roundtrips_raw_and_base64() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let args = vec![
            Value::string("secret"),
            Value::string("aes-128-cbc"),
            Value::string("0123456789abcdef"),
            Value::Int(0),
            Value::string("1234567890abcdef"),
        ];

        let encrypted = builtin_openssl_encrypt(&mut context, args, RuntimeSourceSpan::default())
            .expect("encrypt");
        assert_eq!(encrypted, Value::string("/romcUbbPYFPXuTCiUloyQ=="));
        assert_eq!(
            builtin_openssl_decrypt(
                &mut context,
                vec![
                    encrypted,
                    Value::string("aes-128-cbc"),
                    Value::string("0123456789abcdef"),
                    Value::Int(0),
                    Value::string("1234567890abcdef"),
                ],
                RuntimeSourceSpan::default(),
            )
            .expect("decrypt"),
            Value::string("secret")
        );
    }

    #[test]
    fn openssl_error_queue_drains_failed_cipher_operations() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);

        assert_eq!(
            builtin_openssl_error_string(&mut context, vec![], RuntimeSourceSpan::default())
                .expect("empty queue"),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_openssl_encrypt(
                &mut context,
                vec![
                    Value::string("secret"),
                    Value::string("unknown-cipher"),
                    Value::string("0123456789abcdef"),
                    Value::Int(0),
                    Value::string("1234567890abcdef"),
                ],
                RuntimeSourceSpan::default(),
            )
            .expect("unsupported cipher"),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_openssl_error_string(&mut context, vec![], RuntimeSourceSpan::default())
                .expect("cipher error"),
            Value::string("openssl_encrypt(): Unknown cipher algorithm")
        );
        assert_eq!(
            builtin_openssl_error_string(&mut context, vec![], RuntimeSourceSpan::default())
                .expect("drained queue"),
            Value::Bool(false)
        );
    }

    #[test]
    fn openssl_generated_key_exports_signs_and_verifies() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let mut options = PhpArray::new();
        options.insert(
            ArrayKey::String(PhpString::from("private_key_bits")),
            Value::Int(1024),
        );

        let private_key = builtin_openssl_pkey_new(
            &mut context,
            vec![Value::Array(options)],
            RuntimeSourceSpan::default(),
        )
        .expect("pkey_new");
        let Value::String(private_key_pem) = private_key.clone() else {
            panic!("expected generated private key PEM");
        };
        assert!(
            private_key_pem
                .to_string_lossy()
                .contains("BEGIN PRIVATE KEY")
        );

        let exported = ReferenceCell::new(Value::Null);
        assert_eq!(
            builtin_openssl_pkey_export(
                &mut context,
                vec![
                    private_key.clone(),
                    Value::Reference(exported.clone()),
                    Value::Null,
                ],
                RuntimeSourceSpan::default(),
            )
            .expect("pkey_export"),
            Value::Bool(true)
        );
        assert!(matches!(exported.get(), Value::String(_)));

        let public_key = builtin_openssl_pkey_get_public(
            &mut context,
            vec![private_key.clone()],
            RuntimeSourceSpan::default(),
        )
        .expect("pkey public");
        let details = builtin_openssl_pkey_get_details(
            &mut context,
            vec![private_key.clone()],
            RuntimeSourceSpan::default(),
        )
        .expect("pkey details");
        let Value::Array(details) = details else {
            panic!("expected key details");
        };
        assert_eq!(
            details.get(&ArrayKey::String(PhpString::from("type"))),
            Some(&Value::Int(OPENSSL_KEYTYPE_RSA))
        );

        let signature = ReferenceCell::new(Value::Null);
        assert_eq!(
            builtin_openssl_sign(
                &mut context,
                vec![
                    Value::string("payload"),
                    Value::Reference(signature.clone()),
                    private_key,
                    Value::Int(OPENSSL_ALGO_SHA256),
                ],
                RuntimeSourceSpan::default(),
            )
            .expect("sign"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_openssl_verify(
                &mut context,
                vec![
                    Value::string("payload"),
                    signature.get(),
                    public_key,
                    Value::Int(OPENSSL_ALGO_SHA256),
                ],
                RuntimeSourceSpan::default(),
            )
            .expect("verify generated signature"),
            Value::Int(1)
        );
    }

    #[test]
    fn openssl_aes_gcm_roundtrips_with_tag_and_aad() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let tag = ReferenceCell::new(Value::Null);
        let encrypted = builtin_openssl_encrypt(
            &mut context,
            vec![
                Value::string("secret"),
                Value::string("aes-128-gcm"),
                Value::string("0123456789abcdef"),
                Value::Int(OPENSSL_RAW_DATA),
                Value::string("123456789012"),
                Value::Reference(tag.clone()),
                Value::string("aad"),
                Value::Int(12),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("gcm encrypt");
        let Value::String(tag_bytes) = tag.get() else {
            panic!("expected gcm tag");
        };
        assert_eq!(tag_bytes.as_bytes().len(), 12);
        assert_eq!(
            builtin_openssl_decrypt(
                &mut context,
                vec![
                    encrypted,
                    Value::string("aes-128-gcm"),
                    Value::string("0123456789abcdef"),
                    Value::Int(OPENSSL_RAW_DATA),
                    Value::string("123456789012"),
                    Value::string(tag_bytes.as_bytes().to_vec()),
                    Value::string("aad"),
                ],
                RuntimeSourceSpan::default(),
            )
            .expect("gcm decrypt"),
            Value::string("secret")
        );
    }

    #[test]
    fn openssl_x509_reads_parses_checks_and_verifies_generated_cert() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let private_key = PKey::from_rsa(Rsa::generate(1024).expect("rsa")).expect("pkey");
        let mut name = X509NameBuilder::new().expect("name builder");
        name.append_entry_by_text("CN", "phrust.test")
            .expect("subject CN");
        let name = name.build();
        let mut builder = X509Builder::new().expect("cert builder");
        builder.set_version(2).expect("version");
        let serial_bn = BigNum::from_u32(1).expect("serial bn");
        let serial = Asn1Integer::from_bn(&serial_bn).expect("serial");
        builder.set_serial_number(&serial).expect("serial set");
        builder.set_subject_name(&name).expect("subject");
        builder.set_issuer_name(&name).expect("issuer");
        builder.set_pubkey(&private_key).expect("pubkey");
        let not_before = Asn1Time::days_from_now(0).expect("not before");
        let not_after = Asn1Time::days_from_now(1).expect("not after");
        builder.set_not_before(&not_before).expect("not before set");
        builder.set_not_after(&not_after).expect("not after set");
        builder
            .sign(&private_key, MessageDigest::sha256())
            .expect("sign cert");
        let certificate_pem = builder.build().to_pem().expect("cert pem");
        let private_key_pem = private_key
            .private_key_to_pem_pkcs8()
            .expect("private key pem");
        let certificate = Value::string(certificate_pem.clone());

        let read = builtin_openssl_x509_read(
            &mut context,
            vec![certificate.clone()],
            RuntimeSourceSpan::default(),
        )
        .expect("x509 read");
        assert!(matches!(read, Value::String(_)));
        let parsed = builtin_openssl_x509_parse(
            &mut context,
            vec![certificate.clone()],
            RuntimeSourceSpan::default(),
        )
        .expect("x509 parse");
        let Value::Array(parsed) = parsed else {
            panic!("expected parsed cert");
        };
        assert!(
            parsed
                .get(&ArrayKey::String(PhpString::from("subject")))
                .is_some()
        );
        assert_eq!(
            builtin_openssl_x509_check_private_key(
                &mut context,
                vec![certificate.clone(), Value::string(private_key_pem.clone())],
                RuntimeSourceSpan::default(),
            )
            .expect("x509 check private key"),
            Value::Bool(true)
        );
        let public_key = builtin_openssl_pkey_get_public(
            &mut context,
            vec![Value::string(private_key_pem)],
            RuntimeSourceSpan::default(),
        )
        .expect("public key");
        assert_eq!(
            builtin_openssl_x509_verify(
                &mut context,
                vec![Value::string(certificate_pem), public_key],
                RuntimeSourceSpan::default(),
            )
            .expect("x509 verify"),
            Value::Int(1)
        );
    }

    #[test]
    fn x509_time_encoding_requires_seconds() {
        assert!(der_has_rfc5280_time_encoding(
            b"\x30\x0f\x17\x0d140107000000Z"
        ));
        assert!(!der_has_rfc5280_time_encoding(
            b"\x30\x0d\x17\x0b1401070000Z"
        ));
        assert!(der_has_rfc5280_time_encoding(
            b"\x30\x11\x18\x0f20500107000000Z"
        ));
    }
}
