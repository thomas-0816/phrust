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
            .prepare_http_build_query_values(name, values, compiled, output, stack, state)
        {
            Ok(values) => values,
            Err(result) => return result,
        };
        let values = match validate_internal_builtin_args(
            name, values, compiled, call_span, output, state,
        ) {
            Ok(values) => values,
            Err(result) => return result,
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
            attach_json_builtin_throwable(
                name,
                &result,
                &trace_values,
                call_span,
                output,
                stack,
                state,
                compiled,
            );
            return result;
        }
        if name == "curl_exec" {
            return self.execute_curl_exec_with_callbacks(
                entry, values, output, stack, state, compiled, call_span,
            );
        }
        // The failed-call trace is only ever attached for the JSON builtins
        // (json_encode is handled by its own arm above), so only json_decode
        // pays the argument snapshot — not the ~70k generic builtin calls a
        // WordPress request makes.
        let trace_values = (name == "json_decode").then(|| values.clone());
        let result = execute_builtin_entry(
            entry,
            values,
            output,
            &self.options.runtime_context,
            state,
            builtin_source_span(compiled, call_span),
        );
        if let Some(trace_values) = trace_values {
            attach_json_builtin_throwable(
                name,
                &result,
                &trace_values,
                call_span,
                output,
                stack,
                state,
                compiled,
            );
        }
        result
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

fn attach_json_builtin_throwable(
    name: &str,
    result: &VmResult,
    values: &[Value],
    call_span: Option<php_ir::IrSpan>,
    _output: &OutputBuffer,
    stack: &CallStack,
    state: &mut ExecutionState,
    compiled: &CompiledUnit,
) {
    if result.status.is_success() || !matches!(name, "json_decode" | "json_encode") {
        return;
    }
    let Some(call_span) = call_span else {
        return;
    };
    if !result
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.id() == "E_PHP_RUNTIME_JSON_EXCEPTION")
    {
        return;
    }
    let Some(throwable) = runtime_error_throwable(result) else {
        return;
    };
    tag_throwable_location(&throwable, compiled, call_span);
    state.pending_trace = Some(attach_builtin_failed_call_trace(
        &throwable, compiled, stack, name, values, call_span,
    ));
    state.pending_throw = Some(throwable);
}
