//! Exact runtime operations over authoritative native encodings.
//!
//! These fixed ABIs expose no cold coordinator or compatibility value plane.

use super::{
    NativeComparisonTraversal, NativeComparisonValue, NativeFixedCallablePlan, NativeLastError,
    NativePreparedClosure, NativeRequestFastState, PreparedNativeBinaryThrowableSites,
    PreparedNativeCountThrowableSites, PreparedNativeRuntimeClass,
    PreparedNativeStaticPropertyContract, PreparedNativeThrowableSite,
    PreparedNativeUndefinedConstantContract, native_reference_state,
};
use std::fmt::{self, Write};
use std::sync::Arc;

/// Concrete shared-ABI implementation of the fixed `strlen` callable. This
/// is the builtin body itself over the authoritative native string view; it
/// does not enter a builtin registry, generic dispatcher, or Rust `Value`
/// plane.
#[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
pub(crate) unsafe extern "C" fn jit_native_strlen_php_entry(
    runtime: *mut std::ffi::c_void,
    arguments: *const i64,
    _transition_out: *mut php_jit::JitDeoptState,
    _resume_id: i32,
    _resume_state: *const php_jit::JitDeoptState,
) -> php_jit::JitNativeControlResult {
    if runtime.is_null() || arguments.is_null() {
        return php_jit::JitNativeControlResult::control(
            php_jit::JitCallStatus::ABI_MISMATCH,
            0,
            0,
        );
    }
    let fast = unsafe { &mut *(runtime.cast::<NativeRequestFastState>()) };
    let argument = unsafe { *arguments };
    let Some(length) = fast
        .native_string_view(argument)
        .and_then(|bytes| i64::try_from(bytes.len()).ok())
    else {
        return php_jit::JitNativeControlResult::control(
            php_jit::JitCallStatus::RUNTIME_ERROR,
            0,
            0,
        );
    };
    match fast.publish_direct_int(length) {
        Ok(length) => php_jit::JitNativeControlResult::returning(length),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

#[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
unsafe fn native_unary_predicate_argument<'a>(
    runtime: *mut std::ffi::c_void,
    arguments: *const i64,
) -> Option<(&'a mut NativeRequestFastState, i64)> {
    if runtime.is_null() || arguments.is_null() {
        return None;
    }
    Some((
        unsafe { &mut *(runtime.cast::<NativeRequestFastState>()) },
        unsafe { *arguments },
    ))
}

/// Concrete shared-ABI implementation of `is_string` for callback values.
#[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
pub(crate) unsafe extern "C" fn jit_native_is_string_php_entry(
    runtime: *mut std::ffi::c_void,
    arguments: *const i64,
    _transition_out: *mut php_jit::JitDeoptState,
    _resume_id: i32,
    _resume_state: *const php_jit::JitDeoptState,
) -> php_jit::JitNativeControlResult {
    let Some((fast, argument)) = (unsafe { native_unary_predicate_argument(runtime, arguments) })
    else {
        return native_runtime_contract_violation();
    };
    native_compound_comparison_bool(fast.native_string_view(argument).is_some())
}

/// Concrete shared-ABI implementation of `is_int` and its aliases.
#[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
pub(crate) unsafe extern "C" fn jit_native_is_int_php_entry(
    runtime: *mut std::ffi::c_void,
    arguments: *const i64,
    _transition_out: *mut php_jit::JitDeoptState,
    _resume_id: i32,
    _resume_state: *const php_jit::JitDeoptState,
) -> php_jit::JitNativeControlResult {
    let Some((fast, argument)) = (unsafe { native_unary_predicate_argument(runtime, arguments) })
    else {
        return native_runtime_contract_violation();
    };
    native_compound_comparison_bool(fast.native_value_is_int(argument))
}

/// Concrete shared-ABI implementation of `is_scalar`.
#[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
pub(crate) unsafe extern "C" fn jit_native_is_scalar_php_entry(
    runtime: *mut std::ffi::c_void,
    arguments: *const i64,
    _transition_out: *mut php_jit::JitDeoptState,
    _resume_id: i32,
    _resume_state: *const php_jit::JitDeoptState,
) -> php_jit::JitNativeControlResult {
    let Some((fast, argument)) = (unsafe { native_unary_predicate_argument(runtime, arguments) })
    else {
        return native_runtime_contract_violation();
    };
    native_compound_comparison_bool(matches!(
        fast.native_comparison_value(argument),
        Some(
            NativeComparisonValue::Bool(_)
                | NativeComparisonValue::Int(_)
                | NativeComparisonValue::Float(_)
                | NativeComparisonValue::String(_)
        )
    ))
}

/// Concrete shared-ABI implementation of `is_numeric`.
#[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
pub(crate) unsafe extern "C" fn jit_native_is_numeric_php_entry(
    runtime: *mut std::ffi::c_void,
    arguments: *const i64,
    _transition_out: *mut php_jit::JitDeoptState,
    _resume_id: i32,
    _resume_state: *const php_jit::JitDeoptState,
) -> php_jit::JitNativeControlResult {
    let Some((fast, argument)) = (unsafe { native_unary_predicate_argument(runtime, arguments) })
    else {
        return native_runtime_contract_violation();
    };
    let numeric = match fast.native_comparison_value(argument) {
        Some(NativeComparisonValue::Int(_) | NativeComparisonValue::Float(_)) => true,
        Some(NativeComparisonValue::String(bytes)) => matches!(
            php_runtime::experimental::numeric_string::classify(bytes).kind,
            php_runtime::experimental::numeric_string::NumericStringKind::IntString
                | php_runtime::experimental::numeric_string::NumericStringKind::FloatString
        ),
        _ => false,
    };
    native_compound_comparison_bool(numeric)
}

/// Concrete shared-ABI implementation of one-argument `intval` for callback
/// values. The caller publishes its exact function/continuation identity in
/// `transition_out`, allowing the object-conversion warning to retain the PHP
/// source location without routing through builtin dispatch.
#[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
pub(crate) unsafe extern "C" fn jit_native_intval_php_entry(
    runtime: *mut std::ffi::c_void,
    arguments: *const i64,
    transition_out: *mut php_jit::JitDeoptState,
    _resume_id: i32,
    _resume_state: *const php_jit::JitDeoptState,
) -> php_jit::JitNativeControlResult {
    let Some((fast, argument)) = (unsafe { native_unary_predicate_argument(runtime, arguments) })
    else {
        return native_runtime_contract_violation();
    };
    let object_name = match fast.native_comparison_value(argument) {
        Some(NativeComparisonValue::Object(object)) => Some(object.owner.display_name().to_owned()),
        Some(NativeComparisonValue::OpaqueIdentity(_)) => Some("object".to_owned()),
        _ => None,
    };
    if let Some(object_name) = object_name {
        if transition_out.is_null() {
            return native_runtime_contract_violation();
        }
        let state = unsafe { &*transition_out };
        let Some(compiled) = (unsafe { fast.symbol_query.active_compiled.as_ref() }) else {
            return native_runtime_contract_violation();
        };
        let Some(instruction) = compiled
            .prepared_continuation_instructions(php_ir::FunctionId::new(state.function_id))
            .and_then(|instructions| {
                instructions
                    .get(state.continuation_id as usize)
                    .cloned()
                    .flatten()
            })
        else {
            return native_runtime_contract_violation();
        };
        if emit_exact_native_structured_warning(
            fast,
            "E_PHP_RUNTIME_OBJECT_NUMERIC_CAST_WARNING",
            format!("Object of class {object_name} could not be converted to int"),
            i64::from(instruction.span.file.raw()),
            i64::from(instruction.span.start),
        ) != 0
        {
            return php_jit::JitNativeControlResult::control(
                php_jit::JitCallStatus::RUNTIME_ERROR,
                0,
                0,
            );
        }
        return match fast.publish_direct_int(1) {
            Ok(value) => php_jit::JitNativeControlResult::returning(value),
            Err(_) => php_jit::JitNativeControlResult::control(
                php_jit::JitCallStatus::RUNTIME_ERROR,
                0,
                0,
            ),
        };
    }
    let Some(value) = native_int_cast_value(fast, argument) else {
        return native_runtime_contract_violation();
    };
    publish_native_int_cast(fast, value)
}

#[allow(unsafe_code)] // Safety: the generated entry publishes one valid source transition.
unsafe fn native_php_entry_source(
    fast: &NativeRequestFastState,
    transition_out: *mut php_jit::JitDeoptState,
) -> Option<(i64, i64)> {
    let state = unsafe { transition_out.as_ref() }?;
    let compiled = unsafe { fast.symbol_query.active_compiled.as_ref() }?;
    let instructions =
        compiled.prepared_continuation_instructions(php_ir::FunctionId::new(state.function_id))?;
    let instruction = instructions.get(state.continuation_id as usize)?.as_ref()?;
    Some((
        i64::from(instruction.span.file.raw()),
        i64::from(instruction.span.start),
    ))
}

fn native_trim_char_mask<const NAME: u8>(
    fast: &mut NativeRequestFastState,
    charlist: &[u8],
    source: Option<(i64, i64)>,
) -> Result<[bool; 256], ()> {
    let mut mask = [false; 256];
    let mut index = 0;
    while index < charlist.len() {
        let current = charlist[index];
        if index + 3 < charlist.len()
            && charlist[index + 1] == b'.'
            && charlist[index + 2] == b'.'
            && charlist[index + 3] >= current
        {
            mask[usize::from(current)..=usize::from(charlist[index + 3])].fill(true);
            index += 4;
            continue;
        }
        if index + 1 < charlist.len() && current == b'.' && charlist[index + 1] == b'.' {
            let detail = if index == 0 {
                "Invalid '..'-range, no character to the left of '..'"
            } else if index + 2 >= charlist.len() {
                "Invalid '..'-range, no character to the right of '..'"
            } else if charlist[index - 1] > charlist[index + 2] {
                "Invalid '..'-range, '..'-range needs to be incrementing"
            } else {
                "Invalid '..'-range"
            };
            let function = match NAME {
                1 => "trim",
                2 => "ltrim",
                3 => "rtrim",
                _ => return Err(()),
            };
            let Some((file, start)) = source else {
                return Err(());
            };
            if emit_exact_native_warning(fast, format!("{function}(): {detail}"), file, start) != 0
            {
                return Err(());
            }
            index += 1;
            continue;
        }
        mask[usize::from(current)] = true;
        index += 1;
    }
    Ok(mask)
}

/// Fixed shared-ABI implementation of the two-argument trim family. Each
/// exported entry fixes `MODE` and `NAME`; no builtin selector crosses the
/// generated boundary, and both arguments stay authoritative native strings.
#[allow(unsafe_code)] // Safety: generated fixed-builtin calls publish two native arguments.
unsafe fn native_trim_charlist_php_entry<const MODE: u8, const NAME: u8>(
    runtime: *mut std::ffi::c_void,
    arguments: *const i64,
    transition_out: *mut php_jit::JitDeoptState,
) -> php_jit::JitNativeControlResult {
    if runtime.is_null() || arguments.is_null() {
        return native_runtime_contract_violation();
    }
    let fast = unsafe { &mut *(runtime.cast::<NativeRequestFastState>()) };
    let input = unsafe { *arguments };
    let charlist = unsafe { *arguments.add(1) };
    let Some(input) = fast.native_string_view(input).map(<[u8]>::to_vec) else {
        return native_runtime_contract_violation();
    };
    let Some(charlist) = fast.native_string_view(charlist).map(<[u8]>::to_vec) else {
        return native_runtime_contract_violation();
    };
    let source = unsafe { native_php_entry_source(fast, transition_out) };
    let mask = match charlist.as_slice() {
        [only] => {
            let mut mask = [false; 256];
            mask[usize::from(*only)] = true;
            mask
        }
        _ => match native_trim_char_mask::<NAME>(fast, &charlist, source) {
            Ok(mask) => mask,
            Err(()) => return native_runtime_contract_violation(),
        },
    };
    let mut start = 0;
    let mut end = input.len();
    if MODE & 1 != 0 {
        while start < end && mask[usize::from(input[start])] {
            start += 1;
        }
    }
    if MODE & 2 != 0 {
        while start < end && mask[usize::from(input[end - 1])] {
            end -= 1;
        }
    }
    match fast.publish_direct_string_bytes(&input[start..end]) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => native_runtime_contract_violation(),
    }
}

macro_rules! native_trim_php_entry {
    ($name:ident, $mode:literal, $identity:literal) => {
        #[allow(unsafe_code)]
        pub(crate) unsafe extern "C" fn $name(
            runtime: *mut std::ffi::c_void,
            arguments: *const i64,
            transition_out: *mut php_jit::JitDeoptState,
            _resume_id: i32,
            _resume_state: *const php_jit::JitDeoptState,
        ) -> php_jit::JitNativeControlResult {
            unsafe {
                native_trim_charlist_php_entry::<$mode, $identity>(
                    runtime,
                    arguments,
                    transition_out,
                )
            }
        }
    };
}

native_trim_php_entry!(jit_native_trim_php_entry, 3, 1);
native_trim_php_entry!(jit_native_ltrim_php_entry, 1, 2);
native_trim_php_entry!(jit_native_rtrim_php_entry, 2, 3);

/// Emits the exact diagnostic selected by a generated non-quiet local load.
/// The caller supplies immutable local-name bytes and source coordinates; no
/// local opcode, IR instruction, or value-plane conversion crosses this ABI.
#[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
pub(crate) extern "C" fn jit_native_undefined_variable_warning_abi(
    runtime: *mut NativeRequestFastState,
    name: *const u8,
    name_length: u64,
    file: i64,
    start: i64,
) -> i32 {
    if runtime.is_null() || (name.is_null() && name_length != 0) {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    let Ok(name_length) = usize::try_from(name_length) else {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    };
    let name = if name_length == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(name, name_length) }
    };
    let fast = unsafe { &mut *runtime };
    let name = String::from_utf8_lossy(name);
    let message = format!("Undefined variable ${name}");
    emit_exact_native_warning(fast, message, file, start)
}

