use super::prelude::*;

impl Vm {
    pub(super) fn call_pcre_callback_builtin(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let values = match call_builtin_args_to_positional(
            self, compiled, name, args, None, output, stack, state,
        ) {
            Ok(values) => values,
            Err(InternalBuiltinArgError::Message(message)) => {
                return self.runtime_error(output, compiled, stack, message);
            }
            Err(InternalBuiltinArgError::Fatal(result)) => return *result,
        };
        let trace_values = values.clone();
        let result = match name {
            "preg_replace_callback" => self.call_preg_replace_callback_builtin(
                compiled, values, call_span, output, stack, state,
            ),
            "preg_replace_callback_array" => self.call_preg_replace_callback_array_builtin(
                compiled, values, call_span, output, stack, state,
            ),
            _ => Err(ArrayCallbackError::Message(format!(
                "E_PHP_VM_UNKNOWN_PCRE_CALLBACK_BUILTIN: {name}"
            ))),
        };
        match result {
            Ok(value) => VmResult::success_no_output(Some(value)),
            Err(ArrayCallbackError::Runtime(result)) => *result,
            Err(ArrayCallbackError::BuiltinType { function, actual }) => {
                array_callback_type_error(output, compiled, stack, function, &actual)
            }
            Err(ArrayCallbackError::Message(message)) => match builtin_type_error_message(&message)
            {
                Some(message) => builtin_type_error_result_with_failed_call(
                    output,
                    compiled,
                    stack,
                    state,
                    name,
                    &trace_values,
                    call_span,
                    message.to_owned(),
                ),
                None => self.runtime_error(output, compiled, stack, message),
            },
        }
    }

