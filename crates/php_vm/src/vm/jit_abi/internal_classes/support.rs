use super::*;

pub(super) fn decode_arguments(
    context: &NativeExecutionContext<'_>,
    arguments: &[i64],
) -> Result<Vec<Value>, String> {
    arguments
        .iter()
        .map(|encoded| match context.decode(*encoded)? {
            Value::Reference(reference) => Ok(reference.get()),
            value => Ok(value),
        })
        .collect()
}

pub(super) fn expect_arity(
    name: &str,
    actual: usize,
    min: usize,
    max: usize,
) -> Result<(), String> {
    if (min..=max).contains(&actual) {
        return Ok(());
    }
    let expected = if min == max {
        min.to_string()
    } else {
        format!("{min} to {max}")
    };
    Err(format!(
        "E_PHP_VM_INTERNAL_CLASS_ARITY: {name} expects {expected} argument(s), {actual} given"
    ))
}

pub(super) fn string_argument(name: &str, value: Value) -> Result<String, String> {
    native_string(value)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|error| {
            format!("E_PHP_VM_INTERNAL_CLASS_TYPE_ERROR: {name} expects string: {error}")
        })
}

pub(super) fn int_argument(name: &str, value: &Value) -> Result<i64, String> {
    match value {
        Value::Int(value) => Ok(*value),
        Value::Float(value) => Ok(value.to_f64() as i64),
        Value::Bool(value) => Ok(i64::from(*value)),
        Value::Null => Ok(0),
        Value::String(value) => value.to_string_lossy().trim().parse::<i64>().map_err(|_| {
            format!(
                "E_PHP_VM_INTERNAL_CLASS_TYPE_ERROR: {name} expects an integer-compatible value"
            )
        }),
        Value::Reference(reference) => int_argument(name, &reference.get()),
        value => Err(format!(
            "E_PHP_VM_INTERNAL_CLASS_TYPE_ERROR: {name} expects int, got {}",
            native_value_type_name(value)
        )),
    }
}

pub(super) fn bool_argument(name: &str, value: &Value) -> Result<bool, String> {
    php_runtime::api::to_bool(value).map_err(|error| {
        format!("E_PHP_VM_INTERNAL_CLASS_TYPE_ERROR: {name} expects bool: {error}")
    })
}

pub(super) fn object_argument(
    name: &str,
    value: &Value,
) -> Result<php_runtime::api::ObjectRef, String> {
    match value {
        Value::Object(object) => Ok(object.clone()),
        Value::Reference(reference) => object_argument(name, &reference.get()),
        value => Err(format!(
            "E_PHP_VM_INTERNAL_CLASS_TYPE_ERROR: {name} expects object, got {}",
            native_value_type_name(value)
        )),
    }
}

pub(super) fn normalize_runtime_path(
    context: &NativeExecutionContext<'_>,
    path: &str,
) -> std::path::PathBuf {
    let raw = std::path::Path::new(path);
    let joined = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        context.cwd.join(raw)
    };
    let mut normalized = std::path::PathBuf::new();
    for component in joined.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => normalized.push(component.as_os_str()),
            std::path::Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

pub(super) fn read_runtime_file(
    context: &NativeExecutionContext<'_>,
    path: &str,
) -> Result<Option<String>, String> {
    let path = normalize_runtime_path(context, path);
    if !context
        .options
        .runtime_context
        .filesystem
        .allows_path(&path)
    {
        return Ok(None);
    }
    match std::fs::read_to_string(path) {
        Ok(source) => Ok(Some(source)),
        Err(_) => Ok(None),
    }
}