/// Emits the exact diagnostic selected by a generated non-quiet direct-array
/// read. The normalized array key stays in the authoritative native encoding;
/// no Rust `Value` or generic dimension operation crosses this ABI.
#[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
pub(crate) extern "C" fn jit_native_undefined_array_key_warning_abi(
    runtime: *mut NativeRequestFastState,
    key: i64,
    file: i64,
    start: i64,
) -> i32 {
    if runtime.is_null() {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    let fast = unsafe { &mut *runtime };
    let missing_key = match fast.native_comparison_value(key) {
        Some(NativeComparisonValue::Int(key)) => key.to_string(),
        Some(NativeComparisonValue::String(key)) => {
            format!("\"{}\"", String::from_utf8_lossy(key))
        }
        _ => return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
    };
    emit_exact_native_structured_warning(
        fast,
        "E_PHP_RUNTIME_UNDEFINED_ARRAY_KEY_WARNING",
        format!("Undefined array key {missing_key}"),
        file,
        start,
    )
}

#[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
pub(crate) extern "C" fn jit_native_array_offset_warning_abi(
    runtime: *mut NativeRequestFastState,
    value: i64,
    file: i64,
    start: i64,
) -> i32 {
    if runtime.is_null() {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    let fast = unsafe { &mut *runtime };
    let type_name = if value == php_jit::jit_encode_constant(php_jit::JIT_VALUE_UNINITIALIZED) {
        "null"
    } else {
        match fast.native_comparison_value(value) {
            Some(NativeComparisonValue::Null) => "null",
            Some(NativeComparisonValue::Bool(_)) => "bool",
            Some(NativeComparisonValue::Int(_)) => "int",
            Some(NativeComparisonValue::Float(_)) => "float",
            Some(NativeComparisonValue::String(_)) => "string",
            Some(NativeComparisonValue::Array { .. }) => "array",
            Some(NativeComparisonValue::Object(_)) => "object",
            Some(NativeComparisonValue::Resource(_)) => "resource",
            Some(NativeComparisonValue::OpaqueIdentity(_)) => "object",
            None => return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
        }
    };
    emit_exact_native_warning(
        fast,
        format!("Trying to access array offset on {type_name}"),
        file,
        start,
    )
}

/// Detaches one publication-proven top-level global binding. The generated
/// caller passes the canonical native reference directly; the cold service is
/// entered once to invalidate that identity and republish affected slots.
#[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
pub(crate) extern "C" fn jit_native_global_binding_unset_abi(
    runtime: *mut NativeRequestFastState,
    reference: i64,
) -> i32 {
    if runtime.is_null() {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    let fast = unsafe { &mut *runtime };
    match crate::vm::jit_abi::unset_native_global_binding(
        fast.global_binding.cold_context,
        reference,
    ) {
        Ok(true) => 0,
        Ok(false) | Err(_) => php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
    }
}

/// Rebinds one publication-proven top-level global to an authoritative native
/// reference and republishes every affected trusted slot once.
#[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
pub(crate) extern "C" fn jit_native_global_binding_rebind_abi(
    runtime: *mut NativeRequestFastState,
    destination: i64,
    source: i64,
) -> i32 {
    if runtime.is_null() {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    let fast = unsafe { &mut *runtime };
    match crate::vm::jit_abi::rebind_native_global_binding(
        fast.global_binding.cold_context,
        destination,
        source,
    ) {
        Ok(true) => 0,
        Ok(false) | Err(_) => php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
    }
}

#[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
pub(super) fn emit_exact_native_structured_warning(
    fast: &mut NativeRequestFastState,
    diagnostic_id: &'static str,
    message: String,
    file: i64,
    start: i64,
) -> i32 {
    let compiled = unsafe { fast.symbol_query.active_compiled.as_ref() };
    let path = usize::try_from(file)
        .ok()
        .and_then(|index| compiled.and_then(|compiled| compiled.unit().files.get(index)))
        .map_or_else(|| "<unknown>".to_owned(), |entry| entry.path.clone());
    let offset = u32::try_from(start).unwrap_or(0);
    let span = php_runtime::api::RuntimeSourceSpan {
        file: Some(path.clone()),
        start: offset,
        end: offset,
    };
    let ir_span = php_ir::IrSpan::new(
        php_ir::FileId::new(u32::try_from(file).unwrap_or(u32::MAX)),
        offset,
        offset,
    );
    let line = compiled
        .and_then(|compiled| compiled.source_display_line(ir_span, false))
        .and_then(|line| usize::try_from(line).ok())
        .unwrap_or(1);
    if let Some(last_error) = unsafe { fast.last_error.as_mut() } {
        *last_error = Some(NativeLastError {
            error_type: 2,
            message: message.clone(),
            file: path,
            line,
        });
    }
    let reporting = unsafe { fast.configuration.error_reporting.as_ref() }
        .copied()
        .unwrap_or(-1);
    if reporting & 2 == 0 {
        return 0;
    }
    let Some(diagnostic) = (unsafe { fast.runtime_diagnostic.diagnostic.as_mut() }) else {
        return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
    };
    *diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
        diagnostic_id,
        php_runtime::api::RuntimeSeverity::Warning,
        message,
        span,
        Vec::new(),
        Some(php_runtime::api::PhpReferenceClassification::Warning),
    ));
    0
}

#[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
fn emit_exact_native_warning(
    fast: &mut NativeRequestFastState,
    message: String,
    file: i64,
    start: i64,
) -> i32 {
    emit_exact_native_diagnostic(fast, 2, message, file, start)
}

#[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
pub(super) fn emit_exact_native_diagnostic(
    fast: &mut NativeRequestFastState,
    error_type: i64,
    message: String,
    file: i64,
    start: i64,
) -> i32 {
    let compiled = unsafe { fast.symbol_query.active_compiled.as_ref() };
    let file_index = usize::try_from(file).ok();
    let path = compiled
        .and_then(|compiled| file_index.and_then(|index| compiled.unit().files.get(index)))
        .map_or_else(|| "<unknown>".to_owned(), |entry| entry.path.clone());
    let offset = u32::try_from(start).unwrap_or(0);
    let span = php_ir::IrSpan::new(
        php_ir::FileId::new(u32::try_from(file).unwrap_or(u32::MAX)),
        offset,
        offset,
    );
    let line = compiled
        .and_then(|compiled| compiled.source_display_line(span, false))
        .and_then(|line| usize::try_from(line).ok())
        .unwrap_or(1);
    if let Some(last_error) = unsafe { fast.last_error.as_mut() } {
        *last_error = Some(NativeLastError {
            error_type,
            message: message.clone(),
            file: path.clone(),
            line,
        });
    }
    let reporting = unsafe { fast.configuration.error_reporting.as_ref() }
        .copied()
        .unwrap_or(-1);
    let display = unsafe { fast.configuration.display_errors.as_ref() }
        .copied()
        .unwrap_or(true);
    if reporting & error_type != 0 && display {
        let Some(output) = (unsafe { fast.output.as_mut() }) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let label = if error_type == 8192 {
            "Deprecated"
        } else {
            "Warning"
        };
        output.write_bytes(format!("\n{label}: {message} in {path} on line {line}\n"));
    }
    0
}

/// Appends an already-typed native string without recovering the cold runtime
/// coordinator or materializing a compatibility graph.
#[allow(unsafe_code)] // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
pub(crate) extern "C" fn jit_native_echo_bytes_abi(
    runtime: *mut NativeRequestFastState,
    bytes: *const u8,
    length: u64,
) {
    if runtime.is_null() || (bytes.is_null() && length != 0) {
        return;
    }
    let Ok(length) = usize::try_from(length) else {
        return;
    };
    // SAFETY: the request owner publishes a stable output pointer for the
    // activation lifetime; the optimizing string descriptor supplies an
    // immutable byte range of exactly `length` bytes for this call.
    let output = unsafe { (*runtime).output.as_mut() };
    let Some(output) = output else {
        return;
    };
    let bytes = if length == 0 {
        &[]
    } else {
        // SAFETY: validated non-null above; descriptor publication owns the
        // backing direct-string bytes for the duration of this synchronous call.
        unsafe { std::slice::from_raw_parts(bytes, length) }
    };
    output.write_fast_bytes(bytes);
}

/// Formats one SSA-proven PHP float and publishes the result directly in the
/// authoritative native string arena, without constructing a compatibility graph.
pub(crate) extern "C" fn jit_native_float_to_string_abi(
    runtime: *mut NativeRequestFastState,
    value: f64,
) -> php_jit::JitNativeControlResult {
    let rendered = NativeInlineString::from_float(value);
    // SAFETY: generated exact handlers receive the active request's stable
    // fast-state pointer. The direct publisher does not inspect cold state.
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    match unsafe { &mut *runtime }.publish_direct_string_with(rendered.length, |output| {
        output.copy_from_slice(rendered.as_bytes());
    }) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Classifies one authoritative native string with PHP's exact shared
/// numeric-string parser. This is a pure compiled handler: it receives no
/// request state, capability, operation ID, or compatibility graph.
///
/// # Safety contract
///
/// Generated code supplies the immutable byte range published by the direct
/// string descriptor. The descriptor remains live for this synchronous call.
#[allow(unsafe_code)] // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
pub(crate) extern "C" fn jit_native_numeric_string_abi(
    bytes: *const u8,
    length: u64,
) -> php_jit::JitNativeNumericStringResult {
    let Ok(length) = usize::try_from(length) else {
        return php_jit::JitNativeNumericStringResult::default();
    };
    let bytes = if length == 0 {
        &[]
    } else {
        // SAFETY: guaranteed by the published native descriptor contract
        // documented above.
        unsafe { std::slice::from_raw_parts(bytes, length) }
    };
    let classified = php_runtime::experimental::numeric_string::classify(bytes);
    use php_runtime::experimental::numeric_string::{NumericStringKind, NumericStringValue};
    match (classified.kind, classified.value) {
        (
            NumericStringKind::IntString | NumericStringKind::FloatString,
            Some(NumericStringValue::Int(value)),
        ) => php_jit::JitNativeNumericStringResult {
            kind: php_jit::JIT_NATIVE_NUMERIC_STRING_INT,
            payload: value as u64,
        },
        (
            NumericStringKind::IntString | NumericStringKind::FloatString,
            Some(NumericStringValue::Float(value)),
        ) => php_jit::JitNativeNumericStringResult {
            kind: php_jit::JIT_NATIVE_NUMERIC_STRING_FLOAT,
            payload: value.to_bits(),
        },
        (NumericStringKind::LeadingNumeric, Some(NumericStringValue::Int(value))) => {
            php_jit::JitNativeNumericStringResult {
                kind: php_jit::JIT_NATIVE_NUMERIC_STRING_LEADING_INT,
                payload: value as u64,
            }
        }
        (NumericStringKind::LeadingNumeric, Some(NumericStringValue::Float(value))) => {
            php_jit::JitNativeNumericStringResult {
                kind: php_jit::JIT_NATIVE_NUMERIC_STRING_LEADING_FLOAT,
                payload: value.to_bits(),
            }
        }
        _ => php_jit::JitNativeNumericStringResult {
            kind: php_jit::JIT_NATIVE_NUMERIC_STRING_NON_NUMERIC,
            payload: 0,
        },
    }
}

/// Computes PHP `fmod`'s IEEE floating-point remainder without request state,
/// an operation selector, or compatibility graph conversion.
pub(crate) extern "C" fn jit_native_fmod_f64_abi(dividend: f64, divisor: f64) -> f64 {
    dividend % divisor
}

/// Applies PHP's validated rounding mode without request state, compatibility
/// conversion, or a runtime operation selector.
pub(crate) extern "C" fn jit_native_round_f64_abi(value: f64, precision: i64, mode: i64) -> f64 {
    php_runtime::api::native_round_f64(value, precision, mode)
}

macro_rules! native_unary_math_abi {
    ($name:ident, $method:ident) => {
        pub(crate) extern "C" fn $name(value: f64) -> f64 {
            value.$method()
        }
    };
}

native_unary_math_abi!(jit_native_acos_f64_abi, acos);
native_unary_math_abi!(jit_native_acosh_f64_abi, acosh);
native_unary_math_abi!(jit_native_asin_f64_abi, asin);
native_unary_math_abi!(jit_native_asinh_f64_abi, asinh);
native_unary_math_abi!(jit_native_atan_f64_abi, atan);
native_unary_math_abi!(jit_native_atanh_f64_abi, atanh);
native_unary_math_abi!(jit_native_cos_f64_abi, cos);
native_unary_math_abi!(jit_native_cosh_f64_abi, cosh);
native_unary_math_abi!(jit_native_exp_f64_abi, exp);
native_unary_math_abi!(jit_native_expm1_f64_abi, exp_m1);
native_unary_math_abi!(jit_native_log_f64_abi, ln);
native_unary_math_abi!(jit_native_log10_f64_abi, log10);
native_unary_math_abi!(jit_native_log1p_f64_abi, ln_1p);
native_unary_math_abi!(jit_native_sin_f64_abi, sin);
native_unary_math_abi!(jit_native_sinh_f64_abi, sinh);
native_unary_math_abi!(jit_native_tan_f64_abi, tan);
native_unary_math_abi!(jit_native_tanh_f64_abi, tanh);

pub(crate) extern "C" fn jit_native_atan2_f64_abi(left: f64, right: f64) -> f64 {
    left.atan2(right)
}

pub(crate) extern "C" fn jit_native_deg2rad_f64_abi(value: f64) -> f64 {
    (value / 180.0) * std::f64::consts::PI
}

pub(crate) extern "C" fn jit_native_fpow_f64_abi(base: f64, exponent: f64) -> f64 {
    base.powf(exponent)
}

pub(crate) extern "C" fn jit_native_hypot_f64_abi(left: f64, right: f64) -> f64 {
    left.hypot(right)
}

pub(crate) extern "C" fn jit_native_rad2deg_f64_abi(value: f64) -> f64 {
    (value / std::f64::consts::PI) * 180.0
}

fn native_runtime_contract_violation() -> php_jit::JitNativeControlResult {
    php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::ABI_MISMATCH, 0, 0)
}

fn native_compound_comparison_bool(value: bool) -> php_jit::JitNativeControlResult {
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(if value {
        php_jit::JIT_VALUE_TRUE
    } else {
        php_jit::JIT_VALUE_FALSE
    }))
}

fn native_exact_comparison_order(
    fast: &NativeRequestFastState,
    left: i64,
    right: i64,
) -> Option<(std::cmp::Ordering, bool)> {
    let mut traversal = NativeComparisonTraversal::default();
    let ordering = fast.native_values_compare(left, right, &mut traversal)?;
    Some((ordering, traversal.unordered))
}

/// Exact strict identity over the authoritative native value graph.
pub(crate) extern "C" fn jit_native_identical_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &*runtime };
    fast.native_values_identical(left, right, &mut NativeComparisonTraversal::default())
        .map_or_else(
            native_runtime_contract_violation,
            native_compound_comparison_bool,
        )
}

/// Exact strict non-identity over the authoritative native value graph.
pub(crate) extern "C" fn jit_native_not_identical_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &*runtime };
    fast.native_values_identical(left, right, &mut NativeComparisonTraversal::default())
        .map(|identical| !identical)
        .map_or_else(
            native_runtime_contract_violation,
            native_compound_comparison_bool,
        )
}

/// Exact loose equality over the authoritative native value graph.
pub(crate) extern "C" fn jit_native_equal_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &*runtime };
    fast.native_values_equal(left, right, &mut NativeComparisonTraversal::default())
        .map_or_else(
            native_runtime_contract_violation,
            native_compound_comparison_bool,
        )
}

/// Exact loose inequality over the authoritative native value graph.
pub(crate) extern "C" fn jit_native_not_equal_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &*runtime };
    fast.native_values_equal(left, right, &mut NativeComparisonTraversal::default())
        .map(|equal| !equal)
        .map_or_else(
            native_runtime_contract_violation,
            native_compound_comparison_bool,
        )
}

/// Exact PHP less-than comparison over the authoritative native value graph.
pub(crate) extern "C" fn jit_native_less_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &*runtime };
    let Some((ordering, unordered)) = native_exact_comparison_order(fast, left, right) else {
        return native_runtime_contract_violation();
    };
    native_compound_comparison_bool(!unordered && ordering.is_lt())
}

/// Exact PHP less-than-or-equal comparison over the native value graph.
pub(crate) extern "C" fn jit_native_less_equal_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &*runtime };
    let Some((ordering, unordered)) = native_exact_comparison_order(fast, left, right) else {
        return native_runtime_contract_violation();
    };
    native_compound_comparison_bool(!unordered && !ordering.is_gt())
}

/// Exact PHP greater-than comparison over the authoritative native value graph.
pub(crate) extern "C" fn jit_native_greater_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &*runtime };
    let Some((ordering, unordered)) = native_exact_comparison_order(fast, left, right) else {
        return native_runtime_contract_violation();
    };
    native_compound_comparison_bool(!unordered && ordering.is_gt())
}

/// Exact PHP greater-than-or-equal comparison over the native value graph.
pub(crate) extern "C" fn jit_native_greater_equal_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &*runtime };
    let Some((ordering, unordered)) = native_exact_comparison_order(fast, left, right) else {
        return native_runtime_contract_violation();
    };
    native_compound_comparison_bool(!unordered && !ordering.is_lt())
}

/// Exact PHP three-way comparison over the authoritative native value graph.
pub(crate) extern "C" fn jit_native_spaceship_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &*runtime };
    let Some((ordering, _)) = native_exact_comparison_order(fast, left, right) else {
        return native_runtime_contract_violation();
    };
    php_jit::JitNativeControlResult::returning(match ordering {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    })
}

