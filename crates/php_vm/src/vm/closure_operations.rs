use super::prelude::*;

pub(super) fn is_std_class_object(object: &ObjectRef) -> bool {
    normalize_class_name(&object.class_name()) == "stdclass"
}

pub(super) fn closure_static_method_value(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    stack: &CallStack,
    method: &str,
    args: Vec<CallArgument>,
    output: &mut OutputBuffer,
    source_span: RuntimeSourceSpan,
) -> Result<Value, String> {
    match normalize_method_name(method).as_str() {
        "getcurrent" => {
            let values = call_args_to_positional("Closure::getCurrent", args)?;
            if !values.is_empty() {
                return Err(format!(
                    "E_PHP_VM_TOO_MANY_ARGS: Closure::getCurrent expects 0 argument(s), {} given",
                    values.len()
                ));
            }
            current_closure_value(compiled, state, stack)
        }
        "fromcallable" => {
            let mut values = call_args_to_positional("Closure::fromCallable", args)?;
            if values.len() != 1 {
                let id = if values.is_empty() {
                    "E_PHP_VM_TOO_FEW_ARGS"
                } else {
                    "E_PHP_VM_TOO_MANY_ARGS"
                };
                return Err(format!(
                    "{id}: Closure::fromCallable expects exactly 1 argument, {} given",
                    values.len()
                ));
            }
            let callable = resolve_closure_from_callable_relative_value(
                compiled,
                state,
                stack,
                values.remove(0),
                output,
                source_span,
            )?;
            let callable = acquire_callable_value(compiled, state, stack, callable)
                .map_err(closure_from_callable_error)?;
            Ok(closure_from_acquired_callable(callable))
        }
        "bind" => {
            let mut values = call_args_to_positional("Closure::bind", args)?;
            if values.len() < 2 {
                return Err(format!(
                    "E_PHP_VM_TOO_FEW_ARGS: Closure::bind expects at least 2 arguments, {} given",
                    values.len()
                ));
            }
            if values.len() > 3 {
                return Err(format!(
                    "E_PHP_VM_TOO_MANY_ARGS: Closure::bind expects at most 3 arguments, {} given",
                    values.len()
                ));
            }
            if let Some(scope) = values.get(2) {
                match callable_resolve_reference(scope.clone()) {
                    Value::Null | Value::String(_) | Value::Object(_) => {}
                    other => {
                        return Err(format!(
                            "E_PHP_VM_PARAM_TYPE_MISMATCH: Closure::bind(): Argument #3 ($newScope) must be of type object|string|null, {} given",
                            value_type_name(&other)
                        ));
                    }
                }
            }
            let closure = callable_resolve_reference(values.remove(0));
            let Value::Callable(callable) = closure else {
                return Err(format!(
                    "E_PHP_VM_PARAM_TYPE_MISMATCH: Closure::bind(): Argument #1 ($closure) must be of type Closure, {} given",
                    value_type_name(&closure)
                ));
            };
            let new_this = callable_resolve_reference(values.remove(0));
            let bound_this = match new_this {
                Value::Null => None,
                Value::Object(object) => Some(object),
                other => {
                    return Err(format!(
                        "E_PHP_VM_PARAM_TYPE_MISMATCH: Closure::bind(): Argument #2 ($newThis) must be of type ?object, {} given",
                        value_type_name(&other)
                    ));
                }
            };
            Ok(bind_closure_callable_value(*callable, bound_this))
        }
        _ => Err(format!(
            "E_PHP_VM_UNKNOWN_METHOD: method Closure::{method} is not defined"
        )),
    }
}

