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

    /// Fast path for php-src's repeated `/\G\w/u` offset scan. The generic
    /// builtin remains authoritative for non-ASCII subjects, non-zero flags,
    /// non-reference match outputs, and every wider PCRE shape.
    pub(super) fn try_execute_preg_match_start_offset_ascii_fast(
        &self,
        name: &str,
        values: &[Value],
        state: &mut ExecutionState,
    ) -> Option<VmResult> {
        if name != "preg_match" || values.len() != 5 {
            return None;
        }
        let Value::String(pattern) = effective_value(&values[0]) else {
            return None;
        };
        if pattern.as_bytes() != br"/\G\w/u" {
            return None;
        }
        let Value::String(subject) = effective_value(&values[1]) else {
            return None;
        };
        let Value::Reference(matches) = &values[2] else {
            return None;
        };
        let Ok(flags) = to_int(&values[3]) else {
            return None;
        };
        if flags != 0 {
            return None;
        }
        let Ok(offset) = to_int(&values[4]) else {
            return None;
        };
        if offset < 0 {
            return None;
        }
        let start = offset as usize;
        let subject_bytes = subject.as_bytes();
        if start > subject_bytes.len() {
            return None;
        }
        if !state
            .pcre_cache
            .validate_utf8_ascii_subject_at_offset(&subject, start)
            .ok()?
        {
            return None;
        }
        state.preg_last_error.clear();
        match subject_bytes.get(start).copied() {
            Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_' => {
                set_preg_match_single_byte_match(matches, &subject_bytes[start..start + 1]);
                Some(VmResult::success_no_output(Some(Value::Int(1))))
            }
            _ => {
                set_preg_match_empty_matches(matches);
                Some(VmResult::success_no_output(Some(Value::Int(0))))
            }
        }
    }

    /// Earlier variant of [`try_execute_preg_match_start_offset_ascii_fast`]
    /// for hot call sites where the generic builtin argument binder dominates
    /// the actual `/\G\w/u` ASCII byte check.
    pub(super) fn try_execute_preg_match_start_offset_ascii_call_fast(
        &self,
        name: &str,
        args: &[CallArgument],
        compiled: &CompiledUnit,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Option<VmResult> {
        if name != "preg_match" || args.len() != 5 || args.iter().any(|arg| arg.name.is_some()) {
            return None;
        }
        let Value::String(pattern) = effective_value(&args[0].value) else {
            return None;
        };
        if pattern.as_bytes() != br"/\G\w/u" {
            return None;
        }
        let Value::String(subject) = effective_value(&args[1].value) else {
            return None;
        };
        let matches = match &args[2].value {
            Value::Reference(cell) => cell.clone(),
            _ => call_argument_reference_cell(compiled, Some(state), &args[2], stack)
                .ok()
                .flatten()?,
        };
        let Ok(flags) = to_int(&args[3].value) else {
            return None;
        };
        if flags != 0 {
            return None;
        }
        let Ok(offset) = to_int(&args[4].value) else {
            return None;
        };
        if offset < 0 {
            return None;
        }
        let start = offset as usize;
        let subject_bytes = subject.as_bytes();
        if start > subject_bytes.len() {
            return None;
        }
        if !state
            .pcre_cache
            .validate_utf8_ascii_subject_at_offset(&subject, start)
            .ok()?
        {
            return None;
        }
        state.preg_last_error.clear();
        match subject_bytes.get(start).copied() {
            Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_' => {
                set_preg_match_single_byte_match(&matches, &subject_bytes[start..start + 1]);
                Some(VmResult::success_no_output(Some(Value::Int(1))))
            }
            _ => {
                set_preg_match_empty_matches(&matches);
                Some(VmResult::success_no_output(Some(Value::Int(0))))
            }
        }
    }

    /// Rich-IR version of the `/\G\w/u` ASCII offset fast path. This sits
    /// before generic call-argument materialization for performance-sensitive
    /// scanner loops.
    pub(super) fn try_execute_rich_preg_match_start_offset_ascii_fast(
        &self,
        name: &str,
        args: &[IrCallArg],
        unit: &IrUnit,
        stack: &mut CallStack,
        frame_index: usize,
        state: &mut ExecutionState,
    ) -> Result<Option<Value>, String> {
        if !name.eq_ignore_ascii_case("preg_match")
            || name.contains('\\')
            || args.len() != 5
            || args.iter().any(|arg| arg.name.is_some() || arg.unpack)
        {
            return Ok(None);
        }
        let pattern_value = read_operand_at_frame(unit, stack, frame_index, args[0].value)?;
        let Value::String(pattern) = effective_value(&pattern_value) else {
            return Ok(None);
        };
        if pattern.as_bytes() != br"/\G\w/u" {
            return Ok(None);
        }
        let subject_value = read_operand_at_frame(unit, stack, frame_index, args[1].value)?;
        let Value::String(subject) = effective_value(&subject_value) else {
            return Ok(None);
        };
        let Some(matches_local) = args[2].by_ref_local else {
            return Ok(None);
        };
        if args[2].by_ref_dim.is_some()
            || args[2].by_ref_property.is_some()
            || args[2].by_ref_property_dim.is_some()
        {
            return Ok(None);
        }
        let flags_value = read_operand_at_frame(unit, stack, frame_index, args[3].value)?;
        let Ok(flags) = to_int(&flags_value) else {
            return Ok(None);
        };
        if flags != 0 {
            return Ok(None);
        }
        let offset_value = read_operand_at_frame(unit, stack, frame_index, args[4].value)?;
        let Ok(offset) = to_int(&offset_value) else {
            return Ok(None);
        };
        if offset < 0 {
            return Ok(None);
        }
        let start = offset as usize;
        let subject_bytes = subject.as_bytes();
        if start > subject_bytes.len() {
            return Ok(None);
        }
        if !state
            .pcre_cache
            .validate_utf8_ascii_subject_at_offset(&subject, start)
            .map_err(|error| error.message().to_owned())?
        {
            return Ok(None);
        }

        let matches = stack
            .frame_mut(frame_index)
            .ok_or_else(|| "frame is not active".to_owned())?
            .locals
            .ensure_reference_cell(matches_local)?;
        state.preg_last_error.clear();
        let matched = match subject_bytes.get(start).copied() {
            Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_' => {
                set_preg_match_single_byte_match(&matches, &subject_bytes[start..start + 1]);
                1
            }
            _ => {
                set_preg_match_empty_matches(&matches);
                0
            }
        };
        Ok(Some(Value::Int(matched)))
    }

    /// Dense-bytecode version of the `/\G\w/u` ASCII offset fast path. This
    /// avoids constructing generic call arguments for tight scanner loops.
    pub(super) fn try_execute_dense_preg_match_start_offset_ascii_fast(
        &self,
        name: &str,
        args: &[DenseCallArg],
        compiled: &CompiledUnit,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> Result<Option<Value>, String> {
        if !name.eq_ignore_ascii_case("preg_match")
            || name.contains('\\')
            || args.len() != 5
            || args.iter().any(|arg| arg.name.is_some())
        {
            return Ok(None);
        }
        let Some(pattern_matches) =
            with_dense_string_operand(compiled, stack, args[0].value, |pattern| {
                Ok(Some(pattern.as_bytes() == br"/\G\w/u"))
            })?
        else {
            return Ok(None);
        };
        if !pattern_matches {
            return Ok(None);
        }
        let Some(matches_local) = plain_dense_by_ref_local(&args[2]) else {
            return Ok(None);
        };
        let Some(flags) = dense_operand_exact_int(compiled, stack, args[3].value)? else {
            return Ok(None);
        };
        if flags != 0 {
            return Ok(None);
        }
        let offset = if let Some(offset_local) = plain_dense_by_ref_local(&args[4]) {
            dense_local_exact_int(stack, LocalId::new(offset_local))?
        } else {
            dense_operand_exact_int(compiled, stack, args[4].value)?
        };
        let Some(offset) = offset else {
            return Ok(None);
        };
        if offset < 0 {
            return Ok(None);
        }
        let start = offset as usize;
        let match_result = if let Some(subject_local) = plain_dense_by_ref_local(&args[1]) {
            with_dense_local_string_operand(stack, LocalId::new(subject_local), |subject| {
                let subject_bytes = subject.as_bytes();
                if start > subject_bytes.len() {
                    return Ok(None);
                }
                if !state
                    .pcre_cache
                    .validate_utf8_ascii_subject_at_offset(subject, start)
                    .map_err(|error| error.message().to_owned())?
                {
                    return Ok(None);
                }
                Ok(Some(match subject_bytes.get(start).copied() {
                    Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_' => {
                        PregAsciiOffsetMatch::Matched(byte)
                    }
                    _ => PregAsciiOffsetMatch::NoMatch,
                }))
            })?
        } else {
            with_dense_string_operand(compiled, stack, args[1].value, |subject| {
                let subject_bytes = subject.as_bytes();
                if start > subject_bytes.len() {
                    return Ok(None);
                }
                if !state
                    .pcre_cache
                    .validate_utf8_ascii_subject_at_offset(subject, start)
                    .map_err(|error| error.message().to_owned())?
                {
                    return Ok(None);
                }
                Ok(Some(match subject_bytes.get(start).copied() {
                    Some(byte) if byte.is_ascii_alphanumeric() || byte == b'_' => {
                        PregAsciiOffsetMatch::Matched(byte)
                    }
                    _ => PregAsciiOffsetMatch::NoMatch,
                }))
            })?
        };
        let Some(match_result) = match_result else {
            return Ok(None);
        };

        let matches = stack
            .current_mut()
            .ok_or_else(|| "no active frame".to_owned())?
            .locals
            .ensure_reference_cell(LocalId::new(matches_local))?;
        state.preg_last_error.clear();
        let matched = match match_result {
            PregAsciiOffsetMatch::Matched(byte) => {
                set_preg_match_single_byte_match(&matches, &[byte]);
                1
            }
            PregAsciiOffsetMatch::NoMatch => {
                set_preg_match_empty_matches(&matches);
                0
            }
        };
        Ok(Some(Value::Int(matched)))
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

pub(super) fn set_preg_match_single_byte_match(matches: &ReferenceCell, matched: &[u8]) {
    if let Ok(already_set) = matches.try_with_value_mut(|value| {
        if preg_match_matches_single_byte(value, matched) {
            return true;
        }
        *value = Value::packed_array(vec![Value::String(PhpString::intern(matched))]);
        true
    }) && already_set
    {
        return;
    }
    matches.set(Value::packed_array(vec![Value::String(PhpString::intern(
        matched,
    ))]));
}

pub(super) fn set_preg_match_empty_matches(matches: &ReferenceCell) {
    if let Ok(already_set) = matches.try_with_value_mut(|value| {
        if matches!(value, Value::Array(array) if array.is_empty()) {
            return true;
        }
        *value = Value::packed_array(Vec::new());
        true
    }) && already_set
    {
        return;
    }
    matches.set(Value::packed_array(Vec::new()));
}

fn preg_match_matches_single_byte(value: &Value, matched: &[u8]) -> bool {
    let Value::Array(array) = value else {
        return false;
    };
    if array.len() != 1 {
        return false;
    }
    let Some(Value::String(existing)) = array.packed_element_fast(0) else {
        return false;
    };
    existing.as_bytes() == matched
}

fn plain_dense_by_ref_local(arg: &DenseCallArg) -> Option<u32> {
    if arg.by_ref_dim.is_none()
        && arg.by_ref_property.is_none()
        && arg.by_ref_property_dim.is_none()
    {
        arg.by_ref_local
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PregAsciiOffsetMatch {
    Matched(u8),
    NoMatch,
}

fn dense_local_exact_int(stack: &CallStack, local: LocalId) -> Result<Option<i64>, String> {
    let frame = stack.current().ok_or("no active frame")?;
    let Some(slot) = frame.locals.get_slot(local) else {
        return Err(format!("invalid local local:{}", local.raw()));
    };
    match slot {
        Slot::Value(Value::Int(value)) => Ok(Some(*value)),
        Slot::Reference(cell) => Ok(cell
            .try_with_value(|value| match value {
                Value::Int(value) => Some(*value),
                _ => None,
            })
            .unwrap_or(None)),
        _ => Ok(None),
    }
}

fn dense_operand_exact_int(
    compiled: &CompiledUnit,
    stack: &CallStack,
    operand: DenseOperand,
) -> Result<Option<i64>, String> {
    match operand.kind {
        DenseOperandKind::Register => {
            let frame = stack.current().ok_or("no active frame")?;
            let Some(value) = frame.registers.get(RegId::new(operand.index)) else {
                return Err(format!("invalid register r{}", operand.index));
            };
            Ok(match value {
                Value::Int(value) => Some(*value),
                _ => None,
            })
        }
        DenseOperandKind::Local => {
            let frame = stack.current().ok_or("no active frame")?;
            let Some(slot) = frame.locals.get_slot(LocalId::new(operand.index)) else {
                return Err(format!("invalid local local:{}", operand.index));
            };
            match slot {
                Slot::Value(Value::Int(value)) => Ok(Some(*value)),
                Slot::Reference(cell) => Ok(cell
                    .try_with_value(|value| match value {
                        Value::Int(value) => Some(*value),
                        _ => None,
                    })
                    .unwrap_or(None)),
                _ => Ok(None),
            }
        }
        DenseOperandKind::Constant => {
            let Some(constant) = compiled.unit().constants.get(operand.index as usize) else {
                return Err(format!("invalid constant const:{}", operand.index));
            };
            Ok(match constant {
                IrConstant::Int(value) => Some(*value),
                _ => None,
            })
        }
    }
}

fn with_dense_local_string_operand<T>(
    stack: &CallStack,
    local: LocalId,
    f: impl FnOnce(&PhpString) -> Result<Option<T>, String>,
) -> Result<Option<T>, String> {
    let frame = stack.current().ok_or("no active frame")?;
    let Some(slot) = frame.locals.get_slot(local) else {
        return Err(format!("invalid local local:{}", local.raw()));
    };
    match slot {
        Slot::Value(Value::String(subject)) => f(subject),
        Slot::Reference(cell) => cell
            .try_with_value(|value| match value {
                Value::String(subject) => f(subject),
                _ => Ok(None),
            })
            .unwrap_or(Ok(None)),
        _ => Ok(None),
    }
}

fn with_dense_string_operand<T>(
    compiled: &CompiledUnit,
    stack: &CallStack,
    operand: DenseOperand,
    f: impl FnOnce(&PhpString) -> Result<Option<T>, String>,
) -> Result<Option<T>, String> {
    match operand.kind {
        DenseOperandKind::Register => {
            let frame = stack.current().ok_or("no active frame")?;
            let Some(value) = frame.registers.get(RegId::new(operand.index)) else {
                return Err(format!("invalid register r{}", operand.index));
            };
            match value {
                Value::String(subject) => f(subject),
                _ => Ok(None),
            }
        }
        DenseOperandKind::Local => {
            let frame = stack.current().ok_or("no active frame")?;
            let Some(slot) = frame.locals.get_slot(LocalId::new(operand.index)) else {
                return Err(format!("invalid local local:{}", operand.index));
            };
            match slot {
                Slot::Value(Value::String(subject)) => f(subject),
                Slot::Reference(cell) => cell
                    .try_with_value(|value| match value {
                        Value::String(subject) => f(subject),
                        _ => Ok(None),
                    })
                    .unwrap_or(Ok(None)),
                _ => Ok(None),
            }
        }
        DenseOperandKind::Constant => {
            let Some(constant) = compiled.unit().constants.get(operand.index as usize) else {
                return Err(format!("invalid constant const:{}", operand.index));
            };
            match constant {
                IrConstant::String(value) => f(&PhpString::from_test_str(value)),
                IrConstant::StringBytes(value) => f(&PhpString::from_bytes(value.clone())),
                _ => Ok(None),
            }
        }
    }
}
