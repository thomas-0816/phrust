
/// Constructs the sole baseline builtin service adapter from the request's
/// owned state. Keeping this expansion in one place prevents individual
/// builtin families from inventing fallback request-state owners while still
/// allowing Rust to borrow the disjoint context fields precisely at each
/// cold invocation.
macro_rules! borrow_native_builtin_context {
    ($context:ident) => {
        php_runtime::api::BuiltinContext::with_borrowed_runtime_request_state(
            &mut $context.output,
            &mut $context.cwd,
            Arc::clone(&$context.include_path),
            $context.options.runtime_context.filesystem.clone(),
            Some(&mut $context.resources),
            &mut $context.builtin_request_state,
            &mut $context.ini_registry,
            &mut $context.default_timezone,
            Arc::clone(&$context.environment),
        )
    };
}

fn positional_native_call_argument() -> php_ir::instruction::IrCallArg {
    php_ir::instruction::IrCallArg {
        name: None,
        value: php_ir::Operand::Register(php_ir::RegId::new(0)),
        unpack: false,
        value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    }
}

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

/// Applies PHP's scalar-to-string parameter coercion directly to an encoded
/// native operand. Unsupported aggregate/resource shapes stay on their one
/// baseline continuation; admitted scalars never materialize a Rust `Value`.
fn native_encoded_string(
    context: &NativeRequestColdState<'_>,
    encoded: i64,
) -> Result<Vec<u8>, String> {
    let encoded = context.dereference_direct_encoding(encoded);
    match context.native_encoded_value_kind(encoded) {
        Some(NativeEncodedValueKind::String) => context
            .native_string_name_bytes(encoded)
            .ok_or_else(|| "native string operand has no stable bytes".to_owned()),
        Some(NativeEncodedValueKind::Int) => context
            .native_encoded_int(encoded)
            .map(|value| value.to_string().into_bytes())
            .ok_or_else(|| "native integer operand has no stable payload".to_owned()),
        Some(NativeEncodedValueKind::Float) => context
            .native_encoded_float(encoded)
            .map(|value| value.to_string().into_bytes())
            .ok_or_else(|| "native float operand has no stable payload".to_owned()),
        Some(NativeEncodedValueKind::Bool(true)) => Ok(b"1".to_vec()),
        Some(NativeEncodedValueKind::Bool(false) | NativeEncodedValueKind::Null) => Ok(Vec::new()),
        Some(kind) => Err(format!("native builtin expected string, got {kind:?}")),
        None => Err("native builtin expected a stable string operand".to_owned()),
    }
}

/// Applies the shared native call coercion rules for an internal builtin int
/// parameter and returns its immediate payload. The temporary checked owner
/// is released before returning; admitted bool/float/numeric-string inputs
/// therefore never enter the Rust `Value` plane.
fn native_builtin_int_argument(
    context: &mut NativeRequestColdState<'_>,
    encoded: i64,
    strict: bool,
) -> Result<Option<i64>, String> {
    let Some(checked) =
        context.coerce_native_call_argument_encoded(encoded, &php_ir::IrReturnType::Int, strict)?
    else {
        return Ok(None);
    };
    let value = context.native_encoded_int(checked);
    context.release(checked)?;
    Ok(value)
}

/// Applies PHP's builtin bool-parameter coercion without crossing into the
/// compatibility `Value` representation.
fn native_builtin_bool_argument(
    context: &mut NativeRequestColdState<'_>,
    encoded: i64,
    strict: bool,
) -> Result<Option<bool>, String> {
    let Some(checked) =
        context.coerce_native_call_argument_encoded(encoded, &php_ir::IrReturnType::Bool, strict)?
    else {
        return Ok(None);
    };
    let value = context.native_encoded_bool(checked);
    context.release(checked)?;
    Ok(value)
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
    context: &NativeRequestColdState<'_>,
    value: &Value,
) -> bool {
    let Value::Reference(reference) = value else {
        return false;
    };
    context
        .explicit_reference_ids
        .contains(&reference.gc_debug_id())
        // Direct slots count PHP storage owners rather than temporary
        // `ReferenceCell` clones. More than one owner therefore means two
        // live lvalue locations share this identity and `var_dump()` must
        // expose the reference marker.
        || context
            .baseline_values
            .direct_reference_cells
            .iter()
            .find_map(|(index, candidate)| candidate.ptr_eq(reference).then_some(*index))
            .and_then(|index| context.direct_value_slots.get(index))
            .is_some_and(|slot| slot.refcount > 1)
        // Cold-only references have no direct owner count. At this boundary,
        // the inspected container and iterator clone account for three
        // `ReferenceCell` owners; any additional owner is PHP-visible.
        || reference.gc_refcount_estimate() > 3
}