pub(super) fn bind_closure_callable_value(
    callable: CallableValue,
    bound_this: Option<ObjectRef>,
) -> Value {
    match callable {
        CallableValue::Closure(payload) => {
            // Scope/declaring carry the canonical lookup name; the
            // late-static-binding called class keeps the PHP-visible display
            // spelling, matching what method activation stores in frames.
            let rebound_class = bound_this.as_ref().map(ObjectRef::class_name_handle);
            let rebound_display = bound_this.as_ref().map(object_called_class_handle);
            let context = ClosureContext {
                owner_unit: payload.context.owner_unit,
                scope_class: rebound_class.clone().or(payload.context.scope_class),
                called_class: rebound_display.or(payload.context.called_class),
                declaring_class: rebound_class.or(payload.context.declaring_class),
            };
            Value::closure(
                ClosurePayload::new(payload.function, payload.captures)
                    .with_debug(payload.debug.map(|debug| *debug))
                    .with_bound_this(bound_this.clone())
                    .with_context(context),
            )
        }
        CallableValue::BoundMethod {
            target,
            method,
            scope,
        } => {
            let rebound_target = match bound_this {
                Some(object) => CallableMethodTarget::Object(object),
                None => target,
            };
            Value::bound_method_callable(rebound_target, method, scope)
        }
        other => Value::Callable(Box::new(other)),
    }
}

pub(super) fn callable_closure_should_warn_unbind_this(callable: &CallableValue) -> bool {
    let CallableValue::Closure(payload) = callable else {
        return false;
    };
    payload.bound_this.is_some()
        && payload
            .captures
            .iter()
            .any(|capture| capture.name == "this")
}

pub(super) fn closure_from_acquired_callable(callable: Value) -> Value {
    match callable {
        Value::String(name) => {
            let name = name.to_string_lossy();
            if let Some((class_name, method)) = name.split_once("::") {
                Value::bound_method_callable(
                    CallableMethodTarget::Class(class_name.to_owned()),
                    method.to_owned(),
                    None,
                )
            } else if is_callable_builtin_name(&normalize_function_name(&name)) {
                Value::internal_builtin_callable(normalize_function_name(&name))
            } else {
                Value::user_function_callable(normalize_function_name(&name))
            }
        }
        other => other,
    }
}

pub(super) fn closure_from_callable_error(message: String) -> String {
    let (id, reason) = message
        .split_once(": ")
        .filter(|(id, _)| id.starts_with("E_"))
        .map_or(
            (
                "E_PHP_VM_FIRST_CLASS_CALLABLE_NOT_CALLABLE",
                message.as_str(),
            ),
            |(id, reason)| (id, reason),
        );
    let mut reason = reason.to_owned();
    if let Some(first) = reason.get_mut(0..1) {
        first.make_ascii_lowercase();
    }
    format!("{id}: Failed to create closure from callable: {reason}")
}

pub(super) fn resolve_closure_from_callable_relative_value(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    stack: &CallStack,
    value: Value,
    output: &mut OutputBuffer,
    source_span: RuntimeSourceSpan,
) -> Result<Value, String> {
    match value {
        Value::String(name) => {
            let name_text = name.to_string_lossy();
            let Some(resolved) = resolve_relative_callable_value(
                compiled,
                state,
                stack,
                &name_text,
                output,
                source_span,
            )?
            else {
                return Ok(Value::String(name));
            };
            Ok(resolved)
        }
        Value::Array(array) => {
            let mut resolved = PhpArray::new();
            for (key, value) in array.iter() {
                let value = if key == ArrayKey::Int(0) {
                    match effective_value(value) {
                        Value::String(class_name) => resolve_relative_callable_class_name(
                            compiled,
                            state,
                            stack,
                            &class_name.to_string_lossy(),
                            output,
                            source_span.clone(),
                        )?
                        .map(Value::string)
                        .unwrap_or_else(|| value.clone()),
                        _ => value.clone(),
                    }
                } else {
                    value.clone()
                };
                resolved.insert(key.clone(), value);
            }
            Ok(Value::Array(resolved))
        }
        other => Ok(other),
    }
}

