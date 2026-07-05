//! VM-mediated callbacks used by internal builtins.

use super::prelude::*;

impl Vm {
    pub(super) fn execute_curl_exec_with_callbacks(
        &self,
        entry: BuiltinEntry,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
        call_span: Option<php_ir::IrSpan>,
    ) -> VmResult {
        let handle = values
            .first()
            .and_then(|value| match effective_value(value) {
                Value::Object(object) => Some(object),
                _ => None,
            });
        let mut result = execute_builtin_entry(
            entry,
            values,
            output,
            &self.options.runtime_context,
            state,
            builtin_source_span(compiled, call_span),
        );
        if !result.status.is_success() {
            return result;
        }
        let Some(handle) = handle else {
            return result;
        };
        let original_return_value = result.return_value.clone();
        let mut diagnostics = std::mem::take(&mut result.diagnostics);
        if let Some(result) = self.call_curl_response_callback(
            compiled,
            &handle,
            "__curl_headerfunction",
            "__curl_last_response_headers",
            output,
            stack,
            state,
            &mut diagnostics,
        ) && !result.status.is_success()
        {
            return result;
        }
        if let Some(result) = self.call_curl_response_callback(
            compiled,
            &handle,
            "__curl_writefunction",
            "__curl_last_response_body",
            output,
            stack,
            state,
            &mut diagnostics,
        ) && !result.status.is_success()
        {
            return result;
        }
        VmResult::success_with_diagnostics_no_output(original_return_value, diagnostics)
    }

    fn call_curl_response_callback(
        &self,
        compiled: &CompiledUnit,
        handle: &ObjectRef,
        callback_property: &str,
        payload_property: &str,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
    ) -> Option<VmResult> {
        let callback = handle.get_property(callback_property)?;
        if !curl_callback_is_enabled(&callback) {
            return None;
        }
        let payload = handle
            .get_property(payload_property)
            .unwrap_or_else(|| Value::string(""));
        let callback_result = self.call_callable_with_by_ref_value_warnings(
            compiled,
            callback,
            vec![
                CallArgument::positional(Value::Object(handle.clone())),
                CallArgument::positional(payload),
            ],
            output,
            stack,
            state,
        );
        diagnostics.extend(callback_result.diagnostics.clone());
        Some(callback_result)
    }

    pub(super) fn prepare_var_dump_values(
        &self,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> Result<Vec<Value>, VmResult> {
        values
            .into_iter()
            .map(|value| self.prepare_var_dump_value(value, output, stack, state, compiled))
            .collect()
    }

    fn prepare_var_dump_value(
        &self,
        value: Value,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> Result<Value, VmResult> {
        match value {
            Value::Object(object) => self
                .debug_info_object_value(&object, output, stack, state, compiled)
                .map(|debug_value| debug_value.unwrap_or(Value::Object(object))),
            Value::Reference(cell) => {
                let Value::Object(object) = cell.get() else {
                    return Ok(Value::Reference(cell));
                };
                self.debug_info_object_value(&object, output, stack, state, compiled)
                    .map(|debug_value| {
                        debug_value
                            .map(|value| Value::Reference(ReferenceCell::new(value)))
                            .unwrap_or(Value::Reference(cell))
                    })
            }
            value => Ok(value),
        }
    }

    fn debug_info_object_value(
        &self,
        object: &ObjectRef,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> Result<Option<Value>, VmResult> {
        let Some(return_value) =
            self.call_debug_info_method(compiled, object, output, stack, state)?
        else {
            return Ok(None);
        };
        let Value::Array(properties) = return_value else {
            return Err(self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_DEBUGINFO_RETURN_TYPE: {}::__debugInfo() must return an array",
                    object.display_name()
                ),
            ));
        };
        Ok(Some(Value::Object(debug_info_object(object, properties))))
    }

    fn call_debug_info_method(
        &self,
        compiled: &CompiledUnit,
        object: &ObjectRef,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Option<Value>, VmResult> {
        let Some(_class) = lookup_class_in_state(compiled, state, &object.class_name()) else {
            return Ok(None);
        };
        let resolved = match lookup_resolved_method_in_state(
            compiled,
            state,
            &object.class_name(),
            "__debugInfo",
            None,
        ) {
            Ok(Some(method)) => method,
            Ok(None) => return Ok(None),
            Err(message) => return Err(self.runtime_error(output, compiled, stack, message)),
        };
        if resolved.method.flags.is_static
            || resolved.method.flags.is_private
            || resolved.method.flags.is_protected
        {
            return Ok(None);
        }
        let guard = MagicMethodCall {
            receiver: format!("object:{}", object.id()),
            magic_method: normalize_method_name("__debugInfo"),
            called_method: normalize_method_name("var_dump"),
        };
        if state
            .magic_method_stack
            .iter()
            .any(|active| active == &guard)
        {
            return Err(self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_MAGIC_METHOD_RECURSION: recursive __debugInfo for {}::var_dump",
                    object.class_name()
                ),
            ));
        }
        state.magic_method_stack.push(guard);
        let class_owner = class_owner_in_state(compiled, state, &resolved.class.name);
        let result = self.execute_function(
            &class_owner,
            resolved.method.function,
            FunctionCall::new(Vec::new(), Vec::new())
                .with_call_site_strict_types(compiled.unit().strict_types)
                .with_this(object.clone())
                .with_class_context(
                    resolved.class.name.clone(),
                    object.class_name(),
                    resolved.class.name.clone(),
                ),
            output,
            stack,
            state,
        );
        let _ = state.magic_method_stack.pop();
        if !result.status.is_success() {
            return Err(result);
        }
        Ok(Some(result.return_value.unwrap_or(Value::Null)))
    }

