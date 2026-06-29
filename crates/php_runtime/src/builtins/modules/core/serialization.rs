use super::*;

pub(in crate::builtins::modules) fn builtin_serialize(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("serialize", &args, 1)?;
    serialize_value(&args[0])
        .map(Value::String)
        .map_err(|error| serialization_error("serialize", error.message()))
}

pub(in crate::builtins::modules) fn builtin_setlocale(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() {
        return Err(arity_error("setlocale", "at least one argument"));
    }
    to_int(&args[0]).map_err(|message| conversion_error("setlocale", message))?;
    if args.len() == 1 {
        return Ok(Value::String(PhpString::from_test_str("C")));
    }
    for candidate in &args[1..] {
        let locale = to_string(candidate)
            .map_err(|message| conversion_error("setlocale", message))?
            .to_string_lossy();
        match locale.as_str() {
            "" | "0" | "C" | "POSIX" => return Ok(Value::String(PhpString::from_test_str("C"))),
            _ => {}
        }
    }
    Ok(Value::Bool(false))
}

pub(in crate::builtins::modules) fn builtin_unserialize(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin unserialize expects one or two argument(s)",
        ));
    }
    let Value::String(input) = &args[0] else {
        return Err(type_error("unserialize", "string", &args[0]));
    };
    match unserialize_value(input, UnserializeOptions::default()) {
        Ok(value) => Ok(value),
        Err(_) => {
            context.php_warning(
                "E_PHP_RUNTIME_UNSERIALIZE_OFFSET",
                format!(
                    "unserialize(): Error at offset 0 of {} bytes",
                    input.as_bytes().len()
                ),
                span,
            );
            Ok(Value::Bool(false))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_var_export(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            "builtin var_export expects one or two argument(s)",
        ));
    }
    let return_output = args
        .get(1)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("var_export", message))?
        .unwrap_or(false);
    let mut output = OutputBuffer::new();
    let serialize_precision = context
        .ini_get("serialize_precision")
        .and_then(|value| value.trim().parse::<i32>().ok())
        .unwrap_or(-1);
    let mut formatter = DebugFormatter::with_serialize_precision(serialize_precision);
    formatter.write_var_export_value(&mut output, &args[0], 0);
    if return_output {
        Ok(Value::string(output.into_bytes()))
    } else {
        context.output().write_bytes(output.as_bytes());
        Ok(Value::Null)
    }
}

pub(in crate::builtins::modules) fn serialization_error(name: &str, message: &str) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_SERIALIZATION_ERROR",
        format!("builtin {name} failed: {message}"),
    )
}