pub(super) fn resolve_relative_callable_value(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    stack: &CallStack,
    name: &str,
    output: &mut OutputBuffer,
    source_span: RuntimeSourceSpan,
) -> Result<Option<Value>, String> {
    let Some((class_name, method)) = name.split_once("::") else {
        return Ok(None);
    };
    let Some(resolved_class) = resolve_relative_callable_class_name(
        compiled,
        state,
        stack,
        class_name,
        output,
        source_span,
    )?
    else {
        return Ok(None);
    };
    if let Some(object) = current_this_object(compiled, stack)
        && lookup_resolved_method_in_state(compiled, state, &resolved_class, method, None)?
            .is_some_and(|resolved| !resolved.method.flags.is_static)
    {
        validate_object_method_callable_acquisition(
            compiled,
            state,
            stack,
            &object.class_name(),
            method,
            true,
        )?;
        return Ok(Some(Value::bound_method_callable(
            CallableMethodTarget::Object(object),
            method.to_owned(),
            current_scope_class(compiled, stack),
        )));
    }
    Ok(Some(Value::string(format!("{resolved_class}::{method}"))))
}

pub(super) fn resolve_relative_callable_class_name(
    compiled: &CompiledUnit,
    state: &mut ExecutionState,
    stack: &CallStack,
    class_name: &str,
    output: &mut OutputBuffer,
    source_span: RuntimeSourceSpan,
) -> Result<Option<String>, String> {
    match normalize_class_name(class_name).as_str() {
        "self" => {
            emit_relative_callable_deprecation(compiled, output, stack, state, "self", source_span);
            current_scope_class(compiled, stack)
                .ok_or_else(|| {
                    "E_PHP_VM_INVALID_STATIC_SCOPE: self:: is not available outside class scope"
                        .to_owned()
                })
                .map(Some)
        }
        "static" => {
            emit_relative_callable_deprecation(
                compiled,
                output,
                stack,
                state,
                "static",
                source_span,
            );
            current_called_class(compiled, stack)
                .ok_or_else(|| {
                    "E_PHP_VM_INVALID_STATIC_SCOPE: static:: is not available outside class scope"
                        .to_owned()
                })
                .map(Some)
        }
        "parent" => {
            emit_relative_callable_deprecation(
                compiled,
                output,
                stack,
                state,
                "parent",
                source_span,
            );
            let scope = current_scope_class(compiled, stack).ok_or_else(|| {
                "E_PHP_VM_INVALID_STATIC_SCOPE: parent:: is not available outside class scope"
                    .to_owned()
            })?;
            let class = lookup_class_in_state(compiled, state, &scope)
                .ok_or_else(|| format!("E_PHP_VM_UNKNOWN_CLASS: class {scope} is not defined"))?;
            class
                .parent
                .clone()
                .ok_or_else(|| {
                    format!(
                        "E_PHP_VM_NO_PARENT_CLASS: class {} has no parent",
                        class.name
                    )
                })
                .map(Some)
        }
        _ => Ok(None),
    }
}

pub(super) fn emit_relative_callable_deprecation(
    compiled: &CompiledUnit,
    output: &mut OutputBuffer,
    stack: &CallStack,
    state: &mut ExecutionState,
    keyword: &str,
    source_span: RuntimeSourceSpan,
) {
    if !error_reporting_allows(state, php_runtime::api::PHP_E_DEPRECATED) {
        return;
    }
    let diagnostic = RuntimeDiagnostic::new(
        "E_PHP_VM_RELATIVE_CALLABLE_DEPRECATED",
        RuntimeSeverity::Deprecation,
        format!("Use of \"{keyword}\" in callables is deprecated"),
        source_span,
        stack_trace(compiled, stack),
        None,
    );
    emit_vm_diagnostic(
        output,
        state,
        &diagnostic,
        php_runtime::api::PhpDiagnosticChannel::Deprecated,
        php_runtime::api::PHP_E_DEPRECATED,
    );
    state.diagnostics.push(diagnostic);
}

