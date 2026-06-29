use super::*;

pub(in crate::builtins::modules) fn builtin_header(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=3).contains(&args.len()) {
        return Err(arity_error("header", "one to three argument(s)"));
    }
    let header = string_arg("header", &args[0])?.to_string_lossy();
    let replace = args
        .get(1)
        .map_or(Ok(true), to_bool)
        .map_err(|message| conversion_error("header", message))?;
    let response_code = match args.get(2) {
        Some(Value::Null) | None => None,
        Some(value) => {
            let code = to_int(value).map_err(|message| conversion_error("header", message))?;
            if code == 0 {
                None
            } else if (100..=599).contains(&code) {
                Some(code as u16)
            } else {
                context.php_warning(
                    "E_PHP_RUNTIME_INVALID_HEADER",
                    format!("header(): invalid HTTP response code {code}"),
                    span,
                );
                return Ok(Value::Null);
            }
        }
    };
    if let Err(message) =
        context
            .http_response_mut()
            .add_header_line(&header, replace, response_code)
    {
        context.php_warning(
            "E_PHP_RUNTIME_INVALID_HEADER",
            format!("header(): {message}"),
            span,
        );
    }
    Ok(Value::Null)
}

pub(in crate::builtins::modules) fn builtin_headers_list(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("headers_list", &args, 0)?;
    Ok(Value::Array(PhpArray::from_packed(
        context
            .http_response()
            .headers_list()
            .into_iter()
            .map(Value::string)
            .collect(),
    )))
}

pub(in crate::builtins::modules) fn builtin_header_remove(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("header_remove", "zero or one argument(s)"));
    }
    let name = match args.first().map(deref_value) {
        None | Some(Value::Null) => None,
        Some(value) => Some(string_arg("header_remove", &value)?.to_string_lossy()),
    };
    if let Err(message) = context.http_response_mut().remove_header(name.as_deref()) {
        context.php_warning(
            "E_PHP_RUNTIME_INVALID_HEADER",
            format!("header_remove(): {message}"),
            span,
        );
    }
    Ok(Value::Null)
}

pub(in crate::builtins::modules) fn builtin_headers_sent(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 2 {
        return Err(arity_error("headers_sent", "zero to two argument(s)"));
    }
    Ok(Value::Bool(context.http_response().headers_sent))
}

pub(in crate::builtins::modules) fn builtin_memory_get_usage(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("memory_get_usage", "zero or one argument(s)"));
    }
    let output_bytes = context.output().len() as i64;
    Ok(Value::Int(output_bytes.max(0)))
}

pub(in crate::builtins::modules) fn builtin_memory_get_peak_usage(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    builtin_memory_get_usage(context, args, span)
}

pub(in crate::builtins::modules) fn builtin_setcookie(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    builtin_cookie(context, args, span, false)
}

pub(in crate::builtins::modules) fn builtin_setrawcookie(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    builtin_cookie(context, args, span, true)
}