/// Returns the already-published class name of one direct object, or the exact
/// `get_debug_type()` name of another authoritative native value, without
/// decoding a compatibility graph or entering generic property dispatch.
pub(crate) extern "C" fn jit_native_object_class_name_abi(
    runtime: *mut NativeRequestFastState,
    object: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: the generated call supplies one live direct object and the
    // request owner keeps both the fast state and slot-parallel owner alive.
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let Some(name) = (unsafe { &*runtime }).exact_type_name(object, true) else {
        return php_jit::JitNativeControlResult::control(
            php_jit::JitCallStatus::ABI_MISMATCH,
            0,
            0,
        );
    };
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    match (unsafe { &mut *runtime }).publish_direct_string_bytes(&name) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Returns the exact runtime type name for a representation-dynamic value.
/// Statically classified calls remain direct generated string constants.
pub(crate) extern "C" fn jit_native_type_name_abi(
    runtime: *mut NativeRequestFastState,
    value: i64,
    debug: i64,
) -> php_jit::JitNativeControlResult {
    if runtime.is_null() {
        return native_runtime_contract_violation();
    }
    // SAFETY: generated code passes the live request state and one borrowed
    // authoritative value for this synchronous exact query.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &mut *runtime };
    let Some(name) = fast.exact_type_name(value, debug != 0) else {
        return native_runtime_contract_violation();
    };
    match fast.publish_direct_string_bytes(&name) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Acquires one representation-complete callable directly from authoritative
/// native storage. Unsupported callable shapes request the instruction's one
/// baseline continuation; no generic operation dispatcher is entered.
pub(crate) extern "C" fn jit_native_acquire_callable_abi(
    runtime: *mut NativeRequestFastState,
    value: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: generated code passes the request-stable fast state and one
    // encoded value whose owner remains live for this synchronous call.
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    match (unsafe { &mut *runtime }).acquire_direct_callable(value) {
        Ok(Some(callable)) => php_jit::JitNativeControlResult::returning(callable),
        Ok(None) => native_runtime_contract_violation(),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Resolves one statically named object/class method to a prepared generated
/// entry contract. The leaf never invokes the resolved PHP body.
pub(crate) extern "C" fn jit_native_acquire_method_callable_abi(
    runtime: *mut NativeRequestFastState,
    target: i64,
    method: *const u8,
    method_length: u64,
    caller_function: u64,
    callback_completed: i64,
) -> php_jit::JitNativeControlResult {
    if runtime.is_null() {
        return native_runtime_contract_violation();
    }
    let Ok(method_length) = usize::try_from(method_length) else {
        return native_runtime_contract_violation();
    };
    if method.is_null() {
        return native_runtime_contract_violation();
    }
    // SAFETY: generated code owns the immutable stack bytes for this
    // synchronous resolution call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let method = unsafe { std::slice::from_raw_parts(method, method_length) };
    let Ok(caller_function) = u32::try_from(caller_function) else {
        return native_runtime_contract_violation();
    };
    // SAFETY: the compiled ABI passes the request-owned fast state.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    match unsafe { &mut *runtime }.acquire_direct_method_callable(
        target,
        method,
        caller_function,
        callback_completed != 0,
    ) {
        Ok(super::NativeMethodCallableResolution::Ready(callable)) => {
            php_jit::JitNativeControlResult::returning(callable)
        }
        Ok(super::NativeMethodCallableResolution::InvokeUserCallback(callback)) => {
            php_jit::JitNativeControlResult::control(
                php_jit::JitCallStatus::INVOKE_USER_CALLBACK,
                0,
                callback,
            )
        }
        Ok(super::NativeMethodCallableResolution::NotFound) => native_runtime_contract_violation(),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Resolves one authoritative class-name value to immutable allocation
/// metadata. The leaf neither allocates the object nor invokes PHP code.
pub(crate) extern "C" fn jit_native_acquire_class_plan_abi(
    runtime: *mut NativeRequestFastState,
    class: i64,
    callback_completed: i64,
) -> php_jit::JitNativeControlResult {
    if runtime.is_null() {
        return native_runtime_contract_violation();
    }
    // SAFETY: generated code passes the request-stable fast state and keeps
    // the authoritative class-name owner alive for this synchronous query.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    match unsafe { &mut *runtime }.acquire_direct_class_plan(class, callback_completed != 0) {
        Ok(super::NativeClassPlanResolution::Ready(plan)) => {
            php_jit::JitNativeControlResult::returning(plan as i64)
        }
        Ok(super::NativeClassPlanResolution::InvokeUserCallback(callback)) => {
            php_jit::JitNativeControlResult::control(
                php_jit::JitCallStatus::INVOKE_USER_CALLBACK,
                0,
                callback,
            )
        }
        Ok(super::NativeClassPlanResolution::NotFound) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Publishes one compile-time callable from its immutable target/signature
/// contract. Generated code passes the already-resolved function identity and
/// flags, so this boundary performs no symbol query or dynamic dispatch.
pub(crate) extern "C" fn jit_native_resolve_callable_abi(
    runtime: *mut NativeRequestFastState,
    name: *const u8,
    length: u64,
    function_id: u64,
    visible_arity: u64,
    flags: u64,
    reference_word_0: u64,
    reference_word_1: u64,
    reference_word_2: u64,
    reference_word_3: u64,
) -> php_jit::JitNativeControlResult {
    let Ok(length) = usize::try_from(length) else {
        return native_runtime_contract_violation();
    };
    let (Ok(function_id), Ok(visible_arity), Ok(flags)) = (
        u32::try_from(function_id),
        u32::try_from(visible_arity),
        u32::try_from(flags),
    ) else {
        return native_runtime_contract_violation();
    };
    let allowed_flags = php_jit::JIT_NATIVE_PREPARED_CALLABLE_FIRST_PARAMETER_BY_REFERENCE
        | php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_INT
        | php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_STRING
        | php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_RELEASABLE_SCALAR
        | php_jit::JIT_NATIVE_PREPARED_CALLABLE_DIRECT_PACKED_BINDING
        | php_jit::JIT_NATIVE_PREPARED_CALLABLE_VARIADIC
        | php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_REFERENCE;
    if flags & !allowed_flags != 0 {
        return native_runtime_contract_violation();
    }
    if name.is_null() && length != 0 {
        return php_jit::JitNativeControlResult::control(
            php_jit::JitCallStatus::ABI_MISMATCH,
            0,
            0,
        );
    }
    // SAFETY: generated code passes a stack-backed immutable byte range that
    // remains live for this synchronous call. The zero-length case admits a
    // dangling non-null stack address but never dereferences it.
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let name = unsafe { std::slice::from_raw_parts(name, length) };
    // SAFETY: the generated entry receives the request-stable fast state.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    match (unsafe { &mut *runtime }).publish_fixed_function_callable(
        name,
        NativeFixedCallablePlan {
            function: php_ir::FunctionId::new(function_id),
            runtime_view: 0,
            binding_plan: 0,
            visible_arity,
            parameter_by_reference: [
                reference_word_0,
                reference_word_1,
                reference_word_2,
                reference_word_3,
            ],
            has_receiver: false,
            first_parameter_by_reference: flags
                & php_jit::JIT_NATIVE_PREPARED_CALLABLE_FIRST_PARAMETER_BY_REFERENCE
                != 0,
            variadic: flags & php_jit::JIT_NATIVE_PREPARED_CALLABLE_VARIADIC != 0,
            direct_packed_binding: flags
                & php_jit::JIT_NATIVE_PREPARED_CALLABLE_DIRECT_PACKED_BINDING
                != 0,
            returns_by_reference: flags & php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_REFERENCE
                != 0,
            returns_int: flags & php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_INT != 0,
            returns_string: flags & php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_STRING != 0,
            returns_releasable_scalar: flags
                & php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_RELEASABLE_SCALAR
                != 0,
            magic_dispatch: false,
        },
    ) {
        Ok(callable) => php_jit::JitNativeControlResult::returning(callable),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

fn native_binary_runtime_error() -> php_jit::JitNativeControlResult {
    php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

fn count_native_array_entries(
    fast: &NativeRequestFastState,
    identity: usize,
    entries: &[php_jit::JitNativeDirectArrayEntry],
    active: &mut [usize; 64],
    depth: usize,
) -> Option<usize> {
    if depth == active.len() || active[..depth].contains(&identity) {
        return None;
    }
    active[depth] = identity;
    let mut count = entries.len();
    for entry in entries {
        match fast.native_comparison_value(entry.value) {
            Some(NativeComparisonValue::Array { identity, entries }) => {
                count = count.checked_add(count_native_array_entries(
                    fast,
                    identity,
                    entries,
                    active,
                    depth + 1,
                )?)?;
            }
            Some(_) => {}
            // Publication admits only authoritative direct-array graphs.
            // An opaque value here is an engine contract violation.
            None => return None,
        }
    }
    Some(count)
}

fn native_array_count(
    runtime: *mut NativeRequestFastState,
    value: i64,
    mode: i64,
    prepared_type_error: u64,
    name: &str,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the active request's stable fast state.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &mut *runtime };
    let recursive = if mode == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        false
    } else {
        match fast.native_comparison_value(mode) {
            Some(NativeComparisonValue::Int(0)) => false,
            Some(NativeComparisonValue::Int(1)) => true,
            _ => {
                if prepared_type_error == 0 {
                    return native_runtime_contract_violation();
                }
                // `type_error` is the first field, so the published pointer
                // also identifies the complete immutable count error plan.
                #[allow(unsafe_code)]
                // Safety: the exact native boundary keeps its audited pointer contract.
                let prepared = unsafe {
                    &*(prepared_type_error as usize as *const PreparedNativeCountThrowableSites)
                };
                let message = format!(
                    "{name}(): Argument #2 ($mode) must be either COUNT_NORMAL or COUNT_RECURSIVE"
                );
                let throwable = publish_prepared_exception(
                    fast,
                    &prepared.value_error,
                    message.as_bytes(),
                    0,
                    php_jit::jit_encode_constant(u32::MAX),
                );
                return if throwable.status == php_jit::JitCallStatus::RETURN {
                    php_jit::JitNativeControlResult::control(
                        php_jit::JitCallStatus::THROW,
                        0,
                        throwable.value,
                    )
                } else {
                    throwable
                };
            }
        }
    };
    // An uninitialized local reaches PHP builtins as the null value after the
    // generated undefined-variable diagnostic. Treat it as PHP null here so
    // count()/sizeof() produce their prepared TypeError instead of an engine
    // contract failure.
    let comparison =
        if php_jit::jit_decode_constant(value) == Some(php_jit::JIT_VALUE_UNINITIALIZED) {
            Some(NativeComparisonValue::Null)
        } else {
            fast.native_comparison_value(value)
        };
    let Some(comparison) = comparison else {
        return native_runtime_contract_violation();
    };
    let NativeComparisonValue::Array { identity, entries } = comparison else {
        if matches!(comparison, NativeComparisonValue::Object(object) if object.owner.is_native_countable())
        {
            // Countable bodies are generated PHP calls and are selected by
            // generated call lowering, never invoked from this exact leaf.
            return native_runtime_contract_violation();
        }
        if prepared_type_error == 0 {
            return native_runtime_contract_violation();
        }
        let actual = match comparison {
            NativeComparisonValue::Null => "null",
            NativeComparisonValue::Bool(true) => "true",
            NativeComparisonValue::Bool(false) => "false",
            NativeComparisonValue::Int(_) => "int",
            NativeComparisonValue::Float(_) => "float",
            NativeComparisonValue::String(_) => "string",
            NativeComparisonValue::Object(_) | NativeComparisonValue::OpaqueIdentity(_) => "object",
            NativeComparisonValue::Resource(_) => "resource",
            NativeComparisonValue::Array { .. } => unreachable!(),
        };
        let message = format!(
            "{name}(): Argument #1 ($value) must be of type Countable|array, {actual} given"
        );
        // SAFETY: the generated caller loads this opaque pointer from the
        // request's immutable per-continuation throwable-plan table.
        #[allow(unsafe_code)]
        // Safety: the exact native boundary keeps its audited pointer contract.
        let prepared =
            unsafe { &*(prepared_type_error as usize as *const PreparedNativeThrowableSite) };
        let throwable = publish_prepared_exception(
            fast,
            prepared,
            message.as_bytes(),
            0,
            php_jit::jit_encode_constant(u32::MAX),
        );
        return if throwable.status == php_jit::JitCallStatus::RETURN {
            php_jit::JitNativeControlResult::control(
                php_jit::JitCallStatus::THROW,
                0,
                throwable.value,
            )
        } else {
            throwable
        };
    };
    let count = if recursive {
        let mut active = [0; 64];
        count_native_array_entries(fast, identity, entries, &mut active, 0)
    } else {
        Some(entries.len())
    };
    let Some(count) = count.and_then(|count| i64::try_from(count).ok()) else {
        return native_runtime_contract_violation();
    };
    match fast.publish_direct_int(count) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Exact `count` over authoritative direct-array entries. Generated callers
/// validate Countable objects and invoke their published method entries;
/// this leaf owns array recursion plus PHP-visible type/mode errors.
pub(crate) extern "C" fn jit_native_count_abi(
    runtime: *mut NativeRequestFastState,
    value: i64,
    mode: i64,
    prepared_type_error: u64,
) -> php_jit::JitNativeControlResult {
    native_array_count(runtime, value, mode, prepared_type_error, "count")
}

/// `sizeof` is a distinct fixed target with the same PHP array semantics.
pub(crate) extern "C" fn jit_native_sizeof_abi(
    runtime: *mut NativeRequestFastState,
    value: i64,
    mode: i64,
    prepared_type_error: u64,
) -> php_jit::JitNativeControlResult {
    native_array_count(runtime, value, mode, prepared_type_error, "sizeof")
}

#[derive(Clone, Copy)]
enum NativeBinaryNumber {
    Int(i64),
    Float(f64),
}

impl NativeBinaryNumber {
    fn exact_integer(self) -> Option<i64> {
        match self {
            Self::Int(value) => Some(value),
            Self::Float(value) if php_runtime::api::float_fits_int(value) => {
                let integer = value as i64;
                ((integer as f64) == value).then_some(integer)
            }
            Self::Float(_) => None,
        }
    }
}

fn native_binary_number(value: NativeComparisonValue<'_>) -> Option<NativeBinaryNumber> {
    match value {
        NativeComparisonValue::Null => Some(NativeBinaryNumber::Int(0)),
        NativeComparisonValue::Bool(value) => Some(NativeBinaryNumber::Int(i64::from(value))),
        NativeComparisonValue::Int(value) => Some(NativeBinaryNumber::Int(value)),
        NativeComparisonValue::Float(value) => Some(NativeBinaryNumber::Float(value)),
        NativeComparisonValue::String(bytes) => {
            let classified = php_runtime::experimental::numeric_string::classify(bytes);
            if !matches!(
                classified.kind,
                php_runtime::experimental::numeric_string::NumericStringKind::IntString
                    | php_runtime::experimental::numeric_string::NumericStringKind::FloatString
            ) {
                return None;
            }
            classified.value.map(|value| match value {
                php_runtime::experimental::numeric_string::NumericStringValue::Int(value) => {
                    NativeBinaryNumber::Int(value)
                }
                php_runtime::experimental::numeric_string::NumericStringValue::Float(value) => {
                    NativeBinaryNumber::Float(value)
                }
            })
        }
        NativeComparisonValue::Array { .. }
        | NativeComparisonValue::Object(_)
        | NativeComparisonValue::Resource(_)
        | NativeComparisonValue::OpaqueIdentity(_) => None,
    }
}

fn native_arithmetic_number(
    value: NativeComparisonValue<'_>,
) -> Result<(NativeBinaryNumber, bool), ()> {
    match value {
        NativeComparisonValue::Null => Ok((NativeBinaryNumber::Int(0), false)),
        NativeComparisonValue::Bool(value) => {
            Ok((NativeBinaryNumber::Int(i64::from(value)), false))
        }
        NativeComparisonValue::Int(value) => Ok((NativeBinaryNumber::Int(value), false)),
        NativeComparisonValue::Float(value) => Ok((NativeBinaryNumber::Float(value), false)),
        NativeComparisonValue::Resource(value) => i64::try_from(value)
            .map(|value| (NativeBinaryNumber::Int(value), false))
            .map_err(|_| ()),
        NativeComparisonValue::String(bytes) => {
            let classified = php_runtime::experimental::numeric_string::classify(bytes);
            let leading = classified.kind
                == php_runtime::experimental::numeric_string::NumericStringKind::LeadingNumeric;
            if !matches!(
                classified.kind,
                php_runtime::experimental::numeric_string::NumericStringKind::IntString
                    | php_runtime::experimental::numeric_string::NumericStringKind::FloatString
                    | php_runtime::experimental::numeric_string::NumericStringKind::LeadingNumeric
            ) {
                return Err(());
            }
            classified
                .value
                .map(|value| {
                    let value = match value {
                        php_runtime::experimental::numeric_string::NumericStringValue::Int(
                            value,
                        ) => NativeBinaryNumber::Int(value),
                        php_runtime::experimental::numeric_string::NumericStringValue::Float(
                            value,
                        ) => NativeBinaryNumber::Float(value),
                    };
                    (value, leading)
                })
                .ok_or(())
        }
        NativeComparisonValue::Array { .. }
        | NativeComparisonValue::Object(_)
        | NativeComparisonValue::OpaqueIdentity(_) => Err(()),
    }
}

fn native_arithmetic_type(value: NativeComparisonValue<'_>) -> &'static str {
    match value {
        NativeComparisonValue::Null => "null",
        NativeComparisonValue::Bool(_) => "bool",
        NativeComparisonValue::Int(_) => "int",
        NativeComparisonValue::Float(_) => "float",
        NativeComparisonValue::String(_) => "string",
        NativeComparisonValue::Array { .. } => "array",
        NativeComparisonValue::Object(_) | NativeComparisonValue::OpaqueIdentity(_) => "object",
        NativeComparisonValue::Resource(_) => "resource",
    }
}

fn native_arithmetic_type_error(
    fast: &mut NativeRequestFastState,
    prepared_type_error: u64,
    left: &str,
    operator: &str,
    right: &str,
) -> php_jit::JitNativeControlResult {
    if prepared_type_error == 0 {
        return native_runtime_contract_violation();
    }
    let message = format!("Unsupported operand types: {left} {operator} {right}");
    // SAFETY: generated code loads the immutable per-continuation TypeError
    // plan from the active runtime view.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let prepared =
        unsafe { &*(prepared_type_error as usize as *const PreparedNativeThrowableSite) };
    native_prepared_error(fast, prepared, &message)
}

fn native_prepared_error(
    fast: &mut NativeRequestFastState,
    prepared: &PreparedNativeThrowableSite,
    message: &str,
) -> php_jit::JitNativeControlResult {
    let throwable = publish_prepared_exception(
        fast,
        prepared,
        message.as_bytes(),
        0,
        php_jit::jit_encode_constant(u32::MAX),
    );
    if throwable.status == php_jit::JitCallStatus::RETURN {
        php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::THROW, 0, throwable.value)
    } else {
        throwable
    }
}

fn native_static_property_type_matches(
    value: NativeComparisonValue<'_>,
    type_: &php_ir::IrReturnType,
) -> bool {
    use php_ir::IrReturnType as Type;
    match type_ {
        Type::Int => matches!(value, NativeComparisonValue::Int(_)),
        Type::Float => matches!(value, NativeComparisonValue::Float(_)),
        Type::String => matches!(value, NativeComparisonValue::String(_)),
        Type::Array => matches!(value, NativeComparisonValue::Array { .. }),
        Type::Callable => matches!(value, NativeComparisonValue::OpaqueIdentity(_)),
        Type::Iterable => {
            matches!(
                value,
                NativeComparisonValue::Array { .. } | NativeComparisonValue::OpaqueIdentity(_)
            ) || matches!(value, NativeComparisonValue::Object(object) if object.owner.is_native_traversable())
        }
        Type::Object => matches!(
            value,
            NativeComparisonValue::Object(_) | NativeComparisonValue::OpaqueIdentity(_)
        ),
        Type::Bool => matches!(value, NativeComparisonValue::Bool(_)),
        Type::Null | Type::Void => matches!(value, NativeComparisonValue::Null),
        Type::Mixed => true,
        Type::Never => false,
        Type::False => matches!(value, NativeComparisonValue::Bool(false)),
        Type::True => matches!(value, NativeComparisonValue::Bool(true)),
        Type::Class { name, .. } => matches!(
            value,
            NativeComparisonValue::Object(object)
                if php_ir::module::normalize_class_name(&object.owner.class_name())
                    == php_ir::module::normalize_class_name(name)
        ),
        Type::Nullable { inner } => {
            matches!(value, NativeComparisonValue::Null)
                || native_static_property_type_matches(value, inner)
        }
        Type::Union { members } | Type::Dnf { members } => members
            .iter()
            .any(|member| native_static_property_type_matches(value, member)),
        Type::Intersection { members } => members
            .iter()
            .all(|member| native_static_property_type_matches(value, member)),
    }
}

enum NativeStaticPropertyCoercion {
    Int(i64),
    Float(f64),
    String(Vec<u8>),
    Constant(u32),
}

fn native_static_property_coercion(
    value: NativeComparisonValue<'_>,
    type_: &php_ir::IrReturnType,
    strict: bool,
) -> Option<NativeStaticPropertyCoercion> {
    use php_ir::IrReturnType as Type;
    if let Type::Nullable { inner } = type_ {
        if matches!(value, NativeComparisonValue::Null) {
            return Some(NativeStaticPropertyCoercion::Constant(u32::MAX));
        }
        return native_static_property_coercion(value, inner, strict);
    }
    if matches!(type_, Type::Float)
        && let NativeComparisonValue::Int(value) = value
    {
        return Some(NativeStaticPropertyCoercion::Float(value as f64));
    }
    if strict {
        return None;
    }
    match (type_, value) {
        (Type::Int, NativeComparisonValue::String(bytes)) => {
            use php_runtime::experimental::numeric_string::NumericStringValue;
            let classified = php_runtime::experimental::numeric_string::classify(bytes);
            let value = match classified.value {
                Some(NumericStringValue::Int(value)) => Some(value),
                Some(NumericStringValue::Float(value)) => Some(value as i64),
                None => None,
            };
            value.map(NativeStaticPropertyCoercion::Int)
        }
        (Type::Int, NativeComparisonValue::Float(value)) => {
            Some(NativeStaticPropertyCoercion::Int(value as i64))
        }
        (Type::Int, NativeComparisonValue::Bool(value)) => {
            Some(NativeStaticPropertyCoercion::Int(i64::from(value)))
        }
        (Type::Float, NativeComparisonValue::String(bytes)) => {
            use php_runtime::experimental::numeric_string::NumericStringValue;
            let classified = php_runtime::experimental::numeric_string::classify(bytes);
            let value = match classified.value {
                Some(NumericStringValue::Int(value)) => Some(value as f64),
                Some(NumericStringValue::Float(value)) => Some(value),
                None => None,
            };
            value.map(NativeStaticPropertyCoercion::Float)
        }
        (Type::Float, NativeComparisonValue::Bool(value)) => {
            Some(NativeStaticPropertyCoercion::Float(if value {
                1.0
            } else {
                0.0
            }))
        }
        (Type::String, NativeComparisonValue::Int(value)) => Some(
            NativeStaticPropertyCoercion::String(value.to_string().into_bytes()),
        ),
        (Type::String, NativeComparisonValue::Float(value)) => {
            let mut bytes = [0_u8; php_runtime::api::PHP_FLOAT_STRING_BUFFER_CAPACITY];
            let rendered = php_runtime::api::float_to_php_string_bytes(value, &mut bytes);
            Some(NativeStaticPropertyCoercion::String(rendered.to_vec()))
        }
        (Type::String, NativeComparisonValue::Bool(value)) => {
            Some(NativeStaticPropertyCoercion::String(if value {
                b"1".to_vec()
            } else {
                Vec::new()
            }))
        }
        (
            Type::Bool,
            NativeComparisonValue::Int(_)
            | NativeComparisonValue::Float(_)
            | NativeComparisonValue::String(_),
        ) => Some(NativeStaticPropertyCoercion::Constant(
            if super::native_comparison_truthy(value) {
                php_jit::JIT_VALUE_TRUE
            } else {
                php_jit::JIT_VALUE_FALSE
            },
        )),
        (Type::Union { members } | Type::Dnf { members }, value) => {
            for member in members {
                if let Some(candidate) = native_static_property_coercion(value, member, strict) {
                    return Some(candidate);
                }
            }
            None
        }
        _ => None,
    }
}

fn native_static_property_actual_type(value: NativeComparisonValue<'_>) -> &'static str {
    match value {
        NativeComparisonValue::Null => "null",
        NativeComparisonValue::Bool(_) => "bool",
        NativeComparisonValue::Int(_) => "int",
        NativeComparisonValue::Float(_) => "float",
        NativeComparisonValue::String(_) => "string",
        NativeComparisonValue::Array { .. } => "array",
        NativeComparisonValue::Object(_) | NativeComparisonValue::OpaqueIdentity(_) => "object",
        NativeComparisonValue::Resource(_) => "resource",
    }
}

fn native_static_property_type_name(type_: &php_ir::IrReturnType) -> String {
    use php_ir::IrReturnType as Type;
    match type_ {
        Type::Int => "int".to_owned(),
        Type::Float => "float".to_owned(),
        Type::String => "string".to_owned(),
        Type::Array => "array".to_owned(),
        Type::Callable => "callable".to_owned(),
        Type::Iterable => "iterable".to_owned(),
        Type::Object => "object".to_owned(),
        Type::Bool => "bool".to_owned(),
        Type::Null => "null".to_owned(),
        Type::Void => "void".to_owned(),
        Type::Mixed => "mixed".to_owned(),
        Type::Never => "never".to_owned(),
        Type::False => "false".to_owned(),
        Type::True => "true".to_owned(),
        Type::Class { name, display_name } => display_name.clone().unwrap_or_else(|| name.clone()),
        Type::Nullable { inner } => format!("?{}", native_static_property_type_name(inner)),
        Type::Union { members } => {
            let mut names = members
                .iter()
                .map(native_static_property_type_name)
                .collect::<Vec<_>>();
            if let (Some(int), Some(string)) = (
                names.iter().position(|name| name == "int"),
                names.iter().position(|name| name == "string"),
            ) && int < string
            {
                names.swap(int, string);
            }
            names.join("|")
        }
        Type::Dnf { members } => members
            .iter()
            .map(native_static_property_type_name)
            .collect::<Vec<_>>()
            .join("|"),
        Type::Intersection { members } => members
            .iter()
            .map(native_static_property_type_name)
            .collect::<Vec<_>>()
            .join("&"),
    }
}

/// Enforces one immutable static-property declaration contract. This is an
/// exact typed leaf over native encodings, not a generic property dispatcher.
#[allow(unsafe_code)] // Safety: publication owns the contract for the active runtime view.
pub(crate) extern "C" fn jit_native_static_property_contract_abi(
    runtime: *mut NativeRequestFastState,
    contract: u64,
    encoded: i64,
) -> php_jit::JitNativeControlResult {
    if runtime.is_null() || contract == 0 {
        return native_runtime_contract_violation();
    }
    let fast = unsafe { &mut *runtime };
    let contract = unsafe { &*(contract as usize as *const PreparedNativeStaticPropertyContract) };
    let Some(type_) = contract.type_.as_ref() else {
        let message = format!(
            "Access to undeclared static property {}::${}",
            contract.owner_display_name, contract.property
        );
        return native_prepared_error(fast, &contract.throwable, &message);
    };
    if php_jit::jit_decode_constant(encoded) == Some(php_jit::JIT_VALUE_UNINITIALIZED) {
        let message = format!(
            "Typed static property {}::${} must not be accessed before initialization",
            contract.owner_display_name, contract.property
        );
        return native_prepared_error(fast, &contract.throwable, &message);
    }
    let (exact, actual_type, coercion) = {
        let Some(value) = fast.native_comparison_value(encoded) else {
            return native_runtime_contract_violation();
        };
        (
            native_static_property_type_matches(value, type_),
            native_static_property_actual_type(value),
            native_static_property_coercion(value, type_, contract.strict_types),
        )
    };
    if exact {
        return match fast.retain_direct_encoded(encoded) {
            Ok(()) => php_jit::JitNativeControlResult::returning(encoded),
            Err(_) => native_runtime_contract_violation(),
        };
    }
    let coerced = match coercion {
        Some(NativeStaticPropertyCoercion::Int(value)) => fast.publish_direct_int(value),
        Some(NativeStaticPropertyCoercion::Float(value)) => fast.publish_direct_float(value),
        Some(NativeStaticPropertyCoercion::String(value)) => {
            fast.publish_direct_string_bytes(&value)
        }
        Some(NativeStaticPropertyCoercion::Constant(value)) => {
            return php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(value));
        }
        None => {
            let message = format!(
                "Cannot assign {} to property {}::${} of type {}",
                actual_type,
                contract.owner_display_name,
                contract.property,
                native_static_property_type_name(type_),
            );
            return native_prepared_error(fast, &contract.throwable, &message);
        }
    };
    match coerced {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => native_runtime_contract_violation(),
    }
}

/// Raises the source-prepared PHP Error for one unresolved fixed constant.
/// Dynamic `define()` publication replaces global-constant error slots before
/// execution; this leaf performs no lookup or per-call validation.
#[allow(unsafe_code)] // Safety: publication owns the immutable contract.
pub(crate) extern "C" fn jit_native_undefined_constant_abi(
    runtime: *mut NativeRequestFastState,
    contract: u64,
) -> php_jit::JitNativeControlResult {
    if runtime.is_null() || contract == 0 {
        return native_runtime_contract_violation();
    }
    let fast = unsafe { &mut *runtime };
    let contract =
        unsafe { &*(contract as usize as *const PreparedNativeUndefinedConstantContract) };
    native_prepared_error(fast, &contract.throwable, &contract.message)
}

enum NativeTypedReferenceCandidate {
    Borrowed(i64),
    Int(i64),
    Float(f64),
    String(Vec<u8>),
    Constant(u32),
}

fn native_typed_reference_candidate(
    fast: &NativeRequestFastState,
    encoded: i64,
    contract: &PreparedNativeStaticPropertyContract,
) -> Option<NativeTypedReferenceCandidate> {
    let type_ = contract.type_.as_ref()?;
    let value = fast.native_comparison_value(encoded)?;
    if native_static_property_type_matches(value, type_) {
        return Some(NativeTypedReferenceCandidate::Borrowed(encoded));
    }
    match native_static_property_coercion(value, type_, contract.strict_types)? {
        NativeStaticPropertyCoercion::Int(value) => Some(NativeTypedReferenceCandidate::Int(value)),
        NativeStaticPropertyCoercion::Float(value) => {
            Some(NativeTypedReferenceCandidate::Float(value))
        }
        NativeStaticPropertyCoercion::String(value) => {
            Some(NativeTypedReferenceCandidate::String(value))
        }
        NativeStaticPropertyCoercion::Constant(value) => {
            Some(NativeTypedReferenceCandidate::Constant(value))
        }
    }
}

fn native_typed_reference_candidate_matches(
    fast: &NativeRequestFastState,
    candidate: &NativeTypedReferenceCandidate,
    encoded: i64,
) -> bool {
    let Some(value) = fast.native_comparison_value(encoded) else {
        return false;
    };
    match candidate {
        NativeTypedReferenceCandidate::Borrowed(candidate) => *candidate == encoded,
        NativeTypedReferenceCandidate::Int(candidate) => {
            matches!(value, NativeComparisonValue::Int(value) if value == *candidate)
        }
        NativeTypedReferenceCandidate::Float(candidate) => {
            matches!(value, NativeComparisonValue::Float(value) if value.to_bits() == candidate.to_bits())
        }
        NativeTypedReferenceCandidate::String(candidate) => {
            matches!(value, NativeComparisonValue::String(value) if value == candidate)
        }
        NativeTypedReferenceCandidate::Constant(candidate) => {
            php_jit::jit_decode_constant(encoded) == Some(*candidate)
        }
    }
}

fn native_typed_reference_candidates_identical(
    fast: &NativeRequestFastState,
    left: &NativeTypedReferenceCandidate,
    right: &NativeTypedReferenceCandidate,
    source: i64,
) -> bool {
    if native_typed_reference_candidate_matches(fast, left, source)
        && native_typed_reference_candidate_matches(fast, right, source)
    {
        return true;
    }
    match (left, right) {
        (NativeTypedReferenceCandidate::Int(left), NativeTypedReferenceCandidate::Int(right)) => {
            left == right
        }
        (
            NativeTypedReferenceCandidate::Float(left),
            NativeTypedReferenceCandidate::Float(right),
        ) => left.to_bits() == right.to_bits(),
        (
            NativeTypedReferenceCandidate::String(left),
            NativeTypedReferenceCandidate::String(right),
        ) => left == right,
        (
            NativeTypedReferenceCandidate::Constant(left),
            NativeTypedReferenceCandidate::Constant(right),
        ) => left == right,
        (
            NativeTypedReferenceCandidate::Borrowed(left),
            NativeTypedReferenceCandidate::Borrowed(right),
        ) => left == right,
        _ => false,
    }
}

fn publish_native_typed_reference_candidate(
    fast: &mut NativeRequestFastState,
    candidate: NativeTypedReferenceCandidate,
) -> Result<i64, &'static str> {
    match candidate {
        NativeTypedReferenceCandidate::Borrowed(value) => {
            fast.retain_direct_encoded(value)?;
            Ok(value)
        }
        NativeTypedReferenceCandidate::Int(value) => fast.publish_direct_int(value),
        NativeTypedReferenceCandidate::Float(value) => fast.publish_direct_float(value),
        NativeTypedReferenceCandidate::String(value) => fast.publish_direct_string_bytes(&value),
        NativeTypedReferenceCandidate::Constant(value) => Ok(php_jit::jit_encode_constant(value)),
    }
}

fn native_typed_property_description(contract: &PreparedNativeStaticPropertyContract) -> String {
    let type_ = contract
        .type_
        .as_ref()
        .map(native_static_property_type_name)
        .unwrap_or_else(|| "mixed".to_owned());
    format!(
        "property {}::${} of type {type_}",
        contract.owner_display_name, contract.property
    )
}

/// Attaches one immutable static-property type constraint to a direct native
/// reference. The reference slot remains the only mutable payload authority;
/// `aux` points to a request-owned stable constraint set used by later exact
/// typed-reference stores.
#[allow(unsafe_code)] // Safety: generated code passes a published reference and contract.
pub(crate) extern "C" fn jit_native_typed_static_reference_bind_abi(
    runtime: *mut NativeRequestFastState,
    contract: u64,
    reference: i64,
) -> php_jit::JitNativeControlResult {
    if runtime.is_null() || contract == 0 {
        return native_runtime_contract_violation();
    }
    let fast = unsafe { &mut *runtime };
    let contract = unsafe { &*(contract as usize as *const PreparedNativeStaticPropertyContract) };
    let Some((index, slot)) = fast.direct_slot(reference) else {
        return native_runtime_contract_violation();
    };
    if php_jit::jit_runtime_value_tag(reference) != Some(php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG)
        || slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
        || slot.flags != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
    {
        return native_runtime_contract_violation();
    }

    let uninitialized =
        php_jit::jit_decode_constant(slot.payload as i64) == Some(php_jit::JIT_VALUE_UNINITIALIZED);
    let current = if uninitialized {
        php_jit::jit_encode_constant(u32::MAX)
    } else {
        slot.payload as i64
    };
    let actual_type = fast
        .native_comparison_value(current)
        .map(native_static_property_actual_type)
        .unwrap_or("unknown");
    let Some(candidate) = native_typed_reference_candidate(fast, current, contract) else {
        let message = format!(
            "Cannot assign {actual_type} to {}",
            native_typed_property_description(contract)
        );
        return native_prepared_error(fast, &contract.throwable, &message);
    };

    let existing_contracts = if slot.aux == 0 {
        &[][..]
    } else {
        let set =
            unsafe { &*(slot.aux as usize as *const super::NativeTypedReferenceConstraintSet) };
        set.contracts.as_slice()
    };
    if !existing_contracts.is_empty()
        && !native_typed_reference_candidate_matches(fast, &candidate, current)
    {
        let existing = unsafe {
            &*(existing_contracts[0] as usize as *const PreparedNativeStaticPropertyContract)
        };
        let message = format!(
            "Reference with value of type {actual_type} held by {} is not compatible with {}",
            native_typed_property_description(existing),
            native_typed_property_description(contract),
        );
        return native_prepared_error(fast, &contract.throwable, &message);
    }

    let replacement = if native_typed_reference_candidate_matches(fast, &candidate, current) {
        current
    } else {
        match publish_native_typed_reference_candidate(fast, candidate) {
            Ok(value) => value,
            Err(_) => return native_runtime_contract_violation(),
        }
    };
    let slots = fast.header.active_runtime_view().direct_value_slots as usize
        as *mut php_jit::JitNativeValueSlot;
    if slots.is_null() {
        return native_runtime_contract_violation();
    }
    let constraint_pointer = if slot.aux == 0 {
        let mut set = Box::new(super::NativeTypedReferenceConstraintSet {
            contracts: vec![
                contract as *const PreparedNativeStaticPropertyContract as usize as u64,
            ],
        });
        let pointer = std::ptr::from_mut(set.as_mut()) as usize as u64;
        fast.typed_reference_constraint_sets.push(set);
        pointer
    } else {
        let set =
            unsafe { &mut *(slot.aux as usize as *mut super::NativeTypedReferenceConstraintSet) };
        let pointer = contract as *const PreparedNativeStaticPropertyContract as usize as u64;
        if !set.contracts.contains(&pointer) {
            set.contracts.push(pointer);
        }
        slot.aux
    };
    unsafe {
        (*slots.add(index)).payload = replacement as u64;
        (*slots.add(index)).aux = constraint_pointer;
        (*slots.add(index)).reserved = php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED
            | php_jit::JIT_NATIVE_REFERENCE_TYPED_PROPERTY_GUARD;
    }
    if replacement != current && fast.discard_owned_direct_value(current).is_err() {
        return native_runtime_contract_violation();
    }
    php_jit::JitNativeControlResult::returning(reference)
}

/// Applies every immutable constraint attached to a direct reference before
/// replacing its authoritative payload. Candidate coercions must agree
/// exactly across the constraint set, matching PHP's shared typed-property
/// reference rule.
#[allow(unsafe_code)] // Safety: generated code passes published direct encodings.
pub(crate) extern "C" fn jit_native_typed_reference_store_abi(
    runtime: *mut NativeRequestFastState,
    reference: i64,
    replacement: i64,
) -> php_jit::JitNativeControlResult {
    if runtime.is_null() {
        return native_runtime_contract_violation();
    }
    let fast = unsafe { &mut *runtime };
    let Some((index, slot)) = fast.direct_slot(reference) else {
        return native_runtime_contract_violation();
    };
    if php_jit::jit_runtime_value_tag(reference) != Some(php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG)
        || slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
        || slot.flags != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
        || slot.reserved & php_jit::JIT_NATIVE_REFERENCE_TYPED_PROPERTY_GUARD == 0
        || slot.aux == 0
    {
        return native_runtime_contract_violation();
    }
    let constraints =
        unsafe { &*(slot.aux as usize as *const super::NativeTypedReferenceConstraintSet) };
    if constraints.contracts.is_empty() {
        return native_runtime_contract_violation();
    }
    let actual_type = fast
        .native_comparison_value(replacement)
        .map(native_static_property_actual_type)
        .unwrap_or("unknown");
    let mut selected: Option<(NativeTypedReferenceCandidate, u64)> = None;
    for pointer in constraints.contracts.iter().copied() {
        let contract =
            unsafe { &*(pointer as usize as *const PreparedNativeStaticPropertyContract) };
        let Some(candidate) = native_typed_reference_candidate(fast, replacement, contract) else {
            let message = format!(
                "Cannot assign {actual_type} to reference held by {}",
                native_typed_property_description(contract)
            );
            return native_prepared_error(fast, &contract.throwable, &message);
        };
        if let Some((selected_candidate, selected_pointer)) = selected.as_ref()
            && !native_typed_reference_candidates_identical(
                fast,
                selected_candidate,
                &candidate,
                replacement,
            )
        {
            let first = unsafe {
                &*(*selected_pointer as usize as *const PreparedNativeStaticPropertyContract)
            };
            let message = format!(
                "Cannot assign {actual_type} to reference held by {} and {}, as this would result in an inconsistent type conversion",
                native_typed_property_description(first),
                native_typed_property_description(contract),
            );
            return native_prepared_error(fast, &contract.throwable, &message);
        }
        if selected.is_none() {
            selected = Some((candidate, pointer));
        }
    }
    let Some((candidate, _)) = selected else {
        return native_runtime_contract_violation();
    };
    let replacement = match publish_native_typed_reference_candidate(fast, candidate) {
        Ok(value) => value,
        Err(_) => return native_runtime_contract_violation(),
    };
    let slots = fast.header.active_runtime_view().direct_value_slots as usize
        as *mut php_jit::JitNativeValueSlot;
    if slots.is_null() {
        let _ = fast.discard_owned_direct_value(replacement);
        return native_runtime_contract_violation();
    }
    unsafe {
        (*slots.add(index)).payload = replacement as u64;
    }
    if fast
        .discard_owned_direct_value(slot.payload as i64)
        .is_err()
    {
        return native_runtime_contract_violation();
    }
    php_jit::JitNativeControlResult::returning(reference)
}

fn native_static_property_type_accepts_array(type_: &php_ir::IrReturnType) -> bool {
    use php_ir::IrReturnType as Type;
    match type_ {
        Type::Array | Type::Iterable | Type::Mixed => true,
        Type::Nullable { inner } => native_static_property_type_accepts_array(inner),
        Type::Union { members } | Type::Dnf { members } => members
            .iter()
            .any(native_static_property_type_accepts_array),
        Type::Intersection { members } => members
            .iter()
            .all(native_static_property_type_accepts_array),
        _ => false,
    }
}

/// Auto-initializes an empty array only after every constraint attached to the
/// reference accepts arrays. This leaf owns the allocation and payload commit,
/// so a thrown TypeError cannot leave a partially initialized lvalue behind.
#[allow(unsafe_code)] // Safety: generated code passes a published typed reference.
pub(crate) extern "C" fn jit_native_typed_reference_array_init_abi(
    runtime: *mut NativeRequestFastState,
    reference: i64,
) -> php_jit::JitNativeControlResult {
    if runtime.is_null() {
        return native_runtime_contract_violation();
    }
    let fast = unsafe { &mut *runtime };
    let Some((index, slot)) = fast.direct_slot(reference) else {
        return native_runtime_contract_violation();
    };
    if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
        || slot.flags != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
        || slot.reserved & php_jit::JIT_NATIVE_REFERENCE_TYPED_PROPERTY_GUARD == 0
        || slot.aux == 0
    {
        return native_runtime_contract_violation();
    }
    let constraints =
        unsafe { &*(slot.aux as usize as *const super::NativeTypedReferenceConstraintSet) };
    for pointer in constraints.contracts.iter().copied() {
        let contract =
            unsafe { &*(pointer as usize as *const PreparedNativeStaticPropertyContract) };
        if !contract
            .type_
            .as_ref()
            .is_some_and(native_static_property_type_accepts_array)
        {
            let message = format!(
                "Cannot auto-initialize an array inside a reference held by {}",
                native_typed_property_description(contract)
            );
            return native_prepared_error(fast, &contract.throwable, &message);
        }
    }
    let array = match fast.publish_empty_owned_direct_array() {
        Ok(array) => array,
        Err(_) => return native_runtime_contract_violation(),
    };
    let slots = fast.header.active_runtime_view().direct_value_slots as usize
        as *mut php_jit::JitNativeValueSlot;
    if slots.is_null() {
        let _ = fast.discard_owned_direct_value(array);
        return native_runtime_contract_violation();
    }
    unsafe {
        (*slots.add(index)).payload = array as u64;
    }
    if fast
        .discard_owned_direct_value(slot.payload as i64)
        .is_err()
    {
        return native_runtime_contract_violation();
    }
    php_jit::JitNativeControlResult::returning(array)
}

fn native_binary_exception(
    fast: &mut NativeRequestFastState,
    prepared_type_error: u64,
    class: u8,
    message: &str,
) -> php_jit::JitNativeControlResult {
    if prepared_type_error == 0 {
        return native_runtime_contract_violation();
    }
    // SAFETY: every published Binary continuation owns this immutable family,
    // whose first field is the TypeError site exposed to generated code.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let prepared =
        unsafe { &*(prepared_type_error as usize as *const PreparedNativeBinaryThrowableSites) };
    let site = match class {
        0 => &prepared.division_by_zero,
        1 => &prepared.arithmetic_error,
        _ => &prepared.type_error,
    };
    native_prepared_error(fast, site, message)
}

fn native_float_to_int_warning(value: NativeComparisonValue<'_>) -> Option<String> {
    let (value, string) = match value {
        NativeComparisonValue::Float(value) => (value, None),
        NativeComparisonValue::String(bytes) => {
            let classified = php_runtime::experimental::numeric_string::classify(bytes);
            let Some(php_runtime::experimental::numeric_string::NumericStringValue::Float(value)) =
                classified.value
            else {
                return None;
            };
            (value, Some(bytes))
        }
        _ => return None,
    };
    if !value.is_finite() || value.fract() == 0.0 {
        return None;
    }
    Some(string.map_or_else(
        || format!("Implicit conversion from float {value} to int loses precision"),
        |bytes| {
            format!(
                "Implicit conversion from float-string \"{}\" to int loses precision",
                String::from_utf8_lossy(bytes)
            )
        },
    ))
}

fn native_exact_arithmetic<const OPERATION: u8>(
    fast: &mut NativeRequestFastState,
    left: i64,
    right: i64,
    prepared_type_error: u64,
    file: i64,
    start: i64,
) -> php_jit::JitNativeControlResult {
    if OPERATION == 0
        && matches!(
            fast.native_comparison_value(left),
            Some(NativeComparisonValue::Array { .. })
        )
        && matches!(
            fast.native_comparison_value(right),
            Some(NativeComparisonValue::Array { .. })
        )
    {
        return native_binary_array_union(fast, left, right).map_or_else(
            |_| native_binary_runtime_error(),
            php_jit::JitNativeControlResult::returning,
        );
    }

    if matches!(OPERATION, 6..=8)
        && matches!(
            fast.native_comparison_value(left),
            Some(NativeComparisonValue::String(_))
        )
        && matches!(
            fast.native_comparison_value(right),
            Some(NativeComparisonValue::String(_))
        )
    {
        return native_string_bitwise::<OPERATION>(fast, left, right);
    }

    let Some(left_value) = fast.native_comparison_value(left) else {
        return native_runtime_contract_violation();
    };
    let left_type = native_arithmetic_type(left_value);
    let left = native_arithmetic_number(left_value);
    let Some(right_value) = fast.native_comparison_value(right) else {
        return native_runtime_contract_violation();
    };
    let right_type = native_arithmetic_type(right_value);
    let right = native_arithmetic_number(right_value);
    let float_to_int_warnings = if matches!(OPERATION, 4 | 6..=10) {
        [
            native_float_to_int_warning(left_value),
            native_float_to_int_warning(right_value),
        ]
    } else {
        [None, None]
    };
    let (Ok((left, left_leading)), Ok((right, right_leading))) = (left, right) else {
        let operator = match OPERATION {
            0 => "+",
            1 => "-",
            2 => "*",
            3 => "/",
            4 => "%",
            5 => "**",
            6 => "&",
            7 => "|",
            8 => "^",
            9 => "<<",
            10 => ">>",
            _ => return native_runtime_contract_violation(),
        };
        return native_arithmetic_type_error(
            fast,
            prepared_type_error,
            left_type,
            operator,
            right_type,
        );
    };
    for leading in [left_leading, right_leading] {
        if leading
            && emit_exact_native_warning(
                fast,
                "A non-numeric value encountered".to_owned(),
                file,
                start,
            ) != 0
        {
            return native_binary_runtime_error();
        }
    }
    for warning in float_to_int_warnings.into_iter().flatten() {
        if emit_exact_native_diagnostic(fast, 8192, warning, file, start) != 0 {
            return native_binary_runtime_error();
        }
    }
    if OPERATION == 3 {
        let zero = match right {
            NativeBinaryNumber::Int(value) => value == 0,
            NativeBinaryNumber::Float(value) => value == 0.0,
        };
        if zero {
            return native_binary_exception(fast, prepared_type_error, 0, "Division by zero");
        }
    }
    if OPERATION == 4 && right.exact_integer().unwrap_or(0) == 0 {
        return native_binary_exception(fast, prepared_type_error, 0, "Modulo by zero");
    }
    if matches!(OPERATION, 9 | 10) && right.exact_integer().unwrap_or(0) < 0 {
        return native_binary_exception(
            fast,
            prepared_type_error,
            1,
            "Bit shift by negative number",
        );
    }
    let value = match (left, right) {
        (NativeBinaryNumber::Int(left), NativeBinaryNumber::Int(right)) => {
            let integer = match OPERATION {
                0 => left.checked_add(right),
                1 => left.checked_sub(right),
                2 => left.checked_mul(right),
                3 if left == i64::MIN && right == -1 => None,
                3 if left % right == 0 => Some(left / right),
                3 => {
                    return publish_native_binary_number(
                        fast,
                        NativeBinaryNumber::Float(left as f64 / right as f64),
                    );
                }
                4 => Some(left.checked_rem(right).unwrap_or(0)),
                5 if right >= 0 => u32::try_from(right)
                    .ok()
                    .and_then(|right| left.checked_pow(right)),
                5 => None,
                6 => Some(left & right),
                7 => Some(left | right),
                8 => Some(left ^ right),
                9 => Some(if right >= 64 { 0 } else { left << right }),
                10 => Some(if right >= 64 {
                    left >> 63
                } else {
                    left >> right
                }),
                _ => return native_runtime_contract_violation(),
            };
            integer.map_or_else(
                || {
                    let left = left as f64;
                    let right = right as f64;
                    NativeBinaryNumber::Float(match OPERATION {
                        0 => left + right,
                        1 => left - right,
                        2 => left * right,
                        3 => left / right,
                        5 => left.powf(right),
                        _ => unreachable!("validated arithmetic operation"),
                    })
                },
                NativeBinaryNumber::Int,
            )
        }
        (left, right) => {
            let left = match left {
                NativeBinaryNumber::Int(value) => value as f64,
                NativeBinaryNumber::Float(value) => value,
            };
            let right = match right {
                NativeBinaryNumber::Int(value) => value as f64,
                NativeBinaryNumber::Float(value) => value,
            };
            if matches!(OPERATION, 4 | 6..=10) {
                let left = left as i64;
                let right = right as i64;
                NativeBinaryNumber::Int(match OPERATION {
                    4 => left.checked_rem(right).unwrap_or(0),
                    6 => left & right,
                    7 => left | right,
                    8 => left ^ right,
                    9 => {
                        if right >= 64 {
                            0
                        } else {
                            left << right
                        }
                    }
                    10 => {
                        if right >= 64 {
                            left >> 63
                        } else {
                            left >> right
                        }
                    }
                    _ => unreachable!("validated integer-converting operation"),
                })
            } else {
                NativeBinaryNumber::Float(match OPERATION {
                    0 => left + right,
                    1 => left - right,
                    2 => left * right,
                    3 => left / right,
                    5 => left.powf(right),
                    _ => return native_runtime_contract_violation(),
                })
            }
        }
    };
    publish_native_binary_number(fast, value)
}

macro_rules! native_exact_arithmetic_abi {
    ($name:ident, $operation:literal) => {
        pub(crate) extern "C" fn $name(
            runtime: *mut NativeRequestFastState,
            left: i64,
            right: i64,
            prepared_type_error: u64,
            file: i64,
            start: i64,
        ) -> php_jit::JitNativeControlResult {
            if runtime.is_null() {
                return native_runtime_contract_violation();
            }
            // SAFETY: generated code supplies the active request fast state
            // for this synchronous operation-specific leaf.
            #[allow(unsafe_code)]
            // Safety: the exact native boundary keeps its audited pointer contract.
            native_exact_arithmetic::<$operation>(
                unsafe { &mut *runtime },
                left,
                right,
                prepared_type_error,
                file,
                start,
            )
        }
    };
}

native_exact_arithmetic_abi!(jit_native_add_abi, 0);
native_exact_arithmetic_abi!(jit_native_subtract_abi, 1);
native_exact_arithmetic_abi!(jit_native_multiply_abi, 2);
native_exact_arithmetic_abi!(jit_native_divide_abi, 3);
native_exact_arithmetic_abi!(jit_native_modulo_abi, 4);
native_exact_arithmetic_abi!(jit_native_power_abi, 5);
native_exact_arithmetic_abi!(jit_native_exact_bit_and_abi, 6);
native_exact_arithmetic_abi!(jit_native_exact_bit_or_abi, 7);
native_exact_arithmetic_abi!(jit_native_exact_bit_xor_abi, 8);
native_exact_arithmetic_abi!(jit_native_shift_left_abi, 9);
native_exact_arithmetic_abi!(jit_native_shift_right_abi, 10);

fn publish_native_binary_number(
    fast: &mut NativeRequestFastState,
    value: NativeBinaryNumber,
) -> php_jit::JitNativeControlResult {
    let published = match value {
        NativeBinaryNumber::Int(value) => fast.publish_direct_int(value),
        NativeBinaryNumber::Float(value) => fast.publish_direct_float(value),
    };
    published.map_or_else(
        |_| native_binary_runtime_error(),
        php_jit::JitNativeControlResult::returning,
    )
}

fn native_binary_array_key_equal(fast: &NativeRequestFastState, left: i64, right: i64) -> bool {
    match (
        fast.native_comparison_value(left),
        fast.native_comparison_value(right),
    ) {
        (Some(NativeComparisonValue::Int(left)), Some(NativeComparisonValue::Int(right))) => {
            left == right
        }
        (Some(NativeComparisonValue::String(left)), Some(NativeComparisonValue::String(right))) => {
            left == right
        }
        _ => false,
    }
}

fn native_binary_array_union(
    fast: &mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> Result<i64, &'static str> {
    let (left_entries, left_length) = fast
        .stable_native_array_range(left)
        .ok_or("array-union left operand escaped its publication plan")?;
    let (right_entries, right_length) = fast
        .stable_native_array_range(right)
        .ok_or("array-union right operand escaped its publication plan")?;
    let entry_at = |entries: *const php_jit::JitNativeDirectArrayEntry, index: usize| {
        // SAFETY: both operand owners remain live throughout this
        // synchronous binary operation and native arena reservations do
        // not relocate their stable ranges.
        #[allow(unsafe_code)]
        // Safety: the exact native boundary keeps its audited pointer contract.
        unsafe {
            *entries.add(index)
        }
    };
    let key_in_range = |fast: &NativeRequestFastState,
                        entries: *const php_jit::JitNativeDirectArrayEntry,
                        length: usize,
                        key: i64| {
        (0..length)
            .any(|index| native_binary_array_key_equal(fast, entry_at(entries, index).key, key))
    };
    let appended = (0..right_length)
        .filter(|&right_index| {
            let key = entry_at(right_entries, right_index).key;
            !key_in_range(fast, left_entries, left_length, key)
                && !key_in_range(fast, right_entries, right_index, key)
        })
        .count();
    let Some(output_length) = left_length.checked_add(appended) else {
        return Err("native binary array union length overflow");
    };
    fast.publish_owned_direct_array_with(output_length, |fast, output_index| {
        let entry = if output_index < left_length {
            entry_at(left_entries, output_index)
        } else {
            let appended_index = output_index - left_length;
            let source_index = (0..right_length)
                .filter(|&right_index| {
                    let key = entry_at(right_entries, right_index).key;
                    !key_in_range(fast, left_entries, left_length, key)
                        && !key_in_range(fast, right_entries, right_index, key)
                })
                .nth(appended_index)
                .ok_or("native binary array union lost an appended entry")?;
            entry_at(right_entries, source_index)
        };
        fast.retain_direct_encoded(entry.key)?;
        if let Err(error) = fast.retain_direct_encoded(entry.value) {
            fast.rollback_direct_retain(entry.key);
            return Err(error);
        }
        Ok(entry)
    })
}

struct NativeInlineString {
    bytes: [u8; php_runtime::api::PHP_FLOAT_STRING_BUFFER_CAPACITY],
    length: usize,
}

impl NativeInlineString {
    fn new() -> Self {
        Self {
            bytes: [0; php_runtime::api::PHP_FLOAT_STRING_BUFFER_CAPACITY],
            length: 0,
        }
    }

    fn from_float(value: f64) -> Self {
        let mut output = Self::new();
        let rendered = php_runtime::api::float_to_php_string_bytes(value, &mut output.bytes);
        output.length = rendered.len();
        output
    }

    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }
}

impl Write for NativeInlineString {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let end = self.length.checked_add(value.len()).ok_or(fmt::Error)?;
        let output = self.bytes.get_mut(self.length..end).ok_or(fmt::Error)?;
        output.copy_from_slice(value.as_bytes());
        self.length = end;
        Ok(())
    }
}

// architecture: fixed stack storage avoids a heap allocation in native concat
#[allow(clippy::large_enum_variant)]
enum NativeConcatPart {
    /// Raw native arena bytes remain stable for the synchronous exact call.
    Native {
        bytes: *const u8,
        length: usize,
    },
    Inline(NativeInlineString),
}

impl NativeConcatPart {
    fn from_value(fast: &NativeRequestFastState, value: i64) -> Option<Self> {
        let mut inline = NativeInlineString::new();
        match fast.native_comparison_value(value)? {
            NativeComparisonValue::Null | NativeComparisonValue::Bool(false) => {}
            NativeComparisonValue::Bool(true) => inline.write_str("1").ok()?,
            NativeComparisonValue::Int(value) => write!(inline, "{value}").ok()?,
            NativeComparisonValue::Float(value) => inline = NativeInlineString::from_float(value),
            NativeComparisonValue::String(value) => {
                return Some(Self::Native {
                    bytes: value.as_ptr(),
                    length: value.len(),
                });
            }
            NativeComparisonValue::Resource(value) => {
                write!(inline, "Resource id #{value}").ok()?
            }
            NativeComparisonValue::Array { .. }
            | NativeComparisonValue::Object(_)
            | NativeComparisonValue::OpaqueIdentity(_) => return None,
        }
        Some(Self::Inline(inline))
    }

    fn len(&self) -> usize {
        match self {
            Self::Native { length, .. } => *length,
            Self::Inline(value) => value.length,
        }
    }

    #[allow(unsafe_code)] // Safety: native source slots remain live and stable for the synchronous exact call.
    fn copy_to(&self, output: &mut [u8]) {
        match self {
            Self::Native { bytes, length } => {
                debug_assert_eq!(output.len(), *length);
                if *length != 0 {
                    unsafe {
                        std::ptr::copy_nonoverlapping(*bytes, output.as_mut_ptr(), *length);
                    }
                }
            }
            Self::Inline(value) => output.copy_from_slice(value.as_bytes()),
        }
    }
}

type NativeStringRange = (*const u8, usize);

fn native_string_ranges(
    fast: &NativeRequestFastState,
    left: i64,
    right: i64,
) -> Option<(NativeStringRange, NativeStringRange)> {
    match (
        fast.native_comparison_value(left)?,
        fast.native_comparison_value(right)?,
    ) {
        (NativeComparisonValue::String(left), NativeComparisonValue::String(right)) => {
            Some(((left.as_ptr(), left.len()), (right.as_ptr(), right.len())))
        }
        _ => None,
    }
}

pub(crate) extern "C" fn jit_native_array_union_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: publication proved two live direct arrays and reserved the
    // complete result capacity before the optimizing entry was selected.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    native_binary_array_union(unsafe { &mut *runtime }, left, right).map_or_else(
        |_| native_binary_runtime_error(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_concat_abi(
    runtime: *mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: generated code supplies the active request state.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &mut *runtime };
    let Some(left) = NativeConcatPart::from_value(fast, left) else {
        return native_binary_runtime_error();
    };
    let Some(right) = NativeConcatPart::from_value(fast, right) else {
        return native_binary_runtime_error();
    };
    let Some(length) = left.len().checked_add(right.len()) else {
        return native_binary_runtime_error();
    };
    fast.publish_direct_string_with(length, |output| {
        let (left_output, right_output) = output.split_at_mut(left.len());
        left.copy_to(left_output);
        right.copy_to(right_output);
    })
    .map_or_else(
        |_| native_binary_runtime_error(),
        php_jit::JitNativeControlResult::returning,
    )
}

fn native_string_bitwise<const OPERATION: u8>(
    fast: &mut NativeRequestFastState,
    left: i64,
    right: i64,
) -> php_jit::JitNativeControlResult {
    let Some(((left_bytes, left_length), (right_bytes, right_length))) =
        native_string_ranges(fast, left, right)
    else {
        return native_binary_runtime_error();
    };
    let common = left_length.min(right_length);
    let output_length = if matches!(OPERATION, 1 | 7) {
        left_length.max(right_length)
    } else {
        common
    };
    fast.publish_direct_string_with(output_length, |output| {
        // SAFETY: publication keeps both stable source owners live for this
        // synchronous total native call.
        #[allow(unsafe_code)]
        // Safety: the exact native boundary keeps its audited pointer contract.
        let (left, right) = unsafe {
            (
                std::slice::from_raw_parts(left_bytes, left_length),
                std::slice::from_raw_parts(right_bytes, right_length),
            )
        };
        for (output, (left, right)) in output[..common].iter_mut().zip(left.iter().zip(right)) {
            *output = match OPERATION {
                0 | 6 => left & right,
                1 | 7 => left | right,
                2 | 8 => left ^ right,
                _ => unreachable!("fixed string-bit operation"),
            };
        }
        if matches!(OPERATION, 1 | 7) {
            output[common..].copy_from_slice(if left.len() > common {
                &left[common..]
            } else {
                &right[common..]
            });
        }
    })
    .map_or_else(
        |_| native_binary_runtime_error(),
        php_jit::JitNativeControlResult::returning,
    )
}

macro_rules! native_string_bitwise_abi {
    ($name:ident, $operation:literal) => {
        pub(crate) extern "C" fn $name(
            runtime: *mut NativeRequestFastState,
            left: i64,
            right: i64,
        ) -> php_jit::JitNativeControlResult {
            // SAFETY: generated code supplies the active request state.
            #[allow(unsafe_code)]
            // Safety: the exact native boundary keeps its audited pointer contract.
            native_string_bitwise::<$operation>(unsafe { &mut *runtime }, left, right)
        }
    };
}

native_string_bitwise_abi!(jit_native_bit_and_abi, 0);
native_string_bitwise_abi!(jit_native_bit_or_abi, 1);
native_string_bitwise_abi!(jit_native_bit_xor_abi, 2);

fn native_exact_unary_numeric(
    fast: &mut NativeRequestFastState,
    source: i64,
    negate: bool,
) -> php_jit::JitNativeControlResult {
    let Some(value) = fast
        .native_comparison_value(source)
        .and_then(native_binary_number)
    else {
        return native_runtime_contract_violation();
    };
    let result = if negate {
        match value {
            NativeBinaryNumber::Int(i64::MIN) => NativeBinaryNumber::Float(-(i64::MIN as f64)),
            NativeBinaryNumber::Int(value) => NativeBinaryNumber::Int(-value),
            NativeBinaryNumber::Float(value) => NativeBinaryNumber::Float(-value),
        }
    } else {
        value
    };
    publish_native_binary_number(fast, result)
}

pub(crate) extern "C" fn jit_native_unary_plus_abi(
    runtime: *mut NativeRequestFastState,
    source: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: generated code supplies the live request prefix for this fixed
    // synchronous exact operation.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &mut *runtime };
    native_exact_unary_numeric(fast, source, false)
}

pub(crate) extern "C" fn jit_native_unary_minus_abi(
    runtime: *mut NativeRequestFastState,
    source: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: generated code supplies the live request prefix for this fixed
    // synchronous exact operation.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &mut *runtime };
    native_exact_unary_numeric(fast, source, true)
}

pub(crate) extern "C" fn jit_native_bit_not_abi(
    runtime: *mut NativeRequestFastState,
    source: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: generated code supplies the live request prefix for this fixed
    // synchronous exact operation.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &mut *runtime };
    match fast.native_comparison_value(source) {
        Some(NativeComparisonValue::String(bytes)) => {
            let source = (bytes.as_ptr(), bytes.len());
            fast.publish_direct_string_with(source.1, |output| {
                // SAFETY: the source slot is live and stable for this
                // synchronous exact call.
                #[allow(unsafe_code)]
                // Safety: the exact native boundary keeps its audited pointer contract.
                let source = unsafe { std::slice::from_raw_parts(source.0, source.1) };
                for (output, source) in output.iter_mut().zip(source) {
                    *output = !source;
                }
            })
            .map_or_else(
                |_| native_binary_runtime_error(),
                php_jit::JitNativeControlResult::returning,
            )
        }
        Some(NativeComparisonValue::Int(value)) => fast.publish_direct_int(!value).map_or_else(
            |_| native_binary_runtime_error(),
            php_jit::JitNativeControlResult::returning,
        ),
        Some(NativeComparisonValue::Float(value)) => {
            let Some(value) = NativeBinaryNumber::Float(value).exact_integer() else {
                return native_runtime_contract_violation();
            };
            fast.publish_direct_int(!value).map_or_else(
                |_| native_binary_runtime_error(),
                php_jit::JitNativeControlResult::returning,
            )
        }
        Some(
            NativeComparisonValue::Null
            | NativeComparisonValue::Bool(_)
            | NativeComparisonValue::Array { .. }
            | NativeComparisonValue::Object(_)
            | NativeComparisonValue::Resource(_)
            | NativeComparisonValue::OpaqueIdentity(_),
        )
        | None => native_runtime_contract_violation(),
    }
}

pub(super) fn publish_native_array_cast_entries(
    fast: &mut NativeRequestFastState,
    properties: Vec<(String, i64)>,
) -> Result<i64, &'static str> {
    let length = properties.len();
    let mut properties = properties.into_iter();
    fast.publish_owned_direct_array_with(length, |fast, _| {
        let (name, value) = properties
            .next()
            .ok_or("native array-cast property list is truncated")?;
        let published =
            if let Some(integer) = php_runtime::api::array_key_integer_bytes(name.as_bytes()) {
                fast.publish_direct_int(integer)
            } else {
                fast.publish_direct_string_bytes(name.as_bytes())
            };
        let key = published?;
        if let Err(error) = fast.retain_direct_encoded(value) {
            let _ = fast.discard_owned_direct_value(key);
            return Err(error);
        }
        Ok(php_jit::JitNativeDirectArrayEntry { key, value })
    })
}

/// Reads `get_object_vars()` and `get_mangled_object_vars()` directly from the
/// authoritative object property plane.
///
/// Mangled projection is scope independent. Visible projection is exact for
/// public/private properties and global-scope protected omission. The legacy
/// baseline currently grants protected access only to the declaring class,
/// while the native array-cast key intentionally does not encode that owner;
/// scoped objects containing an initialized protected slot therefore take the
/// single baseline continuation instead of guessing.
pub(super) fn native_object_vars_entries(
    fast: &NativeRequestFastState,
    mut object: i64,
    mangled: bool,
) -> Option<Vec<(String, i64)>> {
    for _ in 0..16 {
        let (_, slot) = fast.direct_slot(object)?;
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            && native_reference_state(slot.reserved)
                != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
        {
            object = slot.payload as i64;
            continue;
        }
        break;
    }
    let (_, descriptor) = fast.direct_slot(object)?;
    if descriptor.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
        || !php_jit::jit_native_object_property_view_is_published(descriptor.flags)
    {
        return None;
    }
    let owner = fast.direct_object(object)?;
    let scope_class = fast
        .current_execution_scope()
        .and_then(|scope| scope.scope_class.as_deref());
    owner.with_native_array_cast_view(
        descriptor.payload,
        |declared_names, declared, dynamic_order, dynamic| {
            let mut result = Vec::with_capacity(declared.len().saturating_add(dynamic_order.len()));
            for (name, slot) in declared_names.iter().zip(declared) {
                if slot.initialized == 0 {
                    continue;
                }
                if mangled || !name.starts_with('\0') {
                    result.push((name.clone(), slot.value));
                    continue;
                }
                let rest = name.strip_prefix('\0')?;
                let (owner_name, visible_name) = rest.split_once('\0')?;
                if owner_name == "*" {
                    if scope_class.is_some() {
                        return None;
                    }
                    continue;
                }
                if scope_class.is_some_and(|scope| scope.eq_ignore_ascii_case(owner_name)) {
                    result.push((visible_name.to_owned(), slot.value));
                }
            }
            result.extend(dynamic_order.iter().filter_map(|name| {
                dynamic
                    .get(name)
                    .filter(|cell| cell.slot.initialized != 0)
                    .map(|cell| (name.clone(), cell.slot.value))
            }));
            Some(result)
        },
    )?
}

/// Implements PHP's complete `(array)` conversion family over authoritative
/// native values. Arrays retain COW identity, objects expose property order
/// with PHP visibility key encoding, null becomes empty, and every other
/// admitted value becomes element zero.
pub(crate) extern "C" fn jit_native_array_cast_abi(
    runtime: *mut NativeRequestFastState,
    mut source: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &mut *runtime };
    for _ in 0..16 {
        let Some((_, slot)) = fast.direct_slot(source) else {
            break;
        };
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            && native_reference_state(slot.reserved)
                != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
        {
            source = slot.payload as i64;
            continue;
        }
        if matches!(
            slot.kind,
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                | php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
        ) {
            return native_runtime_contract_violation();
        }
        break;
    }

    enum ArrayCastSource {
        Identity,
        Empty,
        Scalar,
        Object {
            owner: php_runtime::api::ObjectRef,
            layout_id: u64,
        },
    }
    let source_kind = match fast.native_comparison_value(source) {
        Some(NativeComparisonValue::Array { .. }) => ArrayCastSource::Identity,
        Some(NativeComparisonValue::Null) => ArrayCastSource::Empty,
        Some(NativeComparisonValue::Object(object)) => {
            let Some(layout_id) = object.layout_id else {
                return native_runtime_contract_violation();
            };
            ArrayCastSource::Object {
                owner: object.owner.clone(),
                layout_id,
            }
        }
        Some(NativeComparisonValue::OpaqueIdentity(_)) => {
            match fast.direct_slot(source).map(|(_, slot)| slot.kind) {
                Some(
                    php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER
                    | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_GENERATOR,
                ) => ArrayCastSource::Empty,
                Some(php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE) => ArrayCastSource::Scalar,
                _ => return native_runtime_contract_violation(),
            }
        }
        Some(
            NativeComparisonValue::Bool(_)
            | NativeComparisonValue::Int(_)
            | NativeComparisonValue::Float(_)
            | NativeComparisonValue::String(_)
            | NativeComparisonValue::Resource(_),
        ) => ArrayCastSource::Scalar,
        None => return native_runtime_contract_violation(),
    };

    let result = match source_kind {
        ArrayCastSource::Identity => {
            return match fast.retain_direct_encoded(source) {
                Ok(()) => php_jit::JitNativeControlResult::returning(source),
                Err(_) => php_jit::JitNativeControlResult::control(
                    php_jit::JitCallStatus::RUNTIME_ERROR,
                    0,
                    0,
                ),
            };
        }
        ArrayCastSource::Empty => fast.publish_retained_direct_array_from_iter(std::iter::empty()),
        ArrayCastSource::Scalar => fast.publish_retained_direct_array_from_iter(std::iter::once(
            php_jit::JitNativeDirectArrayEntry {
                key: 0,
                value: source,
            },
        )),
        ArrayCastSource::Object { owner, layout_id } => {
            let Some(properties) = owner.with_native_array_cast_view(
                layout_id,
                |declared_names, declared, dynamic_order, dynamic| {
                    declared_names
                        .iter()
                        .zip(declared)
                        .filter(|(_, slot)| slot.initialized != 0)
                        .map(|(name, slot)| (name.clone(), slot.value))
                        .chain(dynamic_order.iter().filter_map(|name| {
                            dynamic
                                .get(name)
                                .filter(|cell| cell.slot.initialized != 0)
                                .map(|cell| (name.clone(), cell.slot.value))
                        }))
                        .collect::<Vec<_>>()
                },
            ) else {
                return native_runtime_contract_violation();
            };
            publish_native_array_cast_entries(fast, properties)
        }
    };
    match result {
        Ok(array) => php_jit::JitNativeControlResult::returning(array),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Exact explicit integer cast for publication-admitted scalar and array
/// values. Object-like shapes are rejected before optimizer entry; strings
/// and every float payload use PHP's shared numeric conversion directly.
fn native_int_cast_value(fast: &NativeRequestFastState, source: i64) -> Option<i64> {
    Some(match fast.native_comparison_value(source)? {
        NativeComparisonValue::Null => 0,
        NativeComparisonValue::Bool(value) => i64::from(value),
        NativeComparisonValue::Int(value) => value,
        NativeComparisonValue::Float(value) => php_runtime::api::php_float_to_int(value),
        NativeComparisonValue::String(bytes) => {
            php_runtime::experimental::numeric_string::classify(bytes)
                .value
                .map_or(0, |value| value.to_i64())
        }
        NativeComparisonValue::Array { entries, .. } => i64::from(!entries.is_empty()),
        NativeComparisonValue::Resource(value) => value as i64,
        NativeComparisonValue::Object(_) | NativeComparisonValue::OpaqueIdentity(_) => return None,
    })
}

fn publish_native_int_cast(
    fast: &mut NativeRequestFastState,
    value: i64,
) -> php_jit::JitNativeControlResult {
    match fast.publish_direct_int(value) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

pub(crate) extern "C" fn jit_native_int_cast_abi(
    runtime: *mut NativeRequestFastState,
    source: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &mut *runtime };
    let Some(value) = native_int_cast_value(fast, source) else {
        return native_runtime_contract_violation();
    };
    publish_native_int_cast(fast, value)
}

/// Exact explicit float cast for publication-admitted scalar and array values.
/// Object-like shapes are rejected before optimizer entry; native strings use
/// the shared numeric parser without reconstructing a runtime value.
pub(crate) extern "C" fn jit_native_float_cast_abi(
    runtime: *mut NativeRequestFastState,
    source: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &mut *runtime };
    let value = match fast.native_comparison_value(source) {
        Some(NativeComparisonValue::Null) => 0.0,
        Some(NativeComparisonValue::Bool(value)) => f64::from(u8::from(value)),
        Some(NativeComparisonValue::Int(value)) => value as f64,
        Some(NativeComparisonValue::Float(value)) => value,
        Some(NativeComparisonValue::String(bytes)) => {
            php_runtime::experimental::numeric_string::classify(bytes)
                .value
                .map_or(0.0, |value| value.as_f64())
        }
        Some(NativeComparisonValue::Array { entries, .. }) => {
            f64::from(u8::from(!entries.is_empty()))
        }
        Some(NativeComparisonValue::Resource(value)) => value as f64,
        Some(NativeComparisonValue::Object(_) | NativeComparisonValue::OpaqueIdentity(_))
        | None => return native_runtime_contract_violation(),
    };
    match fast.publish_direct_float(value) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Implements PHP's scalar `(string)` conversion family over authoritative
/// native values. Arrays and object-like values are assigned to baseline
/// before optimizer entry because their warning/`__toString` semantics require
/// the source span and full PHP call machinery.
pub(crate) extern "C" fn jit_native_string_cast_abi(
    runtime: *mut NativeRequestFastState,
    mut source: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &mut *runtime };
    for _ in 0..16 {
        let Some((_, slot)) = fast.direct_slot(source) else {
            break;
        };
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            && native_reference_state(slot.reserved)
                != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
        {
            source = slot.payload as i64;
            continue;
        }
        if matches!(
            slot.kind,
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                | php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
        ) {
            return native_runtime_contract_violation();
        }
        break;
    }

    let rendered = match NativeConcatPart::from_value(fast, source) {
        Some(NativeConcatPart::Native { .. }) => {
            return match fast.retain_direct_encoded(source) {
                Ok(()) => php_jit::JitNativeControlResult::returning(source),
                Err(_) => php_jit::JitNativeControlResult::control(
                    php_jit::JitCallStatus::RUNTIME_ERROR,
                    0,
                    0,
                ),
            };
        }
        Some(NativeConcatPart::Inline(rendered)) => rendered,
        None => return native_runtime_contract_violation(),
    };
    match fast.publish_direct_string_with(rendered.length, |output| {
        output.copy_from_slice(rendered.as_bytes());
    }) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Coerces an already-returned scalar callback value to the exact replacement
/// string representation. This boundary deliberately has no baseline outcome:
/// replaying the containing operation would execute PHP-visible callback
/// effects twice. Admission proves a non-reference scalar return before the
/// callback is entered; a representation-contract violation is terminal.
pub(crate) extern "C" fn jit_native_callback_return_string_abi(
    runtime: *mut NativeRequestFastState,
    source: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &mut *runtime };
    let rendered = match NativeConcatPart::from_value(fast, source) {
        Some(NativeConcatPart::Native { .. }) => {
            return match fast.retain_direct_encoded(source) {
                Ok(()) => php_jit::JitNativeControlResult::returning(source),
                Err(_) => php_jit::JitNativeControlResult::control(
                    php_jit::JitCallStatus::RUNTIME_ERROR,
                    0,
                    0,
                ),
            };
        }
        Some(NativeConcatPart::Inline(rendered)) => rendered,
        None => {
            return php_jit::JitNativeControlResult::control(
                php_jit::JitCallStatus::RUNTIME_ERROR,
                0,
                0,
            );
        }
    };
    match fast.publish_direct_string_with(rendered.length, |output| {
        output.copy_from_slice(rendered.as_bytes());
    }) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

pub(super) fn native_object_cast_stdclass() -> php_runtime::api::ObjectRef {
    static STDCLASS_NAME: std::sync::OnceLock<std::sync::Arc<str>> = std::sync::OnceLock::new();
    let class = php_runtime::api::ClassEntry {
        name: std::sync::Arc::clone(STDCLASS_NAME.get_or_init(|| std::sync::Arc::from("stdclass"))),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: php_runtime::api::ClassFlags {
            has_complete_method_table: true,
            ..php_runtime::api::ClassFlags::default()
        },
    };
    php_runtime::api::ObjectRef::from_layout_native_slots(&class, "stdClass", Box::new([]))
}

/// Implements PHP's complete `(object)` conversion family over authoritative
/// native values. Objects preserve identity, arrays become insertion-ordered
/// stdClass properties, null becomes an empty stdClass, and every other
/// admitted value is stored in the `scalar` property. No compatibility graph is
/// decoded or encoded on this path.
pub(crate) extern "C" fn jit_native_object_cast_abi(
    runtime: *mut NativeRequestFastState,
    mut source: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: generated code supplies the live request prefix and the exact
    // call is synchronous with the published native arenas.
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &mut *runtime };

    for _ in 0..16 {
        let Some((_, slot)) = fast.direct_slot(source) else {
            break;
        };
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            && native_reference_state(slot.reserved)
                != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
        {
            source = slot.payload as i64;
            continue;
        }
        if matches!(
            slot.kind,
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                | php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
        ) {
            return native_runtime_contract_violation();
        }
        break;
    }

    enum ObjectCastSource {
        Identity,
        Empty,
        Scalar,
        Array {
            entries: *const php_jit::JitNativeDirectArrayEntry,
            length: usize,
        },
    }

    let source_kind = if let Some((entries, length)) = fast.stable_native_array_range(source) {
        ObjectCastSource::Array { entries, length }
    } else {
        match fast.native_comparison_value(source) {
            Some(NativeComparisonValue::Object(_)) => ObjectCastSource::Identity,
            Some(NativeComparisonValue::Null) => ObjectCastSource::Empty,
            Some(NativeComparisonValue::Array { .. }) => {
                return native_runtime_contract_violation();
            }
            Some(NativeComparisonValue::Float(value)) if value.is_nan() => {
                return native_runtime_contract_violation();
            }
            Some(
                NativeComparisonValue::Bool(_)
                | NativeComparisonValue::Int(_)
                | NativeComparisonValue::Float(_)
                | NativeComparisonValue::String(_)
                | NativeComparisonValue::OpaqueIdentity(_)
                | NativeComparisonValue::Resource(_),
            ) => ObjectCastSource::Scalar,
            None => return native_runtime_contract_violation(),
        }
    };

    if matches!(source_kind, ObjectCastSource::Identity) {
        return match fast.retain_direct_encoded(source) {
            Ok(()) => php_jit::JitNativeControlResult::returning(source),
            Err(_) => php_jit::JitNativeControlResult::control(
                php_jit::JitCallStatus::RUNTIME_ERROR,
                0,
                0,
            ),
        };
    }

    let object = native_object_cast_stdclass();
    let layout_id = object.class_layout_epoch();
    let mut retained = Vec::new();
    let mut properties = Vec::new();
    match source_kind {
        ObjectCastSource::Empty => {}
        ObjectCastSource::Scalar => properties.push(("scalar".to_owned(), source)),
        ObjectCastSource::Array { entries, length } => {
            let mut names = std::collections::HashSet::with_capacity(length);
            for index in 0..length {
                // SAFETY: the source owner remains live throughout this
                // synchronous cast and object allocation does not relocate
                // the request's stable native array arena.
                #[allow(unsafe_code)]
                // Safety: the exact native boundary keeps its audited pointer contract.
                let entry = unsafe { *entries.add(index) };
                let name = match fast.native_comparison_value(entry.key) {
                    Some(NativeComparisonValue::Int(key)) => key.to_string(),
                    Some(NativeComparisonValue::String(key)) => {
                        String::from_utf8_lossy(key).into_owned()
                    }
                    _ => return native_runtime_contract_violation(),
                };
                if !names.insert(name.clone()) {
                    return native_runtime_contract_violation();
                }
                properties.push((name, entry.value));
            }
        }
        ObjectCastSource::Identity => unreachable!("identity returned before allocation"),
    }

    for (name, value) in properties {
        if fast.retain_direct_encoded(value).is_err() {
            for retained_value in retained {
                fast.rollback_direct_retain(retained_value);
            }
            return php_jit::JitNativeControlResult::control(
                php_jit::JitCallStatus::RUNTIME_ERROR,
                0,
                0,
            );
        }
        retained.push(value);
        let slot = php_runtime::api::NativeDeclaredPropertySlot {
            initialized: 1,
            reserved: 0,
            value,
        };
        if object
            .set_native_dynamic_property(layout_id, name, slot)
            .is_err()
        {
            for retained_value in retained {
                fast.rollback_direct_retain(retained_value);
            }
            return php_jit::JitNativeControlResult::control(
                php_jit::JitCallStatus::RUNTIME_ERROR,
                0,
                0,
            );
        }
    }

    match fast.publish_direct_object(object) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            for retained_value in retained {
                fast.rollback_direct_retain(retained_value);
            }
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Allocates one object from a request-published immutable class layout. The
/// opaque plan pointer was resolved and validated before native execution;
/// this call performs no class lookup, flag check, or compatibility graph encode.
#[allow(unsafe_code)] // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
pub(crate) extern "C" fn jit_native_prepared_object_new_abi(
    runtime: *mut NativeRequestFastState,
    prepared: u64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: the active trusted-class table contains pointers owned by an Rc
    // in `runtime_class_cache`; request activation and scoped unit switching
    // keep that owner alive for this synchronous exact call.
    let prepared = unsafe { &*(prepared as usize as *const PreparedNativeRuntimeClass) };
    let result = (|| {
        let fast = unsafe { &mut *runtime };
        let slots = prepared.default_native_slots.clone();
        let mut retained = Vec::new();
        for slot in slots.iter().filter(|slot| slot.initialized != 0) {
            if let Err(error) = fast.retain_direct_encoded(slot.value) {
                for value in retained {
                    fast.rollback_direct_retain(value);
                }
                return Err(error);
            }
            retained.push(slot.value);
        }
        let object = php_runtime::api::ObjectRef::from_layout_native_slots(
            &prepared.entry,
            prepared.display_name.clone(),
            slots,
        );
        debug_assert_eq!(object.class_layout_epoch(), prepared.layout_id);
        match fast.publish_direct_object(object) {
            Ok(value) => Ok(value),
            Err(error) => {
                for value in retained {
                    fast.rollback_direct_retain(value);
                }
                Err(error)
            }
        }
    })();
    match result {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Constructs one internal throwable from immutable publication metadata and
/// authoritative native fields. The exact handler never recovers the cold
/// coordinator, decodes a compatibility graph, or resolves class/source metadata.
fn publish_prepared_throwable_trace(
    fast: &mut NativeRequestFastState,
    prepared: &PreparedNativeThrowableSite,
) -> Result<i64, &'static str> {
    if !prepared.include_function_frame {
        return fast.publish_owned_direct_array_from_iter(std::iter::empty());
    }

    let mut owned = Vec::with_capacity(4);
    let result = (|| {
        let function_key = fast.publish_direct_string_bytes(b"function")?;
        owned.push(function_key);
        let function = fast.publish_direct_string_bytes(prepared.function_name.as_bytes())?;
        owned.push(function);
        let arguments_key = fast.publish_direct_string_bytes(b"args")?;
        owned.push(arguments_key);
        let arguments = fast.native_func_get_args()?;
        owned.push(arguments);
        let entries = [
            php_jit::JitNativeDirectArrayEntry {
                key: function_key,
                value: function,
            },
            php_jit::JitNativeDirectArrayEntry {
                key: arguments_key,
                value: arguments,
            },
        ];
        // Ownership of every key/value moves into the frame on both success
        // and failure; `publish_owned_direct_array` performs rollback.
        owned.clear();
        let frame = fast.publish_owned_direct_array_from_iter(entries.into_iter())?;
        let entries = [php_jit::JitNativeDirectArrayEntry {
            key: 0,
            value: frame,
        }];
        fast.publish_owned_direct_array_from_iter(entries.into_iter())
    })();
    if result.is_err() {
        for value in owned.into_iter().rev() {
            let _ = fast.discard_owned_direct_value(value);
        }
    }
    result
}

pub(super) fn publish_prepared_exception(
    fast: &mut NativeRequestFastState,
    prepared: &PreparedNativeThrowableSite,
    message: &[u8],
    code: i64,
    previous: i64,
) -> php_jit::JitNativeControlResult {
    let mut owned = Vec::with_capacity(4);
    let mut retained = Vec::with_capacity(2);
    let result = (|| {
        let message = fast.publish_direct_string_bytes(message)?;
        owned.push(message);
        let file = fast.publish_direct_string_bytes(&prepared.file)?;
        owned.push(file);
        let line = fast.publish_direct_int(prepared.line)?;
        if php_jit::jit_decode_runtime_value(line).is_some() {
            owned.push(line);
        }
        let trace = publish_prepared_throwable_trace(fast, prepared)?;
        owned.push(trace);
        for value in [code, previous] {
            fast.retain_direct_encoded(value)?;
            retained.push(value);
        }

        let object = php_runtime::api::ObjectRef::from_layout_native_slots(
            &prepared.runtime_class,
            prepared.display_name.clone(),
            Box::new([]),
        );
        let layout_id = object.class_layout_epoch();
        let fields = [
            ("message", message),
            ("file", file),
            ("line", line),
            ("code", code),
            ("previous", previous),
            ("trace", trace),
        ];
        for (name, value) in fields {
            let slot = php_runtime::api::NativeDeclaredPropertySlot {
                initialized: 1,
                reserved: 0,
                value,
            };
            object
                .set_native_dynamic_property(layout_id, name.to_owned(), slot)
                .map_err(|_| "native throwable property publication failed")?;
        }
        fast.publish_direct_object(object)
    })();

    match result {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            for value in retained {
                fast.rollback_direct_retain(value);
            }
            for value in owned {
                let _ = fast.discard_owned_direct_value(value);
            }
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

#[allow(unsafe_code)] // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
pub(crate) extern "C" fn jit_native_prepared_exception_new_abi(
    runtime: *mut NativeRequestFastState,
    prepared: u64,
    message: i64,
    code: i64,
    previous: i64,
) -> php_jit::JitNativeControlResult {
    if runtime.is_null() || prepared == 0 {
        return native_runtime_contract_violation();
    }
    // SAFETY: publication owns the plan for the lifetime of the active unit
    // view. Generated lowering loads the pointer from the exact continuation
    // table and forwards it synchronously.
    let prepared = unsafe { &*(prepared as usize as *const PreparedNativeThrowableSite) };
    let fast = unsafe { &mut *runtime };
    if previous != php_jit::jit_encode_constant(u32::MAX) {
        let Some(previous_object) = fast.direct_object(previous) else {
            return native_runtime_contract_violation();
        };
        if !matches!(
            php_ir::module::normalize_class_name(&previous_object.class_name()).as_str(),
            "exception"
                | "logicexception"
                | "badfunctioncallexception"
                | "badmethodcallexception"
                | "domainexception"
                | "invalidargumentexception"
                | "lengthexception"
                | "outofrangeexception"
                | "runtimeexception"
                | "outofboundsexception"
                | "overflowexception"
                | "rangeexception"
                | "underflowexception"
                | "unexpectedvalueexception"
                | "error"
                | "compileerror"
                | "parseerror"
                | "typeerror"
                | "argumentcounterror"
                | "valueerror"
                | "arithmeticerror"
                | "divisionbyzeroerror"
                | "unhandledmatcherror"
                | "fibererror"
        ) {
            return native_runtime_contract_violation();
        }
    }
    let message = match fast.native_printf_scalar(message) {
        Some(php_runtime::api::NativePrintfScalar::String(bytes)) => bytes.to_vec(),
        Some(php_runtime::api::NativePrintfScalar::Int(value)) => value.to_string().into_bytes(),
        Some(php_runtime::api::NativePrintfScalar::Float(value)) => value.to_string().into_bytes(),
        Some(php_runtime::api::NativePrintfScalar::Bool(true)) => b"1".to_vec(),
        Some(
            php_runtime::api::NativePrintfScalar::Bool(false)
            | php_runtime::api::NativePrintfScalar::Null,
        ) => Vec::new(),
        None => return native_runtime_contract_violation(),
    };
    publish_prepared_exception(fast, prepared, &message, code, previous)
}

/// Allocates one closure from immutable callsite metadata and authoritative
/// native captures. Generated code has already applied by-value/by-reference
/// binding and transferred one owner per capture; this exact handler never
/// constructs a compatibility graph or enters the generic dynamic-code executor.
#[allow(unsafe_code)] // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
pub(crate) extern "C" fn jit_native_prepared_closure_new_abi(
    runtime: *mut NativeRequestFastState,
    prepared: u64,
    captures: *const i64,
    implicit_this: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: generated code loads this opaque pointer from the active
    // compiled unit's trusted closure-plan table.
    let prepared =
        unsafe { &*(prepared as usize as *const crate::compiled_unit::PreparedNativeClosureSite) };
    let fast = unsafe { &mut *runtime };
    let capture_values = if prepared.capture_descriptors.is_empty() {
        &[]
    } else {
        // SAFETY: lowering allocates one packed stack word per immutable
        // descriptor and keeps the stack slot live for this synchronous call.
        unsafe { std::slice::from_raw_parts(captures, prepared.capture_descriptors.len()) }
    };
    let Some(scope) = fast.current_execution_scope().cloned() else {
        for capture in capture_values.iter().copied() {
            fast.rollback_direct_retain(capture);
        }
        if prepared.binds_this {
            fast.rollback_direct_retain(implicit_this);
        }
        return php_jit::JitNativeControlResult::control(
            php_jit::JitCallStatus::ABI_MISMATCH,
            0,
            0,
        );
    };
    let scope_class = scope.scope_class;
    let context = php_runtime::api::ClosureContext {
        owner_unit: scope.unit,
        called_class: scope.called_class.or_else(|| scope_class.clone()),
        scope_class: scope_class.clone(),
        declaring_class: scope_class,
    };
    let closure = php_runtime::api::ClosurePayload::new(prepared.function.raw(), Vec::new())
        .with_debug(prepared.debug.clone())
        .with_context(context);
    let prepared_closure = NativePreparedClosure::new(
        closure,
        Arc::clone(&prepared.capture_descriptors),
        prepared.binds_this.then_some(implicit_this),
        capture_values.to_vec().into_boxed_slice(),
        prepared.fixed_visible_arity,
        prepared.first_parameter_by_reference,
        prepared.returns_int,
        prepared.returns_string,
        prepared.returns_releasable_scalar,
    );
    match fast.publish_prepared_closure_owned(prepared_closure) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Shallow-clones one direct object whose exact class was proven not to have
/// `__clone`; no runtime method lookup or compatibility decode occurs here.
pub(crate) extern "C" fn jit_native_plain_object_clone_abi(
    runtime: *mut NativeRequestFastState,
    object: i64,
) -> php_jit::JitNativeControlResult {
    enum PlainCloneOutcome {
        Returned(i64),
        ContractViolation,
        Error,
    }

    // SAFETY: the exact call executes synchronously with one published fast
    // state. All owner/slot pointers remain stable for its duration.
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let outcome = (|| {
        let fast = unsafe { &mut *runtime };
        let Some((_, descriptor)) = fast.direct_slot(object) else {
            return PlainCloneOutcome::Error;
        };
        let Some(object) = fast.direct_object(object).cloned() else {
            return PlainCloneOutcome::Error;
        };
        if !php_jit::jit_native_object_property_view_is_published(descriptor.flags) {
            return PlainCloneOutcome::ContractViolation;
        }
        let Some((slots, dynamic)) = object.clone_native_property_slots(descriptor.payload) else {
            return PlainCloneOutcome::ContractViolation;
        };
        let mut retained = Vec::new();
        for slot in slots.iter().filter(|slot| slot.initialized != 0).chain(
            dynamic
                .values()
                .filter(|cell| cell.slot.initialized != 0)
                .map(|cell| &cell.slot),
        ) {
            if fast.retain_direct_encoded(slot.value).is_err() {
                for value in retained {
                    fast.rollback_direct_retain(value);
                }
                return PlainCloneOutcome::Error;
            }
            retained.push(slot.value);
        }
        let clone = object.clone_shallow();
        if clone
            .install_native_property_slots(descriptor.payload, slots, dynamic)
            .is_err()
        {
            for value in retained {
                fast.rollback_direct_retain(value);
            }
            return PlainCloneOutcome::Error;
        }
        match fast.publish_direct_object(clone) {
            Ok(value) => PlainCloneOutcome::Returned(value),
            Err(_) => {
                for value in retained {
                    fast.rollback_direct_retain(value);
                }
                PlainCloneOutcome::Error
            }
        }
    })();
    match outcome {
        PlainCloneOutcome::Returned(value) => php_jit::JitNativeControlResult::returning(value),
        PlainCloneOutcome::ContractViolation => native_runtime_contract_violation(),
        PlainCloneOutcome::Error => {
            php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
        }
    }
}

/// Resolves the stable authoritative cell for one undeclared property. A
/// missing stdClass name reserves an uninitialized tombstone; generated code
/// performs the complete operation directly on that cell.
pub(crate) extern "C" fn jit_native_dynamic_property_slot_abi(
    runtime: *mut NativeRequestFastState,
    object: i64,
    property: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &mut *runtime };
    let Some(property) = fast.exact_dynamic_property_name(property) else {
        return native_runtime_contract_violation();
    };
    let Some(slot) = fast.exact_dynamic_property_slot_location(object, property) else {
        return native_runtime_contract_violation();
    };
    php_jit::JitNativeControlResult::returning(slot as usize as i64)
}

/// Resolves the same authoritative dynamic cell for a fixed property name
/// published as an immutable UTF-8 byte view with the compiled callsite.
pub(crate) extern "C" fn jit_native_named_dynamic_property_slot_abi(
    runtime: *mut NativeRequestFastState,
    object: i64,
    name_bytes: i64,
    name_length: i64,
) -> php_jit::JitNativeControlResult {
    if runtime.is_null() || name_bytes == 0 || name_length < 0 {
        return native_runtime_contract_violation();
    }
    let Ok(name_length) = usize::try_from(name_length) else {
        return native_runtime_contract_violation();
    };
    // SAFETY: request publication retains the Arc<str> that owns this exact
    // byte range for the full lifetime of every generated activation.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let name = unsafe { std::slice::from_raw_parts(name_bytes as usize as *const u8, name_length) };
    let Ok(name) = std::str::from_utf8(name) else {
        return native_runtime_contract_violation();
    };
    // SAFETY: generated code passes its request-owned state synchronously.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &mut *runtime };
    let Some(slot) = fast.exact_named_dynamic_property_slot_location(object, name) else {
        return native_runtime_contract_violation();
    };
    php_jit::JitNativeControlResult::returning(slot as usize as i64)
}

/// Resolves the stable authoritative cell for a non-mutating dynamic property
/// test. Missing ordinary names return the request's immutable absence cell;
/// declared visibility and `__isset` shapes take one baseline continuation.
pub(crate) extern "C" fn jit_native_dynamic_property_test_slot_abi(
    runtime: *mut NativeRequestFastState,
    object: i64,
    property: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &mut *runtime };
    let Some(property) = fast.exact_dynamic_property_name(property) else {
        return native_runtime_contract_violation();
    };
    let property_pointer = property.as_ptr();
    let property_length = property.len();
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let property = unsafe {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(
            property_pointer,
            property_length,
        ))
    };
    let Some(slot) = fast.exact_dynamic_property_test_slot_location(object, property) else {
        return native_runtime_contract_violation();
    };
    php_jit::JitNativeControlResult::returning(slot as usize as i64)
}

fn native_throwable_property(
    runtime: *mut NativeRequestFastState,
    object: i64,
    property: &'static str,
) -> php_jit::JitNativeControlResult {
    if runtime.is_null() {
        return native_runtime_contract_violation();
    }
    // SAFETY: generated code passes its request-owned fast state and an
    // authoritative object encoding for this synchronous exact accessor.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let fast = unsafe { &mut *runtime };
    let Some(owner) = fast.direct_object(object) else {
        return native_runtime_contract_violation();
    };
    let _ = owner;
    let Some(slot) = fast.exact_dynamic_property_slot_location(object, property) else {
        return native_runtime_contract_violation();
    };
    // SAFETY: the direct object owns this stable native property cell for the
    // request lifetime. Throwable construction initializes every accessor
    // field before publishing the object descriptor.
    #[allow(unsafe_code)] // Safety: the exact native boundary keeps its audited pointer contract.
    let slot = unsafe { &*slot };
    if slot.initialized == 0 {
        return native_runtime_contract_violation();
    }
    let value = slot.value;
    if fast.retain_direct_encoded(value).is_err() {
        return native_runtime_contract_violation();
    }
    php_jit::JitNativeControlResult::returning(value)
}

macro_rules! throwable_property_accessor {
    ($function:ident, $property:literal) => {
        pub(crate) extern "C" fn $function(
            runtime: *mut NativeRequestFastState,
            object: i64,
        ) -> php_jit::JitNativeControlResult {
            native_throwable_property(runtime, object, $property)
        }
    };
}

throwable_property_accessor!(jit_native_throwable_get_message_abi, "message");
throwable_property_accessor!(jit_native_throwable_get_code_abi, "code");
throwable_property_accessor!(jit_native_throwable_get_file_abi, "file");
throwable_property_accessor!(jit_native_throwable_get_line_abi, "line");
throwable_property_accessor!(jit_native_throwable_get_previous_abi, "previous");
throwable_property_accessor!(jit_native_throwable_get_trace_abi, "trace");
