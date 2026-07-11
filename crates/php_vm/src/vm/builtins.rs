//! Internal builtin registry dispatch for the VM.

use super::prelude::*;

impl Vm {
    pub(super) fn execute_internal_registry_builtin(
        &self,
        name: &str,
        values: Vec<Value>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> VmResult {
        self.record_counter_internal_function_dispatch();
        // Coarse fallback attribution: every clone inside this dispatch
        // (argument coercion, trace snapshots, the builtin body itself, and
        // callbacks it re-enters) counts as `builtin_body` unless a more
        // specific family is entered deeper in the call.
        let _source = layout_source::enter_default(layout_source::BUILTIN_BODY);
        let Some(entry) = self.lookup_internal_function_dispatch(name) else {
            return unknown_builtin_result(name, output);
        };
        if self.options.inline_caches.enabled() {
            if let Some(result) = self.try_execute_fast_builtin_stub(name, &values, output) {
                return result;
            }
            if let Some(result) = self.try_execute_json_encode_fast(name, &values, state) {
                return result;
            }
            if let Some(result) = self.try_execute_array_slice_fast(name, &values) {
                return result;
            }
            if let Some(result) =
                self.try_execute_preg_match_start_offset_ascii_fast(name, &values, state)
            {
                return result;
            }
        }
        if let Some(result) = self.try_execute_direct_count_array(name, &values, output) {
            return result;
        }
        if let Some(result) =
            self.try_execute_countable_object(name, &values, output, stack, state, compiled)
        {
            return result;
        }
        if let Some(result) = self
            .try_execute_iterator_function(name, &values, call_span, output, stack, state, compiled)
        {
            return result;
        }
        self.record_array_count_fast_path_if_applicable(name, &values);
        let values = match self
            .coerce_internal_builtin_string_args(name, values, compiled, output, stack, state)
        {
            Ok(values) => values,
            Err(result) => return result,
        };
        let values = match self
            .prepare_hash_option_values(name, values, call_span, compiled, output, stack, state)
        {
            Ok(values) => values,
            Err(result) => return result,
        };
        let values = match self
            .prepare_http_build_query_values(name, values, compiled, output, stack, state)
        {
            Ok(values) => values,
            Err(result) => return result,
        };
        let trace_values = values.clone();
        let values = match validate_internal_builtin_args(
            name, values, compiled, call_span, output, state,
        ) {
            Ok(values) => values,
            Err(result) => {
                attach_internal_builtin_throwable(
                    name,
                    &result,
                    &trace_values,
                    call_span,
                    stack,
                    state,
                    compiled,
                );
                if state.pending_throw.is_some() {
                    return VmResult::propagating_exception(output.clone());
                }
                return result;
            }
        };
        let values = if matches!(name, "var_dump" | "print_r") {
            match self.prepare_debug_output_values(name, values, output, stack, state, compiled) {
                Ok(values) => values,
                Err(result) => return result,
            }
        } else {
            values
        };
        if name == "json_encode" {
            let trace_values = values.clone();
            let result = self.execute_json_encode_with_serializable(
                entry, values, output, stack, state, compiled, call_span,
            );
            attach_internal_builtin_throwable(
                name,
                &result,
                &trace_values,
                call_span,
                stack,
                state,
                compiled,
            );
            if state.pending_throw.is_some() {
                return VmResult::propagating_exception(output.clone());
            }
            return result;
        }
        if name == "apcu_entry" {
            return self.execute_apcu_entry_with_callback(
                entry, values, output, stack, state, compiled, call_span,
            );
        }
        if name == "curl_exec" {
            return self.execute_curl_exec_with_callbacks(
                entry, values, output, stack, state, compiled, call_span,
            );
        }
        if name == "shm_put_var" {
            return self.execute_sysvshm_put_var_with_serialization(
                entry, values, output, stack, state, compiled, call_span,
            );
        }
        if name == "msg_send"
            && let Some(result) = self.try_execute_sysvmsg_send_with_serialization(
                &values, output, stack, state, compiled, call_span,
            )
        {
            return result;
        }
        if name == "xml_parse" {
            return self.execute_xml_parse_with_handlers(
                entry, values, output, stack, state, compiled, call_span,
            );
        }
        if name == "opcache_compile_file" {
            return self
                .execute_opcache_compile_file(values, output, stack, state, compiled, call_span);
        }
        if name == "stream_wrapper_register" {
            return self.execute_stream_wrapper_register(values, output, stack, state, compiled);
        }
        if name == "stream_get_wrappers" {
            return self.execute_stream_get_wrappers_with_user_wrappers(values, state);
        }
        if name == "fopen"
            && let Some(result) = self.try_execute_user_stream_fopen(
                values.clone(),
                call_span,
                output,
                stack,
                state,
                compiled,
            )
        {
            return result;
        }
        if name == "fclose"
            && let Some(result) = self
                .try_execute_user_stream_fclose(&values, call_span, output, stack, state, compiled)
        {
            return result;
        }
        let trace_values = values.clone();
        let output_len_before = output.total_len();
        let result = execute_builtin_entry(
            entry,
            values,
            output,
            &self.options.runtime_context,
            state,
            builtin_source_span(compiled, call_span),
        );
        let output_changed = output.total_len() != output_len_before;
        attach_internal_builtin_throwable(
            name,
            &result,
            &trace_values,
            call_span,
            stack,
            state,
            compiled,
        );
        if state.pending_throw.is_some() {
            return VmResult::propagating_exception(output.clone());
        }
        self.route_no_output_internal_builtin_diagnostics(
            result,
            output_changed,
            output,
            stack,
            state,
            compiled,
        )
    }

    fn execute_apcu_entry_with_callback(
        &self,
        entry: BuiltinEntry,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
        call_span: Option<php_ir::IrSpan>,
    ) -> VmResult {
        if values.len() < 2 || values.len() > 3 {
            return execute_builtin_entry(
                entry,
                values,
                output,
                &self.options.runtime_context,
                state,
                builtin_source_span(compiled, call_span),
            );
        }
        let key = match to_string(&values[0]) {
            Ok(key) => key,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        if let Some(value) = state.builtins.apcu_state.fetch(key.as_bytes()) {
            return VmResult::success_no_output(Some(value));
        }
        let ttl = match values.get(2) {
            Some(value) => match to_int(value) {
                Ok(ttl) => ttl,
                Err(message) => return self.runtime_error(output, compiled, stack, message),
            },
            None => 0,
        };
        let result = self.call_callable_with_call_span(
            compiled,
            values[1].clone(),
            vec![CallArgument::positional(Value::String(key.clone()))],
            call_span,
            output,
            stack,
            state,
        );
        if !result.status.is_success() {
            return result;
        }
        let value = result.return_value.unwrap_or(Value::Null);
        state
            .builtins
            .apcu_state
            .store(key.as_bytes().to_vec(), value.clone(), ttl);
        VmResult::success_no_output(Some(value))
    }

    fn try_execute_sysvmsg_send_with_serialization(
        &self,
        values: &[Value],
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
        call_span: Option<php_ir::IrSpan>,
    ) -> Option<VmResult> {
        if values.len() < 3 {
            return None;
        }
        let serialize = match values.get(3) {
            Some(value) => php_runtime::api::to_bool(value).ok()?,
            None => true,
        };
        if !serialize {
            return None;
        }
        let Value::Object(queue) = effective_value(&values[0]) else {
            return None;
        };
        if normalize_class_name(&queue.class_name()) != "sysvmessagequeue" {
            return None;
        }
        let queue_id = state
            .builtins
            .sysvmsg_state
            .queue_id_for_object(queue.id())?;
        let message_type = php_runtime::api::to_int(&values[1]).ok()?;
        if message_type <= 0 {
            return None;
        }
        let blocking = match values.get(4) {
            Some(value) => php_runtime::api::to_bool(value).ok()?,
            None => true,
        };

        let trace_values = values.to_vec();
        let payload = match self.serialize_value_with_magic(
            compiled,
            values[2].clone(),
            call_span,
            output,
            stack,
            state,
        ) {
            Ok(payload) => payload.as_bytes().to_vec(),
            Err(result) => {
                attach_internal_builtin_throwable(
                    "msg_send",
                    &result,
                    &trace_values,
                    call_span,
                    stack,
                    state,
                    compiled,
                );
                if state.pending_throw.is_some() {
                    return Some(VmResult::propagating_exception(output.clone()));
                }
                return Some(result);
            }
        };

        if state.builtins.sysvmsg_state.queue(queue_id).is_none() {
            if let Err(result) = self
                .emit_sysvmsg_send_invalid_queue_warning(compiled, output, stack, state, call_span)
            {
                return Some(result);
            }
            assign_optional_reference(values.get(5), Value::Int(php_runtime::api::SYSVMSG_EINVAL));
            return Some(VmResult::success(
                OutputBuffer::new(),
                Some(Value::Bool(false)),
            ));
        }

        let send_flags = if blocking {
            0
        } else {
            php_runtime::api::SYSVMSG_IPC_NOWAIT
        };
        let sent = state.builtins.sysvmsg_state.send_payload(
            queue_id,
            message_type,
            payload,
            true,
            send_flags,
        );
        assign_optional_reference(values.get(5), Value::Int(sent.err().map_or(0, i64::from)));
        Some(VmResult::success(
            OutputBuffer::new(),
            Some(Value::Bool(sent.is_ok())),
        ))
    }

    fn execute_opcache_compile_file(
        &self,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
        call_span: Option<php_ir::IrSpan>,
    ) -> VmResult {
        if values.len() != 1 {
            return self.runtime_error(
                output,
                compiled,
                stack,
                "E_PHP_VM_OPCACHE_ARITY: opcache_compile_file expects one argument",
            );
        }

        let path = match to_string(&values[0]) {
            Ok(path) => path.to_string_lossy(),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        let Some(loader) = &self.options.include_loader else {
            if let Err(result) = self.emit_opcache_compile_warning(
                compiled,
                output,
                stack,
                state,
                call_span,
                "opcache_compile_file(): include loader is not configured",
            ) {
                return result;
            }
            return VmResult::success_no_output(Some(Value::Bool(false)));
        };

        let include_path = state_include_path(state);
        let cwd = state.cwd.clone();
        let resolved = match if let Some(cache) = &self.options.include_cache {
            cache.resolve_with_include_path(loader, None, &path, &include_path, Some(&cwd))
        } else {
            loader.resolve_with_include_path(None, &path, &include_path, Some(&cwd))
        } {
            Ok(resolved) => resolved,
            Err(message) => {
                if let Err(result) = self.emit_opcache_compile_warning(
                    compiled,
                    output,
                    stack,
                    state,
                    call_span,
                    format!("opcache_compile_file(): {}", message.render_message()),
                ) {
                    return result;
                }
                return VmResult::success_no_output(Some(Value::Bool(false)));
            }
        };

        let compile_result = self
            .options
            .include_compiler
            .as_deref()
            .ok_or_else(|| {
                include_vm_error(
                    "E_PHP_VM_INCLUDE_COMPILER_UNAVAILABLE",
                    "include compiler is not configured",
                )
            })
            .and_then(|compiler| {
                if let Some(cache) = &self.options.include_cache {
                    cache
                        .get_or_compile_include(loader, &resolved, compiler)
                        .map(|_| ())
                } else {
                    loader
                        .load_validated_resolved(&resolved)
                        .and_then(|source| compiler.compile_include(source, loader))
                        .map(|_| ())
                }
            });
        if let Err(message) = compile_result {
            if let Err(result) = self.emit_opcache_compile_warning(
                compiled,
                output,
                stack,
                state,
                call_span,
                format!("opcache_compile_file(): {}", message.render_message()),
            ) {
                return result;
            }
            return VmResult::success_no_output(Some(Value::Bool(false)));
        }

        state
            .builtins
            .opcache_state
            .compile_script(resolved.canonical_path.to_string_lossy().into_owned());
        VmResult::success_no_output(Some(Value::Bool(true)))
    }

    fn emit_opcache_compile_warning(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        call_span: Option<php_ir::IrSpan>,
        message: impl Into<String>,
    ) -> Result<(), VmResult> {
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_VM_OPCACHE_COMPILE_FILE",
            RuntimeSeverity::Warning,
            message,
            builtin_source_span(compiled, call_span),
            stack_trace(compiled, stack),
            Some(php_runtime::PhpReferenceClassification::Warning),
        );
        let handled = self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::PHP_E_WARNING,
            &diagnostic,
        )?;
        if !handled && error_reporting_allows(state, php_runtime::PHP_E_WARNING) {
            Self::record_last_error(state, php_runtime::PHP_E_WARNING, &diagnostic);
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::PhpDiagnosticChannel::Warning,
                php_runtime::PHP_E_WARNING,
            );
            state.diagnostics.push(diagnostic);
        }
        Ok(())
    }

