use super::*;
use bcrypt::{DEFAULT_COST, hash as bcrypt_hash, verify as bcrypt_verify};

pub(in crate::builtins::modules) fn builtin_password_hash(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("password_hash", "two or three argument(s)"));
    }
    let password = string_arg("password_hash", &args[0])?;
    validate_password_algorithm("password_hash", args.get(1))?;
    let cost = password_cost(args.get(2))?;
    let hash = bcrypt_hash(password.as_bytes(), cost).map_err(|error| {
        BuiltinError::new(
            "E_PHP_RUNTIME_PASSWORD_HASH",
            format!("password_hash(): failed to hash password: {error}"),
        )
    })?;
    Ok(Value::string(bcrypt_to_php_prefix(&hash)))
}

pub(in crate::builtins::modules) fn builtin_password_verify(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("password_verify", &args, 2)?;
    let password = string_arg("password_verify", &args[0])?;
    let hash = string_arg("password_verify", &args[1])?.to_string_lossy();
    let bcrypt_hash = php_to_bcrypt_prefix(&hash);
    let verified = bcrypt_verify(password.as_bytes(), &bcrypt_hash).unwrap_or(false);
    Ok(Value::Bool(verified))
}

pub(in crate::builtins::modules) fn builtin_password_needs_rehash(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error(
            "password_needs_rehash",
            "two or three argument(s)",
        ));
    }
    validate_password_algorithm("password_needs_rehash", args.get(1))?;
    let hash = string_arg("password_needs_rehash", &args[0])?.to_string_lossy();
    let expected_cost = password_cost(args.get(2))?;
    let Some(actual_cost) = bcrypt_cost_from_hash(&hash) else {
        return Ok(Value::Bool(true));
    };
    Ok(Value::Bool(
        !hash.starts_with("$2y$") || actual_cost != expected_cost,
    ))
}

fn validate_password_algorithm(function: &str, value: Option<&Value>) -> Result<(), BuiltinError> {
    match value.map(deref_value).unwrap_or(Value::Null) {
        Value::Null => Ok(()),
        Value::String(algorithm) => {
            let algorithm = algorithm.to_string_lossy();
            if matches!(algorithm.as_str(), "2y" | "bcrypt") {
                Ok(())
            } else {
                Err(value_error(
                    function,
                    "Argument #2 ($algo) must be a valid password hashing algorithm",
                ))
            }
        }
        Value::Int(1) => Ok(()),
        other => {
            let algorithm = to_string(&other)
                .map(|value| value.to_string_lossy())
                .unwrap_or_else(|_| String::new());
            if matches!(algorithm.as_str(), "2y" | "bcrypt") {
                Ok(())
            } else {
                Err(value_error(
                    function,
                    "Argument #2 ($algo) must be a valid password hashing algorithm",
                ))
            }
        }
    }
}

fn password_cost(options: Option<&Value>) -> Result<u32, BuiltinError> {
    let Some(Value::Array(options)) = options.map(deref_value) else {
        return Ok(DEFAULT_COST);
    };
    let cost = options
        .get(&string_array_key("cost"))
        .map_or(Ok(i64::from(DEFAULT_COST)), |value| {
            to_int(value).map_err(|message| conversion_error("password_hash", message))
        })?;
    if !(4..=31).contains(&cost) {
        return Err(value_error(
            "password_hash",
            "Invalid bcrypt cost parameter specified: cost must be in the range 04-31",
        ));
    }
    Ok(cost as u32)
}

fn bcrypt_to_php_prefix(hash: &str) -> String {
    hash.strip_prefix("$2b$")
        .map_or_else(|| hash.to_string(), |suffix| format!("$2y${suffix}"))
}

fn php_to_bcrypt_prefix(hash: &str) -> String {
    hash.strip_prefix("$2y$")
        .map_or_else(|| hash.to_string(), |suffix| format!("$2b${suffix}"))
}

fn bcrypt_cost_from_hash(hash: &str) -> Option<u32> {
    let normalized = php_to_bcrypt_prefix(hash);
    let mut parts = normalized.split('$');
    let _empty = parts.next()?;
    let version = parts.next()?;
    if !matches!(version, "2a" | "2b" | "2x" | "2y") {
        return None;
    }
    parts.next()?.parse::<u32>().ok()
}
