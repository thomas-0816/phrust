use super::*;
use std::sync::Arc;

pub(super) fn native_string(value: Value) -> Result<Vec<u8>, String> {
    match value {
        Value::String(value) => Ok(value.as_bytes().to_vec()),
        Value::Int(value) => Ok(value.to_string().into_bytes()),
        Value::Float(value) => Ok(value.to_f64().to_string().into_bytes()),
        Value::Bool(true) => Ok(b"1".to_vec()),
        Value::Bool(false) | Value::Null => Ok(Vec::new()),
        Value::Reference(reference) => native_string(reference.get()),
        other => Err(format!("native builtin expected string, got {other:?}")),
    }
}

fn native_dereference_value(mut value: Value) -> Value {
    // Native call metadata may wrap a value more than once while it crosses
    // foreach, method, and builtin boundaries. PHP references are transparent
    // to value-taking builtins, so peel the complete bounded chain here.
    for _ in 0..64 {
        match value {
            Value::Reference(reference) => value = reference.get(),
            value => return value,
        }
    }
    value
}

fn native_reference_is_visibly_aliased(
    context: &NativeExecutionContext<'_>,
    value: &Value,
) -> bool {
    let Value::Reference(reference) = value else {
        return false;
    };
    context
        .explicit_reference_ids
        .contains(&reference.gc_debug_id())
        // One owner lives in the inspected container, one is the iterator's
        // current clone, and native lowering may retain one dead register
        // copy until the frame is released. None is a PHP-visible alias.
        || reference.gc_refcount_estimate() > 3
}

fn prepare_native_sysvshm_serialization(
    context: &mut NativeExecutionContext<'_>,
    arguments: &mut [Value],
) -> Result<(), String> {
    let Some(Value::Object(object)) = arguments.get(2).cloned().map(native_dereference_value)
    else {
        return Ok(());
    };
    let class_name = object.class_name();
    let receiver = context.encode(Value::Object(object.clone()))?;
    let result = if let Some(function) =
        native_method_in_hierarchy(context, &class_name, "__serialize")
    {
        invoke_native_method(context, function, &[receiver])?
    } else if let Some((function, _)) = native_external_method(context, &class_name, "__serialize")
    {
        invoke_native_external_function(
            context,
            function,
            &[receiver],
            Some(class_name),
            context.unit.strict_types,
        )?
    } else {
        return Ok(());
    };
    let result = context.decode(result)?;
    let Value::Array(serialized) = result else {
        return Err(format!(
            "E_PHP_THROW:TypeError:{}::__serialize() must return an array",
            object.display_name()
        ));
    };

    let shared_memory_destroyed = arguments
        .first()
        .cloned()
        .map(native_dereference_value)
        .and_then(|value| match value {
            Value::Object(object) => Some(object.id()),
            _ => None,
        })
        .is_some_and(|object_id| {
            context
                .registered_extensions
                .sysvshm_object_destroyed(object_id)
        });
    if shared_memory_destroyed {
        return Err(
            "E_PHP_THROW:Error:Shared memory block has been destroyed by the serialization function"
                .to_owned(),
        );
    }

    let properties = serialized.iter().map(|(key, value)| {
        let name = match key {
            php_runtime::api::ArrayKey::Int(key) => key.to_string(),
            php_runtime::api::ArrayKey::String(key) => key.to_string_lossy(),
        };
        (name, value.clone())
    });
    arguments[2] = Value::Object(native_metadata_object(&object.display_name(), properties));
    Ok(())
}

fn native_var_dump(value: &Value, indent: usize, output: &mut Vec<u8>) {
    let prefix = " ".repeat(indent);
    match value {
        Value::Null => output.extend_from_slice(b"NULL\n"),
        Value::Bool(value) => {
            output.extend_from_slice(format!("bool({value})\n").as_bytes());
        }
        Value::Int(value) => output.extend_from_slice(format!("int({value})\n").as_bytes()),
        Value::Float(value) => {
            output.extend_from_slice(
                format!("float({})\n", native_php_float_label(value.to_f64())).as_bytes(),
            );
        }
        Value::String(value) => {
            output.extend_from_slice(format!("string({}) \"", value.len()).as_bytes());
            output.extend_from_slice(value.as_bytes());
            output.extend_from_slice(b"\"\n");
        }
        Value::Array(array) => {
            output.extend_from_slice(format!("array({}) {{\n", array.len()).as_bytes());
            for (key, value) in array.iter() {
                output.extend_from_slice(prefix.as_bytes());
                output.extend_from_slice(b"  [");
                match key {
                    php_runtime::api::ArrayKey::Int(key) => {
                        output.extend_from_slice(key.to_string().as_bytes());
                    }
                    php_runtime::api::ArrayKey::String(key) => {
                        output.push(b'\"');
                        output.extend_from_slice(key.as_bytes());
                        output.push(b'\"');
                    }
                }
                output.extend_from_slice(b"]=>\n");
                output.extend_from_slice(prefix.as_bytes());
                output.extend_from_slice(b"  ");
                native_var_dump(value, indent + 2, output);
            }
            output.extend_from_slice(prefix.as_bytes());
            output.extend_from_slice(b"}\n");
        }
        Value::Object(_) => output.extend_from_slice(b"object\n"),
        Value::Resource(resource) => output.extend_from_slice(
            format!(
                "resource({}) of type ({})\n",
                resource.id().get(),
                resource.resource_type()
            )
            .as_bytes(),
        ),
        Value::Uninitialized => output.extend_from_slice(b"NULL\n"),
        Value::Fiber(_) => output.extend_from_slice(b"object(Fiber)\n"),
        Value::Generator(_) => output.extend_from_slice(b"object(Generator)\n"),
        Value::Callable(_) => output.extend_from_slice(b"object(Closure)\n"),
        Value::Reference(reference) => native_var_dump(&reference.get(), indent, output),
    }
}

fn native_var_dump_with_context(
    context: &mut NativeExecutionContext<'_>,
    value: &Value,
    indent: usize,
    output: &mut Vec<u8>,
) -> Result<(), String> {
    if let Value::Callable(callable) = value
        && let php_runtime::api::CallableValue::Closure(closure) = callable.as_ref()
        && let Some(debug) = closure.debug.as_deref()
    {
        let mut static_values = php_runtime::api::PhpArray::new();
        for capture in &closure.captures {
            static_values.insert(
                php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                    capture.name.as_bytes().to_vec(),
                )),
                capture
                    .reference()
                    .map(|reference| reference.get())
                    .or_else(|| capture.value().cloned())
                    .unwrap_or(Value::Null),
            );
        }
        let mut parameters = php_runtime::api::PhpArray::new();
        for parameter in &debug.parameters {
            parameters.insert(
                php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                    format!("${}", parameter.name).into_bytes(),
                )),
                Value::String(PhpString::from_bytes(if parameter.required {
                    b"<required>".to_vec()
                } else {
                    b"<optional>".to_vec()
                })),
            );
        }
        let entries = [
            (
                "name",
                Value::String(PhpString::from_bytes(debug.name.as_bytes().to_vec())),
            ),
            (
                "file",
                Value::String(PhpString::from_bytes(debug.file.as_bytes().to_vec())),
            ),
            ("line", Value::Int(debug.line)),
            ("static", Value::Array(static_values)),
            ("parameter", Value::Array(parameters)),
        ];
        let prefix = " ".repeat(indent);
        output.extend_from_slice(
            format!("object(Closure)#{} ({}) {{\n", closure.id, entries.len()).as_bytes(),
        );
        for (name, value) in entries {
            output.extend_from_slice(prefix.as_bytes());
            output.extend_from_slice(format!("  [\"{name}\"]=>\n").as_bytes());
            output.extend_from_slice(prefix.as_bytes());
            output.extend_from_slice(b"  ");
            native_var_dump_with_context(context, &value, indent + 2, output)?;
        }
        output.extend_from_slice(prefix.as_bytes());
        output.extend_from_slice(b"}\n");
        return Ok(());
    }
    if let Value::Reference(reference) = value {
        return native_var_dump_with_context(context, &reference.get(), indent, output);
    }
    if let Value::Array(array) = value {
        let prefix = " ".repeat(indent);
        output.extend_from_slice(format!("array({}) {{\n", array.len()).as_bytes());
        for (key, value) in array.iter() {
            output.extend_from_slice(prefix.as_bytes());
            output.extend_from_slice(b"  [");
            match key {
                php_runtime::api::ArrayKey::Int(key) => {
                    output.extend_from_slice(key.to_string().as_bytes());
                }
                php_runtime::api::ArrayKey::String(key) => {
                    output.push(b'"');
                    output.extend_from_slice(key.as_bytes());
                    output.push(b'"');
                }
            }
            output.extend_from_slice(b"]=>\n");
            output.extend_from_slice(prefix.as_bytes());
            output.extend_from_slice(b"  ");
            if native_reference_is_visibly_aliased(context, value) {
                output.push(b'&');
            }
            native_var_dump_with_context(context, value, indent + 2, output)?;
        }
        output.extend_from_slice(prefix.as_bytes());
        output.extend_from_slice(b"}\n");
        return Ok(());
    }
    let Value::Object(object) = value else {
        native_var_dump(value, indent, output);
        return Ok(());
    };
    let class = context
        .unit
        .classes
        .iter()
        .find(|class| class.name == normalize_class_name(&object.class_name()))
        .cloned();
    let debug = class
        .as_ref()
        .and_then(|class| {
            class
                .methods
                .iter()
                .find(|method| method.name.eq_ignore_ascii_case("__debugInfo"))
        })
        .map(|method| method.function);
    let mut entries = Vec::<(String, Option<&php_ir::module::ClassPropertyEntry>, Value)>::new();
    if let Some(debug) = debug {
        let receiver = context.encode(Value::Object(object.clone()))?;
        let result = invoke_native_method(context, debug, &[receiver])?;
        let Value::Array(array) = context.decode(result)? else {
            return Err("__debugInfo() must return an array".to_owned());
        };
        entries.extend(array.iter().map(|(key, value)| {
            let key = match key {
                php_runtime::api::ArrayKey::Int(key) => key.to_string(),
                php_runtime::api::ArrayKey::String(key) => key.to_string_lossy(),
            };
            (key, None, value.clone())
        }));
    } else {
        let snapshot = object
            .properties_snapshot()
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        if let Some(class) = &class {
            for property in &class.properties {
                if let Some(value) = snapshot.get(&property.name)
                    && !matches!(value, Value::Uninitialized)
                {
                    entries.push((property.name.clone(), Some(property), value.clone()));
                }
            }
            for (name, value) in snapshot {
                if !class
                    .properties
                    .iter()
                    .any(|property| property.name == name)
                {
                    entries.push((name, None, value));
                }
            }
        } else {
            entries.extend(
                snapshot
                    .into_iter()
                    .map(|(name, value)| (name, None, value)),
            );
        }
    }
    let prefix = " ".repeat(indent);
    let display_name = object.display_name();
    output.extend_from_slice(
        format!(
            "object({})#{} ({}) {{\n",
            display_name,
            object.id(),
            entries.len()
        )
        .as_bytes(),
    );
    for (name, property, value) in entries {
        output.extend_from_slice(prefix.as_bytes());
        output.extend_from_slice(b"  [\"");
        output.extend_from_slice(name.as_bytes());
        output.push(b'"');
        if let Some(property) = property {
            if property.flags.is_private {
                output.extend_from_slice(b":\"");
                output.extend_from_slice(display_name.as_bytes());
                output.extend_from_slice(b"\":private");
            } else if property.flags.is_protected {
                output.extend_from_slice(b":protected");
            }
        }
        output.extend_from_slice(b"]=>\n");
        output.extend_from_slice(prefix.as_bytes());
        output.extend_from_slice(b"  ");
        if native_reference_is_visibly_aliased(context, &value) {
            output.push(b'&');
        }
        native_var_dump_with_context(context, &value, indent + 2, output)?;
    }
    output.extend_from_slice(prefix.as_bytes());
    output.extend_from_slice(b"}\n");
    Ok(())
}

pub(super) fn native_source_line(
    context: &NativeExecutionContext<'_>,
    source: &php_ir::Instruction,
) -> usize {
    native_source_line_for_span(context, source.span)
}

pub(super) fn native_source_line_for_span(
    context: &NativeExecutionContext<'_>,
    span: php_ir::IrSpan,
) -> usize {
    context
        .compiled
        .source_display_line(span, false)
        .and_then(|line| usize::try_from(line).ok())
        .unwrap_or(1)
}

pub(super) fn emit_native_php_warning(
    context: &mut NativeExecutionContext<'_>,
    errno: i64,
    message: &str,
    source: &php_ir::Instruction,
) -> Result<(), String> {
    emit_native_php_diagnostic(context, errno, message, source, true)
}

pub(super) fn emit_native_php_diagnostic(
    context: &mut NativeExecutionContext<'_>,
    errno: i64,
    message: &str,
    source: &php_ir::Instruction,
    leading_newline: bool,
) -> Result<(), String> {
    let path = context
        .unit
        .files
        .get(source.span.file.index())
        .map_or_else(|| "<unknown>".to_owned(), |file| file.path.clone());
    let line = native_source_line(context, source);
    context.record_last_error(errno, message, &path, line);
    if let Some(handler) = context
        .error_handlers
        .last()
        .filter(|handler| handler.levels == -1 || handler.levels & errno != 0)
        .cloned()
    {
        let arguments = [
            Value::Int(errno),
            Value::String(PhpString::from_bytes(message.as_bytes().to_vec())),
            Value::String(PhpString::from_bytes(path.as_bytes().to_vec())),
            Value::Int(line as i64),
        ];
        let _ = invoke_native_callable_value(context, handler.callback, &arguments, source, None)?;
        return Ok(());
    }
    if context.error_reporting & errno == 0 {
        return Ok(());
    }
    let label = match errno {
        php_runtime::api::PHP_E_NOTICE | php_runtime::api::PHP_E_USER_NOTICE => "Notice",
        php_runtime::api::PHP_E_DEPRECATED | php_runtime::api::PHP_E_USER_DEPRECATED => {
            "Deprecated"
        }
        _ => "Warning",
    };
    let prefix = if leading_newline { "\n" } else { "" };
    context.output.write_bytes(format!(
        "{prefix}{label}: {message} in {path} on line {line}\n"
    ));
    Ok(())
}

pub(super) fn emit_native_float_offset_warning(
    context: &mut NativeExecutionContext<'_>,
    target: &Value,
    key: &Value,
    source: Option<&php_ir::Instruction>,
) -> Result<(), String> {
    let Some(source) = source else {
        return Ok(());
    };
    let mut target = target.clone();
    let mut key = key.clone();
    for _ in 0..16 {
        match target {
            Value::Reference(reference) => target = reference.get(),
            _ => break,
        }
    }
    for _ in 0..16 {
        match key {
            Value::Reference(reference) => key = reference.get(),
            _ => break,
        }
    }
    let Value::Float(key) = key else {
        return Ok(());
    };
    let key = key.to_f64();
    if matches!(target, Value::String(_)) {
        emit_native_php_warning(context, 2, "String offset cast occurred", source)
    } else if matches!(target, Value::Array(_)) && key.is_finite() && key.fract() != 0.0 {
        emit_native_php_warning(
            context,
            8192,
            &format!(
                "Implicit conversion from float {} to int loses precision",
                native_php_float_label(key)
            ),
            source,
        )
    } else {
        Ok(())
    }
}