    pub(super) fn try_execute_iterator_function(
        &self,
        name: &str,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> Option<VmResult> {
        match name {
            "iterator_apply" => {
                Some(self.execute_iterator_apply(values, output, stack, state, compiled))
            }
            "iterator_count" => {
                Some(self.execute_iterator_count(values, call_span, output, stack, state, compiled))
            }
            "iterator_to_array" => Some(
                self.execute_iterator_to_array(values, call_span, output, stack, state, compiled),
            ),
            _ => None,
        }
    }

    fn execute_iterator_apply(
        &self,
        values: &[Value],
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> VmResult {
        if !(2..=3).contains(&values.len()) {
            let comparator = if values.len() < 2 {
                "at least"
            } else {
                "at most"
            };
            let expected = if values.len() < 2 { 2 } else { 3 };
            let message = format!(
                "iterator_apply() expects {comparator} {expected} arguments, {} given",
                values.len()
            );
            let diagnostic = RuntimeDiagnostic::new(
                "E_PHP_RUNTIME_BUILTIN_ARITY",
                RuntimeSeverity::FatalError,
                message.clone(),
                RuntimeSourceSpan::default(),
                stack_trace(compiled, stack),
                Some(php_runtime::PhpReferenceClassification::Error),
            );
            return VmResult::runtime_error_with_diagnostic(output.clone(), message, diagnostic);
        }
        let callback = values[1].clone();
        let callback_args = match values.get(2).map(effective_value) {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(array)) => call_args_from_php_array(&array),
            Some(other) => {
                let message = format!(
                    "iterator_apply(): Argument #3 ($args) must be of type ?array, {} given",
                    value_type_name(&other)
                );
                let diagnostic = RuntimeDiagnostic::new(
                    "E_PHP_RUNTIME_BUILTIN_TYPE",
                    RuntimeSeverity::FatalError,
                    message.clone(),
                    RuntimeSourceSpan::default(),
                    stack_trace(compiled, stack),
                    Some(php_runtime::PhpReferenceClassification::TypeError),
                );
                return VmResult::runtime_error_with_diagnostic(
                    output.clone(),
                    message,
                    diagnostic,
                );
            }
        };
        if let Err(error) = validate_array_callback_arg(
            compiled,
            state,
            "iterator_apply",
            2,
            "callback",
            false,
            &callback,
        ) {
            return match error {
                ArrayCallbackError::Runtime(result) => *result,
                ArrayCallbackError::BuiltinType { function, actual } => {
                    array_callback_type_error(output, compiled, stack, function, &actual)
                }
                ArrayCallbackError::Message(message) => {
                    self.runtime_error(output, compiled, stack, message)
                }
            };
        }
        let source = effective_value(&values[0]);
        match iterator_function_accepts_source(compiled, state, &source) {
            Ok(true) => {}
            Ok(false) => {
                return self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_RUNTIME_BUILTIN_TYPE: iterator_count(): Argument #1 ($iterator) must be of type Traversable|array, {} given",
                        type_error_value_name(&source)
                    ),
                );
            }
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        }
        let mut iterator = match self.foreach_iterator_from_value(
            compiled,
            source,
            output,
            stack,
            state,
            ForeachInvalidSourceBehavior::Unsupported,
        ) {
            Ok(iterator) => {
                let mut iterators = HashMap::new();
                iterators.insert(RegId::new(0), iterator);
                iterators
            }
            Err(result) => return result,
        };
        let mut count = 0_i64;
        loop {
            match self.next_foreach_value(
                compiled,
                output,
                stack,
                state,
                &mut iterator,
                RegId::new(0),
                false,
            ) {
                Ok(Some(_)) => {
                    let callback_result = self.call_callable_with_by_ref_value_warnings(
                        compiled,
                        callback.clone(),
                        callback_args.clone(),
                        output,
                        stack,
                        state,
                    );
                    if !callback_result.status.is_success() {
                        return callback_result;
                    }
                    count += 1;
                    let should_continue = callback_result
                        .return_value
                        .as_ref()
                        .is_some_and(|value| to_bool(value).unwrap_or(false));
                    if !should_continue {
                        return VmResult::success_no_output(Some(Value::Int(count)));
                    }
                }
                Ok(None) => return VmResult::success_no_output(Some(Value::Int(count))),
                Err(result) => return result,
            }
        }
    }

    fn execute_iterator_count(
        &self,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> VmResult {
        if values.len() != 1 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_ITERATOR_COUNT_ARITY: iterator_count expects exactly 1 argument, {} given",
                    values.len()
                ),
            );
        }
        let source = effective_value(&values[0]);
        if let Err(result) = self.validate_iterator_function_iterable_arg(
            "iterator_count",
            values,
            source.clone(),
            call_span,
            output,
            stack,
            state,
            compiled,
        ) {
            return result;
        }
        let mut iterator = match self.foreach_iterator_from_value(
            compiled,
            source,
            output,
            stack,
            state,
            ForeachInvalidSourceBehavior::Unsupported,
        ) {
            Ok(iterator) => {
                let mut iterators = HashMap::new();
                iterators.insert(RegId::new(0), iterator);
                iterators
            }
            Err(result) => return result,
        };
        let mut count = 0_i64;
        loop {
            match self.next_foreach_value(
                compiled,
                output,
                stack,
                state,
                &mut iterator,
                RegId::new(0),
                false,
            ) {
                Ok(Some(_)) => count += 1,
                Ok(None) => return VmResult::success_no_output(Some(Value::Int(count))),
                Err(result) => return result,
            }
        }
    }

    fn execute_iterator_to_array(
        &self,
        values: &[Value],
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> VmResult {
        if !(1..=2).contains(&values.len()) {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_ITERATOR_TO_ARRAY_ARITY: iterator_to_array expects 1 or 2 arguments, {} given",
                    values.len()
                ),
            );
        }
        if let Err(result) = self.validate_iterator_function_iterable_arg(
            "iterator_to_array",
            values,
            effective_value(&values[0]),
            call_span,
            output,
            stack,
            state,
            compiled,
        ) {
            return result;
        }
        let preserve_keys = match values.get(1) {
            Some(value) => match to_bool(value) {
                Ok(value) => value,
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            },
            None => true,
        };
        let source = effective_value(&values[0]);
        match iterator_function_accepts_source(compiled, state, &source) {
            Ok(true) => {}
            Ok(false) => {
                return self.runtime_error(
                    output,
                    compiled,
                    stack,
                    format!(
                        "E_PHP_RUNTIME_BUILTIN_TYPE: iterator_to_array(): Argument #1 ($iterator) must be of type Traversable|array, {} given",
                        type_error_value_name(&source)
                    ),
                );
            }
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        }
        let mut iterator = match self.foreach_iterator_from_value(
            compiled,
            source,
            output,
            stack,
            state,
            ForeachInvalidSourceBehavior::Unsupported,
        ) {
            Ok(iterator) => {
                let mut iterators = HashMap::new();
                iterators.insert(RegId::new(0), iterator);
                iterators
            }
            Err(result) => return result,
        };
        let mut result = PhpArray::new();
        loop {
            match self.next_foreach_value(
                compiled,
                output,
                stack,
                state,
                &mut iterator,
                RegId::new(0),
                true,
            ) {
                Ok(Some((key, value))) => {
                    if preserve_keys {
                        let Some(key) = key else {
                            result.append(value);
                            continue;
                        };
                        let key = match array_key_from_value(&key) {
                            Ok(key) => key,
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        result.insert(key, value);
                    } else {
                        result.append(value);
                    }
                }
                Ok(None) => return VmResult::success_no_output(Some(Value::Array(result))),
                Err(result) => return result,
            }
        }
    }

    fn validate_iterator_function_iterable_arg(
        &self,
        function: &str,
        values: &[Value],
        value: Value,
        call_span: Option<php_ir::IrSpan>,
        output: &OutputBuffer,
        stack: &CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> Result<(), VmResult> {
        if iterator_function_accepts_iterable(compiled, state, &value)
            .map_err(|message| self.runtime_error(output, compiled, stack, message))?
        {
            return Ok(());
        }
        let message = format!(
            "{function}(): Argument #1 ($iterator) must be of type Traversable|array, {} given",
            type_error_value_name(&value)
        );
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            RuntimeSeverity::FatalError,
            message.clone(),
            RuntimeSourceSpan::default(),
            stack_trace(compiled, stack),
            Some(php_runtime::PhpReferenceClassification::TypeError),
        );
        if let Some(call_span) = call_span {
            let result = VmResult::runtime_error_with_diagnostic(
                output.clone(),
                message.clone(),
                diagnostic.clone(),
            );
            if let Some(throwable) = runtime_error_throwable(&result) {
                tag_throwable_location(&throwable, compiled, call_span);
                state.pending_trace = Some(capture_backtrace_string_with_builtin_failed_call(
                    compiled, stack, function, values, call_span,
                ));
                state.pending_throw = Some(throwable);
                return Err(result);
            }
        }
        Err(VmResult::runtime_error_with_diagnostic(
            output.clone(),
            message,
            diagnostic,
        ))
    }
}