fn prepare_native_sysvshm_serialization(
    context: &mut NativeRequestColdState<'_>,
    arguments: &mut [Value],
) -> Result<(), String> {
    let Some(Value::Object(object)) = arguments.get(2).cloned().map(native_dereference_value)
    else {
        return Ok(());
    };
    let class_name = object.class_name();
    let receiver = context.encode_native_object_owner(object.clone())?;
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
    let result = context.decode_baseline_value(result)?;
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
    context: &mut NativeRequestColdState<'_>,
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
    let class = native_active_class_handle(context, &object.class_name());
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
        let receiver = context.encode_native_object_owner(object.clone())?;
        let result = invoke_native_method(context, debug, &[receiver])?;
        let Value::Array(array) = context.decode_baseline_value(result)? else {
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
    context: &NativeRequestColdState<'_>,
    source: &php_ir::Instruction,
) -> usize {
    native_source_line_for_span(context, source.span)
}

pub(super) fn native_source_line_for_span(
    context: &NativeRequestColdState<'_>,
    span: php_ir::IrSpan,
) -> usize {
    context
        .compiled
        .source_display_line(span, false)
        .and_then(|line| usize::try_from(line).ok())
        .unwrap_or(1)
}

pub(super) fn emit_native_php_warning(
    context: &mut NativeRequestColdState<'_>,
    errno: i64,
    message: &str,
    source: &php_ir::Instruction,
) -> Result<(), String> {
    emit_native_php_diagnostic(context, errno, message, source, true)
}

pub(super) fn emit_native_php_diagnostic(
    context: &mut NativeRequestColdState<'_>,
    errno: i64,
    message: &str,
    source: &php_ir::Instruction,
    leading_newline: bool,
) -> Result<(), String> {
    emit_native_php_diagnostic_at_span(context, errno, message, source.span, leading_newline)
}

pub(super) fn emit_native_php_diagnostic_at_span(
    context: &mut NativeRequestColdState<'_>,
    errno: i64,
    message: &str,
    span: php_ir::IrSpan,
    leading_newline: bool,
) -> Result<(), String> {
    let path = context
        .unit
        .files
        .get(span.file.index())
        .map_or_else(|| "<unknown>".to_owned(), |file| file.path.clone());
    let line = native_source_line_for_span(context, span);
    context.record_last_error(errno, message, &path, line);
    if let Some(handler) = context
        .registered_callbacks
        .error_handlers
        .last()
        .filter(|handler| handler.levels == -1 || handler.levels & errno != 0)
        .copied()
    {
        // Dynamic user error handlers still consume the baseline call-source
        // carrier. This object is created only after a PHP-visible diagnostic
        // selected such a handler; the successful exact-builtin path never
        // allocates or reconstructs an IR instruction.
        let source = php_ir::Instruction {
            id: php_ir::InstrId::new(0),
            span,
            kind: php_ir::InstructionKind::Nop,
        };
        context.retain(handler.callback)?;
        let mut arguments = Vec::with_capacity(4);
        let invoke_result = (|| {
            arguments.push(context.encode_native_int(errno)?);
            arguments.push(context.encode_direct_string_bytes(message.as_bytes())?);
            arguments.push(context.encode_direct_string_bytes(path.as_bytes())?);
            arguments.push(context.encode_native_int(line as i64)?);
            let mut encoded = Vec::with_capacity(arguments.len() + 1);
            encoded.push(handler.callback);
            encoded.extend_from_slice(&arguments);
            let returned = invoke_native_encoded_callable_value_from(
                context,
                &encoded,
                &source,
                None,
                None,
            )
            .map_err(NativeCallControl::into_baseline_error)?;
            context.release_if_live(returned)
        })();
        let mut release_error = context.release_if_live(handler.callback).err();
        for argument in arguments {
            if let Err(error) = context.release_if_live(argument) {
                release_error.get_or_insert(error);
            }
        }
        invoke_result?;
        if let Some(error) = release_error {
            return Err(error);
        }
        return Ok(());
    }
    if context.error_reporting & errno == 0 {
        return Ok(());
    }
    if !context.display_errors {
        return Ok(());
    }
    let label = match errno {
        php_runtime::api::PHP_E_NOTICE | php_runtime::api::PHP_E_USER_NOTICE => "Notice",
        php_runtime::api::PHP_E_DEPRECATED | php_runtime::api::PHP_E_USER_DEPRECATED => {
            "Deprecated"
        }
        _ => "Warning",
    };
    let html = matches!(
        context.options.runtime_context.request_mode,
        php_runtime::api::RuntimeRequestMode::Http(_)
    );
    context.output.write_bytes(format_native_php_diagnostic(
        label,
        message,
        &path,
        line,
        leading_newline,
        html,
    ));
    Ok(())
}

pub(super) fn format_native_php_diagnostic(
    label: &str,
    message: &str,
    path: &str,
    line: usize,
    leading_newline: bool,
    html: bool,
) -> String {
    if html {
        let prefix = if leading_newline { "<br />\n" } else { "" };
        format!("{prefix}<b>{label}</b>:  {message} in <b>{path}</b> on line <b>{line}</b><br />\n")
    } else {
        let prefix = if leading_newline { "\n" } else { "" };
        format!("{prefix}{label}: {message} in {path} on line {line}\n")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NativeDimensionOperation {
    Fetch { quiet: bool },
    Insert,
    Reference,
    Unset,
}

fn dereferenced_native_diagnostic_value(value: &Value) -> std::borrow::Cow<'_, Value> {
    if !matches!(value, Value::Reference(_)) {
        return std::borrow::Cow::Borrowed(value);
    }
    let mut value = value.clone();
    for _ in 0..16 {
        match value {
            Value::Reference(reference) => value = reference.get(),
            _ => break,
        }
    }
    std::borrow::Cow::Owned(value)
}

pub(super) fn emit_native_dimension_conversion_diagnostic(
    context: &mut NativeRequestColdState<'_>,
    target: &Value,
    key: &Value,
    source: Option<&php_ir::Instruction>,
    operation: NativeDimensionOperation,
) -> Result<(), String> {
    let Some(source) = source else {
        return Ok(());
    };
    let target = dereferenced_native_diagnostic_value(target);
    if matches!(target.as_ref(), Value::Null | Value::Uninitialized)
        && matches!(operation, NativeDimensionOperation::Fetch { quiet: false })
    {
        emit_native_php_warning(
            context,
            php_runtime::api::PHP_E_WARNING,
            "Trying to access array offset on null",
            source,
        )?;
    }
    emit_native_dimension_conversion_diagnostic_for_target(
        context,
        matches!(target.as_ref(), Value::Array(_)),
        matches!(target.as_ref(), Value::String(_)),
        matches!(target.as_ref(), Value::Null | Value::Uninitialized),
        key,
        source,
        operation,
    )
}

pub(super) fn emit_native_array_dimension_conversion_diagnostic(
    context: &mut NativeRequestColdState<'_>,
    key: &Value,
    source: Option<&php_ir::Instruction>,
    operation: NativeDimensionOperation,
) -> Result<(), String> {
    let Some(source) = source else {
        return Ok(());
    };
    emit_native_dimension_conversion_diagnostic_for_target(
        context, true, false, false, key, source, operation,
    )
}

fn emit_native_dimension_conversion_diagnostic_for_target(
    context: &mut NativeRequestColdState<'_>,
    target_is_array: bool,
    target_is_string: bool,
    target_is_nullish: bool,
    key: &Value,
    source: &php_ir::Instruction,
    operation: NativeDimensionOperation,
) -> Result<(), String> {
    let key = dereferenced_native_diagnostic_value(key);
    match key.as_ref() {
        Value::Null | Value::Uninitialized => {
            let array_target = target_is_array
                || target_is_nullish
                    && matches!(
                        operation,
                        NativeDimensionOperation::Insert | NativeDimensionOperation::Reference
                    );
            if array_target && !matches!(operation, NativeDimensionOperation::Unset) {
                emit_native_php_warning(
                    context,
                    php_runtime::api::PHP_E_DEPRECATED,
                    "Using null as an array offset is deprecated, use an empty string instead",
                    source,
                )
            } else if target_is_string
                && !matches!(
                    operation,
                    NativeDimensionOperation::Fetch { quiet: true }
                        | NativeDimensionOperation::Unset
                )
            {
                emit_native_php_warning(
                    context,
                    php_runtime::api::PHP_E_WARNING,
                    "String offset cast occurred",
                    source,
                )
            } else {
                Ok(())
            }
        }
        Value::Float(key) => {
            let key = key.to_f64();
            if target_is_string {
                emit_native_php_warning(
                    context,
                    php_runtime::api::PHP_E_WARNING,
                    "String offset cast occurred",
                    source,
                )
            } else if target_is_array && key.is_finite() && key.fract() != 0.0 {
                emit_native_php_warning(
                    context,
                    php_runtime::api::PHP_E_DEPRECATED,
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
        _ => Ok(()),
    }
}

fn native_deprecated_call_message(
    unit: &php_ir::IrUnit,
    function: php_ir::FunctionId,
) -> Option<String> {
    let function = unit.functions.get(function.index())?;
    let attribute = function.attributes.iter().find(|attribute| {
        attribute
            .resolved_name
            .as_deref()
            .or(attribute.fallback_name.as_deref())
            .unwrap_or(&attribute.name)
            .trim_start_matches('\\')
            .eq_ignore_ascii_case("deprecated")
    })?;
    let custom = attribute.arguments.iter().find_map(|constant| {
        match unit.constants.get(constant.index())? {
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
    Some(message)
}

fn emit_native_deprecated_message(
    context: &mut NativeRequestColdState<'_>,
    message: &str,
    source: &php_ir::Instruction,
) {
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

pub(super) fn emit_native_deprecated_call(
    context: &mut NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
    source: &php_ir::Instruction,
) {
    let Some(message) = native_deprecated_call_message(&context.unit, function) else {
        return;
    };
    emit_native_deprecated_message(context, &message, source);
}

pub(super) fn emit_native_external_deprecated_call(
    context: &mut NativeRequestColdState<'_>,
    target: NativeDynamicFunction,
    source: &php_ir::Instruction,
) {
    let message = context
        .dynamic_units
        .get(target.unit)
        .and_then(|unit| native_deprecated_call_message(unit.compiled.unit(), target.function));
    let Some(message) = message else {
        return;
    };
    emit_native_deprecated_message(context, &message, source);
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

/// Baseline-only compatibility for key sorts whose array/reference shape
/// cannot be mutated through authoritative native entries.
fn execute_baseline_key_sort(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    reverse: bool,
) -> Result<i64, String> {
    let Some(target) = arguments.first() else {
        return Err("ksort() expects an array passed by reference".to_owned());
    };
    let Value::Reference(reference) = context.decode_baseline_value(*target)? else {
        return Err("ksort(): Argument #1 ($array) must be passed by reference".to_owned());
    };
    let Value::Array(array) = reference.get() else {
        return Err("ksort(): Argument #1 ($array) must be of type array".to_owned());
    };
    let flags = arguments
        .get(1)
        .map(|value| context.decode_baseline_value(*value))
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
    context.set_native_reference_value(&reference, Value::Array(sorted))?;
    context.encode_baseline_value(Value::Bool(true))
}

/// Baseline-only callback sort. Callback invocation is intentionally not
/// smuggled through the fixed non-callback sort ABIs.
fn execute_baseline_callback_sort(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
    compare_keys: bool,
    preserve_keys: bool,
) -> Result<i64, String> {
    let [target, callback] = arguments else {
        return Err("array callback sort expects exactly 2 arguments".to_owned());
    };
    if let Some(result) = try_execute_direct_native_callback_sort(
        context,
        *target,
        *callback,
        source,
        compare_keys,
        preserve_keys,
    ) {
        return result;
    }
    let Value::Reference(reference) = context.decode_baseline_value(*target)? else {
        return Err("array callback sort expects an array passed by reference".to_owned());
    };
    let Value::Array(array) = reference.get() else {
        return Err("array callback sort expects an array".to_owned());
    };
    let callback = match context.decode_baseline_value(*callback)? {
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
            let ordering = match context.decode_baseline_value(result)? {
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
    context.set_native_reference_value(&reference, Value::Array(sorted))?;
    context.encode_baseline_value(Value::Bool(true))
}

fn try_execute_direct_native_callback_sort(
    context: &mut NativeRequestColdState<'_>,
    target: i64,
    callback: i64,
    source: &php_ir::Instruction,
    compare_keys: bool,
    preserve_keys: bool,
) -> Option<Result<i64, String>> {
    let array = context.direct_reference_payload(target)?;
    let array = context.direct_array_encoding(array)?;
    let (start, length) = context.direct_array_entry_range(array)?;
    let mut entries = Vec::with_capacity(length);
    let result = (|| {
        for index in 0..length {
            let entry = context.direct_array_entry_at(start, index);
            let key = duplicate_native_callback_owner(context, entry.key)?;
            let value = match duplicate_native_callback_owner(context, entry.value) {
                Ok(value) => value,
                Err(error) => {
                    context.release_if_live(key)?;
                    return Err(error);
                }
            };
            entries.push(php_jit::JitNativeDirectArrayEntry { key, value });
        }

        for index in 1..entries.len() {
            let mut cursor = index;
            while cursor > 0 {
                let left = if compare_keys {
                    entries[cursor - 1].key
                } else {
                    entries[cursor - 1].value
                };
                let right = if compare_keys {
                    entries[cursor].key
                } else {
                    entries[cursor].value
                };
                let result = invoke_native_array_callback_by_value(
                    context,
                    callback,
                    &[left, right],
                    source,
                )?;
                let ordering = consume_native_callback_ordering(context, result)?;
                if ordering <= 0 {
                    break;
                }
                entries.swap(cursor - 1, cursor);
                cursor -= 1;
            }
        }

        if !preserve_keys {
            for (index, entry) in entries.iter_mut().enumerate() {
                let index = i64::try_from(index)
                    .map_err(|_| "callback sort index exceeds the PHP integer domain".to_owned())?;
                let key = context.encode_native_int(index)?;
                context.release_if_live(entry.key)?;
                entry.key = key;
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        return Some(match release_native_callback_entries(context, &entries) {
            Ok(()) => Err(error),
            Err(release_error) => Err(release_error),
        });
    }

    let sorted = match context.publish_owned_direct_array_entries(entries) {
        Ok(sorted) => sorted,
        Err(error) => return Some(Err(error)),
    };
    match context.replace_direct_reference_payload_owned(target, sorted) {
        Ok(true) => Some(Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE))),
        Ok(false) => {
            let release = context.release_if_live(sorted);
            Some(match release {
                Ok(()) => Err("callback sort target lost its native reference identity".to_owned()),
                Err(error) => Err(error),
            })
        }
        Err(error) => {
            let _ = context.release_if_live(sorted);
            Some(Err(error))
        }
    }
}

fn consume_native_callback_ordering(
    context: &mut NativeRequestColdState<'_>,
    encoded: i64,
) -> Result<i64, String> {
    let ordering = match context.native_encoded_value_kind(encoded) {
        Some(NativeEncodedValueKind::Int) => context.native_encoded_int(encoded).unwrap_or(0),
        Some(NativeEncodedValueKind::Float) => {
            context.native_encoded_float(encoded).unwrap_or(0.0) as i64
        }
        Some(NativeEncodedValueKind::String) => context
            .native_string_name_bytes(encoded)
            .and_then(|bytes| std::str::from_utf8(&bytes).ok()?.parse::<i64>().ok())
            .unwrap_or(0),
        Some(NativeEncodedValueKind::Bool(value)) => i64::from(value),
        Some(NativeEncodedValueKind::Null) => 0,
        _ => {
            let value = context.decode_baseline_value(encoded)?;
            match value {
                Value::Int(value) => value,
                Value::Float(value) => value.to_f64() as i64,
                Value::String(value) => value.to_string_lossy().parse::<i64>().unwrap_or(0),
                Value::Bool(value) => i64::from(value),
                _ => 0,
            }
        }
    };
    context.release_if_live(encoded)?;
    Ok(ordering)
}

fn native_array_key_value(key: &php_runtime::api::ArrayKey) -> Value {
    match key {
        php_runtime::api::ArrayKey::Int(key) => Value::Int(*key),
        php_runtime::api::ArrayKey::String(key) => Value::String(key.clone()),
    }
}

/// Invokes one callback from the by-value array family without constructing a
/// parallel Rust `Value` argument plane.
///
/// The shared call-user-func binder already implements PHP's by-reference
/// warning/temporary semantics over native handles. Passing the authoritative
/// element encodings straight through it gives map/filter/reduce/predicate
/// callbacks the same semantics while keeping strings, arrays, objects, and
/// references native across every iteration.
fn invoke_native_array_callback_by_value(
    context: &mut NativeRequestColdState<'_>,
    callback: i64,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    let mut encoded = smallvec::SmallVec::<[i64; 8]>::with_capacity(arguments.len() + 1);
    encoded.push(callback);
    encoded.extend_from_slice(arguments);
    execute_native_call_user_func_encoded(context, &encoded, source, None)
        .map_err(NativeCallControl::into_baseline_error)
}

/// Invokes a cold array-walk callback while keeping the element reference's
/// native descriptor alive long enough to materialize its updated payload
/// back into the compatibility `ReferenceCell`.
///
/// `encode_native_reference_owner` promotes the cell payload into the direct
/// value plane. The ordinary Value callback bridge owns and releases its own
/// handle, so without this independent owner the final release retires the
/// descriptor before the walk can observe the callback mutation.
fn invoke_native_array_walk_reference_value(
    context: &mut NativeRequestColdState<'_>,
    callback: Value,
    reference: &php_runtime::api::ReferenceCell,
    arguments: &[Value],
    source: &php_ir::Instruction,
) -> Result<(), String> {
    let reference_owner = context.encode_native_reference_owner(reference.clone())?;
    let invoked = invoke_native_callable_value(context, callback, arguments, source, None);
    let materialized = context.decode_baseline_value(reference_owner).map(|_| ());
    let release_result = match invoked {
        Ok(result) => context.release_if_live(result),
        Err(error) => Err(error),
    };
    let release_reference = context.release_if_live(reference_owner);
    match (release_result, materialized, release_reference) {
        (Err(error), _, _) | (Ok(()), Err(error), _) | (Ok(()), Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(()), Ok(())) => Ok(()),
    }
}

fn duplicate_native_callback_owner(
    context: &mut NativeRequestColdState<'_>,
    encoded: i64,
) -> Result<i64, String> {
    context
        .duplicate_authoritative_native_value(encoded)?
        .ok_or_else(|| {
            "callback array value requires an explicit baseline representation".to_owned()
        })
}

fn release_native_callback_entries(
    context: &mut NativeRequestColdState<'_>,
    entries: &[php_jit::JitNativeDirectArrayEntry],
) -> Result<(), String> {
    let mut release_error = None;
    for entry in entries.iter().rev() {
        for encoded in [entry.value, entry.key] {
            if let Err(error) = context.release_if_live(encoded) {
                release_error.get_or_insert(error);
            }
        }
    }
    release_error.map_or(Ok(()), Err)
}

fn finish_native_callback_entries(
    context: &mut NativeRequestColdState<'_>,
    entries: Vec<php_jit::JitNativeDirectArrayEntry>,
    result: Result<(), String>,
) -> Result<i64, String> {
    match result {
        Ok(()) => context.publish_owned_direct_array_entries(entries),
        Err(error) => match release_native_callback_entries(context, &entries) {
            Ok(()) => Err(error),
            Err(release_error) => Err(release_error),
        },
    }
}

fn push_native_callback_entry(
    context: &mut NativeRequestColdState<'_>,
    entries: &mut Vec<php_jit::JitNativeDirectArrayEntry>,
    key: Result<i64, String>,
    value: i64,
) -> Result<(), String> {
    let key = match key {
        Ok(key) => key,
        Err(error) => {
            context.release_if_live(value)?;
            return Err(error);
        }
    };
    entries.push(php_jit::JitNativeDirectArrayEntry { key, value });
    Ok(())
}

fn publish_native_packed_callback_values(
    context: &mut NativeRequestColdState<'_>,
    values: &[i64],
) -> Result<i64, String> {
    let mut entries = Vec::with_capacity(values.len());
    let result = (|| {
        for (index, value) in values.iter().copied().enumerate() {
            let value = duplicate_native_callback_owner(context, value)?;
            let index = i64::try_from(index)
                .map_err(|_| "callback array index exceeds the PHP integer domain".to_owned())?;
            let key = context.encode_native_int(index);
            push_native_callback_entry(context, &mut entries, key, value)?;
        }
        Ok(())
    })();
    finish_native_callback_entries(context, entries, result)
}

fn consume_native_callback_truthiness(
    context: &mut NativeRequestColdState<'_>,
    encoded: i64,
) -> Result<bool, String> {
    let truthy = if let Some(truthy) = context.native_encoded_truthy(encoded) {
        Ok(truthy)
    } else {
        // SimpleXML and already-materialized compatibility references are
        // explicitly cold truthiness shapes. Materialize only this callback
        // result once; ordinary scalar/string/array/object results stay native.
        context
            .decode_baseline_value(encoded)
            .map(|value| native_property_truthy(&value))
    };
    let release = context.release_if_live(encoded);
    match (truthy, release) {
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Ok(truthy), Ok(())) => Ok(truthy),
    }
}

fn try_execute_direct_native_array_map(
    context: &mut NativeRequestColdState<'_>,
    callback: i64,
    arrays: &[i64],
    source: &php_ir::Instruction,
) -> Option<Result<i64, String>> {
    let ranges = arrays
        .iter()
        .copied()
        .map(|array| context.direct_array_entry_range(array))
        .collect::<Option<Vec<_>>>()?;
    let callback_is_null = matches!(
        context.native_encoded_value_kind(callback),
        Some(NativeEncodedValueKind::Null)
    );
    if callback_is_null && ranges.len() == 1 {
        return Some(duplicate_native_callback_owner(context, arrays[0]));
    }

    let length = ranges.iter().map(|(_, length)| *length).max().unwrap_or(0);
    let mut output = Vec::with_capacity(length);
    let mut callback_arguments = Vec::with_capacity(ranges.len());
    let result = (|| {
        for index in 0..length {
            callback_arguments.clear();
            for (start, length) in ranges.iter().copied() {
                callback_arguments.push(if index < length {
                    context.direct_array_entry_at(start, index).value
                } else {
                    php_jit::jit_encode_constant(u32::MAX)
                });
            }
            let value = if callback_is_null {
                publish_native_packed_callback_values(context, &callback_arguments)?
            } else {
                invoke_native_array_callback_by_value(
                    context,
                    callback,
                    &callback_arguments,
                    source,
                )?
            };
            let key = if ranges.len() == 1 {
                duplicate_native_callback_owner(
                    context,
                    context.direct_array_entry_at(ranges[0].0, index).key,
                )
            } else {
                i64::try_from(index)
                    .map_err(|_| "array_map() result index exceeds PHP integer range".to_owned())
                    .and_then(|index| context.encode_native_int(index))
            };
            push_native_callback_entry(context, &mut output, key, value)?;
        }
        Ok(())
    })();
    Some(finish_native_callback_entries(context, output, result))
}

fn execute_native_array_map(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    let Some((callback, arrays)) = arguments.split_first() else {
        return Err("array_map() expects at least 2 arguments".to_owned());
    };
    if arrays.is_empty() {
        return Err("array_map() expects at least 2 arguments".to_owned());
    }
    if let Some(result) = try_execute_direct_native_array_map(context, *callback, arrays, source) {
        return result;
    }
    execute_baseline_array_map(context, *callback, arrays, source)
}

/// Explicit cold compatibility for non-direct arrays/callbacks. Direct
/// callback arrays are consumed above and never enter this Rust `Value` plane.
fn execute_baseline_array_map(
    context: &mut NativeRequestColdState<'_>,
    callback: i64,
    arrays: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    let callback = match context.decode_baseline_value(callback)? {
        Value::Reference(reference) => reference.get(),
        callback => callback,
    };
    let arrays = arrays
        .iter()
        .map(|array| match context.decode_baseline_value(*array)? {
            Value::Reference(reference) => match reference.get() {
                Value::Array(array) => Ok(array),
                _ => Err("array_map(): array argument must be of type array".to_owned()),
            },
            Value::Array(array) => Ok(array),
            _ => Err("array_map(): array argument must be of type array".to_owned()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if matches!(callback, Value::Null) && arrays.len() == 1 {
        return context.encode_native_array_owner(arrays[0].clone());
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
            invoke_native_callable_value(context, callback.clone(), &values, source, None)
                .and_then(|encoded| context.decode_baseline_value(encoded))?
        };
        if arrays.len() == 1 {
            let (key, _) = &entries[0][index];
            result.insert(key.clone(), value);
        } else {
            result.append(value);
        }
    }
    context.encode_native_array_owner(result)
}

fn execute_native_array_filter(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    let Some(array) = arguments.first() else {
        return Err("array_filter() expects at least 1 argument".to_owned());
    };
    if let Some(result) = try_execute_direct_native_array_filter(context, arguments, source) {
        return result;
    }
    execute_baseline_array_filter(context, *array, &arguments[1..], source)
}

fn try_execute_direct_native_array_filter(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Option<Result<i64, String>> {
    let (start, length) = context.direct_array_entry_range(arguments[0])?;
    let callback = arguments.get(1).copied().filter(|callback| {
        !matches!(
            context.native_encoded_value_kind(*callback),
            Some(NativeEncodedValueKind::Null)
        )
    });
    let mode = arguments
        .get(2)
        .and_then(|mode| context.native_encoded_int(*mode))
        .unwrap_or(0);

    // Callback-free truthiness has no side effects, so determine native
    // admission for the complete array before retaining any result owners.
    if callback.is_none()
        && (0..length).any(|index| {
            context
                .native_encoded_truthy(context.direct_array_entry_at(start, index).value)
                .is_none()
        })
    {
        return None;
    }

    let mut output = Vec::with_capacity(length);
    let result = (|| {
        for index in 0..length {
            let entry = context.direct_array_entry_at(start, index);
            let keep = if let Some(callback) = callback {
                let mut callback_arguments = smallvec::SmallVec::<[i64; 2]>::new();
                match mode {
                    1 => callback_arguments.extend_from_slice(&[entry.value, entry.key]),
                    2 => callback_arguments.push(entry.key),
                    _ => callback_arguments.push(entry.value),
                }
                let result = invoke_native_array_callback_by_value(
                    context,
                    callback,
                    &callback_arguments,
                    source,
                )?;
                consume_native_callback_truthiness(context, result)?
            } else {
                context.native_encoded_truthy(entry.value).ok_or_else(|| {
                    "callback-free array_filter value changed after native preflight".to_owned()
                })?
            };
            if !keep {
                continue;
            }
            let value = duplicate_native_callback_owner(context, entry.value)?;
            let key = duplicate_native_callback_owner(context, entry.key);
            push_native_callback_entry(context, &mut output, key, value)?;
        }
        Ok(())
    })();
    Some(finish_native_callback_entries(context, output, result))
}

/// Explicit cold compatibility for non-direct arrays and cold truthiness
/// shapes. The ordinary callback loop above never constructs a `PhpArray`.
fn execute_baseline_array_filter(
    context: &mut NativeRequestColdState<'_>,
    array: i64,
    remaining: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    let array = match context.decode_baseline_value(array)? {
        Value::Reference(reference) => match reference.get() {
            Value::Array(array) => array,
            _ => return Err("array_filter(): argument #1 must be of type array".to_owned()),
        },
        Value::Array(array) => array,
        _ => return Err("array_filter(): argument #1 must be of type array".to_owned()),
    };
    let callback = remaining
        .first()
        .map(|callback| context.decode_baseline_value(*callback))
        .transpose()?
        .map(|callback| match callback {
            Value::Reference(reference) => reference.get(),
            callback => callback,
        })
        .filter(|callback| !matches!(callback, Value::Null));
    let mode = remaining
        .get(1)
        .map(|mode| context.decode_baseline_value(*mode))
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
            native_property_truthy(&context.decode_baseline_value(encoded)?)
        } else {
            native_property_truthy(value)
        };
        if keep {
            result.insert(key.clone(), value.clone());
        }
    }
    context.encode_native_array_owner(result)
}

fn native_array_argument(
    context: &mut NativeRequestColdState<'_>,
    encoded: i64,
    function: &str,
) -> Result<php_runtime::api::PhpArray, String> {
    let value = match context.decode_baseline_value(encoded)? {
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
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    if !(2..=3).contains(&arguments.len()) {
        return Err("array_reduce() expects 2 or 3 arguments".to_owned());
    }
    if let Some(result) = try_execute_direct_native_array_reduce(context, arguments, source) {
        return result;
    }
    execute_baseline_array_reduce(context, arguments, source)
}

fn try_execute_direct_native_array_reduce(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Option<Result<i64, String>> {
    let (start, length) = context.direct_array_entry_range(arguments[0])?;
    let mut carry = match arguments.get(2).copied() {
        Some(initial) => match duplicate_native_callback_owner(context, initial) {
            Ok(initial) => initial,
            Err(error) => return Some(Err(error)),
        },
        None => php_jit::jit_encode_constant(u32::MAX),
    };
    for index in 0..length {
        let value = context.direct_array_entry_at(start, index).value;
        let next =
            invoke_native_array_callback_by_value(context, arguments[1], &[carry, value], source);
        match next {
            Ok(next) => {
                if let Err(error) = context.release_if_live(carry) {
                    let _ = context.release_if_live(next);
                    return Some(Err(error));
                }
                carry = next;
            }
            Err(error) => {
                let release = context.release_if_live(carry);
                return Some(match release {
                    Ok(()) => Err(error),
                    Err(release_error) => Err(release_error),
                });
            }
        }
    }
    Some(Ok(carry))
}

/// Explicit cold compatibility for non-direct input arrays.
fn execute_baseline_array_reduce(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    let array = native_array_argument(context, arguments[0], "array_reduce")?;
    let callback = match context.decode_baseline_value(arguments[1])? {
        Value::Reference(reference) => reference.get(),
        value => value,
    };
    let mut carry = arguments
        .get(2)
        .map(|value| context.decode_baseline_value(*value))
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
        carry = context.decode_baseline_value(encoded)?;
    }
    context.encode_baseline_value(carry)
}

fn execute_native_array_walk(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    if !(2..=3).contains(&arguments.len()) {
        return Err("array_walk() expects 2 or 3 arguments".to_owned());
    }
    let Value::Reference(root) = context.decode_baseline_value(arguments[0])? else {
        return Err("array_walk(): Argument #1 ($array) must be passed by reference".to_owned());
    };
    let Value::Array(mut array) = root.get() else {
        return Err(
            "E_PHP_THROW:TypeError:array_walk(): Argument #1 ($array) must be of type array"
                .to_owned(),
        );
    };
    let callback = match context.decode_baseline_value(arguments[1])? {
        Value::Reference(reference) => reference.get(),
        value => value,
    };
    let userdata = arguments
        .get(2)
        .map(|value| context.decode_baseline_value(*value))
        .transpose()?;
    let keys = array.iter().map(|(key, _)| key).collect::<Vec<_>>();
    for key in keys {
        let value = array.get(&key).cloned().unwrap_or(Value::Null);
        let (cell, temporary) = match value {
            Value::Reference(reference) => (reference, false),
            value => (php_runtime::api::ReferenceCell::new(value), true),
        };
        array.insert(key.clone(), Value::Reference(cell.clone()));
        // The root reference is itself a materialized native descriptor.
        // Publish this iteration's authoritative array before entering the
        // compiled callback so native re-entry cannot restore an older root
        // snapshot over the newly bound element reference.
        root.set(Value::Array(array));
        let mut values = vec![Value::Reference(cell.clone()), native_array_key_value(&key)];
        if let Some(userdata) = &userdata {
            values.push(userdata.clone());
        }
        let invoked = invoke_native_array_walk_reference_value(
            context,
            callback.clone(),
            &cell,
            &values,
            source,
        );
        drop(values);
        let Value::Reference(current_root) = context.decode_baseline_value(arguments[0])? else {
            return Err("array_walk() lost its array root reference".to_owned());
        };
        array = match root.get() {
            Value::Array(array) => array,
            _ => {
                return Err("array_walk() callback replaced the array root".to_owned());
            }
        };
        debug_assert!(current_root.ptr_eq(&root));
        // The array entry, this local owner, the materialized direct sidecar,
        // and the inspection clone account for four cold owners. Any further
        // owner is a callback-exported PHP alias and must keep the reference.
        let _temporary_reference = Value::Reference(cell.clone());
        if temporary && cell.gc_refcount_estimate() <= 4 {
            array.insert(key.clone(), cell.get());
        }
        root.set(Value::Array(array.clone()));
        if let Err(error) = invoked {
            return Err(error);
        }
    }
    root.set(Value::Array(array));
    context.encode_baseline_value(Value::Bool(true))
}

fn walk_native_array_recursive(
    context: &mut NativeRequestColdState<'_>,
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
                    context.set_native_reference_value(&reference, Value::Array(nested))?;
                }
                _ => {
                    let mut values = vec![
                        Value::Reference(reference.clone()),
                        native_array_key_value(&key),
                    ];
                    if let Some(userdata) = userdata {
                        values.push(userdata.clone());
                    }
                    invoke_native_array_walk_reference_value(
                        context,
                        callback.clone(),
                        &reference,
                        &values,
                        source,
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
                let invoked = invoke_native_array_walk_reference_value(
                    context,
                    callback.clone(),
                    &reference,
                    &values,
                    source,
                );
                drop(values);
                let _temporary_reference = Value::Reference(reference.clone());
                if reference.gc_refcount_estimate() <= 4 {
                    array.insert(key, reference.get());
                }
                let _ = invoked?;
            }
        }
    }
    Ok(())
}

fn walk_direct_native_array_recursive(
    context: &mut NativeRequestColdState<'_>,
    array: i64,
    callback: i64,
    userdata: Option<i64>,
    source: &php_ir::Instruction,
    active: &mut std::collections::BTreeSet<i64>,
) -> Result<(), String> {
    if !active.insert(array) {
        return Err("array_walk_recursive(): Recursion detected".to_owned());
    }
    let result = (|| {
        let (start, length) = context
            .direct_array_entry_range(array)
            .ok_or_else(|| "array_walk_recursive() lost its native array".to_owned())?;
        for index in 0..length {
            let entry = context.direct_array_entry_at(start, index);
            let key = context
                .native_encoded_plain_array_key(entry.key)
                .ok_or_else(|| "array_walk_recursive() found an invalid native key".to_owned())?;
            if let Some(mut nested) = context.direct_array_encoding(entry.value) {
                if context.direct_array_is_unique(nested) != Some(true) {
                    let separated = context.clone_direct_array_handle(nested)?;
                    let replaced = if context.php_handle_is_reference(entry.value) == Some(true) {
                        context.replace_direct_reference_payload_owned(entry.value, separated)?
                    } else {
                        let inserted =
                            context.direct_array_insert_encoded(array, Some(&key), separated);
                        let release = context.release_if_live(separated);
                        match (inserted, release) {
                            (Err(error), _) | (Ok(()), Err(error)) => return Err(error),
                            (Ok(()), Ok(())) => true,
                        }
                    };
                    if !replaced {
                        context.release_if_live(separated)?;
                        return Err(
                            "array_walk_recursive() could not separate a nested array".to_owned()
                        );
                    }
                    nested =
                        context
                            .direct_array_encoding(
                                context.direct_array_find_encoded(array, &key)?.ok_or_else(
                                    || "array_walk_recursive() lost a separated child".to_owned(),
                                )?,
                            )
                            .ok_or_else(|| {
                                "array_walk_recursive() separated child is not an array".to_owned()
                            })?;
                }
                walk_direct_native_array_recursive(
                    context, nested, callback, userdata, source, active,
                )?;
                continue;
            }

            let was_reference = context.php_handle_is_reference(entry.value) == Some(true);
            let reference = context
                .bind_native_direct_array_element_reference(array, &key)?
                .ok_or_else(|| {
                    "array_walk_recursive() could not bind a native array element".to_owned()
                })?;
            let mut arguments = smallvec::SmallVec::<[i64; 3]>::new();
            arguments.push(reference);
            arguments.push(entry.key);
            if let Some(userdata) = userdata {
                arguments.push(userdata);
            }
            let invoked =
                invoke_native_array_callback_by_value(context, callback, &arguments, source);
            let release_reference = context.release_if_live(reference);
            let collapse = if was_reference {
                Ok(false)
            } else {
                context.collapse_native_direct_array_element_reference(array, &key, reference)
            };
            match (invoked, release_reference, collapse) {
                (Err(error), _, _) | (Ok(_), Err(error), _) | (Ok(_), Ok(()), Err(error)) => {
                    return Err(error);
                }
                (Ok(result), Ok(()), Ok(_)) => context.release_if_live(result)?,
            }
        }
        Ok(())
    })();
    active.remove(&array);
    result
}

fn try_execute_direct_native_array_walk_recursive(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Option<Result<i64, String>> {
    let mut array = context.direct_reference_payload(arguments[0])?;
    array = context.direct_array_encoding(array)?;
    if context.direct_array_is_unique(array) != Some(true) {
        let separated = match context.clone_direct_array_handle(array) {
            Ok(separated) => separated,
            Err(error) => return Some(Err(error)),
        };
        match context.replace_direct_reference_payload_owned(arguments[0], separated) {
            Ok(true) => array = separated,
            Ok(false) => {
                let _ = context.release_if_live(separated);
                return None;
            }
            Err(error) => {
                let _ = context.release_if_live(separated);
                return Some(Err(error));
            }
        }
    }
    let mut active = std::collections::BTreeSet::new();
    Some(
        walk_direct_native_array_recursive(
            context,
            array,
            arguments[1],
            arguments.get(2).copied(),
            source,
            &mut active,
        )
        .map(|()| php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE)),
    )
}

fn execute_native_array_walk_recursive(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    if !(2..=3).contains(&arguments.len()) {
        return Err("array_walk_recursive() expects 2 or 3 arguments".to_owned());
    }
    if let Some(result) = try_execute_direct_native_array_walk_recursive(context, arguments, source)
    {
        return result;
    }
    let Value::Reference(root) = context.decode_baseline_value(arguments[0])? else {
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
    let callback = match context.decode_baseline_value(arguments[1])? {
        Value::Reference(reference) => reference.get(),
        value => value,
    };
    let userdata = arguments
        .get(2)
        .map(|value| context.decode_baseline_value(*value))
        .transpose()?;
    walk_native_array_recursive(context, &mut array, &callback, userdata.as_ref(), source)?;
    root.set(Value::Array(array));
    context.encode_baseline_value(Value::Bool(true))
}

fn execute_native_array_predicate(
    context: &mut NativeRequestColdState<'_>,
    name: &str,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    let [array, callback] = arguments else {
        return Err(format!("{name}() expects exactly 2 arguments"));
    };
    if let Some(result) =
        try_execute_direct_native_array_predicate(context, name, *array, *callback, source)
    {
        return result;
    }
    execute_baseline_array_predicate(context, name, *array, *callback, source)
}

fn try_execute_direct_native_array_predicate(
    context: &mut NativeRequestColdState<'_>,
    name: &str,
    array: i64,
    callback: i64,
    source: &php_ir::Instruction,
) -> Option<Result<i64, String>> {
    let (start, length) = context.direct_array_entry_range(array)?;
    for index in 0..length {
        let entry = context.direct_array_entry_at(start, index);
        let result = match invoke_native_array_callback_by_value(
            context,
            callback,
            &[entry.value, entry.key],
            source,
        ) {
            Ok(result) => result,
            Err(error) => return Some(Err(error)),
        };
        let truthy = match consume_native_callback_truthiness(context, result) {
            Ok(truthy) => truthy,
            Err(error) => return Some(Err(error)),
        };
        if truthy {
            match name {
                "array_any" => {
                    return Some(Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE)));
                }
                "array_find" => {
                    return Some(duplicate_native_callback_owner(context, entry.value));
                }
                "array_find_key" => {
                    return Some(duplicate_native_callback_owner(context, entry.key));
                }
                "array_all" => continue,
                _ => {}
            }
        } else if name == "array_all" {
            return Some(Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE)));
        }
    }
    Some(Ok(php_jit::jit_encode_constant(match name {
        "array_all" => php_jit::JIT_VALUE_TRUE,
        "array_any" => php_jit::JIT_VALUE_FALSE,
        _ => u32::MAX,
    })))
}

/// Explicit cold compatibility for non-direct arrays.
fn execute_baseline_array_predicate(
    context: &mut NativeRequestColdState<'_>,
    name: &str,
    array: i64,
    callback: i64,
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    let array = native_array_argument(context, array, name)?;
    let callback = match context.decode_baseline_value(callback)? {
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
        if native_property_truthy(&context.decode_baseline_value(encoded)?) {
            match name {
                "array_any" => return context.encode_baseline_value(Value::Bool(true)),
                "array_find" => return context.encode_baseline_value(value.clone()),
                "array_find_key" => {
                    return context.encode_baseline_value(native_array_key_value(&key));
                }
                "array_all" => continue,
                _ => {}
            }
        } else if name == "array_all" {
            return context.encode_baseline_value(Value::Bool(false));
        }
    }
    context.encode_baseline_value(match name {
        "array_all" => Value::Bool(true),
        "array_any" => Value::Bool(false),
        _ => Value::Null,
    })
}

fn execute_native_iterator_to_array(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
) -> Result<i64, String> {
    if !(1..=2).contains(&arguments.len()) {
        return Err("iterator_to_array() expects 1 or 2 arguments".to_owned());
    }
    let iterator = match context.decode_baseline_value(arguments[0])? {
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
        .map(|value| context.decode_baseline_value(*value))
        .transpose()?
        .is_none_or(|value| native_property_truthy(&value));
    let class_name = iterator.class_name();
    if native_method_in_hierarchy(context, &class_name, "getIterator").is_some()
        || native_external_method(context, &class_name, "getIterator").is_some()
    {
        let encoded = invoke_baseline_native_bound_method(
            context,
            &php_runtime::api::CallableMethodTarget::Object(iterator.clone()),
            "getIterator",
            &[],
            None,
            context.unit.strict_types,
            None,
        )?;
        iterator = match context.decode_baseline_value(encoded)? {
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
        let has_method = |context: &NativeRequestColdState<'_>, method: &str| {
            native_method_in_hierarchy(context, &class_name, method).is_some()
                || native_external_method(context, &class_name, method).is_some()
        };
        if !["rewind", "valid", "current", "key", "next"]
            .iter()
            .all(|method| has_method(context, method))
        {
            return Err("iterator_to_array() requires a supported Traversable object".to_owned());
        }
        let invoke = |context: &mut NativeRequestColdState<'_>, method: &str| {
            let encoded = invoke_baseline_native_bound_method(
                context,
                &php_runtime::api::CallableMethodTarget::Object(iterator.clone()),
                method,
                &[],
                None,
                context.unit.strict_types,
                None,
            )?;
            context.decode_baseline_value(encoded)
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
    context.encode_native_array_owner(result)
}

fn native_sort_text(value: &Value, case_insensitive: bool) -> Vec<u8> {
    let mut value = native_string(value.clone()).unwrap_or_default();
    if case_insensitive {
        value.make_ascii_lowercase();
    }
    value
}

/// Baseline-only compatibility for value sorts rejected by the exact direct
/// entry handler (COW arrays, materialized references, or unsupported modes).
fn execute_baseline_value_sort(
    context: &mut NativeRequestColdState<'_>,
    name: &str,
    arguments: &[i64],
) -> Result<i64, String> {
    let Some(target) = arguments.first() else {
        return Err(format!("{name}() expects an array passed by reference"));
    };
    let Value::Reference(reference) = context.decode_baseline_value(*target)? else {
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
        .map(|value| context.decode_baseline_value(*value))
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
                    php_runtime::api::native_natural_compare(
                        &native_sort_text(&entries[cursor - 1].1, false),
                        &native_sort_text(&entries[cursor].1, false),
                        case_insensitive,
                    )
                    .cmp(&0)
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
    context.set_native_reference_value(&reference, Value::Array(sorted))?;
    context.encode_baseline_value(Value::Bool(true))
}

/// Baseline-only compatibility for `array_multisort` shapes rejected by the
/// exact direct handler. Rust `Value` reconstruction is deliberately confined
/// to this cold continuation.
fn execute_baseline_array_multisort(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
) -> Result<i64, String> {
    struct BaselineMultisortArray {
        reference: php_runtime::api::ReferenceCell,
        entries: Vec<(php_runtime::api::ArrayKey, Value)>,
        flags: i64,
        reverse: bool,
        direction_seen: bool,
        mode_seen: bool,
    }

    if arguments.is_empty() {
        return Err("array_multisort() expects at least 1 argument".to_owned());
    }
    let mut arrays = Vec::<BaselineMultisortArray>::new();
    for (index, argument) in arguments.iter().enumerate() {
        let decoded = context.decode_baseline_value(*argument)?;
        let (value, reference) = match decoded {
            Value::Reference(reference) => (reference.get(), Some(reference)),
            value => (value, None),
        };
        if let Value::Array(array) = value {
            let Some(reference) = reference else {
                return Err(format!(
                    "array_multisort(): Argument #{} must be passed by reference",
                    index + 1
                ));
            };
            let entries = array
                .iter()
                .map(|(key, value)| (key, value.clone()))
                .collect::<Vec<_>>();
            if arrays
                .first()
                .is_some_and(|existing| existing.entries.len() != entries.len())
            {
                return Err("array_multisort(): Array sizes are inconsistent".to_owned());
            }
            if arrays
                .iter()
                .any(|existing| existing.reference.ptr_eq(&reference))
            {
                return Err(
                    "array_multisort(): Argument arrays must not alias the same reference"
                        .to_owned(),
                );
            }
            arrays.push(BaselineMultisortArray {
                reference,
                entries,
                flags: 0,
                reverse: false,
                direction_seen: false,
                mode_seen: false,
            });
            continue;
        }
        let Value::Int(flag) = value else {
            return Err(format!(
                "array_multisort(): Argument #{} must be an array or a sort flag",
                index + 1
            ));
        };
        let current = arrays
            .last_mut()
            .ok_or_else(|| "array_multisort(): The first argument must be an array".to_owned())?;
        match flag {
            3 | 4 if !current.direction_seen => {
                current.reverse = flag == 3;
                current.direction_seen = true;
            }
            0 | 1 | 2 | 5 | 6 | 8 | 10 | 14 if !current.mode_seen => {
                current.flags = flag;
                current.mode_seen = true;
            }
            3 | 4 => {
                return Err(
                    "array_multisort(): Argument array has multiple sort order flags".to_owned(),
                );
            }
            0 | 1 | 2 | 5 | 6 | 8 | 10 | 14 => {
                return Err(
                    "array_multisort(): Argument array has multiple sort flags".to_owned(),
                );
            }
            _ => return Err(format!("array_multisort(): Invalid sort flag {flag}")),
        }
    }
    let length = arrays
        .first()
        .ok_or_else(|| "array_multisort(): The first argument must be an array".to_owned())?
        .entries
        .len();
    let mut order = (0..length).collect::<Vec<_>>();
    for index in 1..order.len() {
        let mut cursor = index;
        while cursor > 0 {
            let left = order[cursor - 1];
            let right = order[cursor];
            let mut ordering = std::cmp::Ordering::Equal;
            for array in &arrays {
                let left = &array.entries[left].1;
                let right = &array.entries[right].1;
                let mode = array.flags & !8;
                let case_insensitive = array.flags & 8 != 0;
                ordering = if mode == 1 {
                    let numeric = |value: &Value| {
                        native_string(value.clone())
                            .ok()
                            .and_then(|value| String::from_utf8(value).ok())
                            .and_then(|value| value.parse::<f64>().ok())
                            .unwrap_or(0.0)
                    };
                    numeric(left)
                        .partial_cmp(&numeric(right))
                        .unwrap_or(std::cmp::Ordering::Equal)
                } else if mode == 2 || mode == 5 {
                    let mut left = native_sort_text(left, case_insensitive);
                    let mut right = native_sort_text(right, case_insensitive);
                    if case_insensitive {
                        left.make_ascii_lowercase();
                        right.make_ascii_lowercase();
                    }
                    left.cmp(&right)
                } else if mode == 6 {
                    php_runtime::api::native_natural_compare(
                        &native_sort_text(left, false),
                        &native_sort_text(right, false),
                        case_insensitive,
                    )
                    .cmp(&0)
                } else {
                    php_runtime::api::compare_php(left, right)?
                };
                if array.reverse {
                    ordering = ordering.reverse();
                }
                if !ordering.is_eq() {
                    break;
                }
            }
            if !ordering.is_gt() {
                break;
            }
            order.swap(cursor - 1, cursor);
            cursor -= 1;
        }
    }

    for array in arrays {
        let mut sorted = php_runtime::api::PhpArray::new();
        for source in &order {
            let (key, value) = &array.entries[*source];
            match key {
                php_runtime::api::ArrayKey::Int(_) => {
                    sorted.append(value.clone());
                }
                php_runtime::api::ArrayKey::String(_) => {
                    sorted.insert(key.clone(), value.clone());
                }
            }
        }
        context.set_native_reference_value(&array.reference, Value::Array(sorted))?;
    }
    context.encode_baseline_value(Value::Bool(true))
}

pub(super) fn native_builtin_class(
    context: &NativeRequestColdState<'_>,
    name: &str,
) -> Option<crate::compiled_unit::CompiledClass> {
    let normalized = normalize_class_name(name);
    native_active_class_handle(context, &normalized)
        .or_else(|| native_external_class_handle(context, &normalized).map(|(_, class)| class))
}

pub(super) fn native_builtin_class_lineage(
    context: &NativeRequestColdState<'_>,
    name: &str,
) -> Vec<crate::compiled_unit::CompiledClass> {
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

fn native_builtin_is_subclass_of(
    context: &NativeRequestColdState<'_>,
    class_name: &str,
    expected: &str,
) -> bool {
    let expected = normalize_class_name(expected);
    let Some(class) = native_builtin_class(context, class_name) else {
        return false;
    };
    let mut pending = class
        .parent
        .iter()
        .chain(class.interfaces.iter())
        .cloned()
        .collect::<Vec<_>>();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(candidate) = pending.pop() {
        let normalized = normalize_class_name(&candidate);
        if normalized == expected {
            return true;
        }
        if !seen.insert(normalized.clone()) {
            continue;
        }
        let Some(class) = native_builtin_class(context, &normalized) else {
            continue;
        };
        pending.extend(class.parent.iter().cloned());
        pending.extend(class.interfaces.iter().cloned());
    }
    false
}

fn native_builtin_caller_class(
    context: &NativeRequestColdState<'_>,
    caller_locals: Option<(u32, &[php_jit::JitAbiSlot])>,
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

fn native_member_visible_from(
    context: &NativeRequestColdState<'_>,
    is_private: bool,
    is_protected: bool,
    declaring_class: &str,
    caller_class: Option<&str>,
) -> bool {
    if !is_private && !is_protected {
        return true;
    }
    let Some(caller) = caller_class else {
        return false;
    };
    if is_private {
        return normalize_class_name(caller) == normalize_class_name(declaring_class);
    }
    native_class_is_a(context, caller, declaring_class)
}

fn native_property_visible_from(
    context: &NativeRequestColdState<'_>,
    property: &php_ir::module::ClassPropertyEntry,
    declaring_class: &str,
    caller_class: Option<&str>,
) -> bool {
    native_member_visible_from(
        context,
        property.flags.is_private,
        property.flags.is_protected,
        declaring_class,
        caller_class,
    )
}

fn native_object_vars(
    context: &NativeRequestColdState<'_>,
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
            if !mangled
                && !native_property_visible_from(context, property, &class.name, caller_class)
            {
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

enum BaselinePregCallbackExecutionError {
    Semantic(php_runtime::experimental::pcre::PcreFailure),
    Runtime(String),
}

impl From<String> for BaselinePregCallbackExecutionError {
    fn from(error: String) -> Self {
        Self::Runtime(error)
    }
}

fn record_baseline_preg_callback_failure(
    context: &mut NativeRequestColdState<'_>,
    error: &php_runtime::experimental::pcre::PcreFailure,
) {
    context
        .builtin_request_state
        .pcre_mut()
        .last_error_mut()
        .set(
            error.code(),
            php_runtime::experimental::pcre::preg_error_message(error.code()),
        );
}

fn execute_native_preg_replace_callback(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    if !(3..=6).contains(&arguments.len()) {
        return Err("preg_replace_callback() expects 3 to 6 arguments".to_owned());
    }
    let pattern =
        PhpString::from_bytes(native_string(context.decode_baseline_value(arguments[0])?)?);
    let callback = match context.decode_baseline_value(arguments[1])? {
        Value::Reference(reference) => reference.get(),
        callback => callback,
    };
    if matches!(&callback, Value::Array(array) if array.len() != 2) {
        return Err(
            "E_PHP_THROW:TypeError:preg_replace_callback(): Argument #2 ($callback) must be a valid callback, array callback must have exactly two members"
                .to_owned(),
        );
    }
    let subject = match context.decode_baseline_value(arguments[2])? {
        Value::Reference(reference) => reference.get(),
        subject => subject,
    };
    let limit = arguments
        .get(3)
        .map(|limit| context.decode_baseline_value(*limit))
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
            context
                .builtin_request_state
                .pcre_mut()
                .last_error_mut()
                .set(
                    error.code(),
                    php_runtime::experimental::pcre::preg_error_message(error.code()),
                );
            emit_native_php_diagnostic(
                context,
                php_runtime::api::PHP_E_WARNING,
                &format!("preg_replace_callback(): {}", error.message()),
                source,
                true,
            )?;
            return context.encode_baseline_value(Value::Null);
        }
    };
    let replace = |context: &mut NativeRequestColdState<'_>,
                   subject: &[u8],
                   count: &mut i64|
     -> Result<Vec<u8>, BaselinePregCallbackExecutionError> {
        let mut output = Vec::new();
        let mut last_end = 0usize;
        let mut local_count = 0i64;
        let options = context
            .builtin_request_state
            .pcre_mut()
            .cache_mut()
            .match_options_for_subject_bytes_at_offset(&compiled, subject, 0)
            .map_err(BaselinePregCallbackExecutionError::Semantic)?;
        compiled.for_each_php_match_with_options(
            subject,
            0,
            options,
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
                )
                .map_err(BaselinePregCallbackExecutionError::Runtime)?;
                let decoded = context
                    .decode_baseline_value(encoded)
                    .map_err(BaselinePregCallbackExecutionError::Runtime)?;
                output.extend_from_slice(
                    &native_string(decoded).map_err(BaselinePregCallbackExecutionError::Runtime)?,
                );
                last_end = full.end();
                local_count += 1;
                *count += 1;
                Ok(true)
            },
            BaselinePregCallbackExecutionError::Semantic,
        )?;
        output.extend_from_slice(&subject[last_end..]);
        Ok(output)
    };
    let mut count = 0i64;
    let result = (|| -> Result<Value, BaselinePregCallbackExecutionError> {
        Ok(match subject {
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
        })
    })();
    let result = match result {
        Ok(result) => result,
        Err(BaselinePregCallbackExecutionError::Semantic(error)) => {
            record_baseline_preg_callback_failure(context, &error);
            return context.encode_baseline_value(Value::Null);
        }
        Err(BaselinePregCallbackExecutionError::Runtime(error)) => return Err(error),
    };
    context
        .builtin_request_state
        .pcre_mut()
        .last_error_mut()
        .clear();
    if let Some(count_argument) = arguments.get(4)
        && let Value::Reference(reference) = context.decode_baseline_value(*count_argument)?
    {
        context.set_native_reference_value(&reference, Value::Int(count))?;
    }
    context.encode_baseline_value(result)
}

fn execute_native_preg_replace_callback_array(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<Option<i64>, String> {
    if !(2..=4).contains(&arguments.len()) {
        return Ok(None);
    }
    let patterns = match context.decode_baseline_value(arguments[0])? {
        Value::Reference(reference) => reference.get(),
        patterns => patterns,
    };
    let Value::Array(patterns) = patterns else {
        return Ok(None);
    };
    let mut subject = match context.decode_baseline_value(arguments[1])? {
        Value::Reference(reference) => reference.get(),
        subject => subject,
    };
    let limit = arguments
        .get(2)
        .map(|limit| context.decode_baseline_value(*limit))
        .transpose()?
        .map_or(-1, |limit| match limit {
            Value::Int(limit) => limit,
            _ => -1,
        });
    let mut count = 0i64;

    for (pattern, callback) in patterns.iter() {
        let php_runtime::api::ArrayKey::String(pattern) = pattern else {
            return Err(
                "E_PHP_THROW:TypeError:preg_replace_callback_array(): Argument #1 ($pattern) must contain only string patterns"
                    .to_owned(),
            );
        };
        let callback = match callback {
            Value::Reference(reference) => reference.get(),
            callback => callback.clone(),
        };
        if matches!(&callback, Value::Array(array) if array.len() != 2) {
            return Err(
                "E_PHP_THROW:TypeError:preg_replace_callback_array(): Argument #1 ($pattern) must contain only valid callbacks"
                    .to_owned(),
            );
        }
        let compiled = match context
            .builtin_request_state
            .pcre_mut()
            .cache_mut()
            .compile(&pattern)
        {
            Ok(compiled) => compiled,
            Err(error) => {
                context
                    .builtin_request_state
                    .pcre_mut()
                    .last_error_mut()
                    .set(
                        error.code(),
                        php_runtime::experimental::pcre::preg_error_message(error.code()),
                    );
                emit_native_php_diagnostic(
                    context,
                    php_runtime::api::PHP_E_WARNING,
                    &format!("preg_replace_callback_array(): {}", error.message()),
                    source,
                    true,
                )?;
                return context.encode_baseline_value(Value::Null).map(Some);
            }
        };
        let replace = |context: &mut NativeRequestColdState<'_>,
                       source_bytes: &[u8],
                       count: &mut i64|
         -> Result<Vec<u8>, BaselinePregCallbackExecutionError> {
            let mut output = Vec::new();
            let mut last_end = 0usize;
            let mut local_count = 0i64;
            let options = context
                .builtin_request_state
                .pcre_mut()
                .cache_mut()
                .match_options_for_subject_bytes_at_offset(&compiled, source_bytes, 0)
                .map_err(BaselinePregCallbackExecutionError::Semantic)?;
            compiled.for_each_php_match_with_options(
                source_bytes,
                0,
                options,
                |captures| {
                    let Some(full) = captures.get(0) else {
                        return Ok(true);
                    };
                    if limit >= 0 && local_count >= limit {
                        return Ok(false);
                    }
                    output.extend_from_slice(&source_bytes[last_end..full.start()]);
                    let mut matches = php_runtime::api::PhpArray::new();
                    for index in 0..captures.len() {
                        let value = captures.get(index).map_or_else(
                            || Value::String(PhpString::from_bytes(Vec::new())),
                            |capture| {
                                Value::String(PhpString::from_bytes(
                                    source_bytes[capture.start()..capture.end()].to_vec(),
                                ))
                            },
                        );
                        matches.insert(
                            php_runtime::api::ArrayKey::Int(index as i64),
                            value.clone(),
                        );
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
                    )
                    .map_err(BaselinePregCallbackExecutionError::Runtime)?;
                    let decoded = context
                        .decode_baseline_value(encoded)
                        .map_err(BaselinePregCallbackExecutionError::Runtime)?;
                    output.extend_from_slice(
                        &native_string(decoded)
                            .map_err(BaselinePregCallbackExecutionError::Runtime)?,
                    );
                    last_end = full.end();
                    local_count += 1;
                    *count += 1;
                    Ok(true)
                },
                BaselinePregCallbackExecutionError::Semantic,
            )?;
            output.extend_from_slice(&source_bytes[last_end..]);
            Ok(output)
        };
        let replaced = (|| -> Result<Value, BaselinePregCallbackExecutionError> {
            Ok(match subject {
                Value::Array(array) => {
                    let mut result = php_runtime::api::PhpArray::new();
                    for (key, value) in array.iter() {
                        let subject = native_string(value.clone())?;
                        result.insert(
                            key,
                            Value::String(PhpString::from_bytes(replace(
                                context,
                                &subject,
                                &mut count,
                            )?)),
                        );
                    }
                    Value::Array(result)
                }
                value => {
                    let bytes = native_string(value)?;
                    Value::String(PhpString::from_bytes(replace(
                        context,
                        &bytes,
                        &mut count,
                    )?))
                }
            })
        })();
        subject = match replaced {
            Ok(subject) => subject,
            Err(BaselinePregCallbackExecutionError::Semantic(error)) => {
                record_baseline_preg_callback_failure(context, &error);
                return context.encode_baseline_value(Value::Null).map(Some);
            }
            Err(BaselinePregCallbackExecutionError::Runtime(error)) => return Err(error),
        };
    }
    context
        .builtin_request_state
        .pcre_mut()
        .last_error_mut()
        .clear();
    if let Some(count_argument) = arguments.get(3)
        && let Value::Reference(reference) = context.decode_baseline_value(*count_argument)?
    {
        context.set_native_reference_value(&reference, Value::Int(count))?;
    }
    context.encode_baseline_value(subject).map(Some)
}

fn native_ir_function_has_no_by_ref_parameters(function: &php_ir::IrFunction) -> Option<bool> {
    Some(!function.params.iter().any(|parameter| parameter.by_ref))
}

struct NativeCallableReferencePlan {
    display_name: String,
    by_ref_parameters: Vec<Option<String>>,
    target: NativeCallablePlanTarget,
}

enum NativeCallablePlanTarget {
    SameUnitFunction(php_ir::FunctionId),
    ExternalFunction(NativeDynamicFunction),
    Builtin(String),
    BoundMethod {
        target: php_runtime::api::CallableMethodTarget,
        method: String,
    },
    PreparedClosure {
        closure: php_runtime::api::ClosurePayload,
        implicit_this: Option<i64>,
        captures: smallvec::SmallVec<[i64; 8]>,
    },
}

fn native_ir_reference_parameter_names(
    function: &php_ir::IrFunction,
    argument_count: usize,
) -> Vec<Option<String>> {
    (0..argument_count)
        .map(|index| {
            function
                .params
                .get(index)
                .or_else(|| {
                    function
                        .params
                        .last()
                        .filter(|parameter| parameter.variadic)
                })
                .filter(|parameter| parameter.by_ref)
                .map(|parameter| parameter.name.clone())
        })
        .collect()
}

fn native_method_reference_plan(
    context: &NativeRequestColdState<'_>,
    target: php_runtime::api::CallableMethodTarget,
    method: &str,
    argument_count: usize,
) -> Option<NativeCallableReferencePlan> {
    let class = match &target {
        php_runtime::api::CallableMethodTarget::Object(object) => object.class_name(),
        php_runtime::api::CallableMethodTarget::Class(class) => class.clone(),
    };
    if let Some(function) = native_method_in_hierarchy(context, &class, method) {
        let function = context.unit.functions.get(function.index())?;
        return Some(NativeCallableReferencePlan {
            display_name: function.name.clone(),
            by_ref_parameters: native_ir_reference_parameter_names(function, argument_count),
            target: NativeCallablePlanTarget::BoundMethod {
                target,
                method: method.to_owned(),
            },
        });
    }
    if let Some((function, _)) = native_external_method(context, &class, method) {
        let function = context.dynamic_units.get(function.unit).and_then(|unit| {
            unit.compiled
                .unit()
                .functions
                .get(function.function.index())
        })?;
        return Some(NativeCallableReferencePlan {
            display_name: function.name.clone(),
            by_ref_parameters: native_ir_reference_parameter_names(function, argument_count),
            target: NativeCallablePlanTarget::BoundMethod {
                target,
                method: method.to_owned(),
            },
        });
    }
    php_std::generated::arginfo::method_metadata_in_hierarchy(&class, method).map(|metadata| {
        NativeCallableReferencePlan {
            display_name: format!("{}::{}", metadata.class_name, metadata.name),
            by_ref_parameters: (0..argument_count)
                .map(|index| {
                    metadata
                        .params
                        .get(index)
                        .or_else(|| {
                            metadata
                                .params
                                .last()
                                .filter(|parameter| parameter.variadic)
                        })
                        .filter(|parameter| parameter.by_ref)
                        .map(|parameter| parameter.name.to_owned())
                })
                .collect(),
            target: NativeCallablePlanTarget::BoundMethod {
                target,
                method: method.to_owned(),
            },
        }
    })
}

fn native_named_callable_reference_plan(
    context: &NativeRequestColdState<'_>,
    name: &str,
    argument_count: usize,
) -> Option<NativeCallableReferencePlan> {
    if let Some((class, method)) = name.split_once("::") {
        return native_method_reference_plan(
            context,
            php_runtime::api::CallableMethodTarget::Class(class.to_owned()),
            method,
            argument_count,
        );
    }
    if let Some(function) = context.function_id(name) {
        let metadata = context.unit.functions.get(function.index())?;
        return Some(NativeCallableReferencePlan {
            display_name: metadata.name.clone(),
            by_ref_parameters: native_ir_reference_parameter_names(metadata, argument_count),
            target: NativeCallablePlanTarget::SameUnitFunction(function),
        });
    }
    if let Some(function) = context.external_function(name) {
        let metadata = context.dynamic_units.get(function.unit).and_then(|unit| {
            unit.compiled
                .unit()
                .functions
                .get(function.function.index())
        })?;
        return Some(NativeCallableReferencePlan {
            display_name: metadata.name.clone(),
            by_ref_parameters: native_ir_reference_parameter_names(metadata, argument_count),
            target: NativeCallablePlanTarget::ExternalFunction(function),
        });
    }
    php_std::arginfo::function_metadata_indexed(name).map(|function| NativeCallableReferencePlan {
        display_name: function.name.to_owned(),
        by_ref_parameters: (0..argument_count)
            .map(|index| {
                function
                    .params
                    .get(index)
                    .or_else(|| {
                        function
                            .params
                            .last()
                            .filter(|parameter| parameter.variadic)
                    })
                    .filter(|parameter| parameter.by_ref)
                    .map(|parameter| parameter.name.to_owned())
            })
            .collect(),
        target: NativeCallablePlanTarget::Builtin(name.to_owned()),
    })
}

fn native_named_callable_has_no_by_ref_parameters(
    context: &NativeRequestColdState<'_>,
    name: &str,
) -> Option<bool> {
    if let Some((class, method)) = name.split_once("::") {
        return native_method_has_no_by_ref_parameters(context, class, method);
    }
    if let Some(function) = context.function_id(name) {
        return context
            .unit
            .functions
            .get(function.index())
            .and_then(native_ir_function_has_no_by_ref_parameters);
    }
    if let Some(function) = context.external_function(name) {
        return context
            .dynamic_units
            .get(function.unit)
            .and_then(|unit| {
                unit.compiled
                    .unit()
                    .functions
                    .get(function.function.index())
            })
            .and_then(native_ir_function_has_no_by_ref_parameters);
    }
    php_std::arginfo::function_metadata_indexed(name)
        .map(|function| !function.params.iter().any(|parameter| parameter.by_ref))
}

fn native_method_has_no_by_ref_parameters(
    context: &NativeRequestColdState<'_>,
    class: &str,
    method: &str,
) -> Option<bool> {
    if let Some(function) = native_method_in_hierarchy(context, class, method) {
        return context
            .unit
            .functions
            .get(function.index())
            .and_then(native_ir_function_has_no_by_ref_parameters);
    }
    if let Some((function, _)) = native_external_method(context, class, method) {
        return context
            .dynamic_units
            .get(function.unit)
            .and_then(|unit| {
                unit.compiled
                    .unit()
                    .functions
                    .get(function.function.index())
            })
            .and_then(native_ir_function_has_no_by_ref_parameters);
    }
    php_std::generated::arginfo::method_metadata_in_hierarchy(class, method)
        .map(|method| !method.params.iter().any(|parameter| parameter.by_ref))
}

fn native_encoded_callable_reference_plan(
    context: &NativeRequestColdState<'_>,
    encoded: i64,
    argument_count: usize,
) -> Option<NativeCallableReferencePlan> {
    let encoded = context.dereference_direct_encoding(encoded);
    match context.native_encoded_value_kind(encoded)? {
        NativeEncodedValueKind::String => {
            let name = context.native_string_name_bytes(encoded)?;
            let name = String::from_utf8_lossy(&name).into_owned();
            native_named_callable_reference_plan(context, &name, argument_count)
        }
        NativeEncodedValueKind::Object => {
            let target = php_runtime::api::CallableMethodTarget::Object(
                context.native_query_object(encoded)?,
            );
            native_method_reference_plan(context, target, "__invoke", argument_count)
        }
        NativeEncodedValueKind::Array => {
            if let Some(entries) = context.direct_array_entries_for(encoded) {
                if entries.len() != 2 {
                    return None;
                }
                let mut target = None;
                let mut method = None;
                for entry in entries {
                    match context.native_encoded_int(entry.key) {
                        Some(0) => target = Some(entry.value),
                        Some(1) => method = Some(entry.value),
                        _ => return None,
                    }
                }
                let target = context.dereference_direct_encoding(target?);
                let method = context.dereference_direct_encoding(method?);
                let method = context.native_string_name_bytes(method)?;
                let method = String::from_utf8_lossy(&method);
                let target = if let Some(object) = context.native_query_object(target) {
                    php_runtime::api::CallableMethodTarget::Object(object)
                } else {
                    let class = context.native_string_name_bytes(target)?;
                    php_runtime::api::CallableMethodTarget::Class(
                        String::from_utf8_lossy(&class).into_owned(),
                    )
                };
                return native_method_reference_plan(context, target, &method, argument_count);
            }
            None
        }
        NativeEncodedValueKind::Callable => {
            if let Some((closure, implicit_this, captures)) =
                context.prepared_closure_invocation(encoded)
            {
                let function = php_ir::FunctionId::new(closure.function);
                let function_metadata = closure
                    .context
                    .owner_unit
                    .and_then(|unit| context.dynamic_units.get(unit))
                    .map(|unit| unit.compiled.unit())
                    .unwrap_or(&context.unit)
                    .functions
                    .get(function.index())?;
                let display_name = closure
                    .debug
                    .as_ref()
                    .map_or_else(|| "{closure}".to_owned(), |debug| debug.name.clone());
                return Some(NativeCallableReferencePlan {
                    display_name,
                    by_ref_parameters: native_ir_reference_parameter_names(
                        function_metadata,
                        argument_count,
                    ),
                    target: NativeCallablePlanTarget::PreparedClosure {
                        closure,
                        implicit_this,
                        captures,
                    },
                });
            }
            if let Some(callable) = context.prepared_callable_dispatch(encoded) {
                return match callable {
                    NativePreparedCallableDispatch::Named(name) => {
                        native_named_callable_reference_plan(context, &name, argument_count)
                    }
                    NativePreparedCallableDispatch::BoundMethod { target, method } => {
                        native_method_reference_plan(context, target, &method, argument_count)
                    }
                    NativePreparedCallableDispatch::Closure
                    | NativePreparedCallableDispatch::Invalid(_) => None,
                };
            }
            None
        }
        NativeEncodedValueKind::Reference => None,
        _ => None,
    }
}

fn invoke_native_callable_reference_plan(
    context: &mut NativeRequestColdState<'_>,
    plan: &NativeCallableReferencePlan,
    arguments: &[i64],
    source: &php_ir::Instruction,
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    caller_function: Option<u32>,
) -> NativeCallResult {
    let strict = context.unit.strict_types_for_span(source.span);
    match &plan.target {
        NativeCallablePlanTarget::SameUnitFunction(function) => {
            if native_function_is_generator(context, *function) {
                create_baseline_bound_generator_with_metadata_strict(
                    context, *function, arguments, metadata, strict,
                )
            } else {
                invoke_baseline_bound_function_with_metadata_strict(
                    context, *function, arguments, metadata, strict, false,
                )
            }
        }
        NativeCallablePlanTarget::ExternalFunction(function) => {
            if external_function_is_generator(context, *function) {
                create_native_external_generator_with_metadata(
                    context, *function, arguments, metadata, None, strict,
                )
            } else {
                invoke_native_external_function_with_metadata(
                    context, *function, arguments, metadata, None, strict,
                )
            }
        }
        NativeCallablePlanTarget::Builtin(name) => {
            let expanded = bind_native_builtin_arguments(context, name, arguments, metadata)?;
            execute_baseline_native_builtin_control(context, name, &expanded, source, None, None)
        }
        NativeCallablePlanTarget::BoundMethod { target, method } => {
            invoke_baseline_native_bound_method(
                context,
                target,
                method,
                arguments,
                metadata,
                strict,
                caller_function,
            )
        }
        NativeCallablePlanTarget::PreparedClosure {
            closure,
            implicit_this,
            captures,
        } => invoke_native_closure_payload(
            context,
            closure,
            *implicit_this,
            Some(captures),
            arguments,
            source,
            metadata,
        ),
    }
}

fn native_callable_has_no_by_ref_parameters(
    context: &NativeRequestColdState<'_>,
    callable: &Value,
) -> Option<bool> {
    match callable {
        Value::Reference(reference) => {
            native_callable_has_no_by_ref_parameters(context, &reference.get())
        }
        Value::String(name) => {
            native_named_callable_has_no_by_ref_parameters(context, name.to_string_lossy().as_ref())
        }
        Value::Callable(callable) => match callable.as_ref() {
            php_runtime::api::CallableValue::UserFunction { name }
            | php_runtime::api::CallableValue::InternalBuiltin { name } => {
                native_named_callable_has_no_by_ref_parameters(context, name)
            }
            php_runtime::api::CallableValue::Closure(closure) => {
                let function = php_ir::FunctionId::new(closure.function);
                closure
                    .context
                    .owner_unit
                    .and_then(|unit| context.dynamic_units.get(unit))
                    .map(|unit| unit.compiled.unit())
                    .unwrap_or(&context.unit)
                    .functions
                    .get(function.index())
                    .and_then(native_ir_function_has_no_by_ref_parameters)
            }
            php_runtime::api::CallableValue::BoundMethod { target, method, .. } => {
                let class = match target {
                    php_runtime::api::CallableMethodTarget::Object(object) => object.class_name(),
                    php_runtime::api::CallableMethodTarget::Class(class) => class.clone(),
                };
                native_method_has_no_by_ref_parameters(context, &class, method)
            }
            php_runtime::api::CallableValue::MethodPlaceholder { .. }
            | php_runtime::api::CallableValue::UnresolvedDynamic { .. } => None,
        },
        Value::Object(object) => {
            native_method_has_no_by_ref_parameters(context, &object.class_name(), "__invoke")
        }
        Value::Array(array) => {
            let target = array.get(&php_runtime::api::ArrayKey::Int(0))?;
            let method = array.get(&php_runtime::api::ArrayKey::Int(1))?;
            let Value::String(method) = method else {
                return None;
            };
            let class = match target {
                Value::Reference(reference) => match reference.get() {
                    Value::Object(object) => object.class_name(),
                    Value::String(class) => class.to_string_lossy(),
                    _ => return None,
                },
                Value::Object(object) => object.class_name(),
                Value::String(class) => class.to_string_lossy(),
                _ => return None,
            };
            native_method_has_no_by_ref_parameters(
                context,
                &class,
                method.to_string_lossy().as_ref(),
            )
        }
        _ => None,
    }
}

pub(super) fn execute_native_call_user_func_encoded(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
    caller_function: Option<u32>,
) -> NativeCallResult {
    let [callback, call_arguments @ ..] = arguments else {
        return Err("call_user_func() expects a callback".into());
    };
    let callback = *callback;
    let Some(reference_plan) =
        native_encoded_callable_reference_plan(context, callback, call_arguments.len())
    else {
        let direct_callback = context.dereference_direct_encoding(callback);
        if context.native_encoded_value_kind(direct_callback)
            == Some(NativeEncodedValueKind::String)
            && let Some(name) = context.native_string_name_bytes(direct_callback)
        {
            let name = String::from_utf8_lossy(&name);
            let detail = if let Some((class, _)) = name.split_once("::") {
                format!("class \"{class}\" not found")
            } else {
                format!("function \"{name}\" not found or invalid function name")
            };
            return Err(NativeCallControl::throw(
                "TypeError",
                format!(
                    "call_user_func(): Argument #1 ($callback) must be a valid callback, {detail}"
                ),
            ));
        }
        // The dynamic callable dispatcher already consumes this exact native
        // vector. Do not mirror it into request scratch merely to pass the
        // same encodings onward unchanged.
        return invoke_native_encoded_callable_value_from(
            context,
            arguments,
            source,
            None,
            caller_function,
        );
    };
    let needs_temporary_references =
        call_arguments
            .iter()
            .copied()
            .enumerate()
            .any(|(index, value)| {
                reference_plan
                    .by_ref_parameters
                    .get(index)
                    .and_then(Option::as_ref)
                    .is_some()
                    && context.php_handle_is_reference(value) != Some(true)
            });
    if !needs_temporary_references {
        return invoke_native_callable_reference_plan(
            context,
            &reference_plan,
            call_arguments,
            source,
            None,
            caller_function,
        );
    }

    let mut encoded = std::mem::take(&mut context.native_call_encoded_scratch);
    encoded.clear();
    encoded.reserve(call_arguments.len());
    encoded.extend_from_slice(call_arguments);
    let mut temporary_references = Vec::new();
    let result = (|| -> NativeCallResult {
        for (index, value) in call_arguments.iter().copied().enumerate() {
            let Some(parameter_name) = reference_plan
                .by_ref_parameters
                .get(index)
                .and_then(Option::as_ref)
                .filter(|_| context.php_handle_is_reference(value) != Some(true))
            else {
                continue;
            };
            emit_native_php_warning(
                context,
                php_runtime::api::PHP_E_WARNING,
                &format!(
                    "{}(): Argument #{} (${}) must be passed by reference, value given",
                    reference_plan.display_name,
                    index + 1,
                    parameter_name,
                ),
                source,
            )?;
            let payload = context.duplicate_dereferenced_native_value(value)?;
            let reference = match context.encode_direct_reference_payload_owned(payload) {
                Ok(reference) => reference,
                Err(error) => {
                    context.release(payload)?;
                    return Err(error.into());
                }
            };
            encoded[index] = reference;
            temporary_references.push(reference);
        }
        invoke_native_callable_reference_plan(
            context,
            &reference_plan,
            &encoded,
            source,
            None,
            caller_function,
        )
    })();
    let mut release_error = None;
    for reference in temporary_references {
        if let Err(error) = context.release(reference) {
            release_error.get_or_insert(error);
        }
    }
    encoded.clear();
    context.native_call_encoded_scratch = encoded;
    match (result, release_error) {
        (Err(control), _) => Err(control),
        (Ok(_), Some(error)) => Err(error.into()),
        (Ok(value), None) => Ok(value),
    }
}

pub(super) fn execute_native_call_user_func_array_direct(
    context: &mut NativeRequestColdState<'_>,
    callback: i64,
    arguments: i64,
    source: &php_ir::Instruction,
    caller_function: Option<u32>,
) -> Option<NativeCallResult> {
    let (entry_start, entry_count) = context.direct_array_entry_range(arguments)?;
    let reference_plan = native_encoded_callable_reference_plan(context, callback, entry_count);
    let mut encoded = std::mem::take(&mut context.native_call_encoded_scratch);
    encoded.clear();
    encoded.reserve(entry_count + 1);
    encoded.push(callback);
    let mut temporary_references = Vec::new();
    let result = (|| -> NativeCallResult {
        let mut metadata: Option<Vec<php_ir::instruction::IrCallArg>> = None;
        for index in 0..entry_count {
            let entry = context.direct_array_entry_at(entry_start, index);
            let mut encoded_value = entry.value;
            if let Some((callable_name, parameter_name)) =
                reference_plan.as_ref().and_then(|plan| {
                    plan.by_ref_parameters
                        .get(index)
                        .and_then(Option::as_ref)
                        .map(|parameter| (&plan.display_name, parameter))
                })
                && context.php_handle_is_reference(encoded_value) != Some(true)
            {
                emit_native_php_warning(
                    context,
                    php_runtime::api::PHP_E_WARNING,
                    &format!(
                        "{callable_name}(): Argument #{} (${}) must be passed by reference, value given",
                        index + 1,
                        parameter_name,
                    ),
                    source,
                )?;
                let payload = context.duplicate_dereferenced_native_value(encoded_value)?;
                encoded_value = match context.encode_direct_reference_payload_owned(payload) {
                    Ok(reference) => reference,
                    Err(error) => {
                        context.release(payload)?;
                        return Err(error.into());
                    }
                };
                temporary_references.push(encoded_value);
            }
            encoded.push(encoded_value);
            let name = match context.native_encoded_value_kind(entry.key) {
                Some(NativeEncodedValueKind::Int) => None,
                Some(NativeEncodedValueKind::String) => Some(
                    context
                        .native_string_name_bytes(entry.key)
                        .map(|name| String::from_utf8_lossy(&name).into_owned())
                        .ok_or_else(|| {
                            NativeCallControl::from(
                                "call_user_func_array(): string key has no byte storage",
                            )
                        })?,
                ),
                _ => {
                    return Err(format!(
                        "call_user_func_array(): array key must be int or string, {} given",
                        context.native_encoded_type_name(entry.key)
                    )
                    .into());
                }
            };
            if name.is_some() && metadata.is_none() {
                metadata = Some(
                    (0..encoded.len().saturating_sub(2))
                        .map(|_| positional_native_call_argument())
                        .collect(),
                );
            }
            if let Some(metadata) = metadata.as_mut() {
                let mut argument = positional_native_call_argument();
                argument.name = name;
                metadata.push(argument);
            }
        }
        if let Some(plan) = reference_plan.as_ref() {
            invoke_native_callable_reference_plan(
                context,
                plan,
                &encoded[1..],
                source,
                metadata.as_deref(),
                caller_function,
            )
        } else {
            invoke_native_encoded_callable_value_from(
                context,
                &encoded,
                source,
                metadata,
                caller_function,
            )
        }
    })();
    let mut release_error = None;
    for reference in temporary_references {
        if let Err(error) = context.release(reference) {
            release_error.get_or_insert(error);
        }
    }
    encoded.clear();
    context.native_call_encoded_scratch = encoded;
    Some(match (result, release_error) {
        (Err(control), _) => Err(control),
        (Ok(_), Some(error)) => Err(error.into()),
        (Ok(value), None) => Ok(value),
    })
}