pub(super) fn emit_native_deprecated_call(
    context: &mut NativeExecutionContext<'_>,
    function: php_ir::FunctionId,
    source: &php_ir::Instruction,
) {
    let Some(function) = context.unit.functions.get(function.index()) else {
        return;
    };
    let Some(attribute) = function.attributes.iter().find(|attribute| {
        attribute
            .resolved_name
            .as_deref()
            .or(attribute.fallback_name.as_deref())
            .unwrap_or(&attribute.name)
            .trim_start_matches('\\')
            .eq_ignore_ascii_case("deprecated")
    }) else {
        return;
    };
    let custom = attribute.arguments.iter().find_map(|constant| {
        match context.unit.constants.get(constant.index())? {
            php_ir::IrConstant::String(value) => Some(value.clone()),
            php_ir::IrConstant::StringBytes(value) => {
                Some(String::from_utf8_lossy(value).into_owned())
            }
            _ => None,
        }
    });
    let kind = if function.flags.is_method {
        "Method"
    } else {
        "Function"
    };
    let mut message = format!("{kind} {}() is deprecated", function.name);
    if let Some(custom) = custom {
        message.push_str(", ");
        message.push_str(&custom);
    }
    let path = context
        .unit
        .files
        .get(source.span.file.index())
        .map_or("<unknown>", |file| file.path.as_str());
    let line = native_source_line(context, source);
    context.output.write_bytes(format!(
        "\nDeprecated: {message} in {path} on line {line}\n"
    ));
}

fn collect_native_compact_names(value: Value, names: &mut Vec<String>) -> Result<(), String> {
    match value {
        Value::String(name) => {
            names.push(String::from_utf8_lossy(name.as_bytes()).into_owned());
            Ok(())
        }
        Value::Array(values) => {
            for (_, value) in values.iter() {
                collect_native_compact_names(value.clone(), names)?;
            }
            Ok(())
        }
        Value::Reference(reference) => collect_native_compact_names(reference.get(), names),
        value => Err(format!(
            "compact(): Argument must be string or array, {} given",
            native_value_type_name(&value)
        )),
    }
}

fn native_array_key_bytes(key: &php_runtime::api::ArrayKey) -> Vec<u8> {
    match key {
        php_runtime::api::ArrayKey::Int(value) => value.to_string().into_bytes(),
        php_runtime::api::ArrayKey::String(value) => value.as_bytes().to_vec(),
    }
}

fn native_array_key_number(key: &php_runtime::api::ArrayKey) -> f64 {
    match key {
        php_runtime::api::ArrayKey::Int(value) => *value as f64,
        php_runtime::api::ArrayKey::String(value) => {
            value.to_string_lossy().trim().parse::<f64>().unwrap_or(0.0)
        }
    }
}

fn execute_native_key_sort(
    context: &mut NativeExecutionContext<'_>,
    arguments: &[i64],
    reverse: bool,
) -> Result<i64, String> {
    let Some(target) = arguments.first() else {
        return Err("ksort() expects an array passed by reference".to_owned());
    };
    let Value::Reference(reference) = context.decode(*target)? else {
        return Err("ksort(): Argument #1 ($array) must be passed by reference".to_owned());
    };
    let Value::Array(array) = reference.get() else {
        return Err("ksort(): Argument #1 ($array) must be of type array".to_owned());
    };
    let flags = arguments
        .get(1)
        .map(|value| context.decode(*value))
        .transpose()?
        .map_or(0, |value| match value {
            Value::Int(value) => value,
            Value::Reference(reference) => match reference.get() {
                Value::Int(value) => value,
                _ => 0,
            },
            _ => 0,
        });
    let mut entries = array
        .iter()
        .map(|(key, value)| (key, value.clone()))
        .collect::<Vec<_>>();
    entries.sort_by(|(left, _), (right, _)| {
        let ordering = if flags & !8 == 1 {
            native_array_key_number(left)
                .partial_cmp(&native_array_key_number(right))
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            let mut left = native_array_key_bytes(left);
            let mut right = native_array_key_bytes(right);
            if flags & 8 != 0 {
                left.make_ascii_lowercase();
                right.make_ascii_lowercase();
            }
            left.cmp(&right)
        };
        if reverse {
            ordering.reverse()
        } else {
            ordering
        }
    });
    let mut sorted = php_runtime::api::PhpArray::new();
    for (key, value) in entries {
        sorted.insert(key, value);
    }
    reference.set(Value::Array(sorted));
    context.encode(Value::Bool(true))
}

fn execute_native_callback_sort(
    context: &mut NativeExecutionContext<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
    compare_keys: bool,
    preserve_keys: bool,
) -> Result<i64, String> {
    let [target, callback] = arguments else {
        return Err("array callback sort expects exactly 2 arguments".to_owned());
    };
    let Value::Reference(reference) = context.decode(*target)? else {
        return Err("array callback sort expects an array passed by reference".to_owned());
    };
    let Value::Array(array) = reference.get() else {
        return Err("array callback sort expects an array".to_owned());
    };
    let callback = match context.decode(*callback)? {
        Value::Reference(reference) => reference.get(),
        callback => callback,
    };
    let mut entries = array
        .iter()
        .map(|(key, value)| (key, value.clone()))
        .collect::<Vec<_>>();
    for index in 1..entries.len() {
        let mut cursor = index;
        while cursor > 0 {
            let left = if compare_keys {
                native_array_key_value(&entries[cursor - 1].0)
            } else {
                entries[cursor - 1].1.clone()
            };
            let right = if compare_keys {
                native_array_key_value(&entries[cursor].0)
            } else {
                entries[cursor].1.clone()
            };
            let result = invoke_native_callable_value(
                context,
                callback.clone(),
                &[left, right],
                source,
                None,
            )?;
            let ordering = match context.decode(result)? {
                Value::Int(value) => value,
                Value::Float(value) => value.to_f64() as i64,
                Value::String(value) => value.to_string_lossy().parse::<i64>().unwrap_or(0),
                Value::Bool(value) => i64::from(value),
                _ => 0,
            };
            if ordering <= 0 {
                break;
            }
            entries.swap(cursor - 1, cursor);
            cursor -= 1;
        }
    }
    let mut sorted = php_runtime::api::PhpArray::new();
    for (key, value) in entries {
        if preserve_keys {
            sorted.insert(key, value);
        } else {
            sorted.append(value);
        }
    }
    reference.set(Value::Array(sorted));
    context.encode(Value::Bool(true))
}

fn native_array_key_value(key: &php_runtime::api::ArrayKey) -> Value {
    match key {
        php_runtime::api::ArrayKey::Int(key) => Value::Int(*key),
        php_runtime::api::ArrayKey::String(key) => Value::String(key.clone()),
    }
}