fn builtin_cookie(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
    raw: bool,
) -> BuiltinResult {
    let function = if raw { "setrawcookie" } else { "setcookie" };
    if !(1..=7).contains(&args.len()) {
        return Err(arity_error(function, "one to seven argument(s)"));
    }
    let name = string_arg(function, &args[0])?.to_string_lossy();
    let value = args.get(1).map_or(Ok(String::new()), |value| {
        string_arg(function, value).map(|value| value.to_string_lossy())
    })?;
    let options = parse_cookie_options(function, &args)?;
    let Some(header_value) = build_cookie_header_value(&name, &value, &options, raw) else {
        context.php_warning(
            "E_PHP_RUNTIME_INVALID_COOKIE",
            format!("{function}(): invalid cookie name or value"),
            span,
        );
        return Ok(Value::Bool(false));
    };
    if let Err(message) = context.http_response_mut().add_header_line(
        &format!("Set-Cookie: {header_value}"),
        false,
        None,
    ) {
        context.php_warning(
            "E_PHP_RUNTIME_INVALID_COOKIE",
            format!("{function}(): {message}"),
            span,
        );
        return Ok(Value::Bool(false));
    }
    Ok(Value::Bool(true))
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct CookieOptions {
    expires: i64,
    path: String,
    domain: String,
    secure: bool,
    httponly: bool,
    samesite: String,
}

fn parse_cookie_options(function: &str, args: &[Value]) -> Result<CookieOptions, BuiltinError> {
    let mut options = CookieOptions::default();
    let Some(third) = args.get(2) else {
        return Ok(options);
    };
    if let Value::Array(array) = deref_value(third) {
        parse_cookie_options_array(function, &array, &mut options)?;
        return Ok(options);
    }
    options.expires = to_int(third).map_err(|message| conversion_error(function, message))?;
    options.path = args.get(3).map_or(Ok(String::new()), |value| {
        string_arg(function, value).map(|value| value.to_string_lossy())
    })?;
    options.domain = args.get(4).map_or(Ok(String::new()), |value| {
        string_arg(function, value).map(|value| value.to_string_lossy())
    })?;
    options.secure = args
        .get(5)
        .map_or(Ok(false), to_bool)
        .map_err(|message| conversion_error(function, message))?;
    options.httponly = args
        .get(6)
        .map_or(Ok(false), to_bool)
        .map_err(|message| conversion_error(function, message))?;
    Ok(options)
}

fn parse_cookie_options_array(
    function: &str,
    array: &PhpArray,
    options: &mut CookieOptions,
) -> Result<(), BuiltinError> {
    for (key, value) in array.iter() {
        let ArrayKey::String(key) = key else {
            continue;
        };
        match key.to_string_lossy().to_ascii_lowercase().as_str() {
            "expires" => {
                options.expires =
                    to_int(value).map_err(|message| conversion_error(function, message))?;
            }
            "path" => {
                options.path = string_arg(function, value)?.to_string_lossy();
            }
            "domain" => {
                options.domain = string_arg(function, value)?.to_string_lossy();
            }
            "secure" => {
                options.secure =
                    to_bool(value).map_err(|message| conversion_error(function, message))?;
            }
            "httponly" => {
                options.httponly =
                    to_bool(value).map_err(|message| conversion_error(function, message))?;
            }
            "samesite" => {
                options.samesite = string_arg(function, value)?.to_string_lossy();
            }
            _ => {}
        }
    }
    Ok(())
}

fn build_cookie_header_value(
    name: &str,
    value: &str,
    options: &CookieOptions,
    raw: bool,
) -> Option<String> {
    if !valid_cookie_name(name)
        || contains_response_splitting(value)
        || contains_response_splitting(&options.path)
        || contains_response_splitting(&options.domain)
        || contains_response_splitting(&options.samesite)
    {
        return None;
    }
    let encoded_value = if raw {
        if !valid_raw_cookie_value(value) {
            return None;
        }
        value.to_owned()
    } else {
        encode_cookie_value(value)
    };
    let mut header = format!("{name}={encoded_value}");
    if options.expires > 0 {
        header.push_str("; Expires=");
        header.push_str(&crate::datetime::format_timestamp(
            options.expires,
            "GMT",
            "D, d M Y H:i:s \\G\\M\\T",
        ));
    }
    append_cookie_attribute(&mut header, "Path", &options.path)?;
    append_cookie_attribute(&mut header, "Domain", &options.domain)?;
    if options.secure {
        header.push_str("; Secure");
    }
    if options.httponly {
        header.push_str("; HttpOnly");
    }
    append_cookie_attribute(&mut header, "SameSite", &options.samesite)?;
    Some(header)
}

fn append_cookie_attribute(header: &mut String, name: &str, value: &str) -> Option<()> {
    if value.is_empty() {
        return Some(());
    }
    if !valid_cookie_attribute_value(value) {
        return None;
    }
    header.push_str("; ");
    header.push_str(name);
    header.push('=');
    header.push_str(value);
    Some(())
}

fn valid_cookie_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(
            |byte| matches!(byte, 0x21 | 0x23..=0x2b | 0x2d..=0x3a | 0x3c..=0x5b | 0x5d..=0x7e),
        )
}

fn valid_raw_cookie_value(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| matches!(byte, 0x21 | 0x23..=0x2b | 0x2d..=0x3a | 0x3c..=0x5b | 0x5d..=0x7e))
}

fn valid_cookie_attribute_value(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| matches!(byte, 0x21..=0x3a | 0x3c..=0x7e))
}

fn contains_response_splitting(value: &str) -> bool {
    value.bytes().any(|byte| matches!(byte, b'\r' | b'\n'))
}

fn encode_cookie_value(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        if matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.') {
            output.push(char::from(byte));
        } else {
            output.push_str(&format!("%{byte:02X}"));
        }
    }
    output
}

pub(in crate::builtins::modules) fn builtin_http_response_code(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("http_response_code", "zero or one argument(s)"));
    }
    let previous = context.http_response().status_code;
    let Some(value) = args.first() else {
        return Ok(Value::Int(i64::from(previous)));
    };
    if matches!(value, Value::Null) {
        return Ok(Value::Int(i64::from(previous)));
    }
    let code = to_int(value).map_err(|message| conversion_error("http_response_code", message))?;
    if !(100..=599).contains(&code) {
        return Ok(Value::Bool(false));
    }
    context.http_response_mut().set_status_code(code as u16);
    Ok(Value::Int(i64::from(previous)))
}