    pub(super) fn emit_pcre_compile_warning(
        &self,
        compiled: &CompiledUnit,
        function_name: &str,
        call_span: Option<php_ir::IrSpan>,
        error: &php_runtime::pcre::PcreFailure,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), ArrayCallbackError> {
        state.builtins.pcre_state_mut().last_error_mut().set(
            error.code(),
            php_runtime::pcre::preg_error_message(error.code()),
        );
        if error.code() != php_runtime::pcre::PREG_INTERNAL_ERROR {
            return Ok(());
        }

        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_RUNTIME_PCRE_WARNING",
            RuntimeSeverity::Warning,
            format!("{function_name}(): {}", error.message()),
            builtin_source_span(compiled, call_span),
            stack_trace(compiled, stack),
            Some(php_runtime::PhpReferenceClassification::Warning),
        );
        let handled = self
            .dispatch_error_handler(
                compiled,
                output,
                stack,
                state,
                php_runtime::PHP_E_WARNING,
                &diagnostic,
            )
            .map_err(|result| ArrayCallbackError::Runtime(Box::new(result)))?;
        if !handled && error_reporting_allows(state, php_runtime::PHP_E_WARNING) {
            Self::record_last_error(state, php_runtime::PHP_E_WARNING, &diagnostic);
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::PhpDiagnosticChannel::Warning,
                php_runtime::PHP_E_WARNING,
            );
        }
        Ok(())
    }

    pub(super) fn pcre_callback_string_value(
        &self,
        compiled: &CompiledUnit,
        value: &Value,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<PhpString, ArrayCallbackError> {
        self.value_to_string_with_source_span(
            compiled,
            value,
            output,
            stack,
            state,
            builtin_source_span(compiled, call_span),
        )
        .map_err(|result| ArrayCallbackError::Runtime(Box::new(result)))
    }

    pub(super) fn pcre_callback_subject_string_value(
        &self,
        compiled: &CompiledUnit,
        function: &str,
        position: usize,
        param_name: &str,
        value: &Value,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<PhpString, ArrayCallbackError> {
        match effective_value(value) {
            Value::Object(object) => Err(ArrayCallbackError::Message(format!(
                "E_PHP_RUNTIME_BUILTIN_TYPE: {function}(): Argument #{position} (${param_name}) must be of type array|string, {} given",
                object.display_name()
            ))),
            Value::Fiber(_) | Value::Generator(_) => Err(ArrayCallbackError::Message(format!(
                "E_PHP_RUNTIME_BUILTIN_TYPE: {function}(): Argument #{position} (${param_name}) must be of type array|string, object given"
            ))),
            _ => self.pcre_callback_string_value(compiled, value, call_span, output, stack, state),
        }
    }

    pub(super) fn call_preg_replace_callback_builtin(
        &self,
        compiled: &CompiledUnit,
        args: Vec<Value>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, ArrayCallbackError> {
        if !(3..=6).contains(&args.len()) {
            return Err(ArrayCallbackError::Message(
                "E_PHP_VM_BUILTIN_ARITY: preg_replace_callback expects three to six argument(s)"
                    .to_owned(),
            ));
        }
        self.validate_pcre_callback_subject_arg("preg_replace_callback", 3, "subject", &args[2])?;
        let patterns = match effective_value(&args[0]) {
            Value::Array(array) => {
                let mut patterns = Vec::new();
                for (_, value) in array.iter() {
                    match self.pcre_callback_string_value(
                        compiled, value, call_span, output, stack, state,
                    ) {
                        Ok(pattern) => patterns.push(pattern),
                        Err(error) => {
                            self.pcre_callback_coerce_array_subject_for_pattern_error(
                                compiled, &args[2], call_span, output, stack, state,
                            )?;
                            return Err(error);
                        }
                    }
                }
                patterns
            }
            _ => match self
                .pcre_callback_string_value(compiled, &args[0], call_span, output, stack, state)
            {
                Ok(pattern) => vec![pattern],
                Err(error) => {
                    self.pcre_callback_coerce_array_subject_for_pattern_error(
                        compiled, &args[2], call_span, output, stack, state,
                    )?;
                    return Err(error);
                }
            },
        };
        let callback = args[1].clone();
        validate_array_callback_arg(
            compiled,
            state,
            "preg_replace_callback",
            2,
            "callback",
            false,
            &callback,
        )?;
        let limit = args
            .get(3)
            .map(to_int)
            .transpose()
            .map_err(|message| {
                ArrayCallbackError::Message(format!("preg_replace_callback: {message}"))
            })?
            .unwrap_or(-1);
        let flags = args
            .get(5)
            .map(to_int)
            .transpose()
            .map_err(|message| {
                ArrayCallbackError::Message(format!("preg_replace_callback: {message}"))
            })?
            .unwrap_or(0);
        let mut cache = php_runtime::pcre::PcreCache::default();
        let mut compiled_patterns = Vec::new();
        for pattern in patterns {
            let compiled_pattern = match cache.compile(&pattern) {
                Ok(compiled_pattern) => compiled_pattern,
                Err(error) => {
                    self.emit_pcre_compile_warning(
                        compiled,
                        "preg_replace_callback",
                        call_span,
                        &error,
                        output,
                        stack,
                        state,
                    )?;
                    return Ok(Value::Null);
                }
            };
            compiled_patterns.push((compiled_pattern, callback.clone()));
        }
        let mut count = 0i64;
        let value = match effective_value(&args[2]) {
            Value::Array(array) => {
                let mut replaced = PhpArray::new();
                let entries = array
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect::<Vec<_>>();
                for (index, (key, value)) in entries.iter().enumerate() {
                    let text = self.pcre_callback_string_value(
                        compiled, value, call_span, output, stack, state,
                    )?;
                    let bytes = match self.preg_replace_callback_array_bytes(
                        compiled,
                        &compiled_patterns,
                        text.as_bytes(),
                        call_span,
                        limit,
                        flags,
                        &mut count,
                        output,
                        stack,
                        state,
                    ) {
                        Ok(bytes) => bytes,
                        Err(error) => {
                            self.pcre_callback_emit_remaining_array_subject_warnings(
                                compiled,
                                &entries[index + 1..],
                                call_span,
                                output,
                                stack,
                                state,
                            )?;
                            return Err(error);
                        }
                    };
                    replaced.insert(key.clone(), Value::string(bytes));
                }
                Value::Array(replaced)
            }
            subject => {
                let text = self.pcre_callback_subject_string_value(
                    compiled,
                    "preg_replace_callback",
                    3,
                    "subject",
                    &subject,
                    call_span,
                    output,
                    stack,
                    state,
                )?;
                let bytes = self.preg_replace_callback_array_bytes(
                    compiled,
                    &compiled_patterns,
                    text.as_bytes(),
                    call_span,
                    limit,
                    flags,
                    &mut count,
                    output,
                    stack,
                    state,
                )?;
                Value::string(bytes)
            }
        };
        if let Some(Value::Reference(cell)) = args.get(4) {
            cell.set(Value::Int(count));
        }
        Ok(value)
    }

    pub(super) fn validate_pcre_callback_subject_arg(
        &self,
        function: &str,
        position: usize,
        param_name: &str,
        value: &Value,
    ) -> Result<(), ArrayCallbackError> {
        match effective_value(value) {
            Value::Object(object) => Err(ArrayCallbackError::Message(format!(
                "E_PHP_RUNTIME_BUILTIN_TYPE: {function}(): Argument #{position} (${param_name}) must be of type array|string, {} given",
                object.display_name()
            ))),
            Value::Fiber(_) | Value::Generator(_) => Err(ArrayCallbackError::Message(format!(
                "E_PHP_RUNTIME_BUILTIN_TYPE: {function}(): Argument #{position} (${param_name}) must be of type array|string, object given"
            ))),
            _ => Ok(()),
        }
    }

    pub(super) fn pcre_callback_coerce_array_subject_for_pattern_error(
        &self,
        compiled: &CompiledUnit,
        value: &Value,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), ArrayCallbackError> {
        let Value::Array(subjects) = effective_value(value) else {
            return Ok(());
        };
        for (_, subject) in subjects.iter() {
            self.pcre_callback_string_value(compiled, subject, call_span, output, stack, state)?;
        }
        Ok(())
    }

    pub(super) fn pcre_callback_emit_remaining_array_subject_warnings(
        &self,
        compiled: &CompiledUnit,
        entries: &[(ArrayKey, Value)],
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), ArrayCallbackError> {
        for (_, value) in entries {
            if matches!(effective_value(value), Value::Array(_)) {
                self.pcre_callback_string_value(compiled, value, call_span, output, stack, state)?;
            }
        }
        Ok(())
    }

    pub(super) fn call_preg_replace_callback_array_builtin(
        &self,
        compiled: &CompiledUnit,
        args: Vec<Value>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, ArrayCallbackError> {
        if !(2..=6).contains(&args.len()) {
            return Err(ArrayCallbackError::Message(
                "E_PHP_VM_BUILTIN_ARITY: preg_replace_callback_array expects two to six argument(s)"
                    .to_owned(),
            ));
        }
        let Value::Array(patterns) = callable_resolve_reference(args[0].clone()) else {
            return Err(ArrayCallbackError::Message(format!(
                "E_PHP_VM_BUILTIN_TYPE: preg_replace_callback_array expects array, {} given",
                value_type_name(&args[0])
            )));
        };
        let limit = args
            .get(2)
            .map(to_int)
            .transpose()
            .map_err(|message| {
                ArrayCallbackError::Message(format!("preg_replace_callback_array: {message}"))
            })?
            .unwrap_or(-1);
        let flags = args
            .get(4)
            .map(to_int)
            .transpose()
            .map_err(|message| {
                ArrayCallbackError::Message(format!("preg_replace_callback_array: {message}"))
            })?
            .unwrap_or(0);
        let mut cache = php_runtime::pcre::PcreCache::default();
        let mut count = 0i64;
        let mut value = args[1].clone();
        for (key, callback) in patterns.iter() {
            let ArrayKey::String(pattern) = key else {
                return Err(ArrayCallbackError::Message(
                    "E_PHP_RUNTIME_BUILTIN_TYPE: preg_replace_callback_array(): Argument #1 ($pattern) must contain only string patterns as keys"
                        .to_owned(),
                ));
            };
            validate_array_callback_arg(
                compiled,
                state,
                "preg_replace_callback_array",
                1,
                "pattern",
                false,
                callback,
            )
            .map_err(|_| {
                ArrayCallbackError::Message(
                    "E_PHP_RUNTIME_BUILTIN_TYPE: preg_replace_callback_array(): Argument #1 ($pattern) must contain only valid callbacks"
                        .to_owned(),
                )
            })?;
            let compiled_pattern = match cache.compile(&pattern) {
                Ok(compiled_pattern) => compiled_pattern,
                Err(error) => {
                    self.emit_pcre_compile_warning(
                        compiled,
                        "preg_replace_callback_array",
                        call_span,
                        &error,
                        output,
                        stack,
                        state,
                    )?;
                    return Ok(Value::Null);
                }
            };
            value = self.preg_replace_callback_array_subject_value(
                compiled,
                &[(compiled_pattern, callback.clone())],
                value,
                call_span,
                limit,
                flags,
                &mut count,
                output,
                stack,
                state,
            )?;
        }
        if let Some(Value::Reference(cell)) = args.get(3) {
            cell.set(Value::Int(count));
        }
        Ok(value)
    }

    pub(super) fn preg_replace_callback_array_subject_bytes(
        &self,
        compiled: &CompiledUnit,
        patterns: &[(std::sync::Arc<php_runtime::pcre::CompiledPattern>, Value)],
        subjects: PhpArray,
        call_span: Option<php_ir::IrSpan>,
        limit: i64,
        flags: i64,
        count: &mut i64,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<PhpArray, ArrayCallbackError> {
        let mut replaced = subjects;
        for (pattern, callback) in patterns {
            let entries = replaced
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<Vec<_>>();
            for (key, subject) in entries {
                let text = self.pcre_callback_string_value(
                    compiled, &subject, call_span, output, stack, state,
                )?;
                let bytes = self.preg_replace_callback_array_bytes(
                    compiled,
                    &[(pattern.clone(), callback.clone())],
                    text.as_bytes(),
                    call_span,
                    limit,
                    flags,
                    count,
                    output,
                    stack,
                    state,
                )?;
                replaced.insert(key, Value::string(bytes));
            }
        }
        Ok(replaced)
    }

    pub(super) fn preg_replace_callback_array_subject_value(
        &self,
        compiled: &CompiledUnit,
        patterns: &[(std::sync::Arc<php_runtime::pcre::CompiledPattern>, Value)],
        subject: Value,
        call_span: Option<php_ir::IrSpan>,
        limit: i64,
        flags: i64,
        count: &mut i64,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Value, ArrayCallbackError> {
        match effective_value(&subject) {
            Value::Array(subjects) => Ok(Value::Array(
                self.preg_replace_callback_array_subject_bytes(
                    compiled, patterns, subjects, call_span, limit, flags, count, output, stack,
                    state,
                )?,
            )),
            subject => {
                let text = self.pcre_callback_subject_string_value(
                    compiled,
                    "preg_replace_callback_array",
                    2,
                    "subject",
                    &subject,
                    call_span,
                    output,
                    stack,
                    state,
                )?;
                let bytes = self.preg_replace_callback_array_bytes(
                    compiled,
                    patterns,
                    text.as_bytes(),
                    call_span,
                    limit,
                    flags,
                    count,
                    output,
                    stack,
                    state,
                )?;
                Ok(Value::string(bytes))
            }
        }
    }

    pub(super) fn preg_replace_callback_array_bytes(
        &self,
        compiled: &CompiledUnit,
        patterns: &[(std::sync::Arc<php_runtime::pcre::CompiledPattern>, Value)],
        subject: &[u8],
        call_span: Option<php_ir::IrSpan>,
        limit: i64,
        flags: i64,
        count: &mut i64,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Vec<u8>, ArrayCallbackError> {
        let mut current = subject.to_vec();
        for (pattern, callback) in patterns {
            current = self.preg_replace_callback_bytes(
                compiled,
                pattern,
                callback.clone(),
                &current,
                call_span,
                limit,
                flags,
                count,
                output,
                stack,
                state,
            )?;
        }
        Ok(current)
    }

    pub(super) fn preg_replace_callback_bytes(
        &self,
        compiled: &CompiledUnit,
        pattern: &php_runtime::pcre::CompiledPattern,
        callback: Value,
        subject: &[u8],
        call_span: Option<php_ir::IrSpan>,
        limit: i64,
        flags: i64,
        count: &mut i64,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Vec<u8>, ArrayCallbackError> {
        let mut replaced = Vec::new();
        let mut last_end = 0usize;
        for captures in pattern.captures_iter(subject) {
            let captures = captures.map_err(|error| {
                let error = php_runtime::pcre::PcreFailure::from(error);
                ArrayCallbackError::Message(format!(
                    "E_PHP_RUNTIME_PCRE_ERROR: {}",
                    error.message()
                ))
            })?;
            let Some(full) = captures.get(0) else {
                continue;
            };
            if limit >= 0 && *count >= limit {
                break;
            }
            replaced.extend_from_slice(&subject[last_end..full.start()]);
            let callback_result = self.invoke_array_callback(
                compiled,
                callback.clone(),
                vec![php_runtime::pcre::captures_to_array_with_names(
                    &captures,
                    pattern.capture_names(),
                    flags,
                    0,
                )],
                output,
                stack,
                state,
            )?;
            let callback_text = self.pcre_callback_string_value(
                compiled,
                &callback_result,
                call_span,
                output,
                stack,
                state,
            )?;
            replaced.extend_from_slice(callback_text.as_bytes());
            last_end = full.end();
            *count += 1;
        }
        replaced.extend_from_slice(&subject[last_end..]);
        Ok(replaced)
    }
}
