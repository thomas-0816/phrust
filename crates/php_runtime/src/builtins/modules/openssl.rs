//! OpenSSL-compatible helper builtin slice.

use super::core::{
    deref_value, expect_arity, hex_encode, int_arg, read_file_value, string_arg, value_error,
};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{ArrayKey, PhpArray, PhpString, Value};
use ::openssl::hash::{MessageDigest, hash};
use ::openssl::pkey::{PKey, Public};
use ::openssl::sign::Verifier;
use ::openssl::symm::{Cipher, Crypter, Mode};
use ::openssl::x509::X509;
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
const OPENSSL_ALGO_MD5: i64 = 2;
const OPENSSL_ALGO_SHA1: i64 = 1;
const OPENSSL_ALGO_SHA224: i64 = 6;
const OPENSSL_ALGO_SHA256: i64 = 7;
const OPENSSL_ALGO_SHA384: i64 = 8;
const OPENSSL_ALGO_SHA512: i64 = 9;
const OPENSSL_RAW_DATA: i64 = 1;
const OPENSSL_ZERO_PADDING: i64 = 2;
const OPENSSL_DONT_ZERO_PAD_KEY: i64 = 4;

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
    let Some(cipher) = cipher_for_method(&method) else {
        queue_openssl_warning_and_error(
            context,
            "openssl_encrypt",
            "Unknown cipher algorithm",
            _span,
        );
        return Ok(Value::Bool(false));
    };
    if options & OPENSSL_DONT_ZERO_PAD_KEY != 0 {
        queue_openssl_warning_and_error(
            context,
            "openssl_encrypt",
            "Key length cannot be set for the cipher algorithm",
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
    Ok(if options & OPENSSL_RAW_DATA != 0 {
        Value::string(encrypted)
    } else {
        Value::string(general_purpose::STANDARD.encode(encrypted))
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
    let Some(cipher) = cipher_for_method(&method) else {
        queue_openssl_warning_and_error(
            context,
            "openssl_decrypt",
            "Unknown cipher algorithm",
            _span,
        );
        return Ok(Value::Bool(false));
    };
    if options & OPENSSL_DONT_ZERO_PAD_KEY != 0 {
        queue_openssl_warning_and_error(
            context,
            "openssl_decrypt",
            "Key length cannot be set for the cipher algorithm",
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
    Ok(Value::string(decrypted))
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

fn cipher_for_method(method: &str) -> Option<Cipher> {
    match method.to_ascii_lowercase().as_str() {
        "aes-128-cbc" | "aes128" => Some(Cipher::aes_128_cbc()),
        "aes-256-cbc" => Some(Cipher::aes_256_cbc()),
        _ => None,
    }
}

pub(in crate::builtins::modules) fn builtin_openssl_cipher_key_length(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("openssl_cipher_key_length", &args, 1)?;
    let method = string_arg("openssl_cipher_key_length", &args[0])?.to_string_lossy();
    Ok(match cipher_for_method(&method) {
        Some(cipher) => Value::Int(cipher.key_len() as i64),
        None => Value::Bool(false),
    })
}

fn openssl_crypt(
    name: &str,
    cipher: Cipher,
    mode: Mode,
    input: &[u8],
    passphrase: &[u8],
    iv: &[u8],
    pkcs_padding: bool,
) -> Result<Option<Vec<u8>>, BuiltinError> {
    let key = normalized_cipher_input(passphrase, cipher.key_len());
    let Some(iv_len) = cipher.iv_len() else {
        return Err(value_error(name, "cipher requires an IV length"));
    };
    let iv = normalized_cipher_input(iv, iv_len);
    let mut crypter = Crypter::new(cipher, mode, &key, Some(&iv)).map_err(|error| {
        BuiltinError::new(
            "E_PHP_RUNTIME_OPENSSL_CIPHER",
            format!("{name}(): failed to initialize cipher: {error}"),
        )
    })?;
    crypter.pad(pkcs_padding);
    let mut output = vec![0_u8; input.len() + cipher.block_size()];
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
    Ok(Some(output))
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
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("openssl_pkey_get_public", &args, 1)?;
    let _key = string_arg("openssl_pkey_get_public", &args[0])?;
    queue_openssl_error(
        context,
        "openssl_pkey_get_public",
        "Unable to load public key",
    );
    Ok(Value::Bool(false))
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
            let algorithm = string_arg("openssl_verify", &value)?.to_string_lossy();
            message_digest_for_name(&algorithm)
        }
    };
    if digest.is_none() {
        queue_openssl_error(context, "openssl_verify", "Unknown digest algorithm");
    }
    Ok(digest)
}

fn message_digest_for_name(name: &str) -> Option<MessageDigest> {
    match name.to_ascii_lowercase().replace('-', "").as_str() {
        "md5" => Some(MessageDigest::md5()),
        "sha1" => Some(MessageDigest::sha1()),
        "sha224" => Some(MessageDigest::sha224()),
        "sha256" => Some(MessageDigest::sha256()),
        "sha384" => Some(MessageDigest::sha384()),
        "sha512" => Some(MessageDigest::sha512()),
        _ => None,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputBuffer;

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
}