pub(super) fn current_closure_value(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
) -> Result<Value, String> {
    let frame = stack
        .current()
        .ok_or_else(|| "E_PHP_VM_CURRENT_CLOSURE: Current function is not a closure".to_owned())?;
    let function = compiled
        .unit()
        .functions
        .get(frame.function.index())
        .ok_or_else(|| "E_PHP_VM_CURRENT_CLOSURE: Current function is not a closure".to_owned())?;
    if !function.flags.is_closure {
        return Err("E_PHP_VM_CURRENT_CLOSURE: Current function is not a closure".to_owned());
    }

    let mut captures = Vec::with_capacity(function.captures.len());
    for metadata in &function.captures {
        let slot = frame
            .locals
            .iter()
            .find(|(index, _)| *index == metadata.local.index())
            .map(|(_, slot)| slot)
            .ok_or_else(|| format!("invalid local local:{}", metadata.local.raw()))?;
        if metadata.by_ref {
            let Slot::Reference(cell) = slot else {
                return Err(format!(
                    "E_PHP_VM_BY_REF_CAPTURE_MISSING_CELL: closure capture ${} has no reference cell",
                    metadata.name
                ));
            };
            captures.push(ClosureCaptureValue::by_reference(
                metadata.name.clone(),
                cell.clone(),
            ));
        } else {
            captures.push(ClosureCaptureValue::by_value(
                metadata.name.clone(),
                slot.read(),
            ));
        }
    }

    let bound_this = (!function.flags.is_static)
        .then(|| current_this_object(compiled, stack))
        .flatten();
    Ok(Value::closure(
        ClosurePayload::new(frame.function.raw(), captures)
            .with_debug(closure_debug_info(compiled, frame.function))
            .with_bound_this(bound_this)
            .with_context(ClosureContext {
                owner_unit: dynamic_unit_index_for_compiled(state, compiled),
                scope_class: current_scope_class(compiled, stack).map(Arc::from),
                // Display spelling: `static::class` inside the closure must
                // reproduce the PHP-visible name, like method frames do.
                called_class: current_called_class_display(compiled, stack).map(Arc::from),
                declaring_class: current_scope_class(compiled, stack).map(Arc::from),
            }),
    ))
}

pub(super) fn make_closure_value(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    function: FunctionId,
    captured: Vec<ClosureCaptureValue>,
) -> Value {
    let bound_this = compiled
        .unit()
        .functions
        .get(function.index())
        .filter(|closure| closure.flags.is_closure && !closure.flags.is_static)
        .and_then(|_| current_this_object(compiled, stack));
    Value::closure(
        ClosurePayload::new(function.raw(), captured)
            .with_debug(closure_debug_info(compiled, function))
            .with_bound_this(bound_this)
            .with_context(ClosureContext {
                owner_unit: dynamic_unit_index_for_compiled(state, compiled),
                scope_class: current_scope_class(compiled, stack).map(Arc::from),
                // Display spelling: `static::class` inside the closure must
                // reproduce the PHP-visible name, like method frames do.
                called_class: current_called_class_display(compiled, stack).map(Arc::from),
                declaring_class: current_scope_class(compiled, stack).map(Arc::from),
            }),
    )
}

pub(super) fn evaluate_closure_captures(
    unit: &IrUnit,
    stack: &mut CallStack,
    captures: &[ClosureCaptureArg],
) -> Result<Vec<ClosureCaptureValue>, String> {
    let mut values = Vec::with_capacity(captures.len());
    for capture in captures {
        if capture.by_ref {
            let Operand::Local(local) = capture.src else {
                return Err(format!(
                    "E_PHP_VM_BY_REF_CAPTURE_NOT_REFERENCEABLE: closure capture ${} is not a local variable",
                    capture.name
                ));
            };
            let _source = layout_source::enter(layout_source::CLOSURE_CAPTURE_BINDING);
            let cell = stack
                .current_mut()
                .ok_or("no active frame")?
                .locals
                .ensure_reference_cell(local)?;
            values.push(ClosureCaptureValue::by_reference(
                capture.name.clone(),
                cell,
            ));
            continue;
        }
        values.push(ClosureCaptureValue::by_value(
            capture.name.clone(),
            read_operand(unit, stack, capture.src)?,
        ));
    }
    Ok(values)
}