fn execute_native_array_map(
    context: &mut NativeExecutionContext<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    let Some((callback, arrays)) = arguments.split_first() else {
        return Err("array_map() expects at least 2 arguments".to_owned());
    };
    if arrays.is_empty() {
        return Err("array_map() expects at least 2 arguments".to_owned());
    }
    let callback = match context.decode(*callback)? {
        Value::Reference(reference) => reference.get(),
        callback => callback,
    };
    let arrays = arrays
        .iter()
        .map(|array| match context.decode(*array)? {
            Value::Reference(reference) => match reference.get() {
                Value::Array(array) => Ok(array),
                _ => Err("array_map(): array argument must be of type array".to_owned()),
            },
            Value::Array(array) => Ok(array),
            _ => Err("array_map(): array argument must be of type array".to_owned()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if matches!(callback, Value::Null) && arrays.len() == 1 {
        return context.encode(Value::Array(arrays[0].clone()));
    }
    let entries = arrays
        .iter()
        .map(|array| array.iter().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let length = entries.iter().map(Vec::len).max().unwrap_or(0);
    let mut result = php_runtime::api::PhpArray::new();
    for index in 0..length {
        let values = entries
            .iter()
            .map(|entries| {
                entries
                    .get(index)
                    .map_or(Value::Null, |(_, value)| (*value).clone())
            })
            .collect::<Vec<_>>();
        let value = if matches!(callback, Value::Null) {
            Value::Array(php_runtime::api::PhpArray::from_packed(values))
        } else {
            let encoded =
                invoke_native_callable_value(context, callback.clone(), &values, source, None)?;
            context.decode(encoded)?
        };
        if arrays.len() == 1 {
            let (key, _) = &entries[0][index];
            result.insert(key.clone(), value);
        } else {
            result.append(value);
        }
    }
    context.encode(Value::Array(result))
}

fn execute_native_array_filter(
    context: &mut NativeExecutionContext<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    let Some(array) = arguments.first() else {
        return Err("array_filter() expects at least 1 argument".to_owned());
    };
    let array = match context.decode(*array)? {
        Value::Reference(reference) => match reference.get() {
            Value::Array(array) => array,
            _ => return Err("array_filter(): argument #1 must be of type array".to_owned()),
        },
        Value::Array(array) => array,
        _ => return Err("array_filter(): argument #1 must be of type array".to_owned()),
    };
    let callback = arguments
        .get(1)
        .map(|callback| context.decode(*callback))
        .transpose()?
        .map(|callback| match callback {
            Value::Reference(reference) => reference.get(),
            callback => callback,
        })
        .filter(|callback| !matches!(callback, Value::Null));
    let mode = arguments
        .get(2)
        .map(|mode| context.decode(*mode))
        .transpose()?
        .map_or(0, |mode| match mode {
            Value::Int(mode) => mode,
            _ => 0,
        });
    let mut result = php_runtime::api::PhpArray::new();
    for (key, value) in array.iter() {
        let keep = if let Some(callback) = &callback {
            let key_value = native_array_key_value(&key);
            let callback_arguments = match mode {
                1 => vec![value.clone(), key_value],
                2 => vec![key_value],
                _ => vec![value.clone()],
            };
            let encoded = invoke_native_callable_value(
                context,
                callback.clone(),
                &callback_arguments,
                source,
                None,
            )?;
            native_property_truthy(&context.decode(encoded)?)
        } else {
            native_property_truthy(value)
        };
        if keep {
            result.insert(key.clone(), value.clone());
        }
    }
    context.encode(Value::Array(result))
}

fn native_array_argument(
    context: &NativeExecutionContext<'_>,
    encoded: i64,
    function: &str,
) -> Result<php_runtime::api::PhpArray, String> {
    let value = match context.decode(encoded)? {
        Value::Reference(reference) => reference.get(),
        value => value,
    };
    match value {
        Value::Array(array) => Ok(array),
        value => Err(format!(
            "E_PHP_THROW:TypeError:{function}(): Argument #1 ($array) must be of type array, {} given",
            native_value_type_name(&value)
        )),
    }
}

fn execute_native_array_reduce(
    context: &mut NativeExecutionContext<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    if !(2..=3).contains(&arguments.len()) {
        return Err("array_reduce() expects 2 or 3 arguments".to_owned());
    }
    let array = native_array_argument(context, arguments[0], "array_reduce")?;
    let callback = match context.decode(arguments[1])? {
        Value::Reference(reference) => reference.get(),
        value => value,
    };
    let mut carry = arguments
        .get(2)
        .map(|value| context.decode(*value))
        .transpose()?
        .unwrap_or(Value::Null);
    for (_, value) in array.iter() {
        let encoded = invoke_native_callable_value(
            context,
            callback.clone(),
            &[carry, value.clone()],
            source,
            None,
        )?;
        carry = context.decode(encoded)?;
    }
    context.encode(carry)
}

fn execute_native_array_walk(
    context: &mut NativeExecutionContext<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    if !(2..=3).contains(&arguments.len()) {
        return Err("array_walk() expects 2 or 3 arguments".to_owned());
    }
    let Value::Reference(root) = context.decode(arguments[0])? else {
        return Err("array_walk(): Argument #1 ($array) must be passed by reference".to_owned());
    };
    let Value::Array(mut array) = root.get() else {
        return Err(
            "E_PHP_THROW:TypeError:array_walk(): Argument #1 ($array) must be of type array"
                .to_owned(),
        );
    };
    let callback = match context.decode(arguments[1])? {
        Value::Reference(reference) => reference.get(),
        value => value,
    };
    let userdata = arguments
        .get(2)
        .map(|value| context.decode(*value))
        .transpose()?;
    let keys = array.iter().map(|(key, _)| key).collect::<Vec<_>>();
    let mut entries = Vec::with_capacity(keys.len());
    for key in keys {
        let value = array.get(&key).cloned().unwrap_or(Value::Null);
        let cell = match value {
            Value::Reference(reference) => reference,
            value => php_runtime::api::ReferenceCell::new(value),
        };
        array.insert(key.clone(), Value::Reference(cell.clone()));
        entries.push((key, cell));
    }
    root.set(Value::Array(array));
    for (key, cell) in entries {
        let mut values = vec![Value::Reference(cell), native_array_key_value(&key)];
        if let Some(userdata) = &userdata {
            values.push(userdata.clone());
        }
        let _ = invoke_native_callable_value(context, callback.clone(), &values, source, None)?;
    }
    context.encode(Value::Bool(true))
}

fn walk_native_array_recursive(
    context: &mut NativeExecutionContext<'_>,
    array: &mut php_runtime::api::PhpArray,
    callback: &Value,
    userdata: Option<&Value>,
    source: &php_ir::Instruction,
) -> Result<(), String> {
    let keys = array.iter().map(|(key, _)| key).collect::<Vec<_>>();
    for key in keys {
        let value = array.get(&key).cloned().unwrap_or(Value::Null);
        match value {
            Value::Reference(reference) => match reference.get() {
                Value::Array(mut nested) => {
                    walk_native_array_recursive(context, &mut nested, callback, userdata, source)?;
                    reference.set(Value::Array(nested));
                }
                _ => {
                    let mut values =
                        vec![Value::Reference(reference), native_array_key_value(&key)];
                    if let Some(userdata) = userdata {
                        values.push(userdata.clone());
                    }
                    let _ = invoke_native_callable_value(
                        context,
                        callback.clone(),
                        &values,
                        source,
                        None,
                    )?;
                }
            },
            Value::Array(mut nested) => {
                walk_native_array_recursive(context, &mut nested, callback, userdata, source)?;
                array.insert(key, Value::Array(nested));
            }
            value => {
                let reference = php_runtime::api::ReferenceCell::new(value);
                array.insert(key.clone(), Value::Reference(reference.clone()));
                let mut values = vec![
                    Value::Reference(reference.clone()),
                    native_array_key_value(&key),
                ];
                if let Some(userdata) = userdata {
                    values.push(userdata.clone());
                }
                let _ =
                    invoke_native_callable_value(context, callback.clone(), &values, source, None)?;
                array.insert(key, reference.get());
            }
        }
    }
    Ok(())
}

fn execute_native_array_walk_recursive(
    context: &mut NativeExecutionContext<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    if !(2..=3).contains(&arguments.len()) {
        return Err("array_walk_recursive() expects 2 or 3 arguments".to_owned());
    }
    let Value::Reference(root) = context.decode(arguments[0])? else {
        return Err(
            "array_walk_recursive(): Argument #1 ($array) must be passed by reference".to_owned(),
        );
    };
    let Value::Array(mut array) = root.get() else {
        return Err(
            "E_PHP_THROW:TypeError:array_walk_recursive(): Argument #1 ($array) must be of type array"
                .to_owned(),
        );
    };
    let callback = match context.decode(arguments[1])? {
        Value::Reference(reference) => reference.get(),
        value => value,
    };
    let userdata = arguments
        .get(2)
        .map(|value| context.decode(*value))
        .transpose()?;
    walk_native_array_recursive(context, &mut array, &callback, userdata.as_ref(), source)?;
    root.set(Value::Array(array));
    context.encode(Value::Bool(true))
}

fn execute_native_array_predicate(
    context: &mut NativeExecutionContext<'_>,
    name: &str,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    let [array, callback] = arguments else {
        return Err(format!("{name}() expects exactly 2 arguments"));
    };
    let array = native_array_argument(context, *array, name)?;
    let callback = match context.decode(*callback)? {
        Value::Reference(reference) => reference.get(),
        value => value,
    };
    for (key, value) in array.iter() {
        let encoded = invoke_native_callable_value(
            context,
            callback.clone(),
            &[value.clone(), native_array_key_value(&key)],
            source,
            None,
        )?;
        if native_property_truthy(&context.decode(encoded)?) {
            match name {
                "array_any" => return context.encode(Value::Bool(true)),
                "array_find" => return context.encode(value.clone()),
                "array_find_key" => return context.encode(native_array_key_value(&key)),
                "array_all" => continue,
                _ => {}
            }
        } else if name == "array_all" {
            return context.encode(Value::Bool(false));
        }
    }
    context.encode(match name {
        "array_all" => Value::Bool(true),
        "array_any" => Value::Bool(false),
        _ => Value::Null,
    })
}

fn execute_native_iterator_to_array(
    context: &mut NativeExecutionContext<'_>,
    arguments: &[i64],
) -> Result<i64, String> {
    if !(1..=2).contains(&arguments.len()) {
        return Err("iterator_to_array() expects 1 or 2 arguments".to_owned());
    }
    let iterator = match context.decode(arguments[0])? {
        Value::Reference(reference) => reference.get(),
        value => value,
    };
    let Value::Object(mut iterator) = iterator else {
        return Err(
            "E_PHP_THROW:TypeError:iterator_to_array(): Argument #1 ($iterator) must be of type Traversable"
                .to_owned(),
        );
    };
    let preserve_keys = arguments
        .get(1)
        .map(|value| context.decode(*value))
        .transpose()?
        .is_none_or(|value| native_property_truthy(&value));
    let class_name = iterator.class_name();
    if native_method_in_hierarchy(context, &class_name, "getIterator").is_some()
        || native_external_method(context, &class_name, "getIterator").is_some()
    {
        let encoded = invoke_native_bound_method(
            context,
            &php_runtime::api::CallableMethodTarget::Object(iterator.clone()),
            "getIterator",
            &[],
            None,
            context.unit.strict_types,
            None,
        )?;
        iterator = match context.decode(encoded)? {
            Value::Reference(reference) => match reference.get() {
                Value::Object(iterator) => iterator,
                _ => {
                    return Err("IteratorAggregate::getIterator() must return an object".to_owned());
                }
            },
            Value::Object(iterator) => iterator,
            _ => {
                return Err("IteratorAggregate::getIterator() must return an object".to_owned());
            }
        };
    }
    let entries = if let Some(entries) = native_spl_iterator_entries(&iterator) {
        entries
    } else {
        let class_name = iterator.class_name();
        let has_method = |context: &NativeExecutionContext<'_>, method: &str| {
            native_method_in_hierarchy(context, &class_name, method).is_some()
                || native_external_method(context, &class_name, method).is_some()
        };
        if !["rewind", "valid", "current", "key", "next"]
            .iter()
            .all(|method| has_method(context, method))
        {
            return Err("iterator_to_array() requires a supported Traversable object".to_owned());
        }
        let invoke = |context: &mut NativeExecutionContext<'_>, method: &str| {
            let encoded = invoke_native_bound_method(
                context,
                &php_runtime::api::CallableMethodTarget::Object(iterator.clone()),
                method,
                &[],
                None,
                context.unit.strict_types,
                None,
            )?;
            context.decode(encoded)
        };
        let _ = invoke(context, "rewind")?;
        let mut entries = Vec::new();
        while native_property_truthy(&invoke(context, "valid")?) {
            let key = invoke(context, "key")?;
            let value = invoke(context, "current")?;
            entries.push((key, value));
            if entries.len() >= 1_000_000 {
                return Err("iterator_to_array() iterator exceeded the safety limit".to_owned());
            }
            let _ = invoke(context, "next")?;
        }
        entries
    };
    let mut result = php_runtime::api::PhpArray::new();
    for (key, value) in entries {
        if preserve_keys {
            let key = match key {
                Value::Int(key) => php_runtime::api::ArrayKey::Int(key),
                Value::String(key) => php_runtime::api::ArrayKey::String(key),
                _ => {
                    return Err(
                        "E_PHP_THROW:TypeError:Keys must be of type int|string during iteration"
                            .to_owned(),
                    );
                }
            };
            result.insert(key, value);
        } else {
            result.append(value);
        }
    }
    context.encode(Value::Array(result))
}

fn native_sort_text(value: &Value, case_insensitive: bool) -> Vec<u8> {
    let mut value = native_string(value.clone()).unwrap_or_default();
    if case_insensitive {
        value.make_ascii_lowercase();
    }
    value
}

fn native_natural_compare(left: &[u8], right: &[u8]) -> std::cmp::Ordering {
    let mut left_index = 0;
    let mut right_index = 0;
    while left_index < left.len() && right_index < right.len() {
        if left[left_index].is_ascii_digit() && right[right_index].is_ascii_digit() {
            let left_start = left_index;
            let right_start = right_index;
            while left_index < left.len() && left[left_index].is_ascii_digit() {
                left_index += 1;
            }
            while right_index < right.len() && right[right_index].is_ascii_digit() {
                right_index += 1;
            }
            let left_digits = &left[left_start..left_index];
            let right_digits = &right[right_start..right_index];
            let left_trimmed = left_digits
                .iter()
                .position(|byte| *byte != b'0')
                .map_or(&left_digits[left_digits.len()..], |index| {
                    &left_digits[index..]
                });
            let right_trimmed = right_digits
                .iter()
                .position(|byte| *byte != b'0')
                .map_or(&right_digits[right_digits.len()..], |index| {
                    &right_digits[index..]
                });
            let ordering = left_trimmed
                .len()
                .cmp(&right_trimmed.len())
                .then_with(|| left_trimmed.cmp(right_trimmed));
            if !ordering.is_eq() {
                return ordering;
            }
            continue;
        }
        let ordering = left[left_index].cmp(&right[right_index]);
        if !ordering.is_eq() {
            return ordering;
        }
        left_index += 1;
        right_index += 1;
    }
    left.len().cmp(&right.len())
}

fn execute_native_value_sort(
    context: &mut NativeExecutionContext<'_>,
    name: &str,
    arguments: &[i64],
) -> Result<i64, String> {
    let Some(target) = arguments.first() else {
        return Err(format!("{name}() expects an array passed by reference"));
    };
    let Value::Reference(reference) = context.decode(*target)? else {
        return Err(format!(
            "{name}(): Argument #1 ($array) must be passed by reference"
        ));
    };
    let Value::Array(array) = reference.get() else {
        return Err(format!(
            "E_PHP_THROW:TypeError:{name}(): Argument #1 ($array) must be of type array"
        ));
    };
    let flags = arguments
        .get(1)
        .map(|value| context.decode(*value))
        .transpose()?
        .map_or(0, |value| match value {
            Value::Int(value) => value,
            Value::Reference(reference) => match reference.get() {
                Value::Int(value) => value,
                _ => 0,
            },
            _ => 0,
        });
    let reverse = matches!(name, "rsort" | "arsort");
    let preserve_keys = matches!(name, "asort" | "arsort" | "natsort" | "natcasesort");
    let natural = matches!(name, "natsort" | "natcasesort") || flags & !8 == 6;
    let case_insensitive = name == "natcasesort" || flags & 8 != 0;
    let mut entries = array
        .iter()
        .map(|(key, value)| (key, value.clone()))
        .collect::<Vec<_>>();
    for index in 1..entries.len() {
        let mut cursor = index;
        while cursor > 0 {
            let mut ordering = if natural {
                native_natural_compare(
                    &native_sort_text(&entries[cursor - 1].1, case_insensitive),
                    &native_sort_text(&entries[cursor].1, case_insensitive),
                )
            } else if flags & !8 == 1 {
                let left = native_string(entries[cursor - 1].1.clone())
                    .ok()
                    .and_then(|value| String::from_utf8(value).ok())
                    .and_then(|value| value.parse::<f64>().ok())
                    .unwrap_or(0.0);
                let right = native_string(entries[cursor].1.clone())
                    .ok()
                    .and_then(|value| String::from_utf8(value).ok())
                    .and_then(|value| value.parse::<f64>().ok())
                    .unwrap_or(0.0);
                left.partial_cmp(&right)
                    .unwrap_or(std::cmp::Ordering::Equal)
            } else if flags & !8 == 2 {
                native_sort_text(&entries[cursor - 1].1, case_insensitive)
                    .cmp(&native_sort_text(&entries[cursor].1, case_insensitive))
            } else {
                php_runtime::api::compare_php(&entries[cursor - 1].1, &entries[cursor].1)?
            };
            if reverse {
                ordering = ordering.reverse();
            }
            if !ordering.is_gt() {
                break;
            }
            entries.swap(cursor - 1, cursor);
            cursor -= 1;
        }
    }
    let mut sorted = php_runtime::api::PhpArray::new();
    for (key, value) in entries {
        if preserve_keys {
            sorted.insert(key, value);
        } else {
            sorted.append(value);
        }
    }
    reference.set(Value::Array(sorted));
    context.encode(Value::Bool(true))
}

fn native_builtin_class(
    context: &NativeExecutionContext<'_>,
    name: &str,
) -> Option<php_ir::module::ClassEntry> {
    let normalized = normalize_class_name(name);
    context
        .unit
        .classes
        .iter()
        .find(|class| class.name == normalized)
        .cloned()
        .or_else(|| native_external_class(context, &normalized).map(|(_, class)| class))
}

fn native_builtin_class_lineage(
    context: &NativeExecutionContext<'_>,
    name: &str,
) -> Vec<php_ir::module::ClassEntry> {
    let mut lineage = Vec::new();
    let mut current = native_builtin_class(context, name);
    let mut seen = std::collections::BTreeSet::new();
    while let Some(class) = current {
        if !seen.insert(class.name.clone()) {
            break;
        }
        let parent = class.parent.clone();
        lineage.push(class);
        current = parent.and_then(|parent| native_builtin_class(context, &parent));
    }
    lineage
}

fn native_builtin_caller_class(
    context: &NativeExecutionContext<'_>,
    caller_locals: Option<(u32, &[i64])>,
) -> Option<String> {
    let function = caller_locals?.0;
    context.unit.classes.iter().find_map(|class| {
        class
            .methods
            .iter()
            .any(|method| method.function.raw() == function)
            .then(|| class.name.clone())
    })
}

fn native_property_visible_from(
    property: &php_ir::module::ClassPropertyEntry,
    declaring_class: &str,
    caller_class: Option<&str>,
) -> bool {
    if !property.flags.is_private && !property.flags.is_protected {
        return true;
    }
    caller_class.is_some_and(|caller| caller == declaring_class)
}

fn native_object_vars(
    context: &NativeExecutionContext<'_>,
    object: &php_runtime::api::ObjectRef,
    caller_class: Option<&str>,
    mangled: bool,
) -> php_runtime::api::PhpArray {
    let lineage = native_builtin_class_lineage(context, &object.class_name());
    let mut result = php_runtime::api::PhpArray::new();
    let mut declared = std::collections::BTreeSet::new();
    for class in lineage.iter().rev() {
        for property in &class.properties {
            if property.flags.is_static {
                continue;
            }
            declared.insert(property.name.clone());
            if !mangled && !native_property_visible_from(property, &class.name, caller_class) {
                continue;
            }
            let Some(value) = object.get_property(&property.name) else {
                continue;
            };
            if matches!(value, Value::Uninitialized) {
                continue;
            }
            let name = if mangled && property.flags.is_private {
                format!("\0{}\0{}", class.display_name, property.name)
            } else if mangled && property.flags.is_protected {
                format!("\0*\0{}", property.name)
            } else {
                property.name.clone()
            };
            result.insert(
                php_runtime::api::ArrayKey::String(PhpString::from_bytes(name.into_bytes())),
                value,
            );
        }
    }
    for (name, value) in object.properties_snapshot() {
        if !declared.contains(&name) {
            result.insert(
                php_runtime::api::ArrayKey::String(PhpString::from_bytes(name.into_bytes())),
                value,
            );
        }
    }
    result
}

fn execute_native_preg_replace_callback(
    context: &mut NativeExecutionContext<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    if !(3..=6).contains(&arguments.len()) {
        return Err("preg_replace_callback() expects 3 to 6 arguments".to_owned());
    }
    let pattern = PhpString::from_bytes(native_string(context.decode(arguments[0])?)?);
    let callback = match context.decode(arguments[1])? {
        Value::Reference(reference) => reference.get(),
        callback => callback,
    };
    if matches!(&callback, Value::Array(array) if array.len() != 2) {
        return Err(
            "E_PHP_THROW:TypeError:preg_replace_callback(): Argument #2 ($callback) must be a valid callback, array callback must have exactly two members"
                .to_owned(),
        );
    }
    let subject = match context.decode(arguments[2])? {
        Value::Reference(reference) => reference.get(),
        subject => subject,
    };
    let limit = arguments
        .get(3)
        .map(|limit| context.decode(*limit))
        .transpose()?
        .map_or(-1, |limit| match limit {
            Value::Int(limit) => limit,
            _ => -1,
        });
    let compiled = match context
        .builtin_request_state
        .pcre_mut()
        .cache_mut()
        .compile(&pattern)
    {
        Ok(compiled) => compiled,
        Err(error) => {
            emit_native_php_diagnostic(
                context,
                php_runtime::api::PHP_E_WARNING,
                &format!("preg_replace_callback(): {}", error.message()),
                source,
                true,
            )?;
            return context.encode(Value::Null);
        }
    };
    let replace = |context: &mut NativeExecutionContext<'_>,
                   subject: &[u8],
                   count: &mut i64|
     -> Result<Vec<u8>, String> {
        let mut output = Vec::new();
        let mut last_end = 0usize;
        let mut local_count = 0i64;
        compiled.for_each_php_match(
            subject,
            0,
            |captures| {
                let Some(full) = captures.get(0) else {
                    return Ok(true);
                };
                if limit >= 0 && local_count >= limit {
                    return Ok(false);
                }
                output.extend_from_slice(&subject[last_end..full.start()]);
                let mut matches = php_runtime::api::PhpArray::new();
                for index in 0..captures.len() {
                    let value = captures.get(index).map_or_else(
                        || Value::String(PhpString::from_bytes(Vec::new())),
                        |capture| {
                            Value::String(PhpString::from_bytes(
                                subject[capture.start()..capture.end()].to_vec(),
                            ))
                        },
                    );
                    matches.insert(php_runtime::api::ArrayKey::Int(index as i64), value.clone());
                    if let Some(Some(name)) = compiled.capture_names().get(index) {
                        matches.insert(
                            php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                                name.as_bytes().to_vec(),
                            )),
                            value,
                        );
                    }
                }
                let encoded = invoke_native_callable_value(
                    context,
                    callback.clone(),
                    &[Value::Array(matches)],
                    source,
                    None,
                )?;
                output.extend_from_slice(&native_string(context.decode(encoded)?)?);
                last_end = full.end();
                local_count += 1;
                *count += 1;
                Ok(true)
            },
            |error| error.message().to_owned(),
        )?;
        output.extend_from_slice(&subject[last_end..]);
        Ok(output)
    };
    let mut count = 0i64;
    let result = match subject {
        Value::Array(array) => {
            let mut result = php_runtime::api::PhpArray::new();
            for (key, value) in array.iter() {
                let subject = native_string(value.clone())?;
                result.insert(
                    key,
                    Value::String(PhpString::from_bytes(replace(
                        context, &subject, &mut count,
                    )?)),
                );
            }
            Value::Array(result)
        }
        subject => {
            let subject = native_string(subject)?;
            Value::String(PhpString::from_bytes(replace(
                context, &subject, &mut count,
            )?))
        }
    };
    if let Some(count_argument) = arguments.get(4)
        && let Value::Reference(reference) = context.decode(*count_argument)?
    {
        reference.set(Value::Int(count));
    }
    context.encode(result)
}

