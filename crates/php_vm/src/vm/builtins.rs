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
        let values = match validate_internal_builtin_args(
            name, values, compiled, call_span, output, state,
        ) {
            Ok(values) => values,
            Err(result) => return result,
        };
        let values = if name == "var_dump" {
            match self.prepare_var_dump_values(values, output, stack, state, compiled) {
                Ok(values) => values,
                Err(result) => return result,
            }
        } else {
            values
        };
        if name == "curl_exec" {
            return self.execute_curl_exec_with_callbacks(
                entry, values, output, stack, state, compiled, call_span,
            );
        }
        execute_builtin_entry(
            entry,
            values,
            output,
            &self.options.runtime_context,
            state,
            builtin_source_span(compiled, call_span),
        )
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