pub(super) fn initialize_captures(
    function: &IrFunction,
    captures: Vec<ClosureCaptureValue>,
    stack: &mut CallStack,
) -> Result<(), String> {
    if function.captures.is_empty() {
        return Ok(());
    }
    for metadata in &function.captures {
        let capture = captures
            .iter()
            .find(|capture| capture.name == metadata.name)
            .cloned();
        let locals = &mut stack.current_mut().expect("frame was pushed").locals;
        if metadata.by_ref {
            let Some(cell) = capture.and_then(|capture| capture.reference()) else {
                return Err(format!(
                    "E_PHP_VM_BY_REF_CAPTURE_MISSING_CELL: closure capture ${} has no reference cell",
                    metadata.name
                ));
            };
            locals.bind_reference_cell(metadata.local, cell)?;
        } else {
            let value = capture
                .and_then(|capture| capture.value().cloned())
                .unwrap_or(Value::Null);
            locals.set(metadata.local, value)?;
        }
    }
    Ok(())
}

pub(super) fn initialize_this(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    function_id: FunctionId,
    function: &IrFunction,
    this_value: ObjectRef,
    stack: &mut CallStack,
) -> Result<(), String> {
    let Some(index) = function.locals.iter().position(|name| name == "this") else {
        let source = compiled
            .unit()
            .files
            .first()
            .map(|file| file.path.as_str())
            .unwrap_or("<unknown>");
        let context = stack
            .current()
            .map(|frame| {
                let declaring_class = frame.declaring_class.as_deref();
                let dynamic_owner_index = declaring_class
                    .and_then(|class_name| dynamic_class_owner_index_in_state(state, class_name));
                let dynamic_owner_source = dynamic_owner_index
                    .and_then(|unit_index| state.dynamic_units.get(unit_index))
                    .and_then(|unit| unit.unit().files.first())
                    .map(|file| file.path.as_str())
                    .unwrap_or("<none>");
                let compiled_has_declaring_class = declaring_class
                    .and_then(|class_name| compiled.lookup_class(class_name))
                    .is_some();
                let dynamic_has_declaring_class = declaring_class
                    .and_then(|class_name| dynamic_class_entry_in_state(state, class_name))
                    .is_some();
                format!(
                    " source={source} function_id={} this_class={} scope_class={} called_class={} declaring_class={} call_span={:?} compiled_has_declaring_class={} dynamic_has_declaring_class={} dynamic_owner_index={} dynamic_owner_source={}",
                    function_id.raw(),
                    this_value.display_name(),
                    frame.scope_class.as_deref().unwrap_or("<none>"),
                    frame.called_class.as_deref().unwrap_or("<none>"),
                    frame.declaring_class.as_deref().unwrap_or("<none>"),
                    frame.call_span,
                    compiled_has_declaring_class,
                    dynamic_has_declaring_class,
                    dynamic_owner_index
                        .map(|index| index.to_string())
                        .unwrap_or_else(|| "<none>".to_owned()),
                    dynamic_owner_source
                )
            })
            .unwrap_or_else(|| {
                format!(
                    " source={source} function_id={} this_class={} scope_class=<no-frame> called_class=<no-frame> declaring_class=<no-frame> call_span=<no-frame>",
                    function_id.raw(),
                    this_value.display_name(),
                )
            });
        return Err(format!(
            "E_PHP_VM_MISSING_THIS_LOCAL: method {} has no $this local{}",
            function.name, context
        ));
    };
    Ok(stack
        .current_mut()
        .expect("frame was pushed")
        .locals
        .set(LocalId::new(index as u32), Value::Object(this_value))?)
}