    fn emit_sysvmsg_send_invalid_queue_warning(
        &self,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        call_span: Option<php_ir::IrSpan>,
    ) -> Result<(), VmResult> {
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_RUNTIME_SYSVMSG_SEND",
            RuntimeSeverity::Warning,
            "msg_send(): msgsnd failed: Invalid argument",
            builtin_source_span(compiled, call_span),
            stack_trace(compiled, stack),
            Some(php_runtime::PhpReferenceClassification::Warning),
        );
        let handled = self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::PHP_E_WARNING,
            &diagnostic,
        )?;
        if !handled && error_reporting_allows(state, php_runtime::PHP_E_WARNING) {
            Self::record_last_error(state, php_runtime::PHP_E_WARNING, &diagnostic);
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::PhpDiagnosticChannel::Warning,
                php_runtime::PHP_E_WARNING,
            );
            state.diagnostics.push(diagnostic);
        }
        Ok(())
    }

    fn execute_sysvshm_put_var_with_serialization(
        &self,
        entry: BuiltinEntry,
        values: Vec<Value>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
        call_span: Option<php_ir::IrSpan>,
    ) -> VmResult {
        let trace_values = values.clone();
        if values.len() == 3 {
            let object_id = match effective_value(&values[0]) {
                Value::Object(object) => Some(object.id()),
                _ => None,
            };
            if let Err(result) = self.serialize_value_with_magic(
                compiled,
                values[2].clone(),
                call_span,
                output,
                stack,
                state,
            ) {
                attach_internal_builtin_throwable(
                    "shm_put_var",
                    &result,
                    &trace_values,
                    call_span,
                    stack,
                    state,
                    compiled,
                );
                if state.pending_throw.is_some() {
                    return VmResult::propagating_exception(output.clone());
                }
                return result;
            }
            if object_id.is_some_and(|id| state.builtins.sysvshm_state.object_destroyed(id)) {
                let result = self.runtime_error(
                    output,
                    compiled,
                    stack,
                    "E_PHP_RUNTIME_SYSVSHM_INVALID: Shared memory block has been destroyed by the serialization function",
                );
                attach_internal_builtin_throwable(
                    "shm_put_var",
                    &result,
                    &trace_values,
                    call_span,
                    stack,
                    state,
                    compiled,
                );
                if state.pending_throw.is_some() {
                    return VmResult::propagating_exception(output.clone());
                }
                return result;
            }
        }

        let output_len_before = output.total_len();
        let result = execute_builtin_entry(
            entry,
            values,
            output,
            &self.options.runtime_context,
            state,
            builtin_source_span(compiled, call_span),
        );
        let output_changed = output.total_len() != output_len_before;
        attach_internal_builtin_throwable(
            "shm_put_var",
            &result,
            &trace_values,
            call_span,
            stack,
            state,
            compiled,
        );
        if state.pending_throw.is_some() {
            return VmResult::propagating_exception(output.clone());
        }
        self.route_no_output_internal_builtin_diagnostics(
            result,
            output_changed,
            output,
            stack,
            state,
            compiled,
        )
    }

    fn route_no_output_internal_builtin_diagnostics(
        &self,
        mut result: VmResult,
        output_changed: bool,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> VmResult {
        if !result.status.is_success()
            || output_changed
            || !result.output.is_empty()
            || result.diagnostics.is_empty()
        {
            return result;
        }

        let mut diagnostics = Vec::new();
        for diagnostic in std::mem::take(&mut result.diagnostics) {
            let (level, channel) = match diagnostic.severity() {
                RuntimeSeverity::Warning => (
                    php_runtime::PHP_E_WARNING,
                    php_runtime::PhpDiagnosticChannel::Warning,
                ),
                RuntimeSeverity::Deprecation => (
                    php_runtime::PHP_E_DEPRECATED,
                    php_runtime::PhpDiagnosticChannel::Deprecated,
                ),
                _ => {
                    diagnostics.push(diagnostic);
                    continue;
                }
            };
            let handled = match self.dispatch_error_handler(
                compiled,
                output,
                stack,
                state,
                level,
                &diagnostic,
            ) {
                Ok(handled) => handled,
                Err(result) => return result,
            };
            if handled {
                continue;
            }
            if error_reporting_allows(state, level) {
                Self::record_last_error(state, level, &diagnostic);
                emit_vm_diagnostic(output, state, &diagnostic, channel, level);
                diagnostics.push(diagnostic);
            }
        }
        result.diagnostics = diagnostics;
        result
    }

    fn prepare_hash_option_values(
        &self,
        name: &str,
        mut values: Vec<Value>,
        call_span: Option<php_ir::IrSpan>,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Vec<Value>, VmResult> {
        if !matches!(name, "hash" | "hash_file" | "hash_init") {
            return Ok(values);
        }
        let Some(Value::Array(mut options)) = values.get(3).map(effective_value) else {
            return Ok(values);
        };
        let secret_key = ArrayKey::String(PhpString::from("secret"));
        let Some(secret) = options.get(&secret_key).cloned() else {
            return Ok(values);
        };
        if !value_needs_vm_string_coercion_in_state(compiled, state, &secret) {
            return Ok(values);
        }

        self.emit_hash_secret_type_deprecation(name, call_span, compiled, output, stack, state)?;
        let secret = self.value_to_string_with_source_span(
            compiled,
            &secret,
            output,
            stack,
            state,
            builtin_source_span(compiled, call_span),
        )?;
        options.insert(secret_key, Value::String(secret));
        values[3] = Value::Array(options);
        Ok(values)
    }

    fn emit_hash_secret_type_deprecation(
        &self,
        name: &str,
        call_span: Option<php_ir::IrSpan>,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<(), VmResult> {
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_HASH_SECRET_TYPE_DEPRECATED",
            RuntimeSeverity::Deprecation,
            format!(
                "{name}(): Passing a secret of a type other than string is deprecated because it implicitly converts to a string, potentially hiding bugs"
            ),
            builtin_source_span(compiled, call_span),
            stack_trace(compiled, stack),
            Some(php_runtime::PhpReferenceClassification::Deprecation),
        );
        let handled = self.dispatch_error_handler(
            compiled,
            output,
            stack,
            state,
            php_runtime::PHP_E_DEPRECATED,
            &diagnostic,
        )?;
        if !handled && error_reporting_allows(state, php_runtime::PHP_E_DEPRECATED) {
            Self::record_last_error(state, php_runtime::PHP_E_DEPRECATED, &diagnostic);
            emit_vm_diagnostic(
                output,
                state,
                &diagnostic,
                php_runtime::PhpDiagnosticChannel::Deprecated,
                php_runtime::PHP_E_DEPRECATED,
            );
            state.diagnostics.push(diagnostic);
        }
        Ok(())
    }

    fn prepare_http_build_query_values(
        &self,
        name: &str,
        mut values: Vec<Value>,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &CallStack,
        state: &ExecutionState,
    ) -> Result<Vec<Value>, VmResult> {
        if name != "http_build_query" {
            return Ok(values);
        }
        let Some(first) = values.first().cloned() else {
            return Ok(values);
        };
        let Value::Object(object) = effective_value(&first) else {
            return Ok(values);
        };
        let scope = current_scope_class(compiled, stack);
        let entries =
            match object_property_iteration_entries(compiled, state, &object, scope.as_deref()) {
                Ok(entries) => entries,
                Err(message) => return Err(self.runtime_error(output, compiled, stack, message)),
            };
        let mut array = PhpArray::new();
        for entry in entries {
            if let Some(value) = object.get_property(&entry.storage_name) {
                array.insert(
                    ArrayKey::String(PhpString::from_bytes(entry.key.into_bytes())),
                    value,
                );
            }
        }
        values[0] = Value::Array(array);
        Ok(values)
    }

    fn coerce_internal_builtin_string_args(
        &self,
        name: &str,
        mut values: Vec<Value>,
        compiled: &CompiledUnit,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Vec<Value>, VmResult> {
        for &index in internal_builtin_string_arg_positions(name, values.len()) {
            let Some(value) = values.get(index).cloned() else {
                continue;
            };
            if !value_needs_vm_string_coercion_in_state(compiled, state, &value) {
                continue;
            }
            let coerced = self.value_to_string(compiled, &value, output, stack, state)?;
            values[index] = Value::String(coerced);
        }
        if matches!(name, "printf" | "sprintf") {
            for index in 1..values.len() {
                let Some(value) = values.get(index).cloned() else {
                    continue;
                };
                if !value_needs_vm_string_coercion_in_state(compiled, state, &value) {
                    continue;
                }
                let coerced = self.value_to_string(compiled, &value, output, stack, state)?;
                values[index] = Value::String(coerced);
            }
        }
        Ok(values)
    }
}

fn attach_internal_builtin_throwable(
    name: &str,
    result: &VmResult,
    values: &[Value],
    call_span: Option<php_ir::IrSpan>,
    stack: &CallStack,
    state: &mut ExecutionState,
    compiled: &CompiledUnit,
) {
    if result.status.is_success() {
        return;
    }
    let Some(call_span) = call_span else {
        return;
    };
    let Some(throwable) = runtime_error_throwable(result) else {
        return;
    };
    let trace_values = builtin_trace_values(name, values);
    tag_throwable_location(&throwable, compiled, call_span);
    reapply_throwable_diagnostic_overrides(&throwable, result);
    state.pending_trace = Some(attach_builtin_failed_call_trace(
        &throwable,
        compiled,
        stack,
        name,
        &trace_values,
        call_span,
    ));
    state.pending_throw = Some(throwable);
}

fn builtin_trace_values(name: &str, values: &[Value]) -> Vec<Value> {
    let mut values = values.to_vec();
    for &index in builtin_sensitive_parameter_indexes(name) {
        if let Some(value) = values.get_mut(index) {
            *value = sensitive_parameter_value();
        }
    }
    values
}

fn builtin_sensitive_parameter_indexes(name: &str) -> &'static [usize] {
    match name {
        // php-src ext/hash/hash.stub.php marks these arguments with
        // #[\SensitiveParameter]. Generated stdlib arginfo does not yet carry
        // parameter attributes, so keep this bridge narrow and oracle-derived.
        "hash_equals" => &[0, 1],
        "hash_hkdf" | "hash_pbkdf2" => &[1],
        "hash_hmac" | "hash_hmac_file" | "hash_init" => &[2],
        _ => &[],
    }
}

fn assign_optional_reference(value: Option<&Value>, assigned: Value) {
    if let Some(Value::Reference(cell)) = value {
        cell.set(assigned);
    }
}
