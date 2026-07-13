use super::prelude::*;

impl Vm {
    pub(super) fn call_error_handling_builtin(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let values = match call_args_to_positional(name, args) {
            Ok(args) => args,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        match name {
            "error_log" => {
                if values.is_empty() || values.len() > 4 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_ERROR_ARITY: error_log expects one to four arguments",
                    );
                }
                let message = match to_string(&values[0]) {
                    Ok(message) => message.to_string_lossy(),
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                let message_type = match values.get(1) {
                    Some(value) => match to_int(value) {
                        Ok(value) => value,
                        Err(message) => {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    },
                    None => 0,
                };
                match message_type {
                    0 | 4 => VmResult::success_no_output(Some(Value::Bool(true))),
                    3 => {
                        let Some(destination) = values.get(2) else {
                            return VmResult::success_no_output(Some(Value::Bool(false)));
                        };
                        let destination = match to_string(destination) {
                            Ok(destination) => destination.to_string_lossy(),
                            Err(message) => {
                                return self.runtime_error(output, compiled, stack, message);
                            }
                        };
                        if destination.is_empty() {
                            return VmResult::success_no_output(Some(Value::Bool(false)));
                        }
                        match std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&destination)
                            .and_then(|mut file| {
                                use std::io::Write;
                                file.write_all(message.as_bytes())
                            }) {
                            Ok(()) => VmResult::success_no_output(Some(Value::Bool(true))),
                            Err(_) => VmResult::success_no_output(Some(Value::Bool(false))),
                        }
                    }
                    1 => VmResult::success_no_output(Some(Value::Bool(false))),
                    _ => VmResult::success_no_output(Some(Value::Bool(false))),
                }
            }
            "error_reporting" => {
                if values.len() > 1 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_ERROR_ARITY: error_reporting expects zero or one argument",
                    );
                }
                let previous = current_error_reporting(state);
                if let Some(value) = values.first() {
                    let next = match to_int(value) {
                        Ok(value) => value,
                        Err(message) => {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    };
                    let _ = state.ini.set("error_reporting", next.to_string());
                }
                VmResult::success_no_output(Some(Value::Int(previous)))
            }
            "set_error_handler" => {
                if !(1..=2).contains(&values.len()) {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_ERROR_ARITY: set_error_handler expects one or two arguments",
                    );
                }
                let callback = match error_handler_callback_from_value(compiled, values[0].clone())
                {
                    Ok(callback) => callback,
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                let levels = match values.get(1) {
                    Some(value) => match to_int(value) {
                        Ok(value) => value,
                        Err(message) => {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    },
                    None => php_std::constants::E_ALL,
                };
                let previous = state
                    .error_handlers
                    .last()
                    .map(|entry| Value::Callable(Box::new(entry.callback.clone())))
                    .unwrap_or(Value::Null);
                state
                    .error_handlers
                    .push(ErrorHandlerEntry { callback, levels });
                VmResult::success_no_output(Some(previous))
            }
            "get_error_handler" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_ERROR_ARITY: get_error_handler expects no arguments",
                    );
                }
                let handler = state
                    .error_handlers
                    .last()
                    .map(|entry| Value::Callable(Box::new(entry.callback.clone())))
                    .unwrap_or(Value::Null);
                VmResult::success_no_output(Some(handler))
            }
            "restore_error_handler" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_ERROR_ARITY: restore_error_handler expects no arguments",
                    );
                }
                let _ = state.error_handlers.pop();
                VmResult::success_no_output(Some(Value::Bool(true)))
            }
            "error_get_last" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_ERROR_ARITY: error_get_last expects no arguments",
                    );
                }
                VmResult::success_no_output(Some(Self::error_get_last_value(state)))
            }
            "set_exception_handler" => {
                if values.len() != 1 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_ERROR_ARITY: set_exception_handler expects one argument",
                    );
                }
                let callback = match error_handler_callback_from_value(compiled, values[0].clone())
                {
                    Ok(callback) => callback,
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                let previous = state
                    .exception_handlers
                    .last()
                    .map(|callback| Value::Callable(Box::new(callback.clone())))
                    .unwrap_or(Value::Null);
                state.exception_handlers.push(callback);
                VmResult::success_no_output(Some(previous))
            }
            "get_exception_handler" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_ERROR_ARITY: get_exception_handler expects no arguments",
                    );
                }
                let handler = state
                    .exception_handlers
                    .last()
                    .map(|callback| Value::Callable(Box::new(callback.clone())))
                    .unwrap_or(Value::Null);
                VmResult::success_no_output(Some(handler))
            }
            "restore_exception_handler" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_ERROR_ARITY: restore_exception_handler expects no arguments",
                    );
                }
                let _ = state.exception_handlers.pop();
                VmResult::success_no_output(Some(Value::Bool(true)))
            }
            "register_shutdown_function" => {
                if values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_ERROR_ARITY: register_shutdown_function expects at least one argument",
                    );
                }
                let callback =
                    match acquire_callable_value(compiled, state, stack, values[0].clone()) {
                        Ok(callback) => callback,
                        Err(message) => {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    };
                let args = values
                    .into_iter()
                    .skip(1)
                    .map(CallArgument::positional)
                    .collect();
                state
                    .shutdown_functions
                    .push(ShutdownFunctionEntry { callback, args });
                VmResult::success_no_output(Some(Value::Null))
            }
            "trigger_error" | "user_error" => {
                self.call_trigger_error_builtin(compiled, name, values, output, stack, state)
            }
            _ => self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNKNOWN_ERROR_BUILTIN: {name}"),
            ),
        }
    }

    pub(super) fn call_trigger_error_builtin(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        if !(1..=2).contains(&values.len()) {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_ERROR_ARITY: {name} expects one or two arguments"),
            );
        }
        let message = match to_string(&values[0]) {
            Ok(message) => message.to_string_lossy(),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        let level = match values.get(1) {
            Some(value) => match to_int(value) {
                Ok(value) => value,
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            },
            None => php_std::constants::E_USER_NOTICE,
        };
        if !is_supported_user_error_level(level) {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_VALUE_ERROR: {name}(): invalid error level {level}"),
            );
        }

        let diagnostic = RuntimeDiagnostic::new(
            error_level_diagnostic_id(level),
            error_level_severity(level),
            message.clone(),
            RuntimeSourceSpan::default(),
            stack_trace(compiled, stack),
            Some(if level == php_std::constants::E_USER_ERROR {
                php_runtime::api::PhpReferenceClassification::FatalError
            } else {
                php_runtime::api::PhpReferenceClassification::Warning
            }),
        );
        let handled = if level == php_std::constants::E_USER_ERROR {
            false
        } else {
            match self.dispatch_error_handler(compiled, output, stack, state, level, &diagnostic) {
                Ok(handled) => handled,
                Err(result) => return *result,
            }
        };
        if handled {
            return VmResult::success_no_output(Some(Value::Bool(true)));
        }

        let reported = error_reporting_allows(state, level);
        if reported {
            Self::record_last_error(state, level, &diagnostic);
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                error_level_channel(level),
                level,
            );
        }
        if level == php_std::constants::E_USER_ERROR {
            return VmResult::runtime_error_with_diagnostic(
                output.clone(),
                format!("E_PHP_VM_USER_ERROR: {message}"),
                diagnostic,
            );
        }

        let diagnostics = if reported {
            vec![diagnostic]
        } else {
            Vec::new()
        };
        VmResult::success_with_diagnostics_no_output(Some(Value::Bool(true)), diagnostics)
    }

    pub(super) fn dispatch_error_handler(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        level: i64,
        diagnostic: &RuntimeDiagnostic,
    ) -> Result<bool, Box<VmResult>> {
        let Some(handler) = state
            .error_handlers
            .last()
            .cloned()
            .filter(|entry| entry.levels == -1 || (entry.levels & level) != 0)
        else {
            return Ok(false);
        };
        let callback = handler.callback;
        let span = diagnostic.source_span();
        let args = trim_error_handler_args(
            compiled,
            state,
            &callback,
            vec![
                Value::Int(level),
                Value::string(diagnostic.message()),
                Value::string(span.file.clone().unwrap_or_default()),
                Value::Int(span.start as i64),
            ],
        )
        .into_iter()
        .map(CallArgument::positional)
        .collect();
        let result = self.call_callable(
            compiled,
            Value::Callable(Box::new(callback)),
            args,
            output,
            stack,
            state,
        );
        if !result.status.is_success() {
            return Err(Box::new(result));
        }
        Ok(!matches!(
            result.return_value.as_ref(),
            Some(Value::Bool(false))
        ))
    }

    pub(super) fn record_last_error(
        state: &mut ExecutionState,
        level: i64,
        diagnostic: &RuntimeDiagnostic,
    ) {
        let span = diagnostic.source_span();
        state.last_error = Some(LastErrorEntry {
            level,
            message: diagnostic.message().to_owned(),
            file: span.file.clone().unwrap_or_default(),
            line: span.start as i64,
        });
    }

    pub(super) fn error_get_last_value(state: &ExecutionState) -> Value {
        let Some(last_error) = &state.last_error else {
            return Value::Null;
        };
        let mut array = PhpArray::new();
        array.insert(string_key("type"), Value::Int(last_error.level));
        array.insert(
            string_key("message"),
            Value::string(last_error.message.clone()),
        );
        array.insert(string_key("file"), Value::string(last_error.file.clone()));
        array.insert(string_key("line"), Value::Int(last_error.line));
        Value::Array(array)
    }

    pub(super) fn emit_undefined_property_warning(
        &self,
        cursor: ExecutionCursor<'_>,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
        class_name: &str,
        property: &str,
        span: php_ir::IrSpan,
    ) -> Result<(), Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_VM_UNDEFINED_PROPERTY",
            RuntimeSeverity::Warning,
            format!("Undefined property: {class_name}::${property}"),
            runtime_source_span(compiled, span),
            stack_trace(compiled, stack),
            None,
        );
        let handled = self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::api::PHP_E_WARNING,
            &diagnostic,
        )?;
        if !handled && error_reporting_allows(state, php_runtime::api::PHP_E_WARNING) {
            Self::record_last_error(state, php_runtime::api::PHP_E_WARNING, &diagnostic);
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::api::PhpDiagnosticChannel::Warning,
                php_runtime::api::PHP_E_WARNING,
            );
            diagnostics.push(diagnostic);
        }
        Ok(())
    }

    pub(super) fn emit_non_object_property_read_warning(
        &self,
        cursor: ExecutionCursor<'_>,
        diagnostics: &mut Vec<RuntimeDiagnostic>,
        receiver_type: &str,
        property: &str,
        span: php_ir::IrSpan,
    ) -> Result<(), Box<VmResult>> {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_VM_PROPERTY_FETCH_NON_OBJECT",
            RuntimeSeverity::Warning,
            format!("Attempt to read property \"{property}\" on {receiver_type}"),
            runtime_source_span(compiled, span),
            stack_trace(compiled, stack),
            None,
        );
        let handled = self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::api::PHP_E_WARNING,
            &diagnostic,
        )?;
        if !handled && error_reporting_allows(state, php_runtime::api::PHP_E_WARNING) {
            Self::record_last_error(state, php_runtime::api::PHP_E_WARNING, &diagnostic);
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::api::PhpDiagnosticChannel::Warning,
                php_runtime::api::PHP_E_WARNING,
            );
            diagnostics.push(diagnostic);
        }
        Ok(())
    }

    pub(super) fn call_output_buffering_builtin(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
    ) -> VmResult {
        let _profile = self.request_profile_operation_start(
            RequestProfileOperationCategory::Output,
            "output_buffer_builtin",
        );
        let values = match call_args_to_positional(name, args) {
            Ok(args) => args,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        match name {
            "ob_start" => {
                if values.len() > 1 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_OUTPUT_BUFFER_ARITY: ob_start expects zero or one argument in standard-library",
                    );
                }
                if let Some(callback) = values.first()
                    && !matches!(callback, Value::Null)
                {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_OUTPUT_BUFFER_CALLBACK_UNSUPPORTED: ob_start callbacks are not implemented in standard-library",
                    );
                }
                output.start_buffer();
                VmResult::success_no_output(Some(Value::Bool(true)))
            }
            "ob_get_contents" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_OUTPUT_BUFFER_ARITY: ob_get_contents expects no arguments",
                    );
                }
                let value = output
                    .current_buffer_bytes()
                    .map(|bytes| Value::string(bytes.to_vec()))
                    .unwrap_or(Value::Bool(false));
                VmResult::success_no_output(Some(value))
            }
            "ob_get_length" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_OUTPUT_BUFFER_ARITY: ob_get_length expects no arguments",
                    );
                }
                let value = output
                    .current_buffer_len()
                    .map(|length| Value::Int(length as i64))
                    .unwrap_or(Value::Bool(false));
                VmResult::success_no_output(Some(value))
            }
            "ob_get_level" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_OUTPUT_BUFFER_ARITY: ob_get_level expects no arguments",
                    );
                }
                VmResult::success_no_output(Some(Value::Int(output.buffer_level() as i64)))
            }
            "ob_get_clean" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_OUTPUT_BUFFER_ARITY: ob_get_clean expects no arguments",
                    );
                }
                let value = output
                    .pop_buffer_clean()
                    .map(Value::string)
                    .unwrap_or(Value::Bool(false));
                VmResult::success_no_output(Some(value))
            }
            "ob_get_flush" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_OUTPUT_BUFFER_ARITY: ob_get_flush expects no arguments",
                    );
                }
                let value = output
                    .current_buffer_bytes()
                    .map(|bytes| Value::string(bytes.to_vec()))
                    .unwrap_or(Value::Bool(false));
                if !matches!(value, Value::Bool(false)) {
                    output.pop_buffer_flush();
                }
                VmResult::success_no_output(Some(value))
            }
            "ob_end_clean" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_OUTPUT_BUFFER_ARITY: ob_end_clean expects no arguments",
                    );
                }
                VmResult::success_no_output(Some(Value::Bool(output.pop_buffer_clean().is_some())))
            }
            "ob_end_flush" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_OUTPUT_BUFFER_ARITY: ob_end_flush expects no arguments",
                    );
                }
                VmResult::success_no_output(Some(Value::Bool(output.pop_buffer_flush().is_some())))
            }
            "flush" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_OUTPUT_BUFFER_ARITY: flush expects no arguments",
                    );
                }
                output.flush_active_buffers_to_root();
                VmResult::success_no_output(Some(Value::Null))
            }
            _ => self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNKNOWN_OUTPUT_BUFFER_BUILTIN: {name}"),
            ),
        }
    }
}
