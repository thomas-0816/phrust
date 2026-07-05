//! Fast VM builtin intrinsic paths.

use super::prelude::*;

impl Vm {
    pub(super) fn try_execute_fast_builtin_stub(
        &self,
        name: &str,
        values: &[Value],
        _output: &OutputBuffer,
    ) -> Option<VmResult> {
        let (spec_index, spec) = builtin_intrinsic_spec(name)?;
        self.record_counter_builtin_intrinsic_candidate();
        let Some(result) = fast_builtin_stub_result_for_spec(spec_index, spec, values) else {
            let fallback_reason =
                fast_builtin_stub_fallback_reason_for_spec(spec_index, spec, values)
                    .unwrap_or("type");
            self.record_counter_builtin_fast_stub(name, false);
            self.record_counter_builtin_fast_stub_fallback(name, fallback_reason);
            self.record_counter_intrinsic(spec.counter_name, false);
            self.record_counter_intrinsic_fallback(spec.counter_name, fallback_reason);
            return None;
        };
        self.record_counter_builtin_fast_stub(name, true);
        self.record_counter_intrinsic(spec.counter_name, true);
        if name == "count" {
            self.record_counter_array_count_fast_path_hit();
            self.record_counter_internal_count_array_direct_fast_path_hit();
            self.record_counter_count_array_shape_fast_hit();
        }
        Some(VmResult::success_no_output(Some(result)))
    }

    /// Default-flags `json_encode` over scalar/array shapes, bypassing the
    /// serde tree and the full builtin-context construction. Fallback keeps
    /// the generic path authoritative for floats, objects, references,
    /// non-default flags, and every error case.
    pub(super) fn try_execute_json_encode_fast(
        &self,
        name: &str,
        values: &[Value],
        state: &mut ExecutionState,
    ) -> Option<VmResult> {
        if name != "json_encode" {
            return None;
        }
        match values {
            [_] | [_, Value::Int(0)] => {}
            [_, _] => {
                self.record_counter_json_encode_generic_fallback("flags");
                return None;
            }
            _ => {
                self.record_counter_json_encode_generic_fallback("arity");
                return None;
            }
        }
        match php_runtime::builtins::json_fast::json_encode_default_flags(&values[0]) {
            Ok(encoded) => {
                self.record_counter_json_encode_fast_path(encoded.len());
                state.json_last_error = php_runtime::builtins::json_fast::JSON_ENCODE_NO_ERROR;
                Some(VmResult::success_no_output(Some(Value::string(encoded))))
            }
            Err(reason) => {
                self.record_counter_json_encode_generic_fallback(reason);
                None
            }
        }
    }

    /// `array_slice` over values-only packed storage with defaulted or
    /// `false` key preservation; every other shape (and every diagnostic)
    /// stays on the generic path with the reason recorded.
    pub(super) fn try_execute_array_slice_fast(
        &self,
        name: &str,
        values: &[Value],
    ) -> Option<VmResult> {
        if name != "array_slice" {
            return None;
        }
        if !(2..=4).contains(&values.len()) {
            self.record_counter_array_builtin_fast_fallback("array_slice", "arity");
            return None;
        }
        let (Value::Array(array), Value::Int(offset)) = (&values[0], &values[1]) else {
            self.record_counter_array_builtin_fast_fallback("array_slice", "type");
            return None;
        };
        let length = match values.get(2) {
            None | Some(Value::Null) => None,
            Some(Value::Int(length)) => Some(*length),
            Some(_) => {
                self.record_counter_array_builtin_fast_fallback("array_slice", "type");
                return None;
            }
        };
        match values.get(3) {
            None | Some(Value::Bool(false)) => {}
            Some(Value::Bool(true)) => {
                self.record_counter_array_builtin_fast_fallback("array_slice", "preserve_keys");
                return None;
            }
            Some(_) => {
                self.record_counter_array_builtin_fast_fallback("array_slice", "type");
                return None;
            }
        }
        let Some(result) =
            php_runtime::builtins::array_intrinsics::array_slice_packed(array, *offset, length)
        else {
            self.record_counter_array_builtin_fast_fallback("array_slice", "array_shape");
            return None;
        };
        self.record_counter_array_slice_packed_fast_hit();
        Some(VmResult::success_no_output(Some(Value::Array(result))))
    }

    pub(super) fn try_execute_direct_count_array(
        &self,
        name: &str,
        values: &[Value],
        _output: &OutputBuffer,
    ) -> Option<VmResult> {
        if name != "count" {
            return None;
        }
        if values.len() != 1 {
            self.record_counter_array_builtin_fast_fallback("count", "mode");
            return None;
        }
        let Value::Array(array) = effective_value(&values[0]) else {
            self.record_counter_array_builtin_fast_fallback("count", "type");
            return None;
        };
        self.record_counter_array_count_fast_path_hit();
        self.record_counter_internal_count_array_direct_fast_path_hit();
        self.record_counter_count_array_shape_fast_hit();
        Some(VmResult::success_no_output(Some(Value::Int(
            array.len() as i64
        ))))
    }

    pub(super) fn try_execute_countable_object(
        &self,
        name: &str,
        values: &[Value],
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
        compiled: &CompiledUnit,
    ) -> Option<VmResult> {
        if name != "count" || !(1..=2).contains(&values.len()) {
            return None;
        }
        let Value::Object(object) = effective_value(&values[0]) else {
            return None;
        };
        let class_name = object.class_name();
        let is_countable = spl_runtime_marker(&object)
            .and_then(|spl_class| {
                internal_spl_container_instanceof(&spl_class, "Countable")
                    .or_else(|| internal_spl_iterator_instanceof(&spl_class, "Countable"))
                    .or_else(|| internal_spl_file_instanceof(&spl_class, "Countable"))
                    .or_else(|| internal_spl_heap_instanceof(&spl_class, "Countable"))
            })
            .unwrap_or(false)
            || class_implements_in_state(
                compiled,
                state,
                &class_name,
                "Countable",
                &mut Vec::new(),
            )
            .unwrap_or(false);
        if !is_countable {
            return None;
        }
        Some(self.call_object_method_callable(
            compiled,
            object,
            "count",
            Vec::new(),
            None,
            output,
            stack,
            state,
        ))
    }
}