fn execute_native_preg_replace_callback_array(
    context: &mut NativeExecutionContext<'_>,
    arguments: &[i64],
) -> Result<Option<i64>, String> {
    if !(2..=5).contains(&arguments.len()) {
        return Ok(None);
    }
    let patterns = match context.decode(arguments[0])? {
        Value::Reference(reference) => reference.get(),
        patterns => patterns,
    };
    let Value::Array(patterns) = patterns else {
        return Ok(None);
    };
    if !patterns.is_empty() {
        return Ok(None);
    }
    if let Some(count) = arguments.get(3)
        && let Value::Reference(reference) = context.decode(*count)?
    {
        reference.set(Value::Int(0));
    }
    let subject = match context.decode(arguments[1])? {
        Value::Reference(reference) => reference.get(),
        subject => subject,
    };
    context.encode(subject).map(Some)
}

pub(super) fn execute_native_builtin(
    context: &mut NativeExecutionContext<'_>,
    name: &str,
    arguments: &[i64],
    source: &php_ir::Instruction,
    caller_locals: Option<(u32, &[i64])>,
) -> Result<i64, String> {
    let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
    if native_builtin_is_unavailable_target_function(&normalized) {
        return Err(format!(
            "E_PHP_THROW:Error:Call to undefined function {name}()"
        ));
    }
    if matches!(normalized.as_str(), "strftime" | "gmstrftime")
        && !(1..=2).contains(&arguments.len())
    {
        emit_native_php_diagnostic(
            context,
            php_runtime::api::PHP_E_DEPRECATED,
            &format!(
                "Function {normalized}() is deprecated since 8.1, use IntlDateFormatter::format() instead"
            ),
            source,
            true,
        )?;
    }
    validate_native_builtin_arity(&normalized, arguments.len())?;
    validate_native_builtin_types(context, &normalized, arguments, source)?;
    if let Some(result) = execute_native_internal_builtin(context, &normalized, arguments) {
        return result;
    }
    match normalized.as_str() {
        "get_included_files" | "get_required_files" => {
            let files = context
                .included_files
                .iter()
                .map(|path| Value::string(path.to_string_lossy().into_owned()))
                .collect();
            context.encode(Value::packed_array(files))
        }
        "ob_start" => {
            context.output.start_buffer();
            context.encode(Value::Bool(true))
        }
        "ob_get_clean" => {
            let bytes = context
                .output
                .pop_buffer_clean()
                .ok_or_else(|| "ob_get_clean(): Failed to delete buffer".to_owned())?;
            context.encode(Value::String(PhpString::from_bytes(bytes)))
        }
        "ob_get_contents" => {
            let value = context
                .output
                .current_buffer_bytes()
                .map(|bytes| Value::String(PhpString::from_bytes(bytes.to_vec())))
                .unwrap_or(Value::Bool(false));
            context.encode(value)
        }
        "ob_get_level" => context.encode(Value::Int(context.output.buffer_level() as i64)),
        "ob_get_length" => context.encode(
            context
                .output
                .current_buffer_len()
                .map_or(Value::Bool(false), |length| Value::Int(length as i64)),
        ),
        "ob_end_flush" => {
            let value = context.output.pop_buffer_flush().is_some();
            context.encode(Value::Bool(value))
        }
        "ob_end_clean" => {
            let value = context.output.pop_buffer_clean().is_some();
            context.encode(Value::Bool(value))
        }
        "array_map" => execute_native_array_map(context, arguments, source),
        "array_filter" => execute_native_array_filter(context, arguments, source),
        "array_reduce" => execute_native_array_reduce(context, arguments, source),
        "array_walk" => execute_native_array_walk(context, arguments, source),
        "array_walk_recursive" => execute_native_array_walk_recursive(context, arguments, source),
        "iterator_to_array" => execute_native_iterator_to_array(context, arguments),
        "array_any" | "array_all" | "array_find" | "array_find_key" => {
            execute_native_array_predicate(context, &normalized, arguments, source)
        }
        "preg_replace_callback" => execute_native_preg_replace_callback(context, arguments, source),
        "preg_replace_callback_array" => {
            if let Some(result) = execute_native_preg_replace_callback_array(context, arguments)? {
                Ok(result)
            } else {
                Err(
                    "E_PHP_THROW:Error:preg_replace_callback_array requires VM callable dispatch for user callbacks"
                        .to_owned(),
                )
            }
        }
        "sort" | "rsort" | "asort" | "arsort" | "natsort" | "natcasesort" => {
            execute_native_value_sort(context, &normalized, arguments)
        }
        "ksort" => execute_native_key_sort(context, arguments, false),
        "krsort" => execute_native_key_sort(context, arguments, true),
        "usort" => execute_native_callback_sort(context, arguments, source, false, false),
        "uasort" => execute_native_callback_sort(context, arguments, source, false, true),
        "uksort" => execute_native_callback_sort(context, arguments, source, true, true),
        "func_get_args" => {
            let values = context.call_arguments.last().cloned().unwrap_or_default();
            context.encode(Value::Array(php_runtime::api::PhpArray::from_packed(
                values,
            )))
        }
        "compact" => {
            let (function_id, slots) = caller_locals.ok_or_else(|| {
                "compact() requires the active native caller symbol table".to_owned()
            })?;
            let function = context
                .unit
                .functions
                .get(function_id as usize)
                .ok_or_else(|| "compact() caller function metadata is missing".to_owned())?;
            let mut names = Vec::new();
            for argument in arguments {
                collect_native_compact_names(context.decode(*argument)?, &mut names)?;
            }
            let mut result = php_runtime::api::PhpArray::new();
            for name in names {
                let Some(index) = function.locals.iter().position(|local| local == &name) else {
                    continue;
                };
                let Some(slot) = slots.get(index) else {
                    continue;
                };
                // PHP's compact() copies the current value into the result. It
                // never exposes the caller's reference container, even when
                // the source variable was explicitly bound by reference.
                let value = match context.decode(*slot)? {
                    Value::Reference(reference) => reference.get(),
                    value => value,
                };
                result.insert(
                    php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                        name.as_bytes().to_vec(),
                    )),
                    value,
                );
            }
            context.encode(Value::Array(result))
        }
        "implode" => {
            let (separator, values) = match arguments {
                [values] => (Vec::new(), *values),
                [separator, values] => (native_string(context.decode(*separator)?)?, *values),
                _ => return Err("implode() expects 1 or 2 arguments".to_owned()),
            };
            let values = match context.decode(values)? {
                Value::Reference(reference) => reference.get(),
                values => values,
            };
            let Value::Array(values) = values else {
                return Err("implode(): argument #2 must be of type array".to_owned());
            };
            let mut joined = Vec::new();
            for (index, (_, value)) in values.iter().enumerate() {
                if index != 0 {
                    joined.extend_from_slice(&separator);
                }
                let value = match value {
                    Value::Reference(reference) => reference.get(),
                    value => value.clone(),
                };
                joined.extend_from_slice(&native_string(value)?);
            }
            context.encode(Value::String(PhpString::from_bytes(joined)))
        }
        "define" => {
            let [name, value, ..] = arguments else {
                return Err("define() expects a name and value".to_owned());
            };
            let name =
                String::from_utf8_lossy(&native_string(context.decode(*name)?)?).into_owned();
            let value = context.decode(*value)?;
            if context.dynamic_constants.contains_key(&name)
                || context.lookup_constant(&name).is_ok()
            {
                let path = context
                    .unit
                    .files
                    .get(source.span.file.index())
                    .map_or("<unknown>", |file| file.path.as_str());
                let line = native_source_line(context, source);
                context.output.write_bytes(format!(
                    "\nWarning: Constant {name} already defined, this will be an error in PHP 9 in {path} on line {line}\n"
                ));
                return context.encode(Value::Bool(false));
            }
            context.dynamic_constants.insert(name, value);
            context.encode(Value::Bool(true))
        }
        "defined" => {
            let [name] = arguments else {
                return Err("defined() expects exactly 1 argument".to_owned());
            };
            let name =
                String::from_utf8_lossy(&native_string(context.decode(*name)?)?).into_owned();
            context.encode(Value::Bool(
                context.lookup_constant(&name).is_ok()
                    || native_internal_class_constant_exists(&name),
            ))
        }
        "constant" => {
            let [name] = arguments else {
                return Err("constant() expects exactly 1 argument".to_owned());
            };
            let name =
                String::from_utf8_lossy(&native_string(context.decode(*name)?)?).into_owned();
            context.encode(context.lookup_constant(&name)?)
        }
        "print" => {
            let [value] = arguments else {
                return Err("print expects exactly 1 argument".to_owned());
            };
            let value = context.decode(*value)?;
            let mut operation = php_runtime::api::NativeOperationContext::default();
            let status = php_runtime::api::native_echo(&mut operation, &mut context.output, &value);
            if status != php_runtime::api::NativeOperationStatus::Ok {
                return Err("print failed to render its argument".to_owned());
            }
            Ok(1)
        }
        "gettype" => {
            let [value] = arguments else {
                return Err("gettype() expects exactly 1 argument".to_owned());
            };
            let mut value = context.decode(*value)?;
            for _ in 0..16 {
                let Value::Reference(reference) = value else {
                    break;
                };
                value = reference.get();
            }
            let type_name = match value {
                Value::Null => "NULL",
                Value::Bool(_) => "boolean",
                Value::Int(_) => "integer",
                Value::Float(_) => "double",
                Value::String(_) => "string",
                Value::Array(_) => "array",
                Value::Object(_) => "object",
                Value::Resource(_) => "resource",
                Value::Uninitialized => "NULL",
                Value::Fiber(_) | Value::Generator(_) | Value::Callable(_) => "object",
                Value::Reference(_) => unreachable!("references were dereferenced above"),
            };
            context.encode(Value::String(PhpString::from_bytes(
                type_name.as_bytes().to_vec(),
            )))
        }
        "is_int" | "is_integer" | "is_long" => {
            let [value] = arguments else {
                return Err("is_int() expects exactly 1 argument".to_owned());
            };
            let value = context.decode(*value)?;
            let value = match value {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            context.encode(Value::Bool(matches!(value, Value::Int(_))))
        }
        "is_string" => {
            let [value] = arguments else {
                return Err("is_string() expects exactly 1 argument".to_owned());
            };
            let value = context.decode(*value)?;
            let value = match value {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            context.encode(Value::Bool(matches!(value, Value::String(_))))
        }
        "is_bool" => {
            let [value] = arguments else {
                return Err("is_bool() expects exactly 1 argument".to_owned());
            };
            let value = context.decode(*value)?;
            let value = match value {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            context.encode(Value::Bool(matches!(value, Value::Bool(_))))
        }
        "is_null" => {
            let [value] = arguments else {
                return Err("is_null() expects exactly 1 argument".to_owned());
            };
            let value = context.decode(*value)?;
            let value = match value {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            context.encode(Value::Bool(matches!(value, Value::Null)))
        }
        "is_array" => {
            let [value] = arguments else {
                return Err("is_array() expects exactly 1 argument".to_owned());
            };
            let value = context.decode(*value)?;
            let value = match value {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            context.encode(Value::Bool(matches!(value, Value::Array(_))))
        }
        "strlen" => {
            let [value] = arguments else {
                return Err("strlen() expects exactly 1 argument".to_owned());
            };
            let decoded = context.decode(*value)?;
            let bytes = native_string(decoded.clone()).map_err(|_| {
                format!(
                    "E_PHP_THROW:TypeError:strlen(): Argument #1 ($string) must be of type string, {} given",
                    native_value_type_name(&decoded)
                )
            })?;
            i64::try_from(bytes.len()).map_err(|_| "strlen() result overflow".to_owned())
        }
        "trim" => {
            let [value, ..] = arguments else {
                return Err("trim() expects at least 1 argument".to_owned());
            };
            let bytes = native_string(context.decode(*value)?)?;
            let characters = arguments
                .get(1)
                .map(|value| context.decode(*value))
                .transpose()?
                .map(native_string)
                .transpose()?
                .unwrap_or_else(|| b" \n\r\t\x0b\0".to_vec());
            let start = bytes
                .iter()
                .position(|byte| !characters.contains(byte))
                .unwrap_or(bytes.len());
            let end = bytes
                .iter()
                .rposition(|byte| !characters.contains(byte))
                .map_or(start, |index| index + 1);
            let trimmed = bytes[start..end].to_vec();
            context.encode(Value::String(PhpString::from_bytes(trimmed)))
        }
        "strtoupper" => {
            let [value] = arguments else {
                return Err(
                    "E_PHP_THROW:ArgumentCountError:strtoupper() expects exactly 1 argument"
                        .to_owned(),
                );
            };
            let mut bytes = native_string(context.decode(*value)?).map_err(|_| {
                "E_PHP_THROW:TypeError:strtoupper(): Argument #1 ($string) must be of type string, array given"
                    .to_owned()
            })?;
            bytes.make_ascii_uppercase();
            context.encode(Value::String(PhpString::from_bytes(bytes)))
        }
        "count" => {
            let [value, ..] = arguments else {
                return Err("count() expects an argument".to_owned());
            };
            let value = match context.decode(*value)? {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            if let Value::Object(object) = value {
                if object.class_name().eq_ignore_ascii_case("ArrayIterator")
                    && let Some(Value::Array(entries)) = object.get_property("__entries")
                {
                    return context.encode(Value::Int(entries.len() as i64));
                }
                if let Some(entries) = native_dom_collection_entries(&object) {
                    return context.encode(Value::Int(entries.len() as i64));
                }
                if let Some(count) = native_simple_xml_count(&object) {
                    return context.encode(Value::Int(count));
                }
                let function = native_method_in_hierarchy(context, &object.class_name(), "count")
                    .ok_or_else(|| {
                    "count(): argument must be of type Countable|array".to_owned()
                })?;
                let receiver = context.encode(Value::Object(object))?;
                return invoke_native_function_with_metadata(context, function, &[receiver], None);
            }
            let Value::Array(array) = value else {
                return Err("count(): argument must be an array".to_owned());
            };
            let recursive = arguments
                .get(1)
                .map(|mode| context.decode(*mode))
                .transpose()?
                .is_some_and(|mode| matches!(mode, Value::Int(1)));
            fn count_array(array: &php_runtime::api::PhpArray, recursive: bool) -> usize {
                array.iter().fold(array.len(), |count, (_, value)| {
                    if recursive {
                        match value {
                            Value::Array(nested) => count.saturating_add(count_array(nested, true)),
                            Value::Reference(reference) => match reference.get() {
                                Value::Array(nested) => {
                                    count.saturating_add(count_array(&nested, true))
                                }
                                _ => count,
                            },
                            _ => count,
                        }
                    } else {
                        count
                    }
                })
            }
            i64::try_from(count_array(&array, recursive))
                .map_err(|_| "count() result overflow".to_owned())
        }
        "var_dump" => {
            let mut output = Vec::new();
            for argument in arguments {
                let value = context.decode(*argument)?;
                native_var_dump_with_context(context, &value, 0, &mut output)?;
            }
            context.output.write_bytes(output);
            context.encode(Value::Null)
        }
        "get_class" => {
            let Some(value) = arguments.first() else {
                return Err("get_class() without an object context is unavailable".to_owned());
            };
            let value = match context.decode(*value)? {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            let class = match value {
                Value::Object(object) => object.display_name(),
                Value::Array(exception) => {
                    let key = php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                        b"class".to_vec(),
                    ));
                    match exception.get(&key) {
                        Some(Value::String(class)) => class.to_string_lossy(),
                        _ => return Err("get_class(): argument must be an object".to_owned()),
                    }
                }
                _ => return Err("get_class(): argument must be an object".to_owned()),
            };
            context.encode(Value::String(PhpString::from_bytes(class.into_bytes())))
        }
        "get_parent_class" => {
            let Some(value) = arguments.first() else {
                return context.encode(Value::Bool(false));
            };
            let class_name = match context.decode(*value)? {
                Value::Reference(reference) => match reference.get() {
                    Value::Object(object) => object.class_name(),
                    Value::String(name) => name.to_string_lossy(),
                    _ => return context.encode(Value::Bool(false)),
                },
                Value::Object(object) => object.class_name(),
                Value::String(name) => name.to_string_lossy(),
                _ => return context.encode(Value::Bool(false)),
            };
            let Some(parent) =
                native_builtin_class(context, &class_name).and_then(|class| class.parent)
            else {
                return context.encode(Value::Bool(false));
            };
            let display =
                native_builtin_class(context, &parent).map_or(parent, |class| class.display_name);
            context.encode(Value::String(PhpString::from_bytes(display.into_bytes())))
        }
        "is_subclass_of" => {
            let [target, parent, rest @ ..] = arguments else {
                return Err("is_subclass_of() expects 2 or 3 arguments".to_owned());
            };
            let target_value = match context.decode(*target)? {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            let allow_string = rest
                .first()
                .map(|value| context.decode(*value))
                .transpose()?
                .is_none_or(|value| native_property_truthy(&value));
            let class_name = match target_value {
                Value::Object(object) => object.class_name(),
                Value::String(name) if allow_string => name.to_string_lossy(),
                _ => return context.encode(Value::Bool(false)),
            };
            let parent = String::from_utf8_lossy(&native_string(context.decode(*parent)?)?)
                .to_ascii_lowercase();
            let mut current =
                native_builtin_class(context, &class_name).and_then(|class| class.parent);
            while let Some(candidate) = current {
                if normalize_class_name(&candidate) == parent {
                    return context.encode(Value::Bool(true));
                }
                current = native_builtin_class(context, &candidate).and_then(|class| class.parent);
            }
            context.encode(Value::Bool(false))
        }
        "sys_get_temp_dir" => context.encode(Value::String(PhpString::from_bytes(
            std::env::temp_dir().to_string_lossy().as_bytes().to_vec(),
        ))),
        "chdir" => {
            let [directory] = arguments else {
                return Err("chdir() expects exactly 1 argument".to_owned());
            };
            let directory = native_string(context.decode(*directory)?)?;
            let directory =
                std::path::PathBuf::from(String::from_utf8_lossy(&directory).into_owned());
            let resolved = if directory.is_absolute() {
                directory
            } else {
                context.cwd.join(directory)
            };
            let resolved = resolved.canonicalize().map_err(|error| error.to_string())?;
            if !resolved.is_dir() {
                return context.encode(Value::Bool(false));
            }
            context.cwd = resolved;
            context.encode(Value::Bool(true))
        }
        "getcwd" => context.encode(Value::String(PhpString::from_bytes(
            context.cwd.to_string_lossy().as_bytes().to_vec(),
        ))),
        "getenv" => {
            let name = arguments
                .first()
                .map(|name| context.decode(*name))
                .transpose()?;
            if name.as_ref().is_none_or(|name| matches!(name, Value::Null)) {
                let mut values = php_runtime::api::PhpArray::new();
                for (name, value) in &context.environment {
                    values.insert(
                        php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                            name.as_bytes().to_vec(),
                        )),
                        Value::String(PhpString::from_bytes(value.as_bytes().to_vec())),
                    );
                }
                context.encode(Value::Array(values))
            } else if let Some(name) = name {
                let name = String::from_utf8_lossy(&native_string(name)?).into_owned();
                let value = context
                    .environment
                    .iter()
                    .find(|(candidate, _)| candidate == &name)
                    .map_or(Value::Bool(false), |(_, value)| {
                        Value::String(PhpString::from_bytes(value.as_bytes().to_vec()))
                    });
                context.encode(value)
            } else {
                context.encode(Value::Bool(false))
            }
        }
        "putenv" => {
            let Some(assignment) = arguments.first() else {
                return Err("putenv() expects exactly 1 argument".to_owned());
            };
            let assignment =
                String::from_utf8_lossy(&native_string(context.decode(*assignment)?)?).into_owned();
            if assignment.is_empty() {
                return Err("E_PHP_THROW:ValueError:putenv(): Argument #1 ($assignment) must have a valid syntax".to_owned());
            }
            let (name, value) = assignment
                .split_once('=')
                .map_or((assignment.as_str(), None), |(name, value)| {
                    (name, Some(value.to_owned()))
                });
            if name.is_empty() {
                return Err("E_PHP_THROW:ValueError:putenv(): Argument #1 ($assignment) must have a valid syntax".to_owned());
            }
            context
                .environment
                .retain(|(candidate, _)| candidate != name);
            if let Some(value) = value {
                context.environment.push((name.to_owned(), value));
                context.environment.sort();
            }
            context.encode(Value::Bool(true))
        }
        "php_sapi_name" => context.encode(Value::String(PhpString::from_bytes(
            context
                .options
                .runtime_context
                .sapi_name
                .as_bytes()
                .to_vec(),
        ))),
        "php_uname" => {
            let mode = arguments
                .first()
                .map(|mode| context.decode(*mode))
                .transpose()?
                .map(native_string)
                .transpose()?
                .map_or(b'a', |mode| mode.first().copied().unwrap_or(b'a'))
                .to_ascii_lowercase();
            let version = php_source::reference_php_version();
            let value = match mode {
                b's' => "Phrust".to_owned(),
                b'n' => "localhost".to_owned(),
                b'r' => version.to_owned(),
                b'v' => "Stdlib".to_owned(),
                b'm' => "generic".to_owned(),
                _ => format!("Phrust localhost {version} Stdlib generic"),
            };
            context.encode(Value::String(PhpString::from_bytes(value.into_bytes())))
        }
        "get_current_user" => {
            context.encode(Value::String(PhpString::from_bytes(b"phrust".to_vec())))
        }
        "ignore_user_abort" => {
            if arguments.len() > 1 {
                return Err("ignore_user_abort() expects at most 1 argument".to_owned());
            }
            let previous = context
                .ini_registry
                .get("ignore_user_abort")
                .is_some_and(|value| value != "0" && !value.is_empty());
            if let Some(value) = arguments.first() {
                let enabled = php_runtime::api::to_bool(&context.decode(*value)?)?;
                context
                    .ini_registry
                    .set("ignore_user_abort", if enabled { "1" } else { "0" });
            }
            context.encode(Value::Int(i64::from(previous)))
        }
        "ini_set" | "set_include_path" => {
            let (name, value) = if normalized == "set_include_path" {
                let [value] = arguments else {
                    return Err("set_include_path() expects exactly 1 argument".to_owned());
                };
                ("include_path".to_owned(), context.decode(*value)?)
            } else {
                let [name, value] = arguments else {
                    return Err("ini_set() expects exactly 2 arguments".to_owned());
                };
                (
                    String::from_utf8_lossy(&native_string(context.decode(*name)?)?).into_owned(),
                    context.decode(*value)?,
                )
            };
            let value = if normalized == "ini_set" {
                php_runtime::api::to_string(&value)
                    .map_err(|error| format!("ini_set(): argument #2: {error}"))?
                    .to_string_lossy()
            } else {
                String::from_utf8_lossy(&native_string(value)?).into_owned()
            };
            let previous = context.ini_registry.set(&name, &value);
            if name.eq_ignore_ascii_case("include_path") && previous.is_some() {
                context.include_path =
                    std::env::split_paths(std::ffi::OsStr::new(&value)).collect();
            }
            context.encode(previous.map_or(Value::Bool(false), |previous| {
                Value::String(PhpString::from_bytes(previous.into_bytes()))
            }))
        }
        "ini_get" | "get_include_path" => {
            let name = if normalized == "get_include_path" {
                "include_path".to_owned()
            } else {
                let [name] = arguments else {
                    return Err("ini_get() expects exactly 1 argument".to_owned());
                };
                String::from_utf8_lossy(&native_string(context.decode(*name)?)?).into_owned()
            };
            context.encode(
                context
                    .ini_registry
                    .get(&name)
                    .map_or(Value::Bool(false), |value| {
                        Value::String(PhpString::from_bytes(value.as_bytes().to_vec()))
                    }),
            )
        }
        "get_cfg_var" => {
            let [name] = arguments else {
                return Err("get_cfg_var() expects exactly 1 argument".to_owned());
            };
            let name =
                String::from_utf8_lossy(&native_string(context.decode(*name)?)?).into_owned();
            context.encode(
                context
                    .ini_registry
                    .cfg_var(&name)
                    .map_or(Value::Bool(false), |value| {
                        Value::String(PhpString::from_bytes(value.as_bytes().to_vec()))
                    }),
            )
        }
        "ini_get_all" => {
            let extension = arguments
                .first()
                .map(|value| context.decode(*value))
                .transpose()?
                .and_then(|value| match value {
                    Value::Null => None,
                    value => native_string(value)
                        .ok()
                        .map(|value| String::from_utf8_lossy(&value).into_owned()),
                });
            let details = arguments
                .get(1)
                .map(|value| {
                    context
                        .decode(*value)
                        .map(|value| native_property_truthy(&value))
                })
                .transpose()?
                .unwrap_or(true);
            let entries = extension.as_deref().map_or_else(
                || context.ini_registry.entries(),
                |extension| context.ini_registry.entries_for_extension(extension),
            );
            let mut result = php_runtime::api::PhpArray::new();
            for entry in entries {
                let value = if details {
                    let mut detail = php_runtime::api::PhpArray::new();
                    for (name, value) in [
                        (
                            "global_value",
                            Value::String(PhpString::from_bytes(entry.global_value.into_bytes())),
                        ),
                        (
                            "local_value",
                            Value::String(PhpString::from_bytes(entry.local_value.into_bytes())),
                        ),
                        ("access", Value::Int(entry.access)),
                    ] {
                        detail.insert(
                            php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                                name.as_bytes().to_vec(),
                            )),
                            value,
                        );
                    }
                    Value::Array(detail)
                } else {
                    Value::String(PhpString::from_bytes(entry.local_value.into_bytes()))
                };
                result.insert(
                    php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                        entry.name.as_bytes().to_vec(),
                    )),
                    value,
                );
            }
            context.encode(Value::Array(result))
        }
        "tempnam" => {
            let [directory, prefix, ..] = arguments else {
                return Err("tempnam() expects a directory and prefix".to_owned());
            };
            let directory = native_string(context.decode(*directory)?)?;
            let prefix = native_string(context.decode(*prefix)?)?;
            let directory =
                std::path::PathBuf::from(String::from_utf8_lossy(&directory).into_owned());
            let directory = if directory.is_absolute() {
                directory
            } else {
                context.cwd.join(directory)
            };
            if !context
                .options
                .runtime_context
                .filesystem
                .allows_path(&directory)
            {
                return context.encode(Value::Bool(false));
            }
            let prefix = String::from_utf8_lossy(&prefix);
            let mut created = None;
            for _ in 0..1_024 {
                let sequence =
                    NATIVE_TEMPNAM_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let path = directory.join(format!("{prefix}{:x}{sequence:x}", std::process::id()));
                match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                {
                    Ok(_) => {
                        created = Some(path);
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.to_string()),
                }
            }
            let path =
                created.ok_or_else(|| "tempnam() could not create a unique file".to_owned())?;
            context.encode(Value::String(PhpString::from_bytes(
                path.to_string_lossy().as_bytes().to_vec(),
            )))
        }
        "fopen" => {
            let [path, mode] = arguments else {
                return Err("fopen() expects exactly 2 arguments".to_owned());
            };
            let path = native_string(context.decode(*path)?)?;
            let mode = native_string(context.decode(*mode)?)?;
            let path_text = String::from_utf8_lossy(&path).into_owned();
            let mode = String::from_utf8_lossy(&mode);
            let resource = php_runtime::api::StreamWrapperRegistry::new()
                .open(
                    &mut context.resources,
                    &path_text,
                    &mode,
                    &context.cwd,
                    &context.options.runtime_context.filesystem,
                    &context.options.runtime_context.stdin,
                )
                .map_err(|error| error.message().to_owned())?;
            context.encode(Value::Resource(resource))
        }
        "fwrite" => {
            let [resource, data, ..] = arguments else {
                return Err("fwrite() expects at least 2 arguments".to_owned());
            };
            let data = native_string(context.decode(*data)?)?;
            if let Value::Resource(resource) = context.decode(*resource)? {
                let written = resource
                    .write_bytes(&data)
                    .map_err(|error| format!("fwrite() failed to write stream resource: {error}"));
                let written = written?;
                match resource.metadata().uri.as_str() {
                    "php://stdout" => context.output.write_bytes(&data[..written]),
                    "php://stderr" => {
                        use std::io::Write as _;
                        std::io::stderr()
                            .lock()
                            .write_all(&data[..written])
                            .map_err(|error| format!("fwrite() failed to write stderr: {error}"))?;
                    }
                    _ => {}
                }
                return Ok(written as i64);
            }
            Err("fwrite() expects a stream resource".to_owned())
        }
        "fclose" => {
            let [resource] = arguments else {
                return Err("fclose() expects exactly 1 argument".to_owned());
            };
            if let Value::Resource(resource) = context.decode(*resource)? {
                return context.encode(Value::Bool(resource.close()));
            }
            Err("fclose() expects a stream resource".to_owned())
        }
        "file_get_contents" => {
            let [path, ..] = arguments else {
                return Err("file_get_contents() expects a path".to_owned());
            };
            let path = native_string(context.decode(*path)?)?;
            let bytes = std::fs::read(String::from_utf8_lossy(&path).as_ref())
                .map_err(|error| error.to_string())?;
            context.encode(Value::String(PhpString::from_bytes(bytes)))
        }
        "file_put_contents" => {
            use std::io::Write as _;
            let [path, data, rest @ ..] = arguments else {
                return Err("file_put_contents() expects a path and data".to_owned());
            };
            let path = native_string(context.decode(*path)?)?;
            let data = native_string(context.decode(*data)?)?;
            let flags = rest
                .first()
                .map(|flags| context.decode(*flags))
                .transpose()?
                .and_then(|flags| match flags {
                    Value::Int(flags) => Some(flags),
                    _ => None,
                })
                .unwrap_or(0);
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true);
            if flags & 8 != 0 {
                options.append(true);
            } else {
                options.truncate(true);
            }
            let mut file = options
                .open(String::from_utf8_lossy(&path).as_ref())
                .map_err(|error| error.to_string())?;
            file.write_all(&data).map_err(|error| error.to_string())?;
            i64::try_from(data.len()).map_err(|_| "file_put_contents() result overflow".to_owned())
        }
        "unlink" => {
            let [path, ..] = arguments else {
                return Err("unlink() expects a path".to_owned());
            };
            let path = native_string(context.decode(*path)?)?;
            std::fs::remove_file(String::from_utf8_lossy(&path).as_ref())
                .map_err(|error| error.to_string())?;
            context.encode(Value::Bool(true))
        }
        "call_user_func" | "forward_static_call" => {
            let [callback, call_arguments @ ..] = arguments else {
                return Err("call_user_func() expects a callback".to_owned());
            };
            let callback = context.decode(*callback)?;
            let unresolved_name = match &callback {
                Value::String(name) => {
                    let name = name.to_string_lossy();
                    (context.function_id(&name).is_none()
                        && context.external_function(&name).is_none()
                        && php_std::arginfo::function_metadata_indexed(&name).is_none())
                    .then_some(name)
                }
                _ => None,
            };
            let mut values = call_arguments
                .iter()
                .map(|argument| context.decode(*argument))
                .collect::<Result<Vec<_>, _>>()?;
            if let Value::String(name) = &callback {
                let name = name.to_string_lossy();
                if let Some(function) = context
                    .function_id(&name)
                    .and_then(|function| context.unit.functions.get(function.index()))
                {
                    for (index, parameter) in function.params.iter().enumerate() {
                        if parameter.by_ref
                            && values
                                .get(index)
                                .is_some_and(|value| !matches!(value, Value::Reference(_)))
                        {
                            let path = context
                                .unit
                                .files
                                .get(source.span.file.index())
                                .map_or("<unknown>", |file| file.path.as_str());
                            let line = native_source_line(context, source);
                            context.output.write_bytes(format!(
                                "\nWarning: {}(): Argument #{} (${}) must be passed by reference, value given in {} on line {}\n",
                                function.name,
                                index + 1,
                                parameter.name,
                                path,
                                line
                            ));
                            if let Some(value) = values.get_mut(index) {
                                *value = Value::Reference(php_runtime::api::ReferenceCell::new(
                                    value.clone(),
                                ));
                            }
                        }
                    }
                }
            }
            if let Some(name) = unresolved_name {
                return Err(format!(
                    "E_PHP_VM_UNRESOLVED_CALLABLE: function {name} is not defined"
                ));
            }
            invoke_native_callable_value_from(
                context,
                callback,
                &values,
                source,
                None,
                caller_locals.map(|(function, _)| function),
            )
        }
        "spl_autoload_register" => {
            let Some(callback) = arguments.first() else {
                return Err("spl_autoload_register() expects a callback".to_owned());
            };
            let callback = context.decode(*callback)?;
            context.autoload_callbacks.push(callback);
            context.encode(Value::Bool(true))
        }
        "spl_autoload_unregister" => {
            let Some(callback) = arguments.first() else {
                return Err("spl_autoload_unregister() expects a callback".to_owned());
            };
            let callback = context.decode(*callback)?;
            let previous = context.autoload_callbacks.len();
            context
                .autoload_callbacks
                .retain(|candidate| candidate != &callback);
            context.encode(Value::Bool(context.autoload_callbacks.len() != previous))
        }
        "spl_autoload_functions" => context.encode(Value::Array(
            php_runtime::api::PhpArray::from_packed(context.autoload_callbacks.clone()),
        )),
        "register_shutdown_function" => {
            let Some((callback, arguments)) = arguments.split_first() else {
                return Err("register_shutdown_function() expects a callback".to_owned());
            };
            let callback = context.decode(*callback)?;
            let arguments = arguments
                .iter()
                .map(|argument| context.decode(*argument))
                .collect::<Result<Vec<_>, _>>()?;
            context.shutdown_callbacks.push(NativeShutdownCallback {
                callable: callback,
                arguments,
                source: source.clone(),
            });
            context.encode(Value::Null)
        }
        "class_alias" => {
            let [original, alias, ..] = arguments else {
                return Err("class_alias() expects an original and alias".to_owned());
            };
            let original =
                String::from_utf8_lossy(&native_string(context.decode(*original)?)?).into_owned();
            let alias =
                String::from_utf8_lossy(&native_string(context.decode(*alias)?)?).into_owned();
            let normalized_original = normalize_class_name(&original);
            let normalized_alias = normalize_class_name(&alias);
            let exists = context
                .unit
                .classes
                .iter()
                .any(|class| class.name == normalized_original)
                || native_external_class(context, &normalized_original).is_some();
            if !exists {
                return context.encode(Value::Bool(false));
            }
            context
                .class_aliases
                .insert(normalized_alias.clone(), normalized_original);
            context.dynamic_classes.insert(normalized_alias);
            context.encode(Value::Bool(true))
        }
        "get_object_vars" | "get_mangled_object_vars" => {
            let Some(object) = arguments.first() else {
                return Err(format!("{normalized}() expects exactly 1 argument"));
            };
            let object = match context.decode(*object)? {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            let Value::Object(object) = object else {
                return Err(format!(
                    "E_PHP_THROW:TypeError:{normalized}(): Argument #1 ($object) must be of type object"
                ));
            };
            let caller_class = native_builtin_caller_class(context, caller_locals);
            context.encode(Value::Array(native_object_vars(
                context,
                &object,
                caller_class.as_deref(),
                normalized == "get_mangled_object_vars",
            )))
        }
        "get_class_methods" => {
            let Some(target) = arguments.first() else {
                return Err("get_class_methods() expects exactly 1 argument".to_owned());
            };
            let class_name = match context.decode(*target)? {
                Value::Reference(reference) => match reference.get() {
                    Value::Object(object) => object.class_name(),
                    Value::String(name) => name.to_string_lossy(),
                    _ => return context.encode(Value::Bool(false)),
                },
                Value::Object(object) => object.class_name(),
                Value::String(name) => name.to_string_lossy(),
                _ => return context.encode(Value::Bool(false)),
            };
            let caller_class = native_builtin_caller_class(context, caller_locals);
            let mut seen = std::collections::BTreeSet::new();
            let mut methods = php_runtime::api::PhpArray::new();
            for class in native_builtin_class_lineage(context, &class_name) {
                for method in class.methods {
                    let visible = !method.flags.is_private && !method.flags.is_protected
                        || caller_class.as_deref() == Some(class.name.as_str());
                    if visible && seen.insert(method.name.to_ascii_lowercase()) {
                        let display_name = context
                            .unit
                            .functions
                            .get(method.function.index())
                            .and_then(|function| function.name.rsplit_once("::"))
                            .map_or(method.name.as_str(), |(_, name)| name);
                        methods.append(Value::String(PhpString::from_bytes(
                            display_name.as_bytes().to_vec(),
                        )));
                    }
                }
            }
            context.encode(Value::Array(methods))
        }
        "get_class_vars" => {
            let Some(target) = arguments.first() else {
                return Err("get_class_vars() expects exactly 1 argument".to_owned());
            };
            let class_name =
                String::from_utf8_lossy(&native_string(context.decode(*target)?)?).into_owned();
            let caller_class = native_builtin_caller_class(context, caller_locals);
            let mut properties = php_runtime::api::PhpArray::new();
            for class in native_builtin_class_lineage(context, &class_name)
                .into_iter()
                .rev()
            {
                for property in class.properties {
                    if property.flags.is_static
                        || !native_property_visible_from(
                            &property,
                            &class.name,
                            caller_class.as_deref(),
                        )
                    {
                        continue;
                    }
                    let value = property
                        .default
                        .and_then(|constant| context.unit.constants.get(constant.index()))
                        .map(ir_constant_value)
                        .transpose()?
                        .unwrap_or_else(|| {
                            if property.flags.is_typed {
                                Value::Uninitialized
                            } else {
                                Value::Null
                            }
                        });
                    if !matches!(value, Value::Uninitialized) {
                        properties.insert(
                            php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                                property.name.into_bytes(),
                            )),
                            value,
                        );
                    }
                }
            }
            context.encode(Value::Array(properties))
        }
        "function_exists" => {
            let Some(name) = arguments.first() else {
                return Err("function_exists() expects exactly 1 argument".to_owned());
            };
            let name = String::from_utf8_lossy(&native_string(context.decode(*name)?)?)
                .to_ascii_lowercase();
            let exists = context.function_id(&name).is_some()
                || context.external_function(&name).is_some()
                || context.visible_function_names.contains(&name)
                || native_php_function_exists(&name);
            context.encode(Value::Bool(exists))
        }
        "method_exists" | "property_exists" => {
            let [target, member] = arguments else {
                return Err(format!("{normalized}() expects exactly 2 arguments"));
            };
            let target = native_dereference_value(context.decode(*target)?);
            let (class_name, object) = match target {
                Value::Object(object) => (object.class_name(), Some(object)),
                Value::String(class) => (class.to_string_lossy(), None),
                _ => return context.encode(Value::Bool(false)),
            };
            let member =
                String::from_utf8_lossy(&native_string(context.decode(*member)?)?).into_owned();
            let exists = (normalized == "property_exists"
                && object
                    .as_ref()
                    .is_some_and(|object| object.get_property(&member).is_some()))
                || native_builtin_class_lineage(context, &class_name)
                    .into_iter()
                    .any(|class| {
                        if normalized == "method_exists" {
                            class
                                .methods
                                .iter()
                                .any(|method| method.name.eq_ignore_ascii_case(&member))
                        } else {
                            class
                                .properties
                                .iter()
                                .any(|property| property.name == member)
                        }
                    })
                || (normalized == "method_exists"
                    && php_std::ExtensionRegistry::standard_library()
                        .enabled_class(&class_name)
                        .is_some()
                    && php_std::generated::arginfo::method_metadata_in_hierarchy(
                        &class_name,
                        &member,
                    )
                    .is_some())
                || (normalized == "property_exists"
                    && php_std::ExtensionRegistry::standard_library()
                        .enabled_class(&class_name)
                        .is_some()
                    && php_std::generated::arginfo::property_metadata_in_hierarchy(
                        &class_name,
                        &member,
                    )
                    .is_some());
            context.encode(Value::Bool(exists))
        }
        "class_exists" | "interface_exists" | "trait_exists" | "enum_exists" => {
            let Some(name) = arguments.first() else {
                return Err(format!("{normalized}() expects a class name"));
            };
            let name =
                String::from_utf8_lossy(&native_string(context.decode(*name)?)?).into_owned();
            let normalized_name = normalize_class_name(&name);
            let matches_kind = |class: &php_ir::ClassEntry| match normalized.as_str() {
                "interface_exists" => class.flags.is_interface,
                "trait_exists" => class.flags.is_trait,
                "enum_exists" => class.flags.is_enum,
                _ => !class.flags.is_interface && !class.flags.is_trait,
            };
            let matches_internal_kind = |kind: php_std::ClassKind| match normalized.as_str() {
                "interface_exists" => kind == php_std::ClassKind::Interface,
                "trait_exists" => kind == php_std::ClassKind::Trait,
                "enum_exists" => kind == php_std::ClassKind::Enum,
                _ => matches!(kind, php_std::ClassKind::Class | php_std::ClassKind::Enum),
            };
            let mut exists = context
                .unit
                .classes
                .iter()
                .find(|class| {
                    class.name == normalized_name
                        && (!class.flags.is_conditional
                            || context.dynamic_classes.contains(&class.name))
                })
                .is_some_and(matches_kind)
                || native_external_class(context, &normalized_name)
                    .is_some_and(|(_, class)| matches_kind(&class))
                || php_std::ExtensionRegistry::standard_library()
                    .enabled_class(&normalized_name)
                    .is_some_and(|class| matches_internal_kind(class.kind()));
            if normalized == "class_exists"
                && matches!(
                    normalized_name.as_str(),
                    "exception"
                        | "error"
                        | "typeerror"
                        | "valueerror"
                        | "argumentcounterror"
                        | "fibererror"
                )
            {
                exists = true;
            }
            let autoload = arguments
                .get(1)
                .map(|value| context.decode(*value))
                .transpose()?
                .is_none_or(|value| native_property_truthy(&value));
            if !exists && autoload && context.autoload_in_progress.insert(normalized_name.clone()) {
                let callbacks = context.autoload_callbacks.clone();
                let mut callback_error = None;
                for callback in callbacks {
                    if let Err(error) = invoke_native_callable_value(
                        context,
                        callback,
                        &[Value::String(PhpString::from_bytes(
                            name.as_bytes().to_vec(),
                        ))],
                        source,
                        None,
                    ) {
                        callback_error = Some(error);
                        break;
                    }
                    if context.dynamic_classes.contains(&normalized_name) {
                        exists = true;
                        break;
                    }
                }
                context.autoload_in_progress.remove(&normalized_name);
                if let Some(error) = callback_error {
                    return Err(error);
                }
            }
            context.encode(Value::Bool(exists))
        }
        "call_user_func_array" => {
            let [callback, arguments] = arguments else {
                return Err("call_user_func_array() expects exactly 2 arguments".to_owned());
            };
            let callback = match context.decode(*callback)? {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            let arguments = match context.decode(*arguments)? {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            let Value::Array(arguments) = arguments else {
                return Err("call_user_func_array(): argument #2 must be an array".to_owned());
            };
            let mut values = Vec::with_capacity(arguments.len());
            let mut metadata = Vec::with_capacity(arguments.len());
            for (key, value) in arguments.iter() {
                values.push(value.clone());
                metadata.push(php_ir::instruction::IrCallArg {
                    name: match key {
                        php_runtime::api::ArrayKey::Int(_) => None,
                        php_runtime::api::ArrayKey::String(name) => Some(name.to_string_lossy()),
                    },
                    value: php_ir::Operand::Register(php_ir::RegId::new(0)),
                    unpack: false,
                    value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
                    by_ref_local: None,
                    by_ref_dim: None,
                    by_ref_property: None,
                    by_ref_property_dim: None,
                });
            }
            if let Value::String(name) = &callback {
                let name = name.to_string_lossy();
                if let Some(function) = context
                    .function_id(&name)
                    .and_then(|function| context.unit.functions.get(function.index()))
                {
                    for (index, parameter) in function.params.iter().enumerate() {
                        if parameter.by_ref
                            && values
                                .get(index)
                                .is_some_and(|value| !matches!(value, Value::Reference(_)))
                        {
                            let path = context
                                .unit
                                .files
                                .get(source.span.file.index())
                                .map_or("<unknown>", |file| file.path.as_str());
                            let line = native_source_line(context, source);
                            context.output.write_bytes(format!(
                                "\nWarning: {}(): Argument #{} (${}) must be passed by reference, value given in {} on line {}\n",
                                function.name,
                                index + 1,
                                parameter.name,
                                path,
                                line
                            ));
                            if let Some(value) = values.get_mut(index) {
                                *value = Value::Reference(php_runtime::api::ReferenceCell::new(
                                    value.clone(),
                                ));
                            }
                        }
                    }
                }
            }
            let callback_label = match &callback {
                Value::String(name) => name.to_string_lossy(),
                Value::Callable(callable) => match callable.as_ref() {
                    php_runtime::api::CallableValue::UserFunction { name }
                    | php_runtime::api::CallableValue::InternalBuiltin { name } => name.clone(),
                    php_runtime::api::CallableValue::BoundMethod { method, .. } => method.clone(),
                    php_runtime::api::CallableValue::Closure(_) => "Closure".to_owned(),
                    php_runtime::api::CallableValue::MethodPlaceholder { target }
                    | php_runtime::api::CallableValue::UnresolvedDynamic { target } => {
                        target.clone()
                    }
                },
                _ => "dynamic callable".to_owned(),
            };
            invoke_native_callable_value_from(
                context,
                callback,
                &values,
                source,
                Some(metadata),
                caller_locals.map(|(function, _)| function),
            )
            .map_err(|error| {
                if error.starts_with("native runtime value ") {
                    format!("native callback {callback_label} failed: {error}")
                } else {
                    error
                }
            })
        }
        "func_num_args" => {
            let count = context.call_arguments.last().map_or(0, Vec::len);
            context.encode(Value::Int(i64::try_from(count).unwrap_or(i64::MAX)))
        }
        "debug_backtrace" => {
            let options =
                arguments
                    .first()
                    .map_or(Ok(1), |argument| match context.decode(*argument)? {
                        Value::Int(options) => Ok(options),
                        _ => Err("debug_backtrace(): argument #1 must be of type int".to_owned()),
                    })?;
            let limit = arguments.get(1).map_or(Ok(0), |argument| {
                match context.decode(*argument)? {
                    Value::Int(limit) if limit >= 0 => Ok(limit),
                    Value::Int(_) => Err(
                        "debug_backtrace(): argument #2 ($limit) must be greater than or equal to 0"
                            .to_owned(),
                    ),
                    _ => Err("debug_backtrace(): argument #2 must be of type int".to_owned()),
                }
            })?;
            let limit = usize::try_from(limit).unwrap_or(usize::MAX);
            let frames = context
                .call_frames
                .iter()
                .rev()
                .take(if limit == 0 { usize::MAX } else { limit })
                .map(|frame| {
                    let mut value = php_runtime::api::PhpArray::new();
                    let mut insert = |key: &str, entry: Value| {
                        value.insert(
                            php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                                key.as_bytes().to_vec(),
                            )),
                            entry,
                        );
                    };
                    if let Some(file) = frame.file.as_ref() {
                        insert(
                            "file",
                            Value::String(PhpString::from_bytes(file.as_bytes().to_vec())),
                        );
                    }
                    if frame.line > 0 {
                        insert("line", Value::Int(frame.line));
                    }
                    insert(
                        "function",
                        Value::String(PhpString::from_bytes(frame.function.as_bytes().to_vec())),
                    );
                    if let Some(class) = frame.class.as_ref() {
                        insert(
                            "class",
                            Value::String(PhpString::from_bytes(class.as_bytes().to_vec())),
                        );
                    }
                    if let Some(call_type) = frame.call_type {
                        insert(
                            "type",
                            Value::String(PhpString::from_bytes(call_type.as_bytes().to_vec())),
                        );
                    }
                    if options & 1 != 0
                        && let Some(object) = frame.object.as_ref()
                    {
                        insert("object", Value::Object(object.clone()));
                    }
                    if options & 2 == 0 {
                        insert(
                            "args",
                            Value::Array(php_runtime::api::PhpArray::from_packed(
                                frame.arguments.clone(),
                            )),
                        );
                    }
                    Value::Array(value)
                })
                .collect::<Vec<_>>();
            context.encode(Value::Array(php_runtime::api::PhpArray::from_packed(
                frames,
            )))
        }
        "func_get_arg" => {
            let Some(index) = arguments.first() else {
                return Err("func_get_arg() expects exactly 1 argument".to_owned());
            };
            let Value::Int(index) = context.decode(*index)? else {
                return Err("func_get_arg(): argument #1 must be of type int".to_owned());
            };
            let Some(value) = usize::try_from(index)
                .ok()
                .and_then(|index| context.call_arguments.last()?.get(index))
                .cloned()
            else {
                return Err(format!(
                    "func_get_arg(): argument #{index} not passed to function"
                ));
            };
            context.encode(value)
        }
        "is_callable" => {
            let Some(value) = arguments.first() else {
                return Err("is_callable() expects a value".to_owned());
            };
            let value = context.decode(*value)?;
            let autoload_class = match &value {
                Value::String(value) => value
                    .to_string_lossy()
                    .split_once("::")
                    .map(|(class, _)| class.to_owned()),
                Value::Array(array) => {
                    array
                        .get(&php_runtime::api::ArrayKey::Int(0))
                        .and_then(|value| match value {
                            Value::String(class) => Some(class.to_string_lossy()),
                            _ => None,
                        })
                }
                _ => None,
            };
            if let Some(class) = autoload_class {
                native_autoload_class(context, &class, source)?;
            }
            let callable = native_value_is_callable(context, &value);
            context.encode(Value::Bool(callable))
        }
        "get_defined_functions" => {
            let internal = php_extensions::BuiltinRegistry::new()
                .entries()
                .iter()
                .map(|entry| Value::String(PhpString::from_bytes(entry.name().as_bytes().to_vec())))
                .collect::<Vec<_>>();
            let user = context
                .unit
                .function_table
                .iter()
                .map(|entry| Value::String(PhpString::from_bytes(entry.name.as_bytes().to_vec())))
                .collect::<Vec<_>>();
            let mut functions = php_runtime::api::PhpArray::new();
            functions.insert(
                php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"internal".to_vec())),
                Value::Array(php_runtime::api::PhpArray::from_packed(internal)),
            );
            functions.insert(
                php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"user".to_vec())),
                Value::Array(php_runtime::api::PhpArray::from_packed(user)),
            );
            context.encode(Value::Array(functions))
        }
        "get_declared_classes" | "get_declared_interfaces" | "get_declared_traits" => {
            let names = context
                .unit
                .classes
                .iter()
                .filter(|class| match normalized.as_str() {
                    "get_declared_interfaces" => class.flags.is_interface,
                    "get_declared_traits" => class.flags.is_trait,
                    _ => !class.flags.is_interface && !class.flags.is_trait,
                })
                .map(|class| {
                    Value::String(PhpString::from_bytes(
                        class.display_name.as_bytes().to_vec(),
                    ))
                })
                .collect::<Vec<_>>();
            context.encode(Value::Array(php_runtime::api::PhpArray::from_packed(names)))
        }
        "get_defined_constants" => {
            let categorized = arguments
                .first()
                .map(|value| context.decode(*value))
                .transpose()?
                .is_some_and(|value| native_property_truthy(&value));
            let mut core = php_runtime::api::PhpArray::new();
            for constant in php_std::ExtensionRegistry::standard_library().enabled_constants() {
                if let Some(value) = constant.value() {
                    core.insert(
                        php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                            constant.name().as_bytes().to_vec(),
                        )),
                        php_std::constants::constant_to_value(value),
                    );
                }
            }
            let mut user = php_runtime::api::PhpArray::new();
            for (name, value) in context.visible_include_constants() {
                user.insert(
                    php_runtime::api::ArrayKey::String(PhpString::from_bytes(name.into_bytes())),
                    value,
                );
            }
            if categorized {
                let mut result = php_runtime::api::PhpArray::new();
                result.insert(
                    php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"Core".to_vec())),
                    Value::Array(core),
                );
                result.insert(
                    php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"user".to_vec())),
                    Value::Array(user),
                );
                context.encode(Value::Array(result))
            } else {
                for (key, value) in user.iter() {
                    core.insert(key, value.clone());
                }
                context.encode(Value::Array(core))
            }
        }
        "extension_loaded" => {
            let name = arguments
                .first()
                .ok_or_else(|| "extension_loaded() expects exactly 1 argument".to_owned())?;
            let name = native_string(context.decode(*name)?)?;
            let name = String::from_utf8_lossy(&name);
            let loaded = php_std::introspection::extension_loaded(
                php_std::ExtensionRegistry::standard_library(),
                &name,
            );
            context.encode(Value::Bool(loaded))
        }
        "get_loaded_extensions" => {
            let names = php_std::ExtensionRegistry::standard_library()
                .enabled_extension_names()
                .into_iter()
                .map(|name| Value::String(PhpString::from_bytes(name.as_bytes().to_vec())))
                .collect::<Vec<_>>();
            context.encode(Value::Array(php_runtime::api::PhpArray::from_packed(names)))
        }
        "error_reporting" => {
            let previous = context.error_reporting;
            if let Some(value) = arguments.first() {
                let value = match context.decode(*value)? {
                    Value::Reference(reference) => reference.get(),
                    value => value,
                };
                context.error_reporting = match value {
                    Value::Int(value) => value,
                    _ => return Err("error_reporting() expects an int".to_owned()),
                };
            }
            context.encode(Value::Int(previous))
        }
        "error_get_last" => {
            if !arguments.is_empty() {
                return Err("error_get_last() expects exactly 0 arguments".to_owned());
            }
            context.encode(context.last_error_value())
        }
        "error_clear_last" => {
            if !arguments.is_empty() {
                return Err("error_clear_last() expects exactly 0 arguments".to_owned());
            }
            context.last_error = None;
            context.encode(Value::Null)
        }
        "set_error_handler" => {
            let Some(callback) = arguments.first() else {
                return Err("set_error_handler() expects a callback".to_owned());
            };
            let callback = match context.decode(*callback)? {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            if !native_value_is_callable(context, &callback) {
                return Err(
                    "set_error_handler(): Argument #1 ($callback) must be a valid callback"
                        .to_owned(),
                );
            }
            let previous = context
                .error_handlers
                .last()
                .map(|handler| handler.callback.clone())
                .unwrap_or(Value::Null);
            let levels = arguments
                .get(1)
                .map(|levels| context.decode(*levels))
                .transpose()?
                .map_or(-1, |levels| match levels {
                    Value::Reference(reference) => match reference.get() {
                        Value::Int(levels) => levels,
                        _ => -1,
                    },
                    Value::Int(levels) => levels,
                    _ => -1,
                });
            context
                .error_handlers
                .push(NativeErrorHandler { callback, levels });
            context.encode(previous)
        }
        "restore_error_handler" => {
            let _ = context.error_handlers.pop();
            context.encode(Value::Bool(true))
        }
        "set_exception_handler" => {
            let Some(callback) = arguments.first() else {
                return Err("set_exception_handler() expects a callback".to_owned());
            };
            let callback = match context.decode(*callback)? {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            if !native_value_is_callable(context, &callback) {
                return Err(
                    "set_exception_handler(): Argument #1 ($callback) must be a valid callback"
                        .to_owned(),
                );
            }
            let previous = context
                .exception_handlers
                .last()
                .cloned()
                .unwrap_or(Value::Null);
            context.exception_handlers.push(callback);
            context.encode(previous)
        }
        "restore_exception_handler" => {
            let _ = context.exception_handlers.pop();
            context.encode(Value::Bool(true))
        }
        "get_exception_handler" => context.encode(
            context
                .exception_handlers
                .last()
                .cloned()
                .unwrap_or(Value::Null),
        ),
        "trigger_error" | "user_error" => {
            let Some(message) = arguments.first() else {
                return Err(format!("{normalized}() expects a message"));
            };
            let message =
                String::from_utf8_lossy(&native_string(context.decode(*message)?)?).into_owned();
            let level = arguments
                .get(1)
                .map(|level| context.decode(*level))
                .transpose()?
                .map_or(1024, |level| match level {
                    Value::Int(level) => level,
                    Value::Reference(reference) => match reference.get() {
                        Value::Int(level) => level,
                        _ => 1024,
                    },
                    _ => 1024,
                });
            if !matches!(level, 256 | 512 | 1024 | 16384) {
                return Err(format!(
                    "E_PHP_THROW:ValueError:{normalized}(): Argument #2 ($error_level) must be one of E_USER_ERROR, E_USER_WARNING, E_USER_NOTICE, or E_USER_DEPRECATED"
                ));
            }
            emit_native_php_warning(context, level, &message, source)?;
            context.encode(Value::Bool(true))
        }
        "settype" => {
            let [target, type_name] = arguments else {
                return Err("settype() expects exactly 2 arguments".to_owned());
            };
            let Value::Reference(target) = context.decode(*target)? else {
                return Err("settype(): Argument #1 ($var) must be passed by reference".to_owned());
            };
            let type_name = String::from_utf8_lossy(&native_string(context.decode(*type_name)?)?)
                .to_ascii_lowercase();
            let current = target.get();
            let replacement = match type_name.as_str() {
                "null" => Value::Null,
                "bool" | "boolean" => {
                    if matches!(current, Value::Float(value) if value.to_f64().is_nan()) {
                        emit_native_php_warning(
                            context,
                            2,
                            "unexpected NAN value was coerced to bool",
                            source,
                        )?;
                    }
                    Value::Bool(native_property_truthy(&current))
                }
                "int" | "integer" => match current {
                    Value::String(value) => {
                        let classified =
                            php_runtime::experimental::numeric_string::classify_php_string(&value);
                        Value::Int(classified.value.map_or(0, |value| value.to_i64()))
                    }
                    Value::Float(value) => Value::Int(value.to_f64() as i64),
                    Value::Bool(value) => Value::Int(i64::from(value)),
                    Value::Null | Value::Uninitialized => Value::Int(0),
                    Value::Int(value) => Value::Int(value),
                    _ => Value::Int(1),
                },
                "float" | "double" | "real" => match current {
                    Value::Float(value) => Value::Float(value),
                    Value::Int(value) => {
                        Value::Float(php_runtime::api::FloatValue::from_f64(value as f64))
                    }
                    Value::String(value) => {
                        let classified =
                            php_runtime::experimental::numeric_string::classify_php_string(&value);
                        Value::Float(php_runtime::api::FloatValue::from_f64(
                            classified.value.map_or(0.0, |value| match value {
                                php_runtime::experimental::numeric_string::NumericStringValue::Int(
                                    value,
                                ) => value as f64,
                                php_runtime::experimental::numeric_string::NumericStringValue::Float(
                                    value,
                                ) => value,
                            }),
                        ))
                    }
                    _ => Value::Float(php_runtime::api::FloatValue::from_f64(0.0)),
                },
                "string" => match current {
                    Value::Array(_) => {
                        emit_native_php_warning(context, 2, "Array to string conversion", source)?;
                        Value::String(PhpString::from_bytes(b"Array".to_vec()))
                    }
                    value => Value::String(PhpString::from_bytes(native_string(value)?)),
                },
                "array" => {
                    let nan = matches!(current, Value::Float(value) if value.to_f64().is_nan());
                    if nan {
                        emit_native_php_warning(
                            context,
                            2,
                            "unexpected NAN value was coerced to array",
                            source,
                        )?;
                    }
                    let current = target.get();
                    if nan {
                        Value::Array(php_runtime::api::PhpArray::from_packed(vec![current]))
                    } else {
                        match current {
                            Value::Array(array) => Value::Array(array),
                            Value::Null | Value::Uninitialized => {
                                Value::Array(php_runtime::api::PhpArray::new())
                            }
                            value => {
                                Value::Array(php_runtime::api::PhpArray::from_packed(vec![value]))
                            }
                        }
                    }
                }
                "object" => match current {
                    Value::Object(object) => Value::Object(object),
                    Value::Array(array) => {
                        let object = native_metadata_object("stdClass", std::iter::empty());
                        for (key, value) in array.iter() {
                            let name = match key {
                                php_runtime::api::ArrayKey::Int(key) => key.to_string(),
                                php_runtime::api::ArrayKey::String(key) => key.to_string_lossy(),
                            };
                            object.set_property(name, value.clone());
                        }
                        Value::Object(object)
                    }
                    Value::Null | Value::Uninitialized => {
                        Value::Object(native_metadata_object("stdClass", std::iter::empty()))
                    }
                    value => {
                        let object = native_metadata_object("stdClass", std::iter::empty());
                        object.set_property("scalar", value);
                        Value::Object(object)
                    }
                },
                "resource" => {
                    return Err("E_PHP_THROW:ValueError:Cannot convert to resource type".to_owned());
                }
                _ => {
                    return Err(
                        "E_PHP_THROW:ValueError:settype(): Argument #2 ($type) must be a valid type"
                            .to_owned(),
                    );
                }
            };
            target.set(replacement);
            context.encode(Value::Bool(true))
        }
        "set_time_limit" => {
            let [seconds] = arguments else {
                return Err("set_time_limit() expects exactly 1 argument".to_owned());
            };
            let seconds = match context.decode(*seconds)? {
                Value::Int(seconds) => seconds,
                Value::Reference(reference) => match reference.get() {
                    Value::Int(seconds) => seconds,
                    _ => return Err("set_time_limit() expects an integer".to_owned()),
                },
                _ => return Err("set_time_limit() expects an integer".to_owned()),
            };
            if seconds < 0 {
                return Err(
                    "E_PHP_THROW:ValueError:set_time_limit(): Argument #1 ($seconds) must be greater than or equal to 0"
                        .to_owned(),
                );
            }
            context.reset_execution_deadline_seconds(seconds as u64);
            context.encode(Value::Bool(true))
        }
        _ => {
            let Some(entry) = php_extensions::BuiltinRegistry::new().get(&normalized) else {
                return Err(format!(
                    "E_PHP_THROW:Error:Call to undefined function {name}()"
                ));
            };
            let metadata = php_std::arginfo::function_metadata_indexed(&normalized);
            let mut values = arguments
                .iter()
                .enumerate()
                .map(|(index, argument)| {
                    let value = context.decode(*argument)?;
                    let by_ref = metadata
                        .and_then(|function| {
                            function.params.get(index).or_else(|| {
                                function
                                    .params
                                    .last()
                                    .filter(|parameter| parameter.variadic)
                            })
                        })
                        .is_some_and(|parameter| parameter.by_ref);
                    Ok::<Value, String>(if by_ref {
                        value
                    } else if let Value::Reference(reference) = value {
                        reference.get()
                    } else {
                        value
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            if normalized == "shm_put_var" {
                prepare_native_sysvshm_serialization(context, &mut values)?;
            }
            let span = php_runtime::api::RuntimeSourceSpan {
                file: context
                    .unit
                    .files
                    .get(source.span.file.index())
                    .map(|file| file.path.clone()),
                start: source.span.start,
                end: source.span.end,
            };
            let (result, diagnostics) = {
                let mut builtin = php_runtime::api::BuiltinContext::with_runtime_request_state(
                    &mut context.output,
                    context.cwd.clone(),
                    context.options.runtime_context.filesystem.clone(),
                    Some(&mut context.resources),
                    &mut context.builtin_request_state,
                );
                builtin.set_diagnostic_display(php_runtime::api::PhpDiagnosticDisplayOptions {
                    // Diagnostics are synchronously routed below so native
                    // set_error_handler callbacks see builtin warnings/notices.
                    display_errors: false,
                    error_reporting: context.error_reporting,
                    leading_newline: true,
                });
                builtin.set_include_path(context.include_path.clone());
                builtin.set_ini_registry_state(&mut context.ini_registry);
                builtin.set_default_timezone_state(&mut context.default_timezone);
                builtin.set_env_entries(Arc::clone(&context.options.runtime_context.env));
                if let php_runtime::api::RuntimeRequestMode::Http(request) =
                    &context.options.runtime_context.request_mode
                {
                    builtin.set_php_input(Arc::clone(&request.raw_body));
                }
                builtin.set_filter_input_arrays_shared(Rc::clone(&context.filter_input_arrays));
                builtin.set_http_response_state(&mut context.http_response);
                builtin.set_upload_registry(&mut context.upload_registry);
                builtin.set_session_state(&mut context.session, context.session_global.clone());
                builtin.set_session_loader(context.options.runtime_context.session_loader.as_ref());
                builtin.set_session_id_generator(
                    context
                        .options
                        .runtime_context
                        .session_id_generator
                        .as_ref(),
                );
                builtin.sync_session_state_from_global();
                let mut mysql_state = context.mysql_state.borrow_mut();
                builtin.set_mysql_state(&mut mysql_state);
                context.registered_extensions.bind(&mut builtin);
                let result = (entry.function())(&mut builtin, values, span);
                builtin.sync_session_state_from_global();
                let diagnostics = builtin.take_diagnostics();
                (result, diagnostics)
            };
            for diagnostic in diagnostics {
                let errno = match diagnostic.severity() {
                    php_runtime::api::RuntimeSeverity::Notice => php_runtime::api::PHP_E_NOTICE,
                    php_runtime::api::RuntimeSeverity::Deprecation => {
                        php_runtime::api::PHP_E_DEPRECATED
                    }
                    _ => php_runtime::api::PHP_E_WARNING,
                };
                emit_native_php_diagnostic(context, errno, diagnostic.message(), source, true)?;
            }
            match result {
                Ok(value) => context.encode(value),
                Err(error) => {
                    let id = error.diagnostic_id().to_ascii_uppercase();
                    let class = if id.contains("ARITY") || id.contains("ARGUMENT_COUNT") {
                        "ArgumentCountError"
                    } else if id.contains("VALUE") {
                        "ValueError"
                    } else if id.contains("TYPE") {
                        "TypeError"
                    } else {
                        "Error"
                    };
                    Err(format!("E_PHP_THROW:{class}:{}", error.message()))
                }
            }
        }
    }
}

fn validate_native_builtin_arity(name: &str, argument_count: usize) -> Result<(), String> {
    let Some(function) = php_std::arginfo::function_metadata_indexed(name) else {
        return Ok(());
    };
    let required = function
        .params
        .iter()
        .filter(|parameter| {
            !parameter.optional && parameter.default_value.is_none() && !parameter.variadic
        })
        .count();
    // These callback-tail APIs encode a PHP overload in a single variadic
    // stub. The callback(s) inside `...$rest` are still mandatory, so their
    // runtime minimum cannot be inferred by counting fixed parameters.
    let required = match name {
        "array_intersect_uassoc" | "array_intersect_ukey" | "array_uintersect" => 2,
        "array_uintersect_uassoc" => 3,
        _ => required,
    };
    let variadic = function
        .params
        .last()
        .is_some_and(|parameter| parameter.variadic);
    let plural = |count: usize| if count == 1 { "" } else { "s" };
    if argument_count < required {
        let expectation = if name == "strtr" {
            "exactly 2 arguments".to_owned()
        } else if !variadic && required == function.params.len() {
            format!("exactly {required} argument{}", plural(required))
        } else {
            format!("at least {required} argument{}", plural(required))
        };
        return Err(format!(
            "E_PHP_THROW:ArgumentCountError:{}() expects {expectation}, {argument_count} given",
            function.name,
        ));
    }
    if !variadic && argument_count > function.params.len() {
        let maximum = function.params.len();
        let expectation = if name == "strtr" {
            "exactly 3 arguments".to_owned()
        } else if required == maximum {
            format!("exactly {maximum} argument{}", plural(maximum))
        } else {
            format!("at most {maximum} argument{}", plural(maximum))
        };
        return Err(format!(
            "E_PHP_THROW:ArgumentCountError:{}() expects {expectation}, {argument_count} given",
            function.name,
        ));
    }
    Ok(())
}

fn native_php_function_exists(name: &str) -> bool {
    // `print` is a language construct, while the mhash compatibility symbols
    // are conditional on a libmhash-enabled PHP build. Both have internal
    // implementation entries but are absent from the pinned PHP 8.5.7 target
    // function table.
    if matches!(
        name,
        "print"
            | "mhash"
            | "mhash_count"
            | "mhash_get_block_size"
            | "mhash_get_hash_name"
            | "mhash_keygen_s2k"
    ) {
        return false;
    }
    php_std::introspection::function_exists(php_std::ExtensionRegistry::standard_library(), name)
        || php_extensions::BuiltinRegistry::new().contains(name)
}

fn native_internal_class_constant_exists(name: &str) -> bool {
    let Some((class_name, constant_name)) = name.rsplit_once("::") else {
        return false;
    };
    php_std::ExtensionRegistry::standard_library()
        .enabled_class(class_name)
        .is_some()
        && php_std::generated::arginfo::constant_metadata_in_hierarchy(class_name, constant_name)
            .is_some()
}

pub(super) fn native_builtin_is_unavailable_target_function(name: &str) -> bool {
    matches!(
        name.trim_start_matches('\\').to_ascii_lowercase().as_str(),
        "mhash"
            | "mhash_count"
            | "mhash_get_block_size"
            | "mhash_get_hash_name"
            | "mhash_keygen_s2k"
    )
}

fn validate_native_builtin_types(
    context: &NativeExecutionContext<'_>,
    name: &str,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<(), String> {
    let Some(metadata) = php_std::arginfo::function_metadata_indexed(name) else {
        return Ok(());
    };
    if !matches!(metadata.extension, "hash" | "json" | "pcre" | "tokenizer") {
        return Ok(());
    }
    if metadata.params.iter().any(|parameter| {
        parameter
            .type_decl
            .split('|')
            .any(|atom| atom.trim() == "callable")
    }) {
        // Runtime callable validation must accept PHP's array callback form
        // and resolve visibility; the scalar arginfo validator intentionally
        // has no class-table context for that job.
        return Ok(());
    }
    let Some(info) = php_std::arginfo::function_arginfo_indexed(name) else {
        return Ok(());
    };
    let values = arguments
        .iter()
        .map(|argument| {
            context.decode(*argument).map(|value| match value {
                Value::Reference(reference) => reference.get(),
                value => value,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mode = if context.unit.strict_types_for_span(source.span) {
        php_std::arginfo::CoercionMode::Strict
    } else {
        php_std::arginfo::CoercionMode::Weak
    };
    let span = php_runtime::api::RuntimeSourceSpan {
        file: context
            .unit
            .files
            .get(source.span.file.index())
            .map(|file| file.path.clone()),
        start: source.span.start,
        end: source.span.end,
    };
    php_std::arginfo::ArgumentValidator::new(mode)
        .validate(info, &values, span)
        .map(|_| ())
        .map_err(|error| {
            let class = match error.class() {
                php_std::arginfo::ArginfoErrorClass::TypeError => "TypeError",
                php_std::arginfo::ArginfoErrorClass::ValueError => "ValueError",
            };
            format!("E_PHP_THROW:{class}:{}", error.diagnostic().message())
        })
}

#[cfg(test)]
mod arity_tests {
    use super::{native_php_function_exists, validate_native_builtin_arity};

    #[test]
    fn generated_builtin_arity_uses_php_argument_count_diagnostics() {
        assert_eq!(
            validate_native_builtin_arity("abs", 0),
            Err(
                "E_PHP_THROW:ArgumentCountError:abs() expects exactly 1 argument, 0 given"
                    .to_owned()
            )
        );
        assert_eq!(
            validate_native_builtin_arity("array_chunk", 0),
            Err(
                "E_PHP_THROW:ArgumentCountError:array_chunk() expects at least 2 arguments, 0 given"
                    .to_owned()
            )
        );
        assert!(validate_native_builtin_arity("printf", 4).is_ok());
        assert_eq!(
            validate_native_builtin_arity("array_uintersect_uassoc", 0),
            Err(
                "E_PHP_THROW:ArgumentCountError:array_uintersect_uassoc() expects at least 3 arguments, 0 given"
                    .to_owned()
            )
        );
        assert_eq!(
            validate_native_builtin_arity("strtr", 0),
            Err(
                "E_PHP_THROW:ArgumentCountError:strtr() expects exactly 2 arguments, 0 given"
                    .to_owned()
            )
        );
    }

    #[test]
    fn function_exists_uses_the_php_visible_target_surface() {
        assert!(native_php_function_exists("class_alias"));
        assert!(!native_php_function_exists("print"));
        assert!(!native_php_function_exists("mhash"));
    }
}
