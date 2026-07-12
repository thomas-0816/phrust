use super::builtin_adapter::builtin_source_span;
use super::prelude::*;

impl Vm {
    pub(super) fn call_spl_multiple_iterator_attach_method(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        method: &str,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
    ) -> Result<Value, VmResult> {
        let method = normalize_method_name(method);
        let max_args = if method == "offsetset" { 2 } else { 3 };
        validate_spl_iterator_arg_count(&object.class_name(), &args, 1, max_args)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        let iterator = args[0].value.clone();
        let info = args
            .get(1)
            .map(|arg| arg.value.clone())
            .unwrap_or(Value::Null);
        if method == "offsetset" && args.len() != 2 {
            return Err(self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_RUNTIME_BUILTIN_ARITY: {} expects exactly 2 arguments, {} given",
                    object.display_name(),
                    args.len()
                ),
            ));
        }
        self.spl_multiple_iterator_attach(
            compiled,
            object,
            iterator,
            info,
            "MultipleIterator::attachIterator(): Argument #1 ($iterator) must be of type Iterator",
            output,
            stack,
        )?;
        Ok(Value::Null)
    }

    pub(super) fn spl_multiple_iterator_offset_set(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        iterator: Value,
        info: Value,
        output: &mut OutputBuffer,
        stack: &CallStack,
    ) -> Result<(), VmResult> {
        self.spl_multiple_iterator_attach(
            compiled,
            object,
            iterator,
            info,
            "Can only attach objects that implement the Iterator interface",
            output,
            stack,
        )
    }

    pub(super) fn spl_multiple_iterator_attach(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        iterator: Value,
        info: Value,
        type_error_prefix: &str,
        output: &mut OutputBuffer,
        stack: &CallStack,
    ) -> Result<(), VmResult> {
        let Value::Object(iterator_object) = effective_value(&iterator) else {
            let message = spl_multiple_iterator_type_error(type_error_prefix, &iterator);
            return Err(self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_SPL_TYPE_ERROR: {message}"),
            ));
        };
        let is_iterator = object_instanceof(
            compiled,
            &Value::Object(iterator_object.clone()),
            "Iterator",
        )
        .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        if !is_iterator {
            let message = spl_multiple_iterator_type_error(
                type_error_prefix,
                &Value::Object(iterator_object.clone()),
            );
            return Err(self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_SPL_TYPE_ERROR: {message}"),
            ));
        }
        spl_multiple_iterator_attach_validated(object, iterator_object, info)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))
    }

    pub(super) fn call_spl_append_iterator_method(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        method: &str,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        call_span: Option<IrSpan>,
    ) -> Result<Value, VmResult> {
        let method = normalize_method_name(method);
        match method.as_str() {
            "append" => {
                validate_spl_iterator_arg_count(&object.class_name(), &args, 1, 1).map_err(
                    |message| {
                        self.runtime_error_at_optional_span(
                            compiled, output, stack, state, call_span, message,
                        )
                    },
                )?;
                if !spl_bool_property(object, "__append_initialized") {
                    return Err(self.runtime_error_at_optional_span(
                        compiled,
                        output,
                        stack,
                        state,
                        call_span,
                        "E_PHP_VM_SPL_ERROR: The object is in an invalid state as the parent constructor was not called"
                            .to_string(),
                    ));
                }
                let Value::Object(iterator) = effective_value(&args[0].value) else {
                    return Err(self.runtime_error_at_optional_span(
                        compiled,
                        output,
                        stack,
                        state,
                        call_span,
                        format!(
                            "E_PHP_VM_SPL_TYPE_ERROR: AppendIterator::append(): Argument #1 ($iterator) must be of type Iterator, {} given",
                            type_error_value_name(&args[0].value)
                        ),
                    ));
                };
                let is_iterator =
                    object_instanceof(compiled, &Value::Object(iterator.clone()), "Iterator")
                        .map_err(|message| {
                            self.runtime_error_at_optional_span(
                                compiled, output, stack, state, call_span, message,
                            )
                        })?;
                if !is_iterator {
                    return Err(self.runtime_error_at_optional_span(
                        compiled,
                        output,
                        stack,
                        state,
                        call_span,
                        format!(
                            "E_PHP_VM_SPL_TYPE_ERROR: AppendIterator::append(): Argument #1 ($iterator) must be of type Iterator, {} given",
                            type_error_value_name(&Value::Object(iterator.clone()))
                        ),
                    ));
                }
                let iterator_id = iterator.id() as i64;
                if !spl_append_rewound_iterator_ids(object).contains(&iterator_id) {
                    self.call_object_method_value(
                        compiled, iterator, "rewind", output, stack, state,
                    )?;
                    spl_append_note_rewound_iterator_id(object, iterator_id);
                }
                call_spl_iterator_method(
                    object.clone(),
                    "append",
                    args,
                    &self.options.runtime_context,
                )
                .map_err(|message| {
                    self.runtime_error_at_optional_span(
                        compiled, output, stack, state, call_span, message,
                    )
                })
            }
            "rewind" => {
                validate_spl_iterator_arg_count(&object.class_name(), &args, 0, 0).map_err(
                    |message| {
                        self.runtime_error_at_optional_span(
                            compiled, output, stack, state, call_span, message,
                        )
                    },
                )?;
                if let Some(iterator) = spl_append_iterators(object).first().cloned() {
                    self.call_object_method_value(
                        compiled, iterator, "rewind", output, stack, state,
                    )?;
                }
                spl_set_position(object, 0);
                Ok(Value::Null)
            }
            "next" => {
                validate_spl_iterator_arg_count(&object.class_name(), &args, 0, 0).map_err(
                    |message| {
                        self.runtime_error_at_optional_span(
                            compiled, output, stack, state, call_span, message,
                        )
                    },
                )?;
                let indices = spl_append_entry_iterator_indices(object);
                let old_position = spl_position(object);
                let old_index = indices.get(old_position).copied();
                let new_position = old_position.saturating_add(1);
                spl_set_position(object, new_position);
                let new_index = indices.get(new_position).copied();
                if new_index.is_some()
                    && new_index != old_index
                    && let Some(iterator) = new_index
                        .and_then(|index| usize::try_from(index).ok())
                        .and_then(|index| spl_append_iterators(object).get(index).cloned())
                {
                    self.call_object_method_value(
                        compiled, iterator, "rewind", output, stack, state,
                    )?;
                }
                Ok(Value::Null)
            }
            _ => unreachable!("caller validates AppendIterator method names"),
        }
    }

    pub(super) fn call_spl_multiple_iterator_method(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        method: &str,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
        let method = normalize_method_name(method);
        validate_spl_iterator_arg_count(&object.class_name(), &args, 0, 0)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        match method.as_str() {
            "rewind" => {
                let records = spl_multiple_iterator_records(object);
                for (iterator, _) in records {
                    self.call_object_method_value(
                        compiled, iterator, "rewind", output, stack, state,
                    )?;
                }
                spl_set_position(object, 0);
                Ok(Value::Null)
            }
            "next" => {
                let records = spl_multiple_iterator_records(object);
                for (iterator, _) in records {
                    self.call_object_method_value(
                        compiled, iterator, "next", output, stack, state,
                    )?;
                }
                spl_set_position(object, spl_position(object).saturating_add(1));
                Ok(Value::Null)
            }
            "valid" => {
                let records = spl_multiple_iterator_records(object);
                if records.is_empty() {
                    return Ok(Value::Bool(false));
                }
                let need_all = spl_multiple_iterator_needs_all(object);
                let mut any_valid = false;
                for (iterator, _) in records {
                    let valid = self.call_object_method_value(
                        compiled, iterator, "valid", output, stack, state,
                    )?;
                    let valid = to_bool(&valid)
                        .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
                    if need_all && !valid {
                        return Ok(Value::Bool(false));
                    }
                    any_valid |= valid;
                }
                Ok(Value::Bool(if need_all { true } else { any_valid }))
            }
            "current" => self.call_spl_multiple_iterator_collect_method(
                compiled, object, "current", output, stack, state,
            ),
            "key" => self.call_spl_multiple_iterator_collect_method(
                compiled, object, "key", output, stack, state,
            ),
            _ => Err(self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_UNKNOWN_METHOD: method {}::{method} is not defined",
                    object.class_name()
                ),
            )),
        }
    }

    pub(super) fn call_spl_multiple_iterator_collect_method(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        method: &str,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
        let records = spl_multiple_iterator_records(object);
        if records.is_empty() {
            return Err(self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_SPL_RUNTIME_EXCEPTION: Called {method}() on an invalid iterator"),
            ));
        }
        let use_assoc = spl_multiple_iterator_uses_assoc_keys(object);
        let need_all = spl_multiple_iterator_needs_all(object);
        let mut values = PhpArray::new();
        let mut any_valid = false;
        for (index, (iterator, info)) in records.into_iter().enumerate() {
            let valid = self.call_object_method_value(
                compiled,
                iterator.clone(),
                "valid",
                output,
                stack,
                state,
            )?;
            let valid = to_bool(&valid)
                .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
            any_valid |= valid;
            if need_all && !valid {
                return Err(self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_VM_SPL_RUNTIME_EXCEPTION: Called {method}() with non valid sub iterator"
                    ),
                ));
            }
            let outer_key = if use_assoc {
                spl_multiple_iterator_info_key(&info)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?
                    .ok_or_else(|| {
                        self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_VM_SPL_INVALID_ARGUMENT: Sub-Iterator is associated with NULL",
                        )
                    })?
            } else {
                ArrayKey::Int(index as i64)
            };
            let value = if valid {
                self.call_object_method_value(compiled, iterator, method, output, stack, state)?
            } else {
                Value::Null
            };
            values.insert(outer_key, value);
        }
        if !any_valid {
            return Err(self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_SPL_RUNTIME_EXCEPTION: Called {method}() with non valid sub iterator"
                ),
            ));
        }
        Ok(Value::Array(values))
    }

    pub(super) fn call_spl_infinite_iterator_method(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        method: &str,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
        let normalized_method = normalize_method_name(method);
        validate_spl_iterator_arg_count(&object.class_name(), &args, 0, 0)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        let Some(inner) = spl_inner_iterator_delegation_target(object) else {
            return call_spl_iterator_method(
                object.clone(),
                method,
                Vec::new(),
                &self.options.runtime_context,
            )
            .map_err(|message| self.runtime_error(output, compiled, stack, message));
        };
        match normalized_method.as_str() {
            "rewind" => {
                self.call_object_method_value(compiled, inner, "rewind", output, stack, state)?;
                Ok(Value::Null)
            }
            "valid" => {
                let valid = self.call_object_method_value(
                    compiled,
                    inner.clone(),
                    "valid",
                    output,
                    stack,
                    state,
                )?;
                if to_bool(&valid)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?
                {
                    return Ok(valid);
                }
                self.call_object_method_value(
                    compiled,
                    inner.clone(),
                    "rewind",
                    output,
                    stack,
                    state,
                )?;
                self.call_object_method_value(compiled, inner, "valid", output, stack, state)
            }
            "current" => {
                self.call_object_method_value(compiled, inner, "current", output, stack, state)
            }
            "key" => self.call_object_method_value(compiled, inner, "key", output, stack, state),
            "next" => {
                self.call_object_method_value(compiled, inner, "next", output, stack, state)?;
                Ok(Value::Null)
            }
            _ => unreachable!("caller validates InfiniteIterator method names"),
        }
    }

    pub(super) fn call_spl_limit_iterator_method(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        method: &str,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
        let normalized_method = normalize_method_name(method);
        let max_args = if normalized_method == "seek" { 1 } else { 0 };
        validate_spl_iterator_arg_count(&object.class_name(), &args, max_args, max_args)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        let Some(inner) = spl_inner_iterator_delegation_target(object) else {
            return call_spl_iterator_method(
                object.clone(),
                method,
                args,
                &self.options.runtime_context,
            )
            .map_err(|message| self.runtime_error(output, compiled, stack, message));
        };
        match normalized_method.as_str() {
            "rewind" => {
                self.call_object_method_value(
                    compiled,
                    inner.clone(),
                    "rewind",
                    output,
                    stack,
                    state,
                )?;
                let offset = spl_limit_offset(object);
                let inner_supports_seek =
                    spl_delegation_target_supports_method(compiled, state, &inner, "seek");
                if offset > 0 && inner_supports_seek {
                    self.call_object_method_value_with_positional_args(
                        compiled,
                        inner,
                        "seek",
                        vec![Value::Int(offset as i64)],
                        output,
                        stack,
                        state,
                    )?;
                } else {
                    for _ in 0..offset {
                        let valid = self.call_object_method_value(
                            compiled,
                            inner.clone(),
                            "valid",
                            output,
                            stack,
                            state,
                        )?;
                        if !to_bool(&valid).map_err(|message| {
                            self.runtime_error(output, compiled, stack, message)
                        })? {
                            break;
                        }
                        self.call_object_method_value(
                            compiled,
                            inner.clone(),
                            "next",
                            output,
                            stack,
                            state,
                        )?;
                    }
                    if offset > 0 || !inner_supports_seek {
                        self.call_object_method_value(
                            compiled, inner, "valid", output, stack, state,
                        )?;
                    }
                }
                spl_set_position(object, 0);
                spl_set_bool_property(object, "__limit_cached_after_seek", false);
                Ok(Value::Null)
            }
            "valid" => {
                if spl_bool_property(object, "__limit_cached_after_seek") {
                    return Ok(Value::Bool(true));
                }
                if let Some(count) = spl_limit_count(object)
                    && spl_position(object) >= count
                {
                    return Ok(Value::Bool(false));
                }
                self.call_object_method_value(compiled, inner, "valid", output, stack, state)
            }
            "current" => {
                if spl_bool_property(object, "__limit_seek_pending_current_check") {
                    spl_set_bool_property(object, "__limit_seek_pending_current_check", false);
                    self.call_object_method_value(
                        compiled,
                        inner.clone(),
                        "valid",
                        output,
                        stack,
                        state,
                    )?;
                    let current = self.call_object_method_value(
                        compiled,
                        inner.clone(),
                        "current",
                        output,
                        stack,
                        state,
                    )?;
                    self.call_object_method_value(compiled, inner, "key", output, stack, state)?;
                    Ok(current)
                } else if spl_bool_property(object, "__limit_cached_after_seek") {
                    Ok(object
                        .get_property("__limit_cached_current")
                        .map(|value| effective_value(&value))
                        .unwrap_or(Value::Null))
                } else {
                    self.call_object_method_value(compiled, inner, "current", output, stack, state)
                }
            }
            "key" => {
                if spl_bool_property(object, "__limit_cached_after_seek") {
                    Ok(object
                        .get_property("__limit_cached_key")
                        .map(|value| effective_value(&value))
                        .unwrap_or(Value::Null))
                } else {
                    self.call_object_method_value(compiled, inner, "key", output, stack, state)
                }
            }
            "next" => {
                spl_set_position(object, spl_position(object).saturating_add(1));
                spl_set_bool_property(object, "__limit_cached_after_seek", false);
                self.call_object_method_value(compiled, inner, "next", output, stack, state)?;
                Ok(Value::Null)
            }
            "seek" => {
                let position = to_int(&args[0].value)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?
                    .max(0) as usize;
                let offset = spl_limit_offset(object);
                if position < offset {
                    return Err(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_VM_SPL_OUT_OF_BOUNDS: Cannot seek to {position} which is below the offset {offset}"
                        ),
                    ));
                }
                if let Some(count) = spl_limit_count(object) {
                    let upper = offset.saturating_add(count);
                    if position >= upper {
                        return Err(self.runtime_error(
                            output,
                            compiled,
                            stack,
                            format!(
                                "E_PHP_VM_SPL_OUT_OF_BOUNDS: Cannot seek to {position} which is behind offset {offset} plus count {count}"
                            ),
                        ));
                    }
                }
                if spl_delegation_target_supports_method(compiled, state, &inner, "seek") {
                    self.call_object_method_value_with_positional_args(
                        compiled,
                        inner,
                        "seek",
                        vec![Value::Int(position as i64)],
                        output,
                        stack,
                        state,
                    )?;
                    spl_set_bool_property(object, "__limit_seek_pending_current_check", true);
                    spl_set_bool_property(object, "__limit_cached_after_seek", false);
                } else {
                    self.call_object_method_value(
                        compiled,
                        inner.clone(),
                        "rewind",
                        output,
                        stack,
                        state,
                    )?;
                    for _ in 0..position {
                        let valid = self.call_object_method_value(
                            compiled,
                            inner.clone(),
                            "valid",
                            output,
                            stack,
                            state,
                        )?;
                        if !to_bool(&valid).map_err(|message| {
                            self.runtime_error(output, compiled, stack, message)
                        })? {
                            break;
                        }
                        self.call_object_method_value(
                            compiled,
                            inner.clone(),
                            "next",
                            output,
                            stack,
                            state,
                        )?;
                    }
                    self.call_object_method_value(
                        compiled,
                        inner.clone(),
                        "valid",
                        output,
                        stack,
                        state,
                    )?;
                    self.call_object_method_value(
                        compiled,
                        inner.clone(),
                        "valid",
                        output,
                        stack,
                        state,
                    )?;
                    let current = self.call_object_method_value(
                        compiled,
                        inner.clone(),
                        "current",
                        output,
                        stack,
                        state,
                    )?;
                    let key = self
                        .call_object_method_value(compiled, inner, "key", output, stack, state)?;
                    object.set_property("__limit_cached_current", current);
                    object.set_property("__limit_cached_key", key);
                    spl_set_bool_property(object, "__limit_cached_after_seek", true);
                    spl_set_bool_property(object, "__limit_seek_pending_current_check", false);
                }
                spl_set_position(object, position - offset);
                Ok(Value::Null)
            }
            "getposition" => Ok(Value::Int(
                spl_limit_offset(object).saturating_add(spl_position(object)) as i64,
            )),
            _ => unreachable!("caller validates LimitIterator method names"),
        }
    }

    pub(super) fn call_spl_caching_iterator_method(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        method: &str,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
        let normalized_method = normalize_method_name(method);
        validate_spl_iterator_arg_count(&object.class_name(), &args, 0, 0)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        let Some(inner) = spl_inner_iterator_delegation_target(object) else {
            return call_spl_iterator_method(
                object.clone(),
                method,
                args,
                &self.options.runtime_context,
            )
            .map_err(|message| self.runtime_error(output, compiled, stack, message));
        };
        match normalized_method.as_str() {
            "rewind" => {
                self.call_object_method_value(compiled, inner, "rewind", output, stack, state)?;
                spl_set_bool_property(object, "__caching_live_initialized", false);
                spl_set_bool_property(object, "__caching_live_valid", false);
                spl_set_bool_property(object, "__caching_live_next_valid", false);
                Ok(Value::Null)
            }
            "valid" => {
                let valid = self.ensure_spl_caching_iterator_live_current(
                    compiled, object, inner, output, stack, state,
                )?;
                Ok(Value::Bool(valid))
            }
            "current" => {
                self.ensure_spl_caching_iterator_live_current(
                    compiled,
                    object,
                    inner.clone(),
                    output,
                    stack,
                    state,
                )?;
                Ok(object
                    .get_property("__caching_live_current")
                    .map(|value| effective_value(&value))
                    .unwrap_or(Value::Null))
            }
            "key" => {
                self.ensure_spl_caching_iterator_live_current(
                    compiled,
                    object,
                    inner.clone(),
                    output,
                    stack,
                    state,
                )?;
                Ok(object
                    .get_property("__caching_live_key")
                    .map(|value| effective_value(&value))
                    .unwrap_or(Value::Null))
            }
            "next" => {
                if !spl_bool_property(object, "__caching_live_initialized") {
                    self.ensure_spl_caching_iterator_live_current(
                        compiled,
                        object,
                        inner.clone(),
                        output,
                        stack,
                        state,
                    )?;
                }
                if spl_bool_property(object, "__caching_live_next_valid") {
                    let current = object
                        .get_property("__caching_live_next_current")
                        .map(|value| effective_value(&value))
                        .unwrap_or(Value::Null);
                    let key = object
                        .get_property("__caching_live_next_key")
                        .map(|value| effective_value(&value))
                        .unwrap_or(Value::Null);
                    object.set_property("__caching_live_current", current);
                    object.set_property("__caching_live_key", key);
                    spl_set_bool_property(object, "__caching_live_valid", true);
                    self.call_object_method_value(
                        compiled,
                        inner.clone(),
                        "next",
                        output,
                        stack,
                        state,
                    )?;
                    self.refresh_spl_caching_iterator_live_next(
                        compiled, object, inner, output, stack, state,
                    )?;
                } else {
                    spl_set_bool_property(object, "__caching_live_valid", false);
                }
                Ok(Value::Null)
            }
            _ => unreachable!("caller validates CachingIterator method names"),
        }
    }

    pub(super) fn ensure_spl_caching_iterator_live_current(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        inner: ObjectRef,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<bool, VmResult> {
        if spl_bool_property(object, "__caching_live_initialized") {
            return Ok(spl_bool_property(object, "__caching_live_valid"));
        }
        spl_set_bool_property(object, "__caching_live_initialized", true);
        let valid =
            self.call_object_method_value(compiled, inner.clone(), "valid", output, stack, state)?;
        if !to_bool(&valid)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?
        {
            spl_set_bool_property(object, "__caching_live_valid", false);
            spl_set_bool_property(object, "__caching_live_next_valid", false);
            return Ok(false);
        }
        let current = self.call_object_method_value(
            compiled,
            inner.clone(),
            "current",
            output,
            stack,
            state,
        )?;
        let key =
            self.call_object_method_value(compiled, inner.clone(), "key", output, stack, state)?;
        object.set_property("__caching_live_current", current);
        object.set_property("__caching_live_key", key);
        spl_set_bool_property(object, "__caching_live_valid", true);
        self.call_object_method_value(compiled, inner.clone(), "next", output, stack, state)?;
        self.refresh_spl_caching_iterator_live_next(compiled, object, inner, output, stack, state)?;
        Ok(true)
    }

    pub(super) fn refresh_spl_caching_iterator_live_next(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        inner: ObjectRef,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        let valid =
            self.call_object_method_value(compiled, inner.clone(), "valid", output, stack, state)?;
        if !to_bool(&valid)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?
        {
            spl_set_bool_property(object, "__caching_live_next_valid", false);
            return Ok(());
        }
        let current = self.call_object_method_value(
            compiled,
            inner.clone(),
            "current",
            output,
            stack,
            state,
        )?;
        let key = self.call_object_method_value(compiled, inner, "key", output, stack, state)?;
        object.set_property("__caching_live_next_current", current);
        object.set_property("__caching_live_next_key", key);
        spl_set_bool_property(object, "__caching_live_next_valid", true);
        Ok(())
    }

    pub(super) fn call_spl_no_rewind_iterator_method(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        method: &str,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, VmResult> {
        let normalized_method = normalize_method_name(method);
        validate_spl_iterator_arg_count(&object.class_name(), &args, 0, 0)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?;
        if normalized_method == "rewind" {
            if spl_bool_property(object, "__norewind_has_cached_current") {
                object.set_property("__norewind_use_cached_current_once", Value::Bool(true));
            }
            return Ok(Value::Null);
        }
        let Some(inner) = spl_inner_iterator_delegation_target(object) else {
            return call_spl_iterator_method(
                object.clone(),
                method,
                Vec::new(),
                &self.options.runtime_context,
            )
            .map_err(|message| self.runtime_error(output, compiled, stack, message));
        };
        match normalized_method.as_str() {
            "valid" => {
                self.call_object_method_value(compiled, inner, "valid", output, stack, state)
            }
            "current" => {
                if spl_bool_property(object, "__norewind_use_cached_current_once")
                    && spl_bool_property(object, "__norewind_has_cached_current")
                {
                    object.set_property("__norewind_use_cached_current_once", Value::Bool(false));
                    return Ok(object
                        .get_property("__norewind_cached_current")
                        .map(|value| effective_value(&value))
                        .unwrap_or(Value::Null));
                }
                let value = self
                    .call_object_method_value(compiled, inner, "current", output, stack, state)?;
                object.set_property("__norewind_cached_current", value.clone());
                object.set_property("__norewind_has_cached_current", Value::Bool(true));
                object.set_property("__norewind_use_cached_current_once", Value::Bool(false));
                Ok(value)
            }
            "key" => self.call_object_method_value(compiled, inner, "key", output, stack, state),
            "next" => {
                object.set_property("__norewind_has_cached_current", Value::Bool(false));
                object.set_property("__norewind_use_cached_current_once", Value::Bool(false));
                self.call_object_method_value(compiled, inner, "next", output, stack, state)?;
                Ok(Value::Null)
            }
            _ => unreachable!("caller validates NoRewindIterator method names"),
        }
    }

    pub(super) fn call_spl_userland_filter_valid(
        &self,
        compiled: &CompiledUnit,
        object: ObjectRef,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<bool, VmResult> {
        while spl_position(&object) < spl_entries(&object).len() {
            object.set_property("__regex_accept_pre_parent", Value::Bool(true));
            state.suppress_array_to_string_warnings =
                state.suppress_array_to_string_warnings.saturating_add(1);
            let accept = self.call_object_method_value(
                compiled,
                object.clone(),
                "accept",
                output,
                stack,
                state,
            );
            state.suppress_array_to_string_warnings =
                state.suppress_array_to_string_warnings.saturating_sub(1);
            object.set_property("__regex_accept_pre_parent", Value::Bool(false));
            let accepted = match accept {
                Ok(value) => to_bool(&value)
                    .map_err(|message| self.runtime_error(output, compiled, stack, message))?,
                Err(result) => return Err(result),
            };
            if accepted {
                return Ok(true);
            }
            spl_set_position(&object, spl_position(&object).saturating_add(1));
        }
        Ok(false)
    }

    pub(super) fn emit_spl_rii_recursive_caching_child_warning_if_needed(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        if !spl_rii_current_enters_recursive_caching_child(object)
            || spl_rii_array_string_warning_was_emitted(object)
        {
            return Ok(());
        }

        self.emit_array_to_string_warning(
            compiled,
            output,
            stack,
            state,
            builtin_source_span(compiled, call_span),
        )?;
        spl_rii_note_array_string_warning(object);
        Ok(())
    }

    pub(super) fn spl_caching_iterator_to_string(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        source_span: RuntimeSourceSpan,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<PhpString, VmResult> {
        let flags = spl_caching_iterator_flags(object);
        let Some((key, value)) = spl_current_entry(object) else {
            return Ok(PhpString::from_bytes(Vec::new()));
        };
        spl_caching_iterator_note_current_seen(object);
        if flags & SPL_CACHING_CALL_TOSTRING != 0 {
            if let Value::Object(object) = effective_value(&value)
                && !self.object_has_string_conversion(compiled, &object, output, stack, state)?
            {
                return Ok(PhpString::from_bytes(Vec::new()));
            }
            return self.value_to_string(compiled, &value, output, stack, state);
        }
        if flags & SPL_CACHING_TOSTRING_USE_KEY != 0 {
            return self.value_to_string(compiled, &array_key_to_value(key), output, stack, state);
        }
        if flags & SPL_CACHING_TOSTRING_USE_CURRENT != 0 {
            return self.value_to_string(compiled, &value, output, stack, state);
        }
        if flags & SPL_CACHING_TOSTRING_USE_INNER != 0 {
            if let Some(Value::Object(inner)) = object
                .get_property("__inner_iterator")
                .map(|value| effective_value(&value))
            {
                let previous_position = inner.get_property("__position");
                spl_set_position(&inner, spl_position(object));
                let result = self.object_to_string(compiled, inner.clone(), output, stack, state);
                match previous_position {
                    Some(value) => inner.set_property("__position", value),
                    None => {
                        inner.unset_property("__position");
                    }
                }
                return result;
            }
            let mut bytes = self
                .value_to_string(compiled, &array_key_to_value(key), output, stack, state)?
                .as_bytes()
                .to_vec();
            bytes.push(b':');
            bytes.extend_from_slice(
                self.value_to_string(compiled, &value, output, stack, state)?
                    .as_bytes(),
            );
            return Ok(PhpString::from_bytes(bytes));
        }
        Err(self.runtime_error_with_source_span(
            output,
            compiled,
            stack,
            source_span,
            "E_PHP_VM_SPL_BAD_METHOD_CALL: CachingIterator does not fetch string value (see CachingIterator::__construct)"
                .to_owned(),
        ))
    }

    pub(super) fn call_spl_caching_iterator_offset_access_method(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        method: &str,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let normalized = normalize_method_name(method);
        if let Err(message) = validate_spl_iterator_arg_count(&object.class_name(), &args, 1, 1) {
            return self.runtime_error(output, compiled, stack, message);
        }
        if let Err(message) =
            spl_caching_iterator_require_full_cache(object, &object.display_name())
        {
            return self.runtime_error(output, compiled, stack, message);
        }

        let source_span = call_span
            .or_else(|| stack.current().and_then(|frame| frame.call_span))
            .map(|span| runtime_source_span(compiled, span))
            .unwrap_or_default();
        let mut diagnostics = Vec::new();
        let (key, key_string) = match self.spl_caching_iterator_offset_key(
            compiled,
            object,
            &normalized,
            &args[0].value,
            source_span.clone(),
            output,
            stack,
            state,
            &mut diagnostics,
        ) {
            Ok(key) => key,
            Err(result) => return result,
        };

        let cache = spl_caching_iterator_cache(object);
        let value = match normalized.as_str() {
            "offsetexists" => Value::Bool(
                cache
                    .get(&key)
                    .is_some_and(|value| !matches!(effective_value(value), Value::Null)),
            ),
            "offsetget" => match cache.get(&key).map(effective_value) {
                Some(value) => value,
                None => {
                    diagnostics.push(undefined_array_string_key_warning(
                        &key_string,
                        source_span.clone(),
                        stack_trace(compiled, stack),
                    ));
                    Value::Null
                }
            },
            _ => unreachable!("caller limits CachingIterator array access methods"),
        };
        for diagnostic in &diagnostics {
            let (level, channel) = match diagnostic.severity() {
                RuntimeSeverity::Deprecation => (
                    php_runtime::api::PHP_E_DEPRECATED,
                    php_runtime::api::PhpDiagnosticChannel::Deprecated,
                ),
                _ => (
                    php_runtime::api::PHP_E_WARNING,
                    php_runtime::api::PhpDiagnosticChannel::Warning,
                ),
            };
            let handled = match self
                .dispatch_error_handler(compiled, output, stack, state, level, diagnostic)
            {
                Ok(handled) => handled,
                Err(result) => return result,
            };
            if !handled && error_reporting_allows(state, level) {
                emit_vm_diagnostic(output, state, diagnostic, channel, level);
            }
        }
        VmResult::success_with_diagnostics_no_output(Some(value), diagnostics)
    }

    pub(super) fn spl_caching_iterator_offset_key(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        method: &str,
        value: &Value,
        source_span: RuntimeSourceSpan,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Result<(ArrayKey, PhpString), VmResult> {
        let string = match effective_value(value) {
            Value::Null => {
                diagnostics.push(RuntimeDiagnostic::new(
                    "E_PHP_VM_SPL_CACHING_OFFSET_NULL_DEPRECATED",
                    RuntimeSeverity::Deprecation,
                    format!(
                        "{}::{}(): Passing null to parameter #1 ($key) of type string is deprecated",
                        spl_caching_iterator_diagnostic_class(object),
                        spl_iterator_display_method(method)
                    ),
                    source_span,
                    stack_trace(compiled, stack),
                    Some(php_runtime::api::PhpReferenceClassification::Deprecation),
                ));
                PhpString::from_bytes(Vec::new())
            }
            Value::Object(key_object) => {
                if !self.object_has_string_conversion(
                    compiled,
                    &key_object,
                    output,
                    stack,
                    state,
                )? {
                    return Err(self.runtime_error(
                        output,
                        compiled,
                        stack,
                        format!(
                            "E_PHP_RUNTIME_BUILTIN_TYPE: {}::{}(): Argument #1 ($key) must be of type string, {} given",
                            spl_caching_iterator_diagnostic_class(object),
                            spl_iterator_display_method(method),
                            key_object.display_name()
                        ),
                    ));
                }
                self.object_to_string(compiled, key_object, output, stack, state)?
            }
            other => to_string(&other)
                .map_err(|message| self.runtime_error(output, compiled, stack, message))?,
        };
        Ok((ArrayKey::from_php_string(string.clone()), string))
    }
}
