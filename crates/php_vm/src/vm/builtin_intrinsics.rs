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
        match php_runtime::experimental::builtin_intrinsics::json_fast::json_encode_default_flags(
            &values[0],
        ) {
            Ok(encoded) => {
                self.record_counter_json_encode_fast_path(encoded.len());
                state.builtins.set_json_last_error(
                    php_runtime::experimental::builtin_intrinsics::json_fast::JSON_ENCODE_NO_ERROR,
                );
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
            php_runtime::experimental::builtin_intrinsics::array_intrinsics::array_slice_packed(
                array, *offset, length,
            )
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
            .builtins
            .pcre_state_mut()
            .cache_mut()
            .validate_utf8_ascii_subject_at_offset(&subject, start)
            .ok()?
        {
            return None;
        }
        state.builtins.pcre_state_mut().last_error_mut().clear();
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
            .builtins
            .pcre_state_mut()
            .cache_mut()
            .validate_utf8_ascii_subject_at_offset(&subject, start)
            .ok()?
        {
            return None;
        }
        state.builtins.pcre_state_mut().last_error_mut().clear();
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
            .builtins
            .pcre_state_mut()
            .cache_mut()
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
        state.builtins.pcre_state_mut().last_error_mut().clear();
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
                    .builtins
                    .pcre_state_mut()
                    .cache_mut()
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
                    .builtins
                    .pcre_state_mut()
                    .cache_mut()
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
        state.builtins.pcre_state_mut().last_error_mut().clear();
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BuiltinIntrinsicKind {
    ArrayKeyExists,
    CountArray,
    ExplodeSingleByte,
    HtmlSpecialCharsDefault,
    IsArray,
    IsBool,
    IsFloat,
    IsInt,
    IsNull,
    IsNumeric,
    IsObject,
    IsScalar,
    IsString,
    PointerCurrent,
    PointerKey,
    ArrayKeysAll,
    ImplodeStringParts,
    StrContains,
    StrEndsWith,
    StrLen,
    StrReplaceScalar,
    StrStartsWith,
    StrToLower,
    StrToUpper,
    SubstrBytes,
    TrimDefault,
    InArrayStrict,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BuiltinIntrinsicParam {
    name: &'static str,
    type_decl: &'static str,
    optional: bool,
    by_ref: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BuiltinIntrinsicSpec {
    name: &'static str,
    /// Counter label under `intrinsic_hits`/`intrinsic_misses`; carries the
    /// specialized-case suffix where the fast path covers only one shape.
    counter_name: &'static str,
    return_type: &'static str,
    params: &'static [BuiltinIntrinsicParam],
    /// Lowest supplied-argument count the fast path accepts.
    min_arity: usize,
    /// Highest supplied-argument count the fast path accepts.
    exact_arity: usize,
    kind: BuiltinIntrinsicKind,
}

const INTRINSIC_VALUE_PARAM: &[BuiltinIntrinsicParam] = &[BuiltinIntrinsicParam {
    name: "value",
    type_decl: "mixed",
    optional: false,
    by_ref: false,
}];
const INTRINSIC_STRING_PARAM: &[BuiltinIntrinsicParam] = &[BuiltinIntrinsicParam {
    name: "string",
    type_decl: "string",
    optional: false,
    by_ref: false,
}];
const INTRINSIC_COUNT_PARAMS: &[BuiltinIntrinsicParam] = &[
    BuiltinIntrinsicParam {
        name: "value",
        type_decl: "Countable|array",
        optional: false,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "mode",
        type_decl: "int",
        optional: true,
        by_ref: false,
    },
];
const INTRINSIC_STRING_PAIR_PARAMS: &[BuiltinIntrinsicParam] = &[
    BuiltinIntrinsicParam {
        name: "haystack",
        type_decl: "string",
        optional: false,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "needle",
        type_decl: "string",
        optional: false,
        by_ref: false,
    },
];
const INTRINSIC_ARRAY_KEY_EXISTS_PARAMS: &[BuiltinIntrinsicParam] = &[
    BuiltinIntrinsicParam {
        name: "key",
        type_decl: "mixed",
        optional: false,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "array",
        type_decl: "array",
        optional: false,
        by_ref: false,
    },
];
const INTRINSIC_EXPLODE_PARAMS: &[BuiltinIntrinsicParam] = &[
    BuiltinIntrinsicParam {
        name: "separator",
        type_decl: "string",
        optional: false,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "string",
        type_decl: "string",
        optional: false,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "limit",
        type_decl: "int",
        optional: true,
        by_ref: false,
    },
];
const INTRINSIC_HTMLSPECIALCHARS_PARAMS: &[BuiltinIntrinsicParam] = &[
    BuiltinIntrinsicParam {
        name: "string",
        type_decl: "string",
        optional: false,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "flags",
        type_decl: "int",
        optional: true,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "encoding",
        type_decl: "?string",
        optional: true,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "double_encode",
        type_decl: "bool",
        optional: true,
        by_ref: false,
    },
];
const INTRINSIC_STR_REPLACE_PARAMS: &[BuiltinIntrinsicParam] = &[
    BuiltinIntrinsicParam {
        name: "search",
        type_decl: "array|string",
        optional: false,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "replace",
        type_decl: "array|string",
        optional: false,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "subject",
        type_decl: "string|array",
        optional: false,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "count",
        type_decl: "mixed",
        optional: true,
        by_ref: true,
    },
];
const INTRINSIC_SUBSTR_PARAMS: &[BuiltinIntrinsicParam] = &[
    BuiltinIntrinsicParam {
        name: "string",
        type_decl: "string",
        optional: false,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "offset",
        type_decl: "int",
        optional: false,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "length",
        type_decl: "?int",
        optional: true,
        by_ref: false,
    },
];
const INTRINSIC_TRIM_PARAMS: &[BuiltinIntrinsicParam] = &[
    BuiltinIntrinsicParam {
        name: "string",
        type_decl: "string",
        optional: false,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "characters",
        type_decl: "string",
        optional: true,
        by_ref: false,
    },
];
const INTRINSIC_IN_ARRAY_PARAMS: &[BuiltinIntrinsicParam] = &[
    BuiltinIntrinsicParam {
        name: "needle",
        type_decl: "mixed",
        optional: false,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "haystack",
        type_decl: "array",
        optional: false,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "strict",
        type_decl: "bool",
        optional: true,
        by_ref: false,
    },
];
const INTRINSIC_POINTER_ARRAY_PARAM: &[BuiltinIntrinsicParam] = &[BuiltinIntrinsicParam {
    name: "array",
    type_decl: "array|object",
    optional: false,
    by_ref: false,
}];

const INTRINSIC_ARRAY_KEYS_PARAMS: &[BuiltinIntrinsicParam] = &[
    BuiltinIntrinsicParam {
        name: "array",
        type_decl: "array",
        optional: false,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "filter_value",
        type_decl: "mixed",
        optional: true,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "strict",
        type_decl: "bool",
        optional: true,
        by_ref: false,
    },
];
const INTRINSIC_IMPLODE_PARAMS: &[BuiltinIntrinsicParam] = &[
    BuiltinIntrinsicParam {
        name: "separator",
        type_decl: "string|array",
        optional: false,
        by_ref: false,
    },
    BuiltinIntrinsicParam {
        name: "array",
        type_decl: "?array",
        optional: true,
        by_ref: false,
    },
];

const BUILTIN_INTRINSICS: &[BuiltinIntrinsicSpec] = &[
    BuiltinIntrinsicSpec {
        name: "substr",
        counter_name: "substr_bytes",
        return_type: "string",
        params: INTRINSIC_SUBSTR_PARAMS,
        min_arity: 2,
        exact_arity: 3,
        kind: BuiltinIntrinsicKind::SubstrBytes,
    },
    BuiltinIntrinsicSpec {
        name: "trim",
        counter_name: "trim_default",
        return_type: "string",
        params: INTRINSIC_TRIM_PARAMS,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::TrimDefault,
    },
    BuiltinIntrinsicSpec {
        name: "in_array",
        counter_name: "in_array_strict",
        return_type: "bool",
        params: INTRINSIC_IN_ARRAY_PARAMS,
        min_arity: 3,
        exact_arity: 3,
        kind: BuiltinIntrinsicKind::InArrayStrict,
    },
    BuiltinIntrinsicSpec {
        name: "array_key_exists",
        counter_name: "array_key_exists",
        return_type: "bool",
        params: INTRINSIC_ARRAY_KEY_EXISTS_PARAMS,
        min_arity: 2,
        exact_arity: 2,
        kind: BuiltinIntrinsicKind::ArrayKeyExists,
    },
    BuiltinIntrinsicSpec {
        name: "count",
        counter_name: "count",
        return_type: "int",
        params: INTRINSIC_COUNT_PARAMS,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::CountArray,
    },
    BuiltinIntrinsicSpec {
        name: "explode",
        counter_name: "explode_single_byte",
        return_type: "array",
        params: INTRINSIC_EXPLODE_PARAMS,
        min_arity: 2,
        exact_arity: 2,
        kind: BuiltinIntrinsicKind::ExplodeSingleByte,
    },
    BuiltinIntrinsicSpec {
        name: "htmlspecialchars",
        counter_name: "htmlspecialchars_default",
        return_type: "string",
        params: INTRINSIC_HTMLSPECIALCHARS_PARAMS,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::HtmlSpecialCharsDefault,
    },
    BuiltinIntrinsicSpec {
        name: "is_array",
        counter_name: "is_array",
        return_type: "bool",
        params: INTRINSIC_VALUE_PARAM,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::IsArray,
    },
    BuiltinIntrinsicSpec {
        name: "is_int",
        counter_name: "is_int",
        return_type: "bool",
        params: INTRINSIC_VALUE_PARAM,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::IsInt,
    },
    BuiltinIntrinsicSpec {
        name: "is_string",
        counter_name: "is_string",
        return_type: "bool",
        params: INTRINSIC_VALUE_PARAM,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::IsString,
    },
    BuiltinIntrinsicSpec {
        name: "strlen",
        counter_name: "strlen",
        return_type: "int",
        params: INTRINSIC_STRING_PARAM,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::StrLen,
    },
    BuiltinIntrinsicSpec {
        name: "str_contains",
        counter_name: "str_contains",
        return_type: "bool",
        params: INTRINSIC_STRING_PAIR_PARAMS,
        min_arity: 2,
        exact_arity: 2,
        kind: BuiltinIntrinsicKind::StrContains,
    },
    BuiltinIntrinsicSpec {
        name: "str_ends_with",
        counter_name: "str_ends_with",
        return_type: "bool",
        params: INTRINSIC_STRING_PAIR_PARAMS,
        min_arity: 2,
        exact_arity: 2,
        kind: BuiltinIntrinsicKind::StrEndsWith,
    },
    BuiltinIntrinsicSpec {
        name: "str_replace",
        counter_name: "str_replace_scalar",
        return_type: "string|array",
        params: INTRINSIC_STR_REPLACE_PARAMS,
        min_arity: 3,
        exact_arity: 3,
        kind: BuiltinIntrinsicKind::StrReplaceScalar,
    },
    BuiltinIntrinsicSpec {
        name: "str_starts_with",
        counter_name: "str_starts_with",
        return_type: "bool",
        params: INTRINSIC_STRING_PAIR_PARAMS,
        min_arity: 2,
        exact_arity: 2,
        kind: BuiltinIntrinsicKind::StrStartsWith,
    },
    BuiltinIntrinsicSpec {
        name: "strtolower",
        counter_name: "strtolower",
        return_type: "string",
        params: INTRINSIC_STRING_PARAM,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::StrToLower,
    },
    BuiltinIntrinsicSpec {
        name: "strtoupper",
        counter_name: "strtoupper_ascii",
        return_type: "string",
        params: INTRINSIC_STRING_PARAM,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::StrToUpper,
    },
    BuiltinIntrinsicSpec {
        name: "is_bool",
        counter_name: "is_bool",
        return_type: "bool",
        params: INTRINSIC_VALUE_PARAM,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::IsBool,
    },
    BuiltinIntrinsicSpec {
        name: "is_float",
        counter_name: "is_float",
        return_type: "bool",
        params: INTRINSIC_VALUE_PARAM,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::IsFloat,
    },
    BuiltinIntrinsicSpec {
        name: "is_null",
        counter_name: "is_null",
        return_type: "bool",
        params: INTRINSIC_VALUE_PARAM,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::IsNull,
    },
    BuiltinIntrinsicSpec {
        name: "is_numeric",
        counter_name: "is_numeric",
        return_type: "bool",
        params: INTRINSIC_VALUE_PARAM,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::IsNumeric,
    },
    BuiltinIntrinsicSpec {
        name: "is_object",
        counter_name: "is_object",
        return_type: "bool",
        params: INTRINSIC_VALUE_PARAM,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::IsObject,
    },
    BuiltinIntrinsicSpec {
        name: "is_scalar",
        counter_name: "is_scalar",
        return_type: "bool",
        params: INTRINSIC_VALUE_PARAM,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::IsScalar,
    },
    BuiltinIntrinsicSpec {
        name: "current",
        counter_name: "current_array_pointer",
        return_type: "mixed",
        params: INTRINSIC_POINTER_ARRAY_PARAM,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::PointerCurrent,
    },
    BuiltinIntrinsicSpec {
        name: "key",
        counter_name: "key_array_pointer",
        return_type: "int|string|null",
        params: INTRINSIC_POINTER_ARRAY_PARAM,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::PointerKey,
    },
    BuiltinIntrinsicSpec {
        name: "array_keys",
        counter_name: "array_keys_all",
        return_type: "array",
        params: INTRINSIC_ARRAY_KEYS_PARAMS,
        min_arity: 1,
        exact_arity: 1,
        kind: BuiltinIntrinsicKind::ArrayKeysAll,
    },
    BuiltinIntrinsicSpec {
        name: "implode",
        counter_name: "implode_string_parts",
        return_type: "string",
        params: INTRINSIC_IMPLODE_PARAMS,
        min_arity: 2,
        exact_arity: 2,
        kind: BuiltinIntrinsicKind::ImplodeStringParts,
    },
];

fn fast_builtin_stub_result_for_spec(
    spec_index: usize,
    spec: &'static BuiltinIntrinsicSpec,
    values: &[Value],
) -> Option<Value> {
    if fast_builtin_stub_fallback_reason_for_spec(spec_index, spec, values).is_some() {
        return None;
    }
    match (spec.kind, values) {
        (BuiltinIntrinsicKind::ArrayKeyExists, [key, Value::Array(array)]) => {
            let key = ArrayKey::from_value(key)?;
            Some(Value::Bool(array.get(&key).is_some()))
        }
        (BuiltinIntrinsicKind::CountArray, [Value::Array(array)]) => {
            Some(Value::Int(array.len() as i64))
        }
        (BuiltinIntrinsicKind::IsArray, [value]) => {
            Some(Value::Bool(matches!(value, Value::Array(_))))
        }
        (BuiltinIntrinsicKind::IsInt, [value]) => Some(Value::Bool(matches!(value, Value::Int(_)))),
        (BuiltinIntrinsicKind::IsBool, [value]) => {
            Some(Value::Bool(matches!(value, Value::Bool(_))))
        }
        (BuiltinIntrinsicKind::IsFloat, [value]) => {
            Some(Value::Bool(matches!(value, Value::Float(_))))
        }
        (BuiltinIntrinsicKind::IsNull, [value]) => Some(Value::Bool(matches!(value, Value::Null))),
        (BuiltinIntrinsicKind::IsNumeric, [value]) => Some(Value::Bool(match value {
            Value::Int(_) | Value::Float(_) => true,
            // Matches the numeric-string classifier: only a whole trimmed
            // int/float string is numeric; leading-numeric ("12abc") and
            // non-numeric strings are not. References are rejected as by_ref
            // before reaching here, so this sees the effective value.
            Value::String(bytes) => matches!(
                classify_php_string(bytes).kind,
                NumericStringKind::IntString | NumericStringKind::FloatString
            ),
            _ => false,
        })),
        (BuiltinIntrinsicKind::IsObject, [value]) => Some(Value::Bool(matches!(
            value,
            Value::Object(_) | Value::Fiber(_) | Value::Generator(_) | Value::Callable(_)
        ))),
        (BuiltinIntrinsicKind::IsScalar, [value]) => Some(Value::Bool(matches!(
            value,
            Value::Bool(_) | Value::Int(_) | Value::Float(_) | Value::String(_)
        ))),
        (BuiltinIntrinsicKind::PointerCurrent, [Value::Array(array)]) => {
            Some(array.pointer_value().unwrap_or(Value::Bool(false)))
        }
        (BuiltinIntrinsicKind::PointerKey, [Value::Array(array)]) => {
            Some(array.pointer_key().map_or(Value::Null, array_key_to_value))
        }
        (BuiltinIntrinsicKind::ArrayKeysAll, [Value::Array(array)]) => {
            let keys = array
                .iter()
                .map(|(key, _)| array_key_to_value(key))
                .collect::<Vec<_>>();
            Some(Value::Array(PhpArray::from_packed(keys)))
        }
        (
            BuiltinIntrinsicKind::ImplodeStringParts,
            [Value::String(separator), Value::Array(parts)],
        ) => php_runtime::experimental::builtin_intrinsics::string_intrinsics::implode_string_parts(separator, parts)
            .map(Value::String),
        (BuiltinIntrinsicKind::IsString, [value]) => {
            Some(Value::Bool(matches!(value, Value::String(_))))
        }
        (BuiltinIntrinsicKind::StrLen, [Value::String(string)]) => {
            Some(Value::Int(string.len() as i64))
        }
        (BuiltinIntrinsicKind::StrContains, [Value::String(haystack), Value::String(needle)]) => {
            Some(Value::Bool(byte_slice_contains(
                haystack.as_bytes(),
                needle.as_bytes(),
            )))
        }
        (BuiltinIntrinsicKind::StrStartsWith, [Value::String(haystack), Value::String(needle)]) => {
            Some(Value::Bool(
                haystack.as_bytes().starts_with(needle.as_bytes()),
            ))
        }
        (BuiltinIntrinsicKind::StrEndsWith, [Value::String(haystack), Value::String(needle)]) => {
            Some(Value::Bool(
                haystack.as_bytes().ends_with(needle.as_bytes()),
            ))
        }
        (BuiltinIntrinsicKind::StrToLower, [Value::String(string)]) => Some(Value::String(
            php_runtime::experimental::builtin_intrinsics::string_intrinsics::strtolower_ascii(string),
        )),
        (BuiltinIntrinsicKind::StrToUpper, [Value::String(string)]) => Some(Value::String(
            php_runtime::experimental::builtin_intrinsics::string_intrinsics::strtoupper_ascii(string),
        )),
        (
            BuiltinIntrinsicKind::StrReplaceScalar,
            [
                Value::String(search),
                Value::String(replace),
                Value::String(subject),
            ],
        ) => Some(Value::String(
            php_runtime::experimental::builtin_intrinsics::string_intrinsics::str_replace_scalar(search, replace, subject),
        )),
        (BuiltinIntrinsicKind::HtmlSpecialCharsDefault, [Value::String(string)]) => {
            Some(Value::String(
                php_runtime::experimental::builtin_intrinsics::string_intrinsics::htmlspecialchars_default(string),
            ))
        }
        (
            BuiltinIntrinsicKind::ExplodeSingleByte,
            [Value::String(separator), Value::String(subject)],
        ) if separator.len() == 1 => Some(Value::Array(
            php_runtime::experimental::builtin_intrinsics::string_intrinsics::explode_single_byte(
                separator.as_bytes()[0],
                subject,
            ),
        )),
        (
            BuiltinIntrinsicKind::SubstrBytes,
            [Value::String(string), Value::Int(offset), rest @ ..],
        ) => {
            let length = match rest {
                [] | [Value::Null] => None,
                [Value::Int(length)] => Some(*length),
                _ => return None,
            };
            Some(Value::String(
                php_runtime::experimental::builtin_intrinsics::string_intrinsics::substr_bytes(string, *offset, length),
            ))
        }
        (BuiltinIntrinsicKind::TrimDefault, [Value::String(string)]) => Some(Value::String(
            php_runtime::experimental::builtin_intrinsics::string_intrinsics::trim_ascii_default(string),
        )),
        (
            BuiltinIntrinsicKind::InArrayStrict,
            [
                needle @ (Value::Int(_) | Value::String(_)),
                Value::Array(haystack),
                Value::Bool(true),
            ],
        ) => {
            Some(Value::Bool(haystack.iter().any(|(_, value)| {
                php_runtime::api::identical(needle, value)
            })))
        }
        _ => None,
    }
}

fn builtin_intrinsic_spec(name: &str) -> Option<(usize, &'static BuiltinIntrinsicSpec)> {
    BUILTIN_INTRINSICS
        .iter()
        .enumerate()
        .find(|(_, spec)| spec.name == name)
}

/// Memoized per-spec result of `builtin_intrinsic_metadata_matches`.
///
/// The check compares the static intrinsic spec against the static generated
/// arginfo table (a linear scan over ~2.3k entries), so recomputing it on
/// every intrinsic builtin call dominated the fast-stub dispatch cost.
fn builtin_intrinsic_metadata_matches_cached(spec_index: usize) -> bool {
    static RESULTS: std::sync::LazyLock<Vec<bool>> = std::sync::LazyLock::new(|| {
        BUILTIN_INTRINSICS
            .iter()
            .map(builtin_intrinsic_metadata_matches)
            .collect()
    });
    RESULTS[spec_index]
}

fn fast_builtin_stub_fallback_reason_for_spec(
    spec_index: usize,
    spec: &'static BuiltinIntrinsicSpec,
    values: &[Value],
) -> Option<&'static str> {
    if !builtin_intrinsic_metadata_matches_cached(spec_index) {
        return Some("metadata");
    }
    if values.len() < spec.min_arity || values.len() > spec.exact_arity {
        return Some("arity");
    }
    if values
        .iter()
        .any(|value| matches!(value, Value::Reference(_)))
    {
        return Some("by_ref");
    }
    builtin_intrinsic_type_fallback(spec.kind, values)
}

fn builtin_intrinsic_metadata_matches(spec: &BuiltinIntrinsicSpec) -> bool {
    let Some(metadata) = php_std::arginfo::function_metadata_indexed(spec.name) else {
        return false;
    };
    if metadata.return_type != spec.return_type || metadata.params.len() != spec.params.len() {
        return false;
    }
    metadata
        .params
        .iter()
        .zip(spec.params.iter())
        .all(|(actual, expected)| {
            actual.name == expected.name
                && actual.type_decl == expected.type_decl
                && actual.optional == expected.optional
                && actual.by_ref == expected.by_ref
                && !actual.variadic
        })
}

/// `None` when the argument shape fits the intrinsic fast path; otherwise the
/// per-builtin fallback reason recorded in the counters.
fn builtin_intrinsic_type_fallback(
    kind: BuiltinIntrinsicKind,
    values: &[Value],
) -> Option<&'static str> {
    match (kind, values) {
        (BuiltinIntrinsicKind::ArrayKeyExists, [key, Value::Array(_)]) => {
            if ArrayKey::from_value(key).is_some() {
                None
            } else {
                Some("type")
            }
        }
        (BuiltinIntrinsicKind::CountArray, [Value::Array(_)])
        | (
            BuiltinIntrinsicKind::StrLen
            | BuiltinIntrinsicKind::StrToLower
            | BuiltinIntrinsicKind::StrToUpper
            | BuiltinIntrinsicKind::HtmlSpecialCharsDefault,
            [Value::String(_)],
        )
        | (
            BuiltinIntrinsicKind::StrContains
            | BuiltinIntrinsicKind::StrStartsWith
            | BuiltinIntrinsicKind::StrEndsWith,
            [Value::String(_), Value::String(_)],
        )
        | (
            BuiltinIntrinsicKind::IsArray
            | BuiltinIntrinsicKind::IsBool
            | BuiltinIntrinsicKind::IsFloat
            | BuiltinIntrinsicKind::IsInt
            | BuiltinIntrinsicKind::IsNull
            | BuiltinIntrinsicKind::IsNumeric
            | BuiltinIntrinsicKind::IsObject
            | BuiltinIntrinsicKind::IsScalar
            | BuiltinIntrinsicKind::IsString,
            [_],
        )
        | (
            BuiltinIntrinsicKind::PointerCurrent
            | BuiltinIntrinsicKind::PointerKey
            | BuiltinIntrinsicKind::ArrayKeysAll,
            [Value::Array(_)],
        ) => None,
        (BuiltinIntrinsicKind::ImplodeStringParts, [Value::String(_), Value::Array(parts)]) => {
            if parts
                .iter()
                .all(|(_, value)| matches!(value, Value::String(_)))
            {
                None
            } else {
                Some("non_string_parts")
            }
        }
        (BuiltinIntrinsicKind::ExplodeSingleByte, [Value::String(separator), Value::String(_)]) => {
            match separator.len() {
                0 => Some("empty_separator"),
                1 => None,
                _ => Some("multi_byte_separator"),
            }
        }
        (
            BuiltinIntrinsicKind::StrReplaceScalar,
            [Value::String(_), Value::String(_), Value::String(_)],
        ) => None,
        (
            BuiltinIntrinsicKind::SubstrBytes,
            [Value::String(_), Value::Int(_)]
            | [Value::String(_), Value::Int(_), Value::Int(_) | Value::Null],
        ) => None,
        (BuiltinIntrinsicKind::TrimDefault, [Value::String(_)]) => None,
        (
            BuiltinIntrinsicKind::InArrayStrict,
            [
                Value::Int(_) | Value::String(_),
                Value::Array(_),
                Value::Bool(strict),
            ],
        ) => {
            if *strict {
                None
            } else {
                Some("loose_comparison")
            }
        }
        (BuiltinIntrinsicKind::StrReplaceScalar, [search, replace, subject]) => {
            if [search, replace, subject]
                .iter()
                .any(|value| matches!(value, Value::Array(_)))
            {
                Some("array_args")
            } else {
                Some("type")
            }
        }
        _ => Some("type"),
    }
}

fn byte_slice_contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty()
        || haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

pub(super) fn try_execute_simple_literal_pcre_builtin(
    name: &str,
    values: &[Value],
    state: &mut ExecutionState,
) -> Option<VmResult> {
    let result = match name {
        "preg_match" => try_execute_simple_literal_preg_match(values)?,
        "preg_replace" => try_execute_simple_literal_preg_replace(values)?,
        "preg_split" => try_execute_simple_literal_preg_split(values)?,
        "preg_grep" => try_execute_simple_literal_preg_grep(values)?,
        _ => return None,
    };
    state.builtins.pcre_state_mut().last_error_mut().clear();
    Some(VmResult::success_no_output(Some(result)))
}

fn simple_literal_pcre_pattern_value(
    value: &Value,
) -> Option<php_runtime::experimental::pcre::SimpleLiteralPattern> {
    let Value::String(pattern) = value else {
        return None;
    };
    php_runtime::experimental::pcre::simple_literal_pattern(pattern)
        .ok()
        .flatten()
}

fn simple_literal_string_value(value: &Value) -> Option<&PhpString> {
    match value {
        Value::String(value) => Some(value),
        _ => None,
    }
}

fn simple_literal_int_value(value: Option<&Value>, default: i64) -> Option<i64> {
    match value {
        Some(Value::Int(value)) => Some(*value),
        Some(_) => None,
        None => Some(default),
    }
}

fn simple_literal_find(haystack: &[u8], needle: &[u8], start: usize) -> Option<(usize, usize)> {
    if needle.is_empty() || start > haystack.len() {
        return None;
    }
    let last_start = haystack.len().checked_sub(needle.len())?;
    let first = needle[0];
    let mut index = start;
    while index <= last_start {
        if haystack[index] == first && &haystack[index..index + needle.len()] == needle {
            return Some((index, index + needle.len()));
        }
        index += 1;
    }
    None
}

fn try_execute_simple_literal_preg_match(values: &[Value]) -> Option<Value> {
    if values.len() != 2 {
        return None;
    }
    let literal = simple_literal_pcre_pattern_value(&values[0])?;
    let subject = simple_literal_string_value(&values[1])?;
    Some(Value::Int(
        simple_literal_find(subject.as_bytes(), literal.as_bytes(), 0)
            .is_some()
            .into(),
    ))
}

fn try_execute_simple_literal_preg_replace(values: &[Value]) -> Option<Value> {
    if values.len() < 3 || values.len() > 5 {
        return None;
    }
    let literal = simple_literal_pcre_pattern_value(&values[0])?;
    let replacement = simple_literal_string_value(&values[1])?;
    if replacement
        .as_bytes()
        .iter()
        .any(|byte| matches!(*byte, b'$' | b'\\'))
    {
        return None;
    }
    let subject = simple_literal_string_value(&values[2])?;
    let limit = simple_literal_int_value(values.get(3), -1)?;
    let mut count = 0i64;
    let replaced = simple_literal_replace(
        subject.as_bytes(),
        literal.as_bytes(),
        replacement.as_bytes(),
        limit,
        &mut count,
    );
    if let Some(Value::Reference(cell)) = values.get(4) {
        cell.set(Value::Int(count));
    }
    Some(Value::string(replaced))
}

fn simple_literal_replace(
    subject: &[u8],
    needle: &[u8],
    replacement: &[u8],
    limit: i64,
    count: &mut i64,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(subject.len());
    let mut cursor = 0usize;
    loop {
        if limit >= 0 && *count >= limit {
            break;
        }
        let Some((start, end)) = simple_literal_find(subject, needle, cursor) else {
            break;
        };
        output.extend_from_slice(&subject[cursor..start]);
        output.extend_from_slice(replacement);
        cursor = end;
        *count += 1;
    }
    output.extend_from_slice(&subject[cursor..]);
    output
}

fn try_execute_simple_literal_preg_split(values: &[Value]) -> Option<Value> {
    if values.len() < 2 || values.len() > 4 {
        return None;
    }
    let literal = simple_literal_pcre_pattern_value(&values[0])?;
    let subject = simple_literal_string_value(&values[1])?;
    let limit = simple_literal_int_value(values.get(2), -1)?;
    let flags = simple_literal_int_value(values.get(3), 0)?;
    if flags & php_runtime::experimental::pcre::PREG_SPLIT_DELIM_CAPTURE != 0 {
        return None;
    }
    let mut pieces = PhpArray::new();
    let mut last_end = 0usize;
    let mut emitted = 0i64;
    while last_end <= subject.as_bytes().len() {
        if limit > 0 && emitted >= limit - 1 {
            break;
        }
        let Some((start, end)) =
            simple_literal_find(subject.as_bytes(), literal.as_bytes(), last_end)
        else {
            break;
        };
        simple_literal_append_split_piece(
            &mut pieces,
            &subject.as_bytes()[last_end..start],
            last_end,
            flags,
        );
        last_end = end;
        emitted += 1;
    }
    simple_literal_append_split_piece(
        &mut pieces,
        &subject.as_bytes()[last_end..],
        last_end,
        flags,
    );
    Some(Value::Array(pieces))
}

fn simple_literal_append_split_piece(
    pieces: &mut PhpArray,
    bytes: &[u8],
    offset: usize,
    flags: i64,
) {
    if flags & php_runtime::experimental::pcre::PREG_SPLIT_NO_EMPTY != 0 && bytes.is_empty() {
        return;
    }
    let value = if flags & php_runtime::experimental::pcre::PREG_SPLIT_OFFSET_CAPTURE != 0 {
        Value::packed_array(vec![
            Value::string(bytes.to_vec()),
            Value::Int(offset as i64),
        ])
    } else {
        Value::string(bytes.to_vec())
    };
    pieces.insert(ArrayKey::Int(pieces.len() as i64), value);
}

fn try_execute_simple_literal_preg_grep(values: &[Value]) -> Option<Value> {
    if values.len() < 2 || values.len() > 3 {
        return None;
    }
    let literal = simple_literal_pcre_pattern_value(&values[0])?;
    let Value::Array(input) = &values[1] else {
        return None;
    };
    let flags = simple_literal_int_value(values.get(2), 0)?;
    let invert = flags & php_runtime::experimental::pcre::PREG_GREP_INVERT != 0;
    let mut output = PhpArray::new();
    for (key, value) in input.iter() {
        let Value::String(text) = value else {
            return None;
        };
        let is_match = simple_literal_find(text.as_bytes(), literal.as_bytes(), 0).is_some();
        if is_match != invert {
            output.insert(key.clone(), value.clone());
        }
    }
    Some(Value::Array(output))
}
