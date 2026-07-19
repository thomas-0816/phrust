use super::*;

fn helper_status(status: php_runtime::api::NativeOperationStatus) -> i32 {
    if status == php_runtime::api::NativeOperationStatus::Ok {
        0
    } else {
        php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
    }
}

const fn unary_operation_name(op: u32) -> &'static str {
    match op {
        0 => "unary_plus",
        1 => "unary_minus",
        2 => "unary_not",
        _ => "unary_bit_not",
    }
}

const fn binary_operation_name(op: u32) -> &'static str {
    match op {
        0 => "binary_add",
        1 => "binary_sub",
        2 => "binary_mul",
        3 => "binary_div",
        4 => "binary_mod",
        5 => "binary_concat",
        6 => "binary_pow",
        7 => "binary_bit_and",
        8 => "binary_bit_or",
        9 => "binary_bit_xor",
        10 => "binary_shift_left",
        _ => "binary_shift_right",
    }
}

const fn compare_operation_name(op: u32) -> &'static str {
    match op {
        0 => "compare_equal",
        1 => "compare_not_equal",
        2 => "compare_identical",
        3 => "compare_not_identical",
        4 => "compare_less",
        5 => "compare_less_equal",
        6 => "compare_greater",
        7 => "compare_greater_equal",
        _ => "compare_spaceship",
    }
}

const fn cast_operation_name(op: u32) -> &'static str {
    match op {
        0 => "cast_bool",
        1 => "cast_int",
        2 => "cast_float",
        3 => "cast_string",
        4 => "cast_array",
        5 => "cast_object",
        _ => "cast_void",
    }
}

fn dereference_native_dimension_value(mut value: Value) -> Value {
    for _ in 0..16 {
        let Value::Reference(reference) = value else {
            break;
        };
        value = reference.get();
    }
    value
}

pub(super) fn fast_plain_local_fetch(value: i64, quiet: bool) -> Option<i64> {
    if php_jit::jit_decode_runtime_value(value).is_some() {
        return None;
    }
    match php_jit::jit_decode_constant(value) {
        None | Some(u32::MAX) | Some(php_jit::JIT_VALUE_FALSE) | Some(php_jit::JIT_VALUE_TRUE) => {
            Some(value)
        }
        Some(php_jit::JIT_VALUE_UNINITIALIZED) => {
            quiet.then(|| php_jit::jit_encode_constant(u32::MAX))
        }
        Some(_) => None,
    }
}

fn immediate_integer(value: i64) -> Option<i64> {
    (php_jit::jit_decode_constant(value).is_none()
        && php_jit::jit_decode_runtime_value(value).is_none())
    .then_some(value)
}

fn encoded_bool(value: bool) -> i64 {
    php_jit::jit_encode_constant(if value {
        php_jit::JIT_VALUE_TRUE
    } else {
        php_jit::JIT_VALUE_FALSE
    })
}

pub(super) fn fast_native_truthy(value: i64) -> Option<bool> {
    if let Some(value) = immediate_integer(value) {
        return Some(value != 0);
    }
    match php_jit::jit_decode_constant(value) {
        Some(u32::MAX | php_jit::JIT_VALUE_UNINITIALIZED | php_jit::JIT_VALUE_FALSE) => Some(false),
        Some(php_jit::JIT_VALUE_TRUE) => Some(true),
        _ => None,
    }
}

pub(super) fn fast_native_unary(op: u32, src: i64) -> Option<i64> {
    let op = if op & 0x8000_0000 != 0 { op & 0x3 } else { op };
    if op == 2 {
        return fast_native_truthy(src).map(|value| encoded_bool(!value));
    }
    let src = immediate_integer(src)?;
    match op {
        0 => Some(src),
        1 => src.checked_neg(),
        3 => Some(!src),
        _ => None,
    }
}

pub(super) fn fast_native_binary(op: u32, lhs: i64, rhs: i64) -> Option<i64> {
    let lhs = immediate_integer(lhs)?;
    let rhs = immediate_integer(rhs)?;
    match op {
        0 => lhs.checked_add(rhs),
        1 => lhs.checked_sub(rhs),
        2 => lhs.checked_mul(rhs),
        3 if rhs != 0 && !(lhs == i64::MIN && rhs == -1) && lhs % rhs == 0 => Some(lhs / rhs),
        4 if rhs != 0 => Some(lhs.checked_rem(rhs).unwrap_or(0)),
        6 if rhs >= 0 => u32::try_from(rhs)
            .ok()
            .and_then(|exponent| lhs.checked_pow(exponent)),
        7 => Some(lhs & rhs),
        8 => Some(lhs | rhs),
        9 => Some(lhs ^ rhs),
        10 if rhs >= 0 => Some(lhs.wrapping_shl(rhs as u32)),
        11 if rhs >= 0 => Some(lhs.wrapping_shr(rhs as u32)),
        _ => None,
    }
}

pub(super) fn fast_native_compare(op: u32, lhs: i64, rhs: i64) -> Option<i64> {
    let lhs = immediate_integer(lhs)?;
    let rhs = immediate_integer(rhs)?;
    let result = match op {
        0 | 2 => lhs == rhs,
        1 | 3 => lhs != rhs,
        4 => lhs < rhs,
        5 => lhs <= rhs,
        6 => lhs > rhs,
        7 => lhs >= rhs,
        8 => {
            return Some(match lhs.cmp(&rhs) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            });
        }
        _ => return None,
    };
    Some(encoded_bool(result))
}

pub(super) fn fast_native_cast(op: u32, src: i64) -> Option<i64> {
    let op = if op & 0x8000_0000 != 0 { op & 0x7 } else { op };
    match op {
        0 => fast_native_truthy(src).map(encoded_bool),
        1 => {
            if let Some(value) = immediate_integer(src) {
                Some(value)
            } else {
                match php_jit::jit_decode_constant(src) {
                    Some(php_jit::JIT_VALUE_TRUE) => Some(1),
                    Some(
                        u32::MAX | php_jit::JIT_VALUE_UNINITIALIZED | php_jit::JIT_VALUE_FALSE,
                    ) => Some(0),
                    _ => None,
                }
            }
        }
        6 => Some(php_jit::jit_encode_constant(u32::MAX)),
        _ => None,
    }
}

pub(in crate::vm) extern "C" fn jit_native_execution_poll_abi() -> i32 {
    with_native_context_for("execution_poll", |context| {
        if context
            .execution_deadline_at
            .is_none_or(|deadline| std::time::Instant::now() < deadline)
        {
            return 0;
        }
        context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
            "E_PHP_VM_EXECUTION_TIMEOUT",
            php_runtime::api::RuntimeSeverity::RecoverableError,
            "maximum execution time exceeded",
            php_runtime::api::RuntimeSourceSpan::default(),
            Vec::new(),
            None,
        ));
        php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
fn write_native_value(out: *mut i64, value: i64) -> bool {
    let Some(out) = std::ptr::NonNull::new(out) else {
        return false;
    };
    // SAFETY: Generated code supplies a non-null stack-owned i64 out slot for
    // the duration of this synchronous helper call.
    unsafe { out.as_ptr().write(value) };
    true
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(in crate::vm) extern "C" fn jit_native_unary_abi(op: u32, src: i64, out: *mut i64) -> i32 {
    if let Some(value) = fast_native_unary(op, src) {
        return if write_native_value(out, value) {
            0
        } else {
            php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
        };
    }
    with_native_context_for("unary", |context| {
        let (op, function, source_key) = if op & 0x8000_0000 != 0 {
            let function = (op >> 2) & 0x03ff;
            let continuation = (op >> 12) & 0x07_ffff;
            (op & 0x3, Some(function), Some((function, continuation)))
        } else {
            (op, None, None)
        };
        context.attribute_active_helper(unary_operation_name(op), function);
        let Ok(src) = context.decode(src) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let op = match op {
            0 => php_runtime::api::NativeUnaryOp::Plus,
            1 => php_runtime::api::NativeUnaryOp::Minus,
            2 => php_runtime::api::NativeUnaryOp::Not,
            3 => php_runtime::api::NativeUnaryOp::BitNot,
            _ => return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
        };
        if op == php_runtime::api::NativeUnaryOp::BitNot
            && let Some(message) = native_implicit_float_to_int_message(&src)
            && let Some(source) = source_key.and_then(|(function, continuation)| {
                context.instruction_for_continuation(function, continuation)
            })
            && emit_native_php_warning(context, 8192, &message, &source).is_err()
        {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        }
        let mut operation = php_runtime::api::NativeOperationContext::default();
        let mut result = Value::Null;
        let status = php_runtime::api::native_unary(&mut operation, op, &src, &mut result);
        if status != php_runtime::api::NativeOperationStatus::Ok {
            return helper_status(status);
        }
        match context.encode(result) {
            Ok(value) if write_native_value(out, value) => 0,
            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

fn native_stringable_value(
    context: &mut NativeExecutionContext<'_>,
    value: Value,
) -> Result<Value, String> {
    let Value::Object(object) = value else {
        return Ok(value);
    };
    if let Some(value) = native_simple_xml_text(&object) {
        return Ok(value);
    }
    let class_name = object.class_name();
    let receiver = context.encode(Value::Object(object))?;
    let result = if let Some(function) =
        native_method_in_hierarchy(context, &class_name, "__toString")
    {
        invoke_native_method(context, function, &[receiver])?
    } else if let Some((function, _)) = native_external_method(context, &class_name, "__toString") {
        invoke_native_external_function(
            context,
            function,
            &[receiver],
            Some(class_name.clone()),
            context.unit.strict_types,
        )?
    } else {
        return Err(format!(
            "Object of class {class_name} could not be converted to string"
        ));
    };
    let result = context.decode(result)?;
    if matches!(result, Value::String(_)) {
        Ok(result)
    } else {
        Err("Method __toString() must return a string value".to_owned())
    }
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(in crate::vm) extern "C" fn jit_native_binary_abi(
    op: u32,
    lhs: i64,
    rhs: i64,
    function: i64,
    continuation: i64,
    out: *mut i64,
) -> i32 {
    if let Some(value) = fast_native_binary(op, lhs, rhs) {
        return if write_native_value(out, value) {
            0
        } else {
            php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
        };
    }
    with_native_context_for("binary", |context| {
        context.attribute_active_helper(
            binary_operation_name(op),
            u32::try_from(function).ok(),
        );
        let source_key = u32::try_from(function)
            .ok()
            .zip(u32::try_from(continuation).ok());
        let (Ok(mut lhs), Ok(mut rhs)) = (context.decode(lhs), context.decode(rhs)) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let op = match op {
            0 => php_runtime::api::NativeBinaryOp::Add,
            1 => php_runtime::api::NativeBinaryOp::Sub,
            2 => php_runtime::api::NativeBinaryOp::Mul,
            3 => php_runtime::api::NativeBinaryOp::Div,
            4 => php_runtime::api::NativeBinaryOp::Mod,
            5 => php_runtime::api::NativeBinaryOp::Concat,
            6 => php_runtime::api::NativeBinaryOp::Pow,
            7 => php_runtime::api::NativeBinaryOp::BitAnd,
            8 => php_runtime::api::NativeBinaryOp::BitOr,
            9 => php_runtime::api::NativeBinaryOp::BitXor,
            10 => php_runtime::api::NativeBinaryOp::ShiftLeft,
            11 => php_runtime::api::NativeBinaryOp::ShiftRight,
            _ => return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
        };
        let warns_for_leading_numeric = matches!(
            op,
            php_runtime::api::NativeBinaryOp::Add
                | php_runtime::api::NativeBinaryOp::Sub
                | php_runtime::api::NativeBinaryOp::Mul
                | php_runtime::api::NativeBinaryOp::Div
                | php_runtime::api::NativeBinaryOp::Mod
                | php_runtime::api::NativeBinaryOp::Pow
                | php_runtime::api::NativeBinaryOp::ShiftLeft
                | php_runtime::api::NativeBinaryOp::ShiftRight
        );
        for value in [&lhs, &rhs] {
            if warns_for_leading_numeric
                && let Value::String(value) = value
                && php_runtime::experimental::numeric_string::classify_php_string(value).kind
                    == php_runtime::experimental::numeric_string::NumericStringKind::LeadingNumeric
                && let Some(source) = source_key.and_then(|(function, continuation)| {
                    context.instruction_for_continuation(function, continuation)
                })
                && emit_native_php_warning(
                    context,
                    2,
                    "A non-numeric value encountered",
                    &source,
                )
                .is_err()
            {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
        }
        if matches!(
            op,
            php_runtime::api::NativeBinaryOp::Mod
                | php_runtime::api::NativeBinaryOp::BitAnd
                | php_runtime::api::NativeBinaryOp::BitOr
                | php_runtime::api::NativeBinaryOp::BitXor
                | php_runtime::api::NativeBinaryOp::ShiftLeft
                | php_runtime::api::NativeBinaryOp::ShiftRight
        ) {
            for value in [&lhs, &rhs] {
                if let Some(message) = native_implicit_float_to_int_message(value)
                    && let Some(source) = source_key.and_then(|(function, continuation)| {
                        context.instruction_for_continuation(function, continuation)
                    })
                    && emit_native_php_warning(context, 8192, &message, &source).is_err()
                {
                    return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                }
            }
        }
        if op == php_runtime::api::NativeBinaryOp::Concat {
            if matches!(lhs, Value::Object(_)) {
                lhs = match native_stringable_value(context, lhs) {
                    Ok(value) => value,
                    Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                };
            }
            if matches!(rhs, Value::Object(_)) {
                rhs = match native_stringable_value(context, rhs) {
                    Ok(value) => value,
                    Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                };
            }
        }
        let mut operation = php_runtime::api::NativeOperationContext::default();
        let mut result = Value::Null;
        let status = php_runtime::api::native_binary(&mut operation, op, &lhs, &rhs, &mut result);
        if status != php_runtime::api::NativeOperationStatus::Ok {
            let operation_message = operation
                .message
                .unwrap_or_else(|| "native binary operation failed".to_owned());
            if operation_message == "division by zero" {
                let mut stack = Vec::new();
                if let Some(function) = usize::try_from(function)
                    .ok()
                    .and_then(|index| context.unit.functions.get(index))
                {
                    let mut callee = function.name.clone();
                    stack.push(php_runtime::api::RuntimeStackFrame::new(callee.clone()));
                    while let Some(caller) = context.unit.functions.iter().find(|candidate| {
                        !stack.iter().any(|frame| frame.function() == candidate.name)
                            && candidate
                                .blocks
                                .iter()
                                .flat_map(|block| &block.instructions)
                                .any(|instruction| {
                                    matches!(
                                        &instruction.kind,
                                        php_ir::InstructionKind::CallFunction { name, .. }
                                            if name.eq_ignore_ascii_case(&callee)
                                    )
                                })
                    }) {
                        callee = caller.name.clone();
                        stack.push(php_runtime::api::RuntimeStackFrame::new(callee.clone()));
                    }
                }
                context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
                    "E_PHP_RUNTIME_DIVISION_BY_ZERO",
                    php_runtime::api::RuntimeSeverity::RecoverableError,
                    operation_message,
                    php_runtime::api::RuntimeSourceSpan::default(),
                    stack,
                    None,
                ));
            } else {
                let symbol = match op {
                    php_runtime::api::NativeBinaryOp::Add => "+",
                    php_runtime::api::NativeBinaryOp::Sub => "-",
                    php_runtime::api::NativeBinaryOp::Mul => "*",
                    php_runtime::api::NativeBinaryOp::Div => "/",
                    php_runtime::api::NativeBinaryOp::Mod => "%",
                    _ => "operation",
                };
                let message = format!(
                    "Unsupported operand types: {} {symbol} {}",
                    native_value_type_name(&lhs),
                    native_value_type_name(&rhs)
                );
                if let Some(source) = source_key.and_then(|(function, continuation)| {
                    context.instruction_for_continuation(function, continuation)
                }) {
                    let path = context
                        .unit
                        .files
                        .get(source.span.file.index())
                        .map_or("<unknown>", |file| file.path.as_str());
                    let line = native_source_line(context, &source);
                    context.output.write_bytes(format!(
                        "\nFatal error: Uncaught TypeError: {message} in {path}:{line}\nStack trace:\n#0 {{main}}\n  thrown in {path} on line {line}\n"
                    ));
                } else {
                    context.output.write_slices(&[
                        b"\nFatal error: Uncaught TypeError: ",
                        message.as_bytes(),
                        b"\n",
                    ]);
                }
                context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
                    "E_PHP_RUNTIME_TYPE_ERROR",
                    php_runtime::api::RuntimeSeverity::FatalError,
                    message,
                    php_runtime::api::RuntimeSourceSpan::default(),
                    Vec::new(),
                    None,
                ));
            }
            return helper_status(status);
        }
        match context.encode(result) {
            Ok(value) if write_native_value(out, value) => 0,
            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(in crate::vm) extern "C" fn jit_native_compare_abi(
    op: u32,
    lhs: i64,
    rhs: i64,
    out: *mut i64,
) -> i32 {
    if let Some(value) = fast_native_compare(op, lhs, rhs) {
        return if write_native_value(out, value) {
            0
        } else {
            php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
        };
    }
    with_native_context_for("compare", |context| {
        context.attribute_active_helper(compare_operation_name(op), None);
        let (Ok(lhs), Ok(rhs)) = (context.decode(lhs), context.decode(rhs)) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let op = match op {
            0 => php_runtime::api::NativeCompareOp::Equal,
            1 => php_runtime::api::NativeCompareOp::NotEqual,
            2 => php_runtime::api::NativeCompareOp::Identical,
            3 => php_runtime::api::NativeCompareOp::NotIdentical,
            4 => php_runtime::api::NativeCompareOp::Less,
            5 => php_runtime::api::NativeCompareOp::LessEqual,
            6 => php_runtime::api::NativeCompareOp::Greater,
            7 => php_runtime::api::NativeCompareOp::GreaterEqual,
            8 => php_runtime::api::NativeCompareOp::Spaceship,
            _ => return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
        };
        let mut operation = php_runtime::api::NativeOperationContext::default();
        let mut result = Value::Null;
        let status = php_runtime::api::native_compare(&mut operation, op, &lhs, &rhs, &mut result);
        if status != php_runtime::api::NativeOperationStatus::Ok {
            return helper_status(status);
        }
        match context.encode(result) {
            Ok(value) if write_native_value(out, value) => 0,
            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(in crate::vm) extern "C" fn jit_native_cast_abi(op: u32, src: i64, out: *mut i64) -> i32 {
    if let Some(value) = fast_native_cast(op, src) {
        return if write_native_value(out, value) {
            0
        } else {
            php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
        };
    }
    with_native_context_for("cast", |context| {
        let (op, function, source) = if op & 0x8000_0000 != 0 {
            let function = (op >> 3) & 0x03ff;
            let continuation = (op >> 13) & 0x03_ffff;
            (
                op & 0x7,
                Some(function),
                context.instruction_for_continuation(function, continuation),
            )
        } else {
            (op, None, None)
        };
        context.attribute_active_helper(cast_operation_name(op), function);
        let Ok(mut src) = context.decode(src) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let op = match op {
            0 => php_runtime::api::NativeCastOp::Bool,
            1 => php_runtime::api::NativeCastOp::Int,
            2 => php_runtime::api::NativeCastOp::Float,
            3 => php_runtime::api::NativeCastOp::String,
            4 => php_runtime::api::NativeCastOp::Array,
            5 => php_runtime::api::NativeCastOp::Object,
            6 => php_runtime::api::NativeCastOp::Void,
            _ => return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
        };
        if op == php_runtime::api::NativeCastOp::String && matches!(src, Value::Object(_)) {
            src = match native_stringable_value(context, src) {
                Ok(value) => value,
                Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        if op == php_runtime::api::NativeCastOp::String && matches!(src, Value::Array(_)) {
            if let Some(source) = &source {
                let path = context
                    .unit
                    .files
                    .get(source.span.file.index())
                    .map_or("<unknown>", |file| file.path.as_str());
                let line = native_source_line(context, source);
                context.output.write_bytes(format!(
                    "\nWarning: Array to string conversion in {path} on line {line}\n"
                ));
            }
            return match context.encode(Value::String(PhpString::from_bytes(b"Array".to_vec()))) {
                Ok(value) if write_native_value(out, value) => 0,
                _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        if op == php_runtime::api::NativeCastOp::Object && !matches!(src, Value::Object(_)) {
            let object = native_metadata_object("stdClass", std::iter::empty());
            match src {
                Value::Array(array) => {
                    for (key, value) in array.iter() {
                        let name = match key {
                            php_runtime::api::ArrayKey::Int(key) => key.to_string(),
                            php_runtime::api::ArrayKey::String(key) => key.to_string_lossy(),
                        };
                        object.set_property(name, value.clone());
                    }
                }
                Value::Null | Value::Uninitialized => {}
                value => object.set_property("scalar", value),
            }
            return match context.encode(Value::Object(object)) {
                Ok(value) if write_native_value(out, value) => 0,
                _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        if matches!(
            op,
            php_runtime::api::NativeCastOp::Int | php_runtime::api::NativeCastOp::Float
        ) && let Value::Object(object) = &src
        {
            if let Some(source) = &source
                && emit_native_php_warning(
                    context,
                    2,
                    &format!(
                        "Object of class {} could not be converted to {}",
                        object.display_name(),
                        if op == php_runtime::api::NativeCastOp::Int {
                            "int"
                        } else {
                            "float"
                        }
                    ),
                    source,
                )
                .is_err()
            {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
            let result = if op == php_runtime::api::NativeCastOp::Int {
                Value::Int(1)
            } else {
                Value::float(1.0)
            };
            return match context.encode(result) {
                Ok(value) if write_native_value(out, value) => 0,
                _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        if let Value::Float(value) = &src {
            let value = value.to_f64();
            let warning = match op {
                php_runtime::api::NativeCastOp::Int
                    if !value.is_finite() || value < i64::MIN as f64 || value > i64::MAX as f64 =>
                {
                    Some(format!(
                        "The float {} is not representable as an int, cast occurred",
                        native_php_float_label(value)
                    ))
                }
                php_runtime::api::NativeCastOp::Bool if value.is_nan() => {
                    Some("unexpected NAN value was coerced to bool".to_owned())
                }
                php_runtime::api::NativeCastOp::Array if value.is_nan() => {
                    Some("unexpected NAN value was coerced to array".to_owned())
                }
                _ => None,
            };
            if let (Some(warning), Some(source)) = (warning, source.as_ref())
                && emit_native_php_warning(context, 2, &warning, source).is_err()
            {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
        }
        let mut operation = php_runtime::api::NativeOperationContext::default();
        let mut result = Value::Null;
        let status = php_runtime::api::native_cast(&mut operation, op, &src, &mut result);
        if status != php_runtime::api::NativeOperationStatus::Ok {
            return helper_status(status);
        }
        match context.encode(result) {
            Ok(value) if write_native_value(out, value) => 0,
            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_echo_abi(src: i64) -> i32 {
    with_native_context_for("echo", |context| {
        if php_jit::jit_decode_runtime_value(src).is_none() {
            if let Some(constant) = php_jit::jit_decode_constant(src) {
                if constant == u32::MAX || constant == php_jit::JIT_VALUE_FALSE {
                    return 0;
                }
                if constant == php_jit::JIT_VALUE_TRUE {
                    context.output.write_bytes("1");
                    return 0;
                }
            } else {
                context.output.write_bytes(src.to_string());
                return 0;
            }
        }
        let Ok(mut src) = context.decode(src) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        if matches!(src, Value::Object(_)) {
            src = match native_stringable_value(context, src) {
                Ok(value) => value,
                Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        let mut operation = php_runtime::api::NativeOperationContext::default();
        helper_status(php_runtime::api::native_echo(
            &mut operation,
            &mut context.output,
            &src,
        ))
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_local_fetch_abi(
    quiet: u32,
    value: i64,
    function: i64,
    local: i64,
    file: i64,
    start: i64,
    out: *mut i64,
) -> i32 {
    if quiet & !(1 | php_jit::JIT_LOCAL_FETCH_PLAIN_LOCAL) != 0 {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    let quiet_read = quiet & 1 != 0;
    if quiet & php_jit::JIT_LOCAL_FETCH_PLAIN_LOCAL != 0
        && let Some(value) = fast_plain_local_fetch(value, quiet_read)
    {
        return if write_native_value(out, value) {
            0
        } else {
            php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
        };
    }
    with_native_context_for("local_fetch", |context| {
        context.attribute_active_helper("load_local", u32::try_from(function).ok());
        if quiet & php_jit::JIT_LOCAL_FETCH_PLAIN_LOCAL != 0 {
            match context.retain_plain_php_handle(value) {
                Ok(Some(value)) => {
                    context.record_local_read_reason("plain_initialized_local");
                    return if write_native_value(out, value) {
                        0
                    } else {
                        php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                    };
                }
                Ok(None) => {}
                Err(error) => {
                    record_native_helper_failure(
                        context,
                        format!("local fetch could not retain {value}: {error}"),
                    );
                    return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                }
            }
        }
        let Some(function_index) = usize::try_from(function).ok() else {
            context.record_local_read_reason("unknown");
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        let Some(local) = usize::try_from(local).ok() else {
            context.record_local_read_reason("unknown");
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        let Some((synthetic_local, ordinary_function_local)) =
            context.unit.functions.get(function_index).map(|function| {
                match function.locals.get(local) {
                    None => (true, false),
                    Some(name) => (
                        false,
                        !function.flags.is_top_level
                            && !matches!(
                                name.as_str(),
                                "GLOBALS"
                                    | "_SERVER"
                                    | "_GET"
                                    | "_POST"
                                    | "_FILES"
                                    | "_COOKIE"
                                    | "_SESSION"
                                    | "_REQUEST"
                                    | "_ENV"
                            ),
                    ),
                }
            })
        else {
            context.record_local_read_reason("unknown");
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        if synthetic_local {
            context.record_local_read_reason("synthetic_compiler_local");
            let decoded = match context.decode(value) {
                Ok(Value::Reference(reference)) => reference.get(),
                Ok(Value::Uninitialized) => Value::Null,
                Ok(value) => value,
                Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
            return match context.encode(decoded) {
                Ok(value) if write_native_value(out, value) => 0,
                _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        if ordinary_function_local {
            let mut decoded = match context.decode(value) {
                Ok(decoded) => decoded,
                Err(error) => {
                    let name = context.unit.functions[function_index].locals[local].clone();
                    record_native_helper_failure(
                        context,
                        format!("local fetch for ${name} could not decode {value}: {error}"),
                    );
                    return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                }
            };
            context.record_local_read_reason(match decoded {
                Value::Reference(_) => "reference_dereference",
                Value::Uninitialized if !quiet_read => "uninitialized_warning",
                _ => "plain_initialized_local",
            });
            let reference = if let Value::Reference(reference) = decoded {
                decoded = reference.get();
                Some(reference)
            } else {
                None
            };
            if let Some(reference) = &reference
                && let Some(encoded) = context.native_scalar_encoding(&decoded)
            {
                reference.publish_native_scalar(encoded);
            }
            if decoded == Value::Uninitialized && !quiet_read {
                let name = context.unit.functions[function_index].locals[local].clone();
                emit_native_undefined_variable_warning(context, &name, file, start);
            }
            let result = if decoded == Value::Uninitialized {
                Value::Null
            } else {
                decoded
            };
            return match context.encode(result) {
                Ok(value) if write_native_value(out, value) => 0,
                _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        let function = &context.unit.functions[function_index];
        let is_top_level = function.flags.is_top_level;
        let name = function.locals[local].clone();
        let global_reason = if name == "GLOBALS" {
            Some("GLOBALS")
        } else if matches!(
            name.as_str(),
            "_SERVER" | "_GET" | "_POST" | "_FILES" | "_COOKIE" | "_SESSION" | "_REQUEST" | "_ENV"
        ) {
            Some("superglobal")
        } else if is_top_level {
            Some("top_level_global")
        } else {
            None
        };
        if let Some(reason) = global_reason {
            context.record_local_read_reason(reason);
        }
        if name == "GLOBALS" {
            let globals = native_globals_array(context);
            return match context.encode(globals) {
                Ok(value) if write_native_value(out, value) => 0,
                _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        if name == "this" && is_top_level {
            let message = "Using $this when not in object context".to_owned();
            context.output.write_slices(&[
                b"\nFatal error: Uncaught Error: ",
                message.as_bytes(),
                b"\n",
            ]);
            context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
                "E_PHP_VM_THIS_OUTSIDE_OBJECT",
                php_runtime::api::RuntimeSeverity::FatalError,
                message,
                php_runtime::api::RuntimeSourceSpan::default(),
                Vec::new(),
                None,
            ));
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        }
        if is_top_level && let Some(mut inherited) = context.inherited_globals.get(&name).cloned() {
            if matches!(inherited, Value::Uninitialized) {
                return match context.encode(Value::Null) {
                    Ok(value) if write_native_value(out, value) => 0,
                    _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                };
            }
            if let Value::Reference(reference) = inherited {
                inherited = reference.get();
            }
            return match context.encode(inherited) {
                Ok(value) if write_native_value(out, value) => 0,
                _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        if is_top_level
            && let Some(mut global) = context.options.runtime_context.global_value(&name)
        {
            if let Value::Reference(reference) = global {
                global = reference.get();
            }
            return match context.encode(global) {
                Ok(value) if write_native_value(out, value) => 0,
                _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        let mut decoded = match context.decode(value) {
            Ok(decoded) => decoded,
            Err(error) => {
                record_native_helper_failure(
                    context,
                    format!("local fetch for ${name} could not decode {value}: {error}"),
                );
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
        };
        if global_reason.is_none() {
            context.record_local_read_reason(match decoded {
                Value::Reference(_) => "reference_dereference",
                Value::Uninitialized if !quiet_read => "uninitialized_warning",
                _ => "plain_initialized_local",
            });
        }
        // Reading a PHP variable yields the referenced value. The reference
        // cell itself remains in the native local slot so subsequent stores
        // still write through the alias.
        let reference = if let Value::Reference(reference) = decoded {
            decoded = reference.get();
            Some(reference)
        } else {
            None
        };
        if let Some(reference) = &reference
            && let Some(encoded) = context.native_scalar_encoding(&decoded)
        {
            reference.publish_native_scalar(encoded);
        }
        if decoded == Value::Uninitialized && !quiet_read {
            emit_native_undefined_variable_warning(context, &name, file, start);
        }
        let result = if decoded == Value::Uninitialized {
            Value::Null
        } else {
            decoded
        };
        match context.encode(result) {
            Ok(value) if write_native_value(out, value) => 0,
            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

fn emit_native_undefined_variable_warning(
    context: &mut NativeExecutionContext<'_>,
    name: &str,
    file: i64,
    start: i64,
) {
    let path = usize::try_from(file)
        .ok()
        .and_then(|index| context.unit.files.get(index))
        .map_or_else(|| "<unknown>".to_owned(), |file| file.path.clone());
    let line = usize::try_from(start).ok().map_or(1, |start| {
        std::fs::read(&path).ok().map_or(1, |bytes| {
            bytes
                .iter()
                .take(start)
                .filter(|byte| **byte == b'\n')
                .count()
                + 1
        })
    });
    let message = format!("Undefined variable ${name}");
    context.record_last_error(2, &message, &path, line);
    context
        .output
        .write_bytes(format!("\nWarning: {message} in {path} on line {line}\n"));
}

pub(in crate::vm) extern "C" fn jit_native_local_store_abi(
    op: u32,
    current: i64,
    value: i64,
    function: i64,
    local: i64,
    out: *mut i64,
) -> i32 {
    if op & !(php_jit::JIT_LOCAL_STORE_PLAIN_LOCAL | php_jit::JIT_LOCAL_STORE_MOVE_INPUT) != 0 {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    let move_input = op & php_jit::JIT_LOCAL_STORE_MOVE_INPUT != 0;
    with_native_context_for("local_store", |context| {
        context.attribute_active_helper("store_local", u32::try_from(function).ok());
        if op & php_jit::JIT_LOCAL_STORE_PLAIN_LOCAL != 0 && !move_input {
            match context.replace_plain_php_handle(current, value) {
                Ok(Some(())) => {
                    context.record_local_store_reason("plain_initialized_local");
                    return if write_native_value(out, value) {
                        0
                    } else {
                        php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                    };
                }
                Ok(None) => {}
                Err(error) => {
                    record_native_helper_failure(
                        context,
                        format!("local store could not replace {current} with {value}: {error}"),
                    );
                    return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                }
            }
        }
        let Some(local) = usize::try_from(local).ok() else {
            context.record_local_store_reason("unknown");
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        let Some((name, is_top_level)) = usize::try_from(function)
            .ok()
            .and_then(|index| context.unit.functions.get(index))
            .map(|function| {
                (
                    function.locals.get(local).cloned(),
                    function.flags.is_top_level,
                )
            })
        else {
            context.record_local_store_reason("unknown");
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        if !is_top_level
            && name.as_deref() != Some("GLOBALS")
            && context.php_handle_is_reference(current) == Some(false)
            && context.php_handle_is_reference(value) == Some(false)
        {
            context.record_local_store_reason("plain_initialized_local");
            if let Err(error) = context.retain(value) {
                record_native_helper_failure(
                    context,
                    format!("local store could not retain replacement {value}: {error}"),
                );
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
            if let Err(error) = context.release(current) {
                record_native_helper_failure(
                    context,
                    format!("local store could not release current value {current}: {error}"),
                );
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
            return if write_native_value(out, value) {
                0
            } else {
                php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
            };
        }
        let inherited_global = is_top_level
            .then(|| {
                name.as_ref()
                    .and_then(|name| context.inherited_globals.get(name))
                    .cloned()
            })
            .flatten();
        let inherited_reference = inherited_global
            .as_ref()
            .filter(|value| matches!(value, Value::Reference(_)))
            .cloned();
        let current_value = match inherited_reference
            .clone()
            .map_or_else(|| context.decode(current), Ok)
        {
            Ok(current) => current,
            Err(error) => {
                record_native_helper_failure(
                    context,
                    format!("local store could not decode current value {current}: {error}"),
                );
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
        };
        context.record_local_store_reason(if name.is_none() {
            "synthetic_compiler_local"
        } else if name.as_deref() == Some("GLOBALS") {
            "GLOBALS"
        } else if name.as_deref().is_some_and(|name| {
            matches!(
                name,
                "_SERVER"
                    | "_GET"
                    | "_POST"
                    | "_FILES"
                    | "_COOKIE"
                    | "_SESSION"
                    | "_REQUEST"
                    | "_ENV"
            )
        }) {
            "superglobal"
        } else if is_top_level {
            "top_level_global"
        } else if matches!(current_value, Value::Reference(_)) {
            "reference_dereference"
        } else {
            "plain_initialized_local"
        });
        let mut replacement = match context.decode(value) {
            Ok(value) => value,
            Err(error) => {
                record_native_helper_failure(
                    context,
                    format!("local store could not decode replacement {value}: {error}"),
                );
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
        };
        let replacement_was_reference = matches!(replacement, Value::Reference(_));
        if let Value::Reference(other) = replacement {
            replacement = other.get();
        }
        if name.as_deref() == Some("GLOBALS") {
            publish_native_globals_array(context, &replacement);
            context.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
        }
        if let Value::Reference(reference) = current_value {
            if is_top_level && let Some(name) = name.as_ref().filter(|name| *name != "GLOBALS") {
                context
                    .inherited_globals
                    .insert(name.clone(), Value::Reference(reference.clone()));
            }
            let previous = reference.get();
            let membership_changed = rooted_membership_may_change(&previous, &replacement);
            let synchronize_destructor_roots = membership_changed
                && (context.value_has_native_destructor(&previous)
                    || context.value_has_native_destructor(&replacement));
            let replacement_scalar = context.native_scalar_encoding(&replacement);
            reference.set(replacement);
            if let Some(encoded) = replacement_scalar {
                reference.publish_native_scalar(encoded);
            }
            if membership_changed {
                context.mark_rooted_container_dirty(&Value::Reference(reference.clone()));
                if synchronize_destructor_roots {
                    context.synchronize_request_roots();
                }
            }
            if let Err(error) = context.finalize_replaced_value(previous) {
                record_native_helper_failure(
                    context,
                    format!("local store could not finalize replaced value: {error}"),
                );
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
            if move_input && let Err(error) = context.release_if_live(value) {
                record_native_helper_failure(
                    context,
                    format!("local store could not consume moved value {value}: {error}"),
                );
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
            let stored = match context.decode(current) {
                Ok(Value::Reference(current_reference)) if current_reference.ptr_eq(&reference) => {
                    current
                }
                _ if inherited_reference.is_some() => {
                    if let Err(error) = context.release_if_live(current) {
                        record_native_helper_failure(
                            context,
                            format!(
                                "local store could not release replaced value {current}: {error}"
                            ),
                        );
                        return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                    }
                    match context.encode(Value::Reference(reference)) {
                        Ok(stored) => stored,
                        Err(error) => {
                            record_native_helper_failure(
                                context,
                                format!(
                                    "local store could not encode inherited reference: {error}"
                                ),
                            );
                            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                        }
                    }
                }
                _ => current,
            };
            if write_native_value(out, stored) {
                0
            } else {
                php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
            }
        } else {
            let stored = if replacement_was_reference {
                match context.encode(replacement.clone()) {
                    Ok(stored) => stored,
                    Err(error) => {
                        record_native_helper_failure(
                            context,
                            format!(
                                "local store could not encode dereferenced replacement: {error}"
                            ),
                        );
                        return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                    }
                }
            } else {
                value
            };
            if is_top_level && let Some(name) = name.as_ref().filter(|name| *name != "GLOBALS") {
                let membership_changed = inherited_global
                    .as_ref()
                    .is_none_or(|previous| rooted_membership_may_change(previous, &replacement));
                context
                    .inherited_globals
                    .insert(name.clone(), replacement.clone());
                if membership_changed {
                    context.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
                    context.synchronize_destructor_root_change(
                        inherited_global.as_ref().unwrap_or(&Value::Uninitialized),
                        &replacement,
                    );
                }
            }
            if !move_input && let Err(error) = context.retain(stored) {
                record_native_helper_failure(
                    context,
                    format!("local store could not retain replacement {stored}: {error}"),
                );
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
            if let Err(error) = context.release(current) {
                record_native_helper_failure(
                    context,
                    format!("local store could not release current value {current}: {error}"),
                );
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
            if write_native_value(out, stored) {
                0
            } else {
                php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
            }
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_value_lifecycle_abi(
    op: u32,
    encoded: i64,
    out: *mut i64,
) -> i32 {
    // Null, booleans, integers, and immutable constant handles do not own a
    // request-arena slot. Their retain/release semantics are exact no-ops, so
    // avoid the TLS context lookup, telemetry attribution, source lookup, and
    // refcount machinery entirely. Unknown values still take this guarded
    // path and runtime handles retain the full PHP destructor semantics below.
    let lifecycle_op = if op & 0x8000_0000 != 0 { op & 1 } else { op };
    if lifecycle_op <= 1 && php_jit::jit_decode_runtime_value(encoded).is_none() {
        return if write_native_value(out, encoded) {
            0
        } else {
            php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
        };
    }
    let helper_id = if op & 1 == 0 {
        "value_retain"
    } else {
        "value_release"
    };
    with_native_context_for(helper_id, |context| {
        let frame_cleanup = op & 0x8000_0000 != 0 && (op >> 11) & 0x0f_ffff == 0x0f_ffff;
        let (op, function, continuation) = if op & 0x8000_0000 != 0 {
            let function = (op >> 1) & 0x03ff;
            let continuation = (op >> 11) & 0x0f_ffff;
            (op & 1, Some(function), Some(continuation))
        } else {
            (op, None, None)
        };
        context.attribute_active_helper("value_lifecycle", function);
        let source = context
            .options
            .collect_counters
            .then(|| {
                function
                    .zip(continuation)
                    .and_then(|(function, continuation)| {
                        (!frame_cleanup)
                            .then(|| context.instruction_for_continuation(function, continuation))
                            .flatten()
                    })
            })
            .flatten();
        if context.options.collect_counters {
            let lifecycle_reason = if frame_cleanup {
                "frame_cleanup"
            } else {
                source
                    .as_ref()
                    .map_or("helper_result", |instruction| match instruction.kind {
                        php_ir::InstructionKind::Move { .. } => "copy",
                        php_ir::InstructionKind::StoreLocal { .. } => "store",
                        php_ir::InstructionKind::CallFunction { .. }
                        | php_ir::InstructionKind::CallMethod { .. }
                        | php_ir::InstructionKind::CallStaticMethod { .. }
                        | php_ir::InstructionKind::CallCallable { .. } => "argument",
                        php_ir::InstructionKind::Discard { .. }
                        | php_ir::InstructionKind::UnsetLocal { .. }
                        | php_ir::InstructionKind::UnsetDim { .. } => "temporary",
                        php_ir::InstructionKind::Throw { .. } => "exception_cleanup",
                        _ => "helper_result",
                    })
            };
            context.record_lifecycle_reason(op == 0, lifecycle_reason);
        }
        if op == 0 {
            context.record_ownership_clone();
        }
        let result = match op {
            0 => context.retain(encoded),
            1 => context.release(encoded),
            _ => return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
        };
        match result {
            Ok(()) if write_native_value(out, encoded) => 0,
            Ok(()) => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            Err(error) => {
                let operation = if op == 0 { "retain" } else { "release" };
                let source = source.or_else(|| {
                    function
                        .zip(continuation)
                        .and_then(|(function, continuation)| {
                            (!frame_cleanup)
                                .then(|| {
                                    context.instruction_for_continuation(function, continuation)
                                })
                                .flatten()
                        })
                });
                let source = source.as_ref().map_or_else(String::new, |instruction| {
                    format!(" while executing {:?}", instruction.kind)
                });
                record_native_helper_failure(
                    context,
                    format!(
                        "native value lifecycle {operation} of {encoded} failed{source}: {error}"
                    ),
                );
                php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
            }
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

fn native_globals_array(context: &mut NativeExecutionContext<'_>) -> Value {
    const RUNTIME_GLOBALS: &[&str] = &[
        "argc", "argv", "_SERVER", "_ENV", "_GET", "_POST", "_COOKIE", "_REQUEST", "_FILES",
        "_SESSION",
    ];

    // Make the runtime-owned superglobals part of the inherited table first.
    // The table is already ordered, so walking it below produces the same
    // deterministic key order as constructing a temporary BTreeSet, without
    // allocating and hashing a second copy of every global name on each
    // `$GLOBALS` read.
    for name in RUNTIME_GLOBALS {
        if context.inherited_globals.contains_key(*name) {
            continue;
        }
        let Some(value) = context.options.runtime_context.global_value(name) else {
            continue;
        };
        let reference = match value {
            Value::Reference(reference) => reference,
            value => php_runtime::api::ReferenceCell::new(value),
        };
        context
            .inherited_globals
            .insert((*name).to_owned(), Value::Reference(reference));
    }

    let mut globals = php_runtime::api::PhpArray::with_capacity(context.inherited_globals.len());
    for (name, value) in &mut context.inherited_globals {
        if name == "GLOBALS" || matches!(value, Value::Uninitialized) {
            continue;
        }
        let reference = match value {
            Value::Reference(reference) => reference.clone(),
            value => {
                let reference = php_runtime::api::ReferenceCell::new(value.clone());
                *value = Value::Reference(reference.clone());
                reference
            }
        };
        globals.insert(
            php_runtime::api::ArrayKey::String(PhpString::from_bytes(name.as_bytes().to_vec())),
            Value::Reference(reference),
        );
    }
    Value::Array(globals)
}

fn publish_native_globals_array(context: &mut NativeExecutionContext<'_>, value: &Value) {
    let Value::Array(globals) = value else {
        return;
    };
    let present = globals
        .iter()
        .filter_map(|(key, _)| match key {
            php_runtime::api::ArrayKey::String(name) => Some(name.to_string_lossy()),
            php_runtime::api::ArrayKey::Int(_) => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    for (name, value) in &mut context.inherited_globals {
        if name != "GLOBALS" && !present.contains(name) {
            *value = Value::Uninitialized;
        }
    }
    for (key, value) in globals.iter() {
        let php_runtime::api::ArrayKey::String(name) = key else {
            continue;
        };
        let name = name.to_string_lossy();
        if name == "GLOBALS" {
            continue;
        }
        if let Some(Value::Reference(existing)) = context.inherited_globals.get(&name) {
            let replacement = match value {
                Value::Reference(reference) => reference.get(),
                value => value.clone(),
            };
            existing.set(replacement);
            continue;
        }
        let reference = match value {
            Value::Reference(reference) => reference.clone(),
            value => php_runtime::api::ReferenceCell::new(value.clone()),
        };
        context
            .inherited_globals
            .insert(name, Value::Reference(reference));
    }
}

fn native_array_element_reference(
    array: &mut php_runtime::api::PhpArray,
    key: php_runtime::api::ArrayKey,
) -> php_runtime::api::ReferenceCell {
    match array.get(&key).cloned() {
        Some(Value::Reference(reference)) => reference,
        Some(value) => {
            let reference = php_runtime::api::ReferenceCell::new(value);
            array.insert(key, Value::Reference(reference.clone()));
            reference
        }
        None => {
            let reference = php_runtime::api::ReferenceCell::new(Value::Null);
            array.insert(key, Value::Reference(reference.clone()));
            reference
        }
    }
}

pub(in crate::vm) extern "C" fn jit_native_reference_bind_abi(
    op: u32,
    encoded: i64,
    key: i64,
    reserved: i64,
    out: *mut i64,
) -> i32 {
    let raw_op = op;
    let op = if raw_op & 0x8000_0000 != 0 {
        raw_op & 1
    } else {
        raw_op
    };
    if op > 5 {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    with_native_context_for("reference_bind", |context| {
        if op == 5 {
            let function_id = match u32::try_from(key) {
                Ok(function) => function,
                Err(_) => return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
            };
            let instruction_locator = reserved as u64;
            let block = (instruction_locator >> 32) as usize;
            let source_instruction_id = instruction_locator as u32;
            let instruction = context
                .unit
                .functions
                .get(function_id as usize)
                .and_then(|function| function.blocks.get(block))
                .and_then(|block| {
                    block
                        .instructions
                        .iter()
                        .find(|instruction| instruction.id.raw() == source_instruction_id)
                })
                .cloned();
            let Some(instruction) = instruction else {
                return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
            };
            return match execute_native_static_property(
                context,
                &instruction,
                &[encoded],
                function_id,
            ) {
                Some(Ok(value)) if write_native_value(out, value) => 0,
                Some(Ok(_)) => php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
                Some(Err(error)) if error.starts_with("E_PHP_THROW:") => {
                    let payload = error.trim_start_matches("E_PHP_THROW:");
                    let (class, message) = payload.split_once(':').unwrap_or(("Error", payload));
                    match encode_native_throwable_at(context, class, message, instruction.span) {
                        Ok(value) if write_native_value(out, value) => {
                            php_jit::JitCallStatus::THROW.0 as i32
                        }
                        _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                    }
                }
                Some(Err(error)) => {
                    record_native_helper_failure(context, error);
                    php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                }
                None => php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
            };
        }
        if op == 4 {
            let (Ok(function), Ok(local)) = (u32::try_from(key), u32::try_from(reserved)) else {
                return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
            };
            let Some(function) = context.unit.functions.get(function as usize) else {
                return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
            };
            // Region lowering appends deterministic synthetic locals for
            // reference locations. They have no PHP-visible top-level name
            // and therefore require no request-global publication.
            if encoded != php_jit::jit_encode_constant(u32::MAX)
                && let Some(name) = function.locals.get(local as usize)
                && function.flags.is_top_level
                && name != "GLOBALS"
            {
                let Ok(value) = context.decode(encoded) else {
                    return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                };
                context.inherited_globals.insert(name.clone(), value);
                context.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
            }
            return if write_native_value(out, encoded) {
                0
            } else {
                php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
            };
        }
        if op == 2 {
            let (Ok(function), Ok(local)) = (u32::try_from(key), u32::try_from(reserved)) else {
                return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
            };
            let Ok(default) = context.decode(encoded) else {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            };
            let key = (context.unit_identity, function, local);
            let inserted = !context.static_locals.contains_key(&key);
            let reference = context
                .static_locals
                .entry(key)
                .or_insert_with(|| php_runtime::api::ReferenceCell::new(default))
                .clone();
            if inserted {
                context.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
            }
            return match context.encode(Value::Reference(reference)) {
                Ok(value) if write_native_value(out, value) => 0,
                _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        if op == 3 {
            let Some(property) = native_property_name(context, key, reserved, true) else {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            };
            let Ok(mut object) = context.decode(encoded) else {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            };
            for _ in 0..16 {
                let Value::Reference(reference) = object else {
                    break;
                };
                object = reference.get();
            }
            let Value::Object(object) = object else {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            };
            let (reference, added) = match object.get_property(&property) {
                Some(Value::Reference(reference)) => (reference, false),
                Some(value) => {
                    let reference = php_runtime::api::ReferenceCell::new(value);
                    object.set_property(property, Value::Reference(reference.clone()));
                    (reference, true)
                }
                None => {
                    let reference = php_runtime::api::ReferenceCell::new(Value::Null);
                    object.set_property(property, Value::Reference(reference.clone()));
                    (reference, true)
                }
            };
            if added {
                context.add_rooted_nested_container(
                    &Value::Object(object.clone()),
                    &Value::Reference(reference.clone()),
                );
            }
            return match context.encode(Value::Reference(reference)) {
                Ok(value) if write_native_value(out, value) => 0,
                _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        if op == 1 {
            let source = if raw_op & 0x8000_0000 != 0 {
                let function = (raw_op >> 1) & 0x3ff;
                let continuation = (raw_op >> 11) & 0x0f_ffff;
                context.instruction_for_continuation(function, continuation)
            } else {
                None
            };
            let Ok(key) = context.decode(key) else {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            };
            let key = dereference_native_dimension_value(key);
            let Ok(target) = context.decode(encoded) else {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            };
            if emit_native_dimension_conversion_diagnostic(
                context,
                &target,
                &key,
                source.as_deref(),
                NativeDimensionOperation::Reference,
            )
            .is_err()
            {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
            let Some(key) = php_runtime::api::ArrayKey::from_value(&key) else {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            };
            let reference = match context.decode(encoded) {
                Ok(Value::Reference(root)) => {
                    let mut value = root.get();
                    let Value::Array(array) = &mut value else {
                        return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                    };
                    let reference = native_array_element_reference(array, key);
                    root.set(value);
                    reference
                }
                Ok(_) => {
                    let Ok(reference) = context.mutate_array_with(encoded, |array| {
                        native_array_element_reference(array, key)
                    }) else {
                        return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                    };
                    reference
                }
                Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
            context.add_rooted_nested_container(&target, &Value::Reference(reference.clone()));
            context
                .explicit_reference_ids
                .insert(reference.gc_debug_id());
            return match context.encode(Value::Reference(reference)) {
                Ok(value) if write_native_value(out, value) => 0,
                _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        let Ok(value) = context.decode(encoded) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        if matches!(value, Value::Reference(_)) {
            return if write_native_value(out, encoded) {
                0
            } else {
                php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
            };
        }
        let reference = php_runtime::api::ReferenceCell::new(value);
        context
            .explicit_reference_ids
            .insert(reference.gc_debug_id());
        match context.encode(Value::Reference(reference)) {
            Ok(value) if write_native_value(out, value) => 0,
            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_return_check_abi(
    op: u32,
    encoded: i64,
    function: i64,
    out: *mut i64,
) -> i32 {
    if op != 0 {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    with_native_context_for("return_check", |context| {
        let Some(function) = usize::try_from(function)
            .ok()
            .and_then(|function| context.unit.functions.get(function))
        else {
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        if function.flags.is_generator
            || function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(|instruction| {
                    matches!(
                        instruction.kind,
                        php_ir::InstructionKind::Yield { .. }
                            | php_ir::InstructionKind::YieldFrom { .. }
                    )
                })
        {
            return if write_native_value(out, encoded) {
                0
            } else {
                php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
            };
        }
        let Some(type_) = function.return_type.clone() else {
            return if write_native_value(out, encoded) {
                0
            } else {
                php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
            };
        };
        let function_name = function.name.clone();
        let Ok(value) = context.decode(encoded) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        if !native_value_matches_ir_type_in_context(context, &value, &type_) {
            context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
                "E_PHP_VM_RETURN_TYPE_MISMATCH",
                php_runtime::api::RuntimeSeverity::RecoverableError,
                format!(
                    "{}(): Return value must be of type {}, {} returned",
                    function_name,
                    native_ir_type_name(&type_),
                    native_value_type_name(&value)
                ),
                php_runtime::api::RuntimeSourceSpan::default(),
                Vec::new(),
                None,
            ));
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        }
        if write_native_value(out, encoded) {
            0
        } else {
            php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_argument_check_abi(
    op: u32,
    encoded: i64,
    target_function: i64,
    parameter_flags: i64,
    caller_function: i64,
    continuation: i64,
    out: *mut i64,
) -> i32 {
    if op != 0 {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    with_native_context_for("argument_check", |context| {
        let Ok(target_function) = u32::try_from(target_function) else {
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        let target_function = php_ir::FunctionId::new(target_function);
        let requested_index = (parameter_flags as u64 & u64::from(u32::MAX)) as usize;
        let strict = parameter_flags as u64 & (1_u64 << 32) != 0;
        let Some(target) =
            NativeFunctionMetadataPtr::from_compiled(&context.compiled, target_function)
        else {
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        let Some(parameter) = target
            .params
            .get(requested_index)
            .or_else(|| target.params.last().filter(|parameter| parameter.variadic))
        else {
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        let Some(type_) = parameter.type_.as_ref() else {
            return if write_native_value(out, encoded) {
                0
            } else {
                php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
            };
        };
        let source = u32::try_from(caller_function)
            .ok()
            .zip(u32::try_from(continuation).ok())
            .and_then(|(function, continuation)| {
                context.instruction_for_continuation(function, continuation)
            });
        let Ok(value) = context.decode(encoded) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let trace_argument = match &value {
            Value::Reference(reference) => reference.get(),
            value => value.clone(),
        };
        let (checked, reference) = if parameter.by_ref {
            let Value::Reference(reference) = value else {
                return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
            };
            if matches!(reference.get(), Value::Uninitialized) {
                reference.set(Value::Null);
            }
            (
                native_coerce_call_argument(reference.get(), type_, strict),
                Some(reference),
            )
        } else {
            let value = match value {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            (native_coerce_call_argument(value, type_, strict), None)
        };
        if !(native_value_matches_ir_type_in_context(context, &checked, type_)
            || matches!(type_, php_ir::IrReturnType::Callable)
                && native_value_is_callable(context, &checked))
        {
            let message = format!(
                "{}(): Argument #{} (${}) must be of type {}, {} given",
                target.name,
                requested_index + 1,
                parameter.name,
                native_ir_type_name(type_),
                native_value_type_name(&checked)
            );
            let throwable = encode_native_throwable_at(context, "TypeError", &message, target.span)
                .and_then(|encoded| context.decode(encoded))
                .map(|throwable| {
                    native_throwable_with_frame(throwable, &target.name, vec![trace_argument])
                })
                .map(|throwable| {
                    if let Some(source) = &source {
                        native_throwable_with_call_source(context, throwable, source.span)
                    } else {
                        throwable
                    }
                })
                .and_then(|throwable| context.encode(throwable));
            return match throwable {
                Ok(encoded) if write_native_value(out, encoded) => {
                    php_jit::JitCallStatus::THROW.0 as i32
                }
                _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        let checked = if let Some(reference) = reference {
            reference.set(checked);
            encoded
        } else {
            match context.encode(checked) {
                Ok(value) => value,
                Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            }
        };
        if write_native_value(out, checked) {
            0
        } else {
            php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_exception_new_abi(
    op: u32,
    message: i64,
    function: i64,
    continuation: i64,
    out: *mut i64,
) -> i32 {
    if op != 0 {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    with_native_context_for("exception_new", |context| {
        let (Ok(function), Ok(continuation)) =
            (u32::try_from(function), u32::try_from(continuation))
        else {
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        let Some(source) = context.instruction_for_continuation(function, continuation) else {
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        let php_ir::InstructionKind::MakeException { ref class_name, .. } = source.kind else {
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        let Ok(message) = context.decode(message) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let mut exception = php_runtime::api::PhpArray::new();
        exception.insert(
            php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"class".to_vec())),
            Value::String(PhpString::from_bytes(class_name.as_bytes().to_vec())),
        );
        exception.insert(
            php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"message".to_vec())),
            message,
        );
        exception.insert(
            php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"file".to_vec())),
            Value::String(PhpString::from_bytes(
                context
                    .unit
                    .files
                    .get(source.span.file.index())
                    .map_or_else(Vec::new, |file| file.path.as_bytes().to_vec()),
            )),
        );
        exception.insert(
            php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"line".to_vec())),
            Value::Int(i64::try_from(native_source_line(context, &source)).unwrap_or(i64::MAX)),
        );
        match context.encode(Value::Array(exception)) {
            Ok(value) if write_native_value(out, value) => 0,
            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_array_new_abi(op: u32, out: *mut i64) -> i32 {
    if op != 0 {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    with_native_context_for("array_new", |context| {
        match context.encode(Value::Array(php_runtime::api::PhpArray::new())) {
            Ok(value) if write_native_value(out, value) => 0,
            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

fn insert_native_array_value(
    target: &mut php_runtime::api::PhpArray,
    key: Option<php_runtime::api::ArrayKey>,
    value: Value,
) {
    if let Some(key) = key {
        if let Some(Value::Reference(reference)) = target.get(&key)
            && !matches!(value, Value::Reference(_))
        {
            reference.set(value);
        } else {
            target.insert(key, value);
        }
    } else {
        target.append(value);
    }
}

pub(in crate::vm) extern "C" fn jit_native_array_insert_abi(
    append: u32,
    array: i64,
    key: i64,
    value: i64,
    out: *mut i64,
) -> i32 {
    with_native_context_for("array_insert", |context| {
        let plain_array_index = context.plain_array_storage_index(array);
        let (append, source) = if append & 0x8000_0000 != 0 {
            let function = (append >> 1) & 0x3ff;
            let continuation = (append >> 11) & 0x0f_ffff;
            (
                append & 1,
                context.instruction_for_continuation(function, continuation),
            )
        } else {
            (append, None)
        };
        let Ok(value) = context.decode(value) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let key = if append == 1 {
            None
        } else if append == 0 {
            let Ok(key) = context.decode(key) else {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            };
            let key = dereference_native_dimension_value(key);
            let Ok(target) = context.decode(array) else {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            };
            if emit_native_dimension_conversion_diagnostic(
                context,
                &target,
                &key,
                source.as_deref(),
                NativeDimensionOperation::Insert,
            )
            .is_err()
            {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
            let Some(key) = php_runtime::api::ArrayKey::from_value(&key) else {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            };
            Some(key)
        } else {
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        if key.is_none() {
            let append_overflow = if let Some(index) = plain_array_index {
                context
                    .array_at(index)
                    .is_ok_and(|target| !target.can_append())
            } else {
                match context.decode(array) {
                    Ok(Value::Array(target)) => !target.can_append(),
                    Ok(Value::Reference(reference)) => {
                        matches!(reference.get(), Value::Array(target) if !target.can_append())
                    }
                    _ => false,
                }
            };
            if append_overflow {
                return match encode_native_throwable(
                    context,
                    "Error",
                    php_runtime::api::PHP_ARRAY_APPEND_OVERFLOW_MESSAGE,
                ) {
                    Ok(value) if write_native_value(out, value) => {
                        php_jit::JitCallStatus::THROW.0 as i32
                    }
                    _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                };
            }
        }
        if let Some(index) = plain_array_index {
            let result = context.mutate_array_at_with(index, |target| {
                insert_native_array_value(target, key, value)
            });
            return match result {
                Ok(()) if write_native_value(out, array) => 0,
                Ok(()) => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                Err(error) => {
                    record_native_helper_failure(context, error);
                    php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                }
            };
        }
        let mutate = |target: &mut php_runtime::api::PhpArray| {
            if let Some(key) = key.clone() {
                if let Some(Value::Reference(reference)) = target.get(&key)
                    && !matches!(value, Value::Reference(_))
                {
                    reference.set(value.clone());
                } else {
                    target.insert(key, value.clone());
                }
            } else {
                target.append(value.clone());
            }
        };
        let result = match context.decode(array) {
            Ok(Value::Reference(reference)) => {
                let mut value = reference.get();
                if matches!(value, Value::Null | Value::Uninitialized) {
                    value = Value::Array(php_runtime::api::PhpArray::new());
                }
                let Value::Array(target) = &mut value else {
                    return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                };
                mutate(target);
                reference.set(value);
                Ok(array)
            }
            Ok(Value::Null | Value::Uninitialized) => {
                let mut target = php_runtime::api::PhpArray::new();
                mutate(&mut target);
                context.encode(Value::Array(target))
            }
            Ok(Value::Array(_)) => context.mutate_array(array, mutate).map(|()| array),
            Ok(Value::String(string)) => {
                let Some(php_runtime::api::ArrayKey::Int(index)) = key else {
                    let message = "Cannot access offset of type string on string";
                    return match encode_native_throwable(context, "TypeError", message) {
                        Ok(value) if write_native_value(out, value) => {
                            php_jit::JitCallStatus::THROW.0 as i32
                        }
                        _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                    };
                };
                let mut bytes = string.as_bytes().to_vec();
                let resolved = if index < 0 {
                    i64::try_from(bytes.len()).unwrap_or(i64::MAX) + index
                } else {
                    index
                };
                if resolved < 0 {
                    if let Some(source) = &source {
                        let path = context
                            .unit
                            .files
                            .get(source.span.file.index())
                            .map_or("<unknown>", |file| file.path.as_str());
                        let line = native_source_line(context, source);
                        context.output.write_bytes(format!(
                            "\nWarning: Illegal string offset {index} in {path} on line {line}\n"
                        ));
                    }
                    context.encode(Value::String(string))
                } else {
                    let resolved = usize::try_from(resolved).unwrap_or(usize::MAX);
                    if resolved >= bytes.len() {
                        bytes.resize(resolved.saturating_add(1), b' ');
                    }
                    let replacement = match &value {
                        Value::String(value) => value.as_bytes().first().copied().unwrap_or(0),
                        Value::Int(value) => value.to_string().as_bytes()[0],
                        _ => 0,
                    };
                    bytes[resolved] = replacement;
                    context.encode(Value::String(PhpString::from_bytes(bytes)))
                }
            }
            Ok(target) => Err(format!(
                "native array insertion target is {}, not array",
                native_value_type_name(&target)
            )),
            Err(error) => Err(format!(
                "native array insertion target {array} could not be decoded: {error}"
            )),
        };
        match result {
            Ok(result) if write_native_value(out, result) => 0,
            Ok(_) => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            Err(error) => {
                record_native_helper_failure(context, error);
                php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
            }
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_object_new_abi(class: u32, out: *mut i64) -> i32 {
    with_native_context_for("object_new", |context| {
        let Some(class) = context.unit.classes.get(class as usize) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        if class.flags.is_abstract
            || class.flags.is_interface
            || class.flags.is_trait
            || class.flags.is_enum
        {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        }
        if let Some(parent) = class.parent.as_deref() {
            let parent = normalize_class_name(parent);
            let internal = php_std::ExtensionRegistry::standard_library()
                .enabled_class(&parent)
                .is_some()
                || matches!(
                    parent.as_str(),
                    "stdclass"
                        | "exception"
                        | "errorexception"
                        | "error"
                        | "typeerror"
                        | "valueerror"
                        | "argumentcounterror"
                        | "fibererror"
                        | "closure"
                        | "generator"
                        | "fiber"
                        | "arrayobject"
                        | "arrayiterator"
                );
            if !internal
                && !context
                    .unit
                    .classes
                    .iter()
                    .any(|class| class.name == parent)
                && !context.dynamic_classes.contains(&parent)
            {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
        }
        let Ok(object) = new_native_object(context, None, class) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        match context.encode(Value::Object(object)) {
            Ok(value) if write_native_value(out, value) => 0,
            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

fn native_property_name(
    context: &NativeExecutionContext<'_>,
    function: i64,
    instruction_locator: i64,
    assign: bool,
) -> Option<String> {
    let function_and_argument = function as u64;
    let function = usize::try_from(function_and_argument as u32).ok()?;
    let argument_index = u32::try_from(function_and_argument >> 32)
        .ok()?
        .checked_sub(1)
        .and_then(|index| usize::try_from(index).ok());
    let instruction_locator = instruction_locator as u64;
    let block = u32::try_from(instruction_locator >> 32).ok()?;
    let instruction_id = instruction_locator as u32;
    context
        .unit
        .functions
        .get(function)?
        .blocks
        .get(block as usize)?
        .instructions
        .iter()
        .find(|instruction| instruction.id.raw() == instruction_id)
        .and_then(|instruction| match &instruction.kind {
            php_ir::InstructionKind::FetchProperty { property, .. } if !assign => {
                Some(property.clone())
            }
            php_ir::InstructionKind::AssignProperty { property, .. } if assign => {
                Some(property.clone())
            }
            php_ir::InstructionKind::BindReferenceProperty { property, .. } if assign => {
                Some(property.clone())
            }
            php_ir::InstructionKind::BindReferenceFromProperty { property, .. } if assign => {
                Some(property.clone())
            }
            php_ir::InstructionKind::BindReferenceFromPropertyDim { property, .. } if assign => {
                Some(property.clone())
            }
            php_ir::InstructionKind::BindReferencePropertyDim { property, .. } => {
                Some(property.clone())
            }
            php_ir::InstructionKind::BindReferenceDimFromProperty { property, .. } => {
                Some(property.clone())
            }
            php_ir::InstructionKind::CallFunction { args, .. } if assign => {
                let property = |argument: &php_ir::instruction::IrCallArg| {
                    argument
                        .by_ref_property
                        .as_ref()
                        .map(|target| target.property.clone())
                        .or_else(|| {
                            argument
                                .by_ref_property_dim
                                .as_ref()
                                .map(|target| target.property.clone())
                        })
                };
                argument_index.map_or_else(
                    || args.iter().find_map(property),
                    |index| args.get(index).and_then(property),
                )
            }
            _ => None,
        })
}

fn native_property_span(
    context: &NativeExecutionContext<'_>,
    function: i64,
    instruction_locator: i64,
) -> Option<php_ir::IrSpan> {
    let function = usize::try_from(function).ok()?;
    let instruction_locator = instruction_locator as u64;
    let block = u32::try_from(instruction_locator >> 32).ok()?;
    let instruction_id = instruction_locator as u32;
    context
        .unit
        .functions
        .get(function)?
        .blocks
        .get(block as usize)?
        .instructions
        .iter()
        .find(|instruction| instruction.id.raw() == instruction_id)
        .map(|instruction| instruction.span)
}

fn invoke_native_property_method(
    context: &mut NativeExecutionContext<'_>,
    owner_unit: Option<usize>,
    class: &php_ir::module::ClassEntry,
    function: php_ir::FunctionId,
    arguments: &[i64],
    strict: bool,
) -> Result<i64, String> {
    if let Some(unit) = owner_unit {
        invoke_native_external_function(
            context,
            NativeDynamicFunction { unit, function },
            arguments,
            Some(class.name.clone()),
            strict,
        )
    } else {
        invoke_native_method(context, function, arguments)
    }
}

pub(in crate::vm) extern "C" fn jit_native_property_fetch_abi(
    op: u32,
    object: i64,
    function: i64,
    instruction_id: i64,
    out: *mut i64,
) -> i32 {
    if op > 2 {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    with_native_context_for("property_fetch", |context| {
        if op == 1 {
            let mut value = match context.decode(object) {
                Ok(value) => value,
                Err(error) => {
                    record_native_helper_failure(
                        context,
                        format!("native ::class receiver could not be decoded: {error}"),
                    );
                    return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                }
            };
            for _ in 0..16 {
                let Value::Reference(reference) = value else {
                    break;
                };
                value = reference.get();
            }
            let Value::Object(object) = value else {
                let message = format!(
                    "Cannot use \"::class\" on {}",
                    native_value_type_name(&value)
                );
                let throwable = match native_property_span(context, function, instruction_id) {
                    Some(span) => encode_native_throwable_at(context, "TypeError", &message, span),
                    None => encode_native_throwable(context, "TypeError", &message),
                };
                return match throwable {
                    Ok(value) if write_native_value(out, value) => {
                        php_jit::JitCallStatus::THROW.0 as i32
                    }
                    _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                };
            };
            return match context.encode(Value::String(PhpString::from_bytes(
                object.display_name().as_bytes().to_vec(),
            ))) {
                Ok(value) if write_native_value(out, value) => 0,
                Ok(_) => php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
                Err(error) => {
                    record_native_helper_failure(
                        context,
                        format!("native ::class result could not be encoded: {error}"),
                    );
                    php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                }
            };
        }
        if op == 2 {
            let function_id = match u32::try_from(function) {
                Ok(function) => function,
                Err(_) => return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
            };
            let instruction_locator = instruction_id as u64;
            let block = (instruction_locator >> 32) as usize;
            let source_instruction_id = instruction_locator as u32;
            let instruction = context
                .unit
                .functions
                .get(function_id as usize)
                .and_then(|function| function.blocks.get(block))
                .and_then(|block| {
                    block
                        .instructions
                        .iter()
                        .find(|instruction| instruction.id.raw() == source_instruction_id)
                })
                .cloned();
            let Some(instruction) = instruction else {
                return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
            };
            return match execute_native_static_property(
                context,
                &instruction,
                &[object],
                function_id,
            ) {
                Some(Ok(value)) if write_native_value(out, value) => 0,
                Some(Ok(_)) => php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
                Some(Err(error)) if error.starts_with("E_PHP_THROW:") => {
                    let payload = error.trim_start_matches("E_PHP_THROW:");
                    let (class, message) = payload.split_once(':').unwrap_or(("Error", payload));
                    let throwable = encode_native_throwable_at(
                        context,
                        class,
                        message,
                        instruction.span,
                    );
                    match throwable {
                        Ok(value) if write_native_value(out, value) => {
                            php_jit::JitCallStatus::THROW.0 as i32
                        }
                        _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                    }
                }
                Some(Err(error)) => {
                    record_native_helper_failure(context, error);
                    php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                }
                None => php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
            };
        }
        let Some(property) = native_property_name(context, function, instruction_id, false) else {
            context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
                "E_PHP_NATIVE_PROPERTY_FETCH",
                php_runtime::api::RuntimeSeverity::FatalError,
                format!(
                    "native property metadata is missing for function {function} locator {instruction_id}"
                ),
                php_runtime::api::RuntimeSourceSpan::default(),
                Vec::new(),
                None,
            ));
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let mut object = match context.decode(object) {
            Ok(object) => object,
            Err(error) => {
                context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
                    "E_PHP_NATIVE_PROPERTY_FETCH",
                    php_runtime::api::RuntimeSeverity::FatalError,
                    format!("native property ${property} receiver could not be decoded: {error}"),
                    php_runtime::api::RuntimeSourceSpan::default(),
                    Vec::new(),
                    None,
                ));
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
        };
        for _ in 0..16 {
            let Value::Reference(reference) = object else {
                break;
            };
            object = reference.get();
        }
        let Value::Object(object) = object else {
            let type_name = native_value_type_name(&object);
            context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
                "E_PHP_NATIVE_PROPERTY_FETCH",
                php_runtime::api::RuntimeSeverity::FatalError,
                format!("native property ${property} receiver is {type_name}, not object"),
                php_runtime::api::RuntimeSourceSpan::default(),
                Vec::new(),
                None,
            ));
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let class_name = normalize_class_name(&object.class_name());
        if let Some(value) = native_simple_xml_property(&object, &property) {
            return match context.encode(value) {
                Ok(value) if write_native_value(out, value) => 0,
                Ok(_) => {
                    record_native_helper_failure(
                        context,
                        format!("native SimpleXML property ${property} result pointer is null"),
                    );
                    php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                }
                Err(error) => {
                    record_native_helper_failure(
                        context,
                        format!("native SimpleXML property ${property} could not be encoded: {error}"),
                    );
                    php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                }
            };
        }
        let local_class = native_active_class_handle(context, &class_name);
        let (owner_unit, class) = local_class.map_or_else(
            || {
                native_external_class_handle(context, &class_name)
                    .map_or((None, None), |(unit, class)| (Some(unit), Some(class)))
            },
            |class| (None, Some(class)),
        );
        let caller_owns_scope = owner_unit.is_none()
            && class.as_ref().is_some_and(|class| {
                class
                    .methods
                    .iter()
                    .any(|method| method.function.raw() == function as u32)
            });
        let entry = class
            .as_ref()
            .and_then(|class| class.properties.iter().find(|entry| entry.name == property));
        let accessible = entry.is_none_or(|entry| {
            (!entry.flags.is_private && !entry.flags.is_protected) || caller_owns_scope
        });
        if accessible
            && let Some(hook) = entry.and_then(|entry| entry.hooks.get)
            && hook.raw() != function as u32
        {
            let Some(class_metadata) = class.as_ref() else {
                record_native_helper_failure(
                    context,
                    format!("native getter hook owner metadata for ${property} is missing"),
                );
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            };
            let strict = context
                .unit
                .strict_types_for_function(php_ir::FunctionId::new(function as u32));
            let result = context
                .encode(Value::Object(object.clone()))
                .and_then(|receiver| {
                    invoke_native_property_method(
                        context,
                        owner_unit,
                        class_metadata,
                        hook,
                        &[receiver],
                        strict,
                    )
                });
            return match result {
                Ok(value) if write_native_value(out, value) => 0,
                Ok(_) => {
                    record_native_helper_failure(
                        context,
                        format!("native getter hook for ${property} returned a null result pointer"),
                    );
                    php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                }
                Err(error) => {
                    record_native_helper_failure(
                        context,
                        format!("native getter hook for ${property} failed: {error}"),
                    );
                    php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                }
            };
        }
        // PHP consults __get() only when the requested property does not
        // exist or is inaccessible. Dynamic properties have no class-table
        // entry, so check object storage before falling through to magic
        // access; otherwise a real dynamic value is incorrectly hidden by
        // __get().
        if entry.is_none()
            && let Some(mut value) = object.get_property(&property)
        {
            for _ in 0..16 {
                let Value::Reference(reference) = value else {
                    break;
                };
                value = reference.get();
            }
            return match context.encode(value) {
                Ok(value) if write_native_value(out, value) => 0,
                Ok(_) => {
                    record_native_helper_failure(
                        context,
                        format!("native dynamic property ${property} result pointer is null"),
                    );
                    php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                }
                Err(error) => {
                    record_native_helper_failure(
                        context,
                        format!(
                            "native dynamic property ${property} could not be encoded: {error}"
                        ),
                    );
                    php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                }
            };
        }
        if (!accessible || entry.is_none())
            && let Some(method) = class.as_ref().and_then(|class| {
                class
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("__get"))
            })
            && method.function.raw() != function as u32
        {
            let Some(class_metadata) = class.as_ref() else {
                record_native_helper_failure(
                    context,
                    format!("native __get owner metadata for ${property} is missing"),
                );
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            };
            let strict = context
                .unit
                .strict_types_for_function(php_ir::FunctionId::new(function as u32));
            let result = context
                .encode(Value::Object(object.clone()))
                .and_then(|receiver| {
                    context
                        .encode(Value::String(PhpString::from_bytes(
                            property.as_bytes().to_vec(),
                        )))
                        .map(|name| (receiver, name))
                })
                .and_then(|(receiver, name)| {
                    invoke_native_property_method(
                        context,
                        owner_unit,
                        class_metadata,
                        method.function,
                        &[receiver, name],
                        strict,
                    )
                });
            return match result {
                Ok(value) if write_native_value(out, value) => 0,
                Ok(_) => {
                    record_native_helper_failure(
                        context,
                        format!("native __get for ${property} returned a null result pointer"),
                    );
                    php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                }
                Err(error) => {
                    record_native_helper_failure(
                        context,
                        format!("native __get for ${property} failed: {error}"),
                    );
                    php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                }
            };
        }
        let mut value = object.get_property(&property).unwrap_or(Value::Null);
        for _ in 0..16 {
            let Value::Reference(reference) = value else {
                break;
            };
            value = reference.get();
        }
        if matches!(value, Value::Uninitialized)
            && entry.is_some_and(|entry| entry.type_.is_some())
        {
            let display_class = class
                .as_ref()
                .map_or_else(|| object.display_name(), |class| class.display_name.clone());
            let message = format!(
                "Typed property {display_class}::${property} must not be accessed before initialization"
            );
            let span = native_property_span(context, function, instruction_id)
                .unwrap_or_else(|| php_ir::IrSpan::new(php_ir::FileId::new(0), 0, 0));
            return match encode_native_throwable_at(context, "Error", &message, span) {
                Ok(value) if write_native_value(out, value) => {
                    php_jit::JitCallStatus::THROW.0 as i32
                }
                _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        match context.encode(value) {
            Ok(value) if write_native_value(out, value) => 0,
            Ok(_) => {
                context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
                    "E_PHP_NATIVE_PROPERTY_FETCH",
                    php_runtime::api::RuntimeSeverity::FatalError,
                    format!("native property ${property} result pointer is null"),
                    php_runtime::api::RuntimeSourceSpan::default(),
                    Vec::new(),
                    None,
                ));
                php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
            }
            Err(error) => {
                context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
                    "E_PHP_NATIVE_PROPERTY_FETCH",
                    php_runtime::api::RuntimeSeverity::FatalError,
                    format!("native property ${property} result could not be encoded: {error}"),
                    php_runtime::api::RuntimeSourceSpan::default(),
                    Vec::new(),
                    None,
                ));
                php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
            }
        }
    })
    .unwrap_or(php_jit::JitCallStatus::ABI_MISMATCH.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_property_assign_abi(
    op: u32,
    object: i64,
    value: i64,
    function: i64,
    instruction_id: i64,
    out: *mut i64,
) -> i32 {
    if op > 3 {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    let encoded_value = value;
    with_native_context_for("property_assign", |context| {
        let Some(property) = native_property_name(context, function, instruction_id, true) else {
            record_native_helper_failure(
                context,
                format!(
                    "native property assignment metadata is missing for function {function} locator {instruction_id}"
                ),
            );
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let mut object = match context.decode(object) {
            Ok(object) => object,
            Err(error) => {
                record_native_helper_failure(
                    context,
                    format!(
                        "native property ${property} assignment receiver could not be decoded: {error}"
                    ),
                );
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
        };
        for _ in 0..16 {
            let Value::Reference(reference) = object else {
                break;
            };
            object = reference.get();
        }
        let Value::Object(object) = object else {
            record_native_helper_failure(
                context,
                format!(
                    "native property ${property} assignment receiver is {}, not object",
                    native_value_type_name(&object)
                ),
            );
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let value = match context.decode(value) {
            Ok(value) => value,
            Err(error) => {
                record_native_helper_failure(
                    context,
                    format!(
                        "native property ${property} assignment value could not be decoded: {error}"
                    ),
                );
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
        };
        let bind_reference = op & 1 != 0;
        let move_value = op & 2 != 0;
        let input_was_reference = matches!(value, Value::Reference(_));
        let can_reuse_input = move_value && (bind_reference || !input_was_reference);
        let value = if bind_reference {
            value
        } else {
            dereference_native_assignment_value(value)
        };
        let checked_value = dereference_native_assignment_value(value.clone());
        let class_name = object.class_name();
        let class = native_active_class_handle(context, &class_name);
        let caller_owns_scope = class.as_ref().is_some_and(|class| {
            class
                .methods
                .iter()
                .any(|method| method.function.raw() == function as u32)
        });
        let entry = class
            .as_ref()
            .and_then(|class| class.properties.iter().find(|entry| entry.name == property));
        let accessible = entry.is_none_or(|entry| {
            (!entry.flags.is_private && !entry.flags.is_protected) || caller_owns_scope
        });
        if accessible
            && let Some(hook) = entry.and_then(|entry| entry.hooks.set)
            && hook.raw() != function as u32
        {
            let result = context
                .encode(Value::Object(object.clone()))
                .and_then(|receiver| context.encode(value.clone()).map(|value| (receiver, value)))
                .and_then(|(receiver, value)| {
                    invoke_native_method(context, hook, &[receiver, value])
                });
            return match result {
                Ok(_) if can_reuse_input => {
                    if write_native_value(out, encoded_value) {
                        0
                    } else {
                        php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                    }
                }
                Ok(_) => match context.encode(value) {
                    Ok(value) if write_native_value(out, value) => 0,
                    _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                },
                Err(_) => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        if (!accessible || entry.is_none())
            && let Some(method) = class.as_ref().and_then(|class| {
                class
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("__set"))
            })
            && method.function.raw() != function as u32
        {
            let result = context
                .encode(Value::Object(object.clone()))
                .and_then(|receiver| {
                    context
                        .encode(Value::String(PhpString::from_bytes(
                            property.as_bytes().to_vec(),
                        )))
                        .map(|name| (receiver, name))
                })
                .and_then(|(receiver, name)| {
                    context
                        .encode(value.clone())
                        .map(|value| (receiver, name, value))
                })
                .and_then(|(receiver, name, value)| {
                    invoke_native_method(context, method.function, &[receiver, name, value])
                });
            return match result {
                Ok(_) if can_reuse_input => {
                    if write_native_value(out, encoded_value) {
                        0
                    } else {
                        php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
                    }
                }
                Ok(_) => match context.encode(value) {
                    Ok(value) if write_native_value(out, value) => 0,
                    _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                },
                Err(_) => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        if entry.is_some_and(|entry| entry.flags.is_readonly)
            && object
                .get_property(&property)
                .is_some_and(|current| !matches!(current, Value::Uninitialized))
        {
            let display_class = class
                .as_ref()
                .map_or_else(|| class_name.clone(), |class| class.display_name.clone());
            let message = format!("Cannot modify readonly property {display_class}::${property}");
            return match encode_native_throwable(context, "Error", &message) {
                Ok(value) if write_native_value(out, value) => {
                    php_jit::JitCallStatus::THROW.0 as i32
                }
                _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        let property_type = entry
            .and_then(|entry| entry.type_.clone())
            .and_then(|type_| {
                class
                    .as_ref()
                    .map(|class| (class.display_name.clone(), type_))
            });
        if let Some((display_class, type_)) = property_type
            && !native_value_matches_ir_type_in_context(context, &checked_value, &type_)
        {
            let message = format!(
                "Cannot assign {} to property {}::${} of type {}",
                native_assignment_type_name(&checked_value),
                display_class,
                property,
                native_ir_type_name(&type_)
            );
            return match encode_native_throwable(context, "TypeError", &message) {
                Ok(value) if write_native_value(out, value) => {
                    php_jit::JitCallStatus::THROW.0 as i32
                }
                _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
        }
        if bind_reference {
            let previous = object
                .get_property(&property)
                .unwrap_or(Value::Uninitialized);
            let membership_changed = rooted_membership_may_change(&previous, &value);
            object.set_property(property.as_str(), value.clone());
            if membership_changed {
                context.mark_rooted_container_dirty(&Value::Object(object.clone()));
            }
            if let Err(error) = context.finalize_replaced_value(previous) {
                record_native_helper_failure(
                    context,
                    format!("property ${property} could not finalize replaced value: {error}"),
                );
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
        } else if let Some(Value::Reference(reference)) = object.get_property(&property) {
            let previous = reference.get();
            let membership_changed = rooted_membership_may_change(&previous, &value);
            reference.set(value.clone());
            if membership_changed {
                context.mark_rooted_container_dirty(&Value::Reference(reference));
            }
            if let Err(error) = context.finalize_replaced_value(previous) {
                record_native_helper_failure(
                    context,
                    format!("property ${property} could not finalize replaced value: {error}"),
                );
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
        } else {
            let previous = object
                .get_property(&property)
                .unwrap_or(Value::Uninitialized);
            let membership_changed = rooted_membership_may_change(&previous, &value);
            object.set_property(property.as_str(), value.clone());
            if membership_changed {
                context.mark_rooted_container_dirty(&Value::Object(object.clone()));
            }
            if let Err(error) = context.finalize_replaced_value(previous) {
                record_native_helper_failure(
                    context,
                    format!("property ${property} could not finalize replaced value: {error}"),
                );
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
        }
        if can_reuse_input {
            if write_native_value(out, encoded_value) {
                return 0;
            }
            record_native_helper_failure(
                context,
                format!("native property ${property} assignment output pointer is null"),
            );
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        }
        match context.encode(value) {
            Ok(value) if write_native_value(out, value) => 0,
            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_object_clone_abi(
    op: u32,
    object: i64,
    out: *mut i64,
) -> i32 {
    if op != 0 {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    with_native_context_for("object_clone", |context| {
        let Ok(Value::Object(object)) = context.decode(object) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let clone = object.clone_shallow();
        if let Some(function) = native_method_in_hierarchy(context, &clone.class_name(), "__clone")
        {
            let receiver = match context.encode(Value::Object(clone.clone())) {
                Ok(receiver) => receiver,
                Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
            if invoke_native_method(context, function, &[receiver]).is_err() {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
        }
        match context.encode(Value::Object(clone)) {
            Ok(value) if write_native_value(out, value) => 0,
            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_object_clone_with_abi(
    op: u32,
    object: i64,
    replacements: i64,
    out: *mut i64,
) -> i32 {
    if op != 0 {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    with_native_context_for("object_clone_with", |context| {
        let (Ok(Value::Object(object)), Ok(Value::Array(replacements))) =
            (context.decode(object), context.decode(replacements))
        else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let clone = object.clone_shallow();
        if let Some(function) = native_method_in_hierarchy(context, &clone.class_name(), "__clone")
        {
            let receiver = match context.encode(Value::Object(clone.clone())) {
                Ok(receiver) => receiver,
                Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
            if invoke_native_method(context, function, &[receiver]).is_err() {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
        }
        for (key, value) in replacements.iter() {
            let php_runtime::api::ArrayKey::String(name) = key else {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            };
            let name = name.to_string_lossy();
            let class_name = object.class_name();
            let property = context
                .unit
                .classes
                .iter()
                .find(|class| normalize_class_name(&class.display_name) == normalize_class_name(&class_name))
                .and_then(|class| {
                    class
                        .properties
                        .iter()
                        .find(|property| property.name == name)
                        .map(|property| {
                            (
                                class.display_name.clone(),
                                property.flags,
                                property.hooks.set,
                            )
                        })
                });
            let access_error = property.as_ref().and_then(|(class, flags, _)| {
                if flags.is_private {
                    Some(format!("Cannot access private property {class}::${name}"))
                } else if flags.is_readonly {
                    Some(format!(
                        "Cannot modify protected(set) readonly property {class}::${name} from global scope"
                    ))
                } else {
                    None
                }
            });
            if let Some(message) = access_error {
                context.output.write_slices(&[
                    b"\nFatal error: Uncaught Error: ",
                    message.as_bytes(),
                    b"\n",
                ]);
                context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
                    "E_PHP_VM_CLONE_WITH_PROPERTY_ACCESS",
                    php_runtime::api::RuntimeSeverity::FatalError,
                    message,
                    php_runtime::api::RuntimeSourceSpan::default(),
                    Vec::new(),
                    None,
                ));
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
            if let Some(hook) = property.and_then(|(_, _, hook)| hook) {
                let receiver = match context.encode(Value::Object(clone.clone())) {
                    Ok(receiver) => receiver,
                    Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                };
                let value = match context.encode(value.clone()) {
                    Ok(value) => value,
                    Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                };
                if invoke_native_method(context, hook, &[receiver, value]).is_err() {
                    return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                }
            } else if let Some(Value::Reference(reference)) = clone.get_property(&name) {
                reference.set(value.clone());
            } else {
                clone.set_property(name, value.clone());
            }
        }
        match context.encode(Value::Object(clone)) {
            Ok(value) if write_native_value(out, value) => 0,
            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

fn finish_native_array_fetch(
    context: &mut NativeExecutionContext<'_>,
    found: Option<Value>,
    key: &php_runtime::api::ArrayKey,
    quiet: u32,
    array_target: bool,
    source: Option<&php_ir::Instruction>,
    out: *mut i64,
) -> i32 {
    if found.is_none() && quiet == 0 && array_target {
        let missing_key = match key {
            php_runtime::api::ArrayKey::Int(key) => key.to_string(),
            php_runtime::api::ArrayKey::String(key) => {
                format!("\"{}\"", key.to_string_lossy())
            }
        };
        let message = format!("Undefined array key {missing_key}");
        context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
            "E_PHP_RUNTIME_UNDEFINED_ARRAY_KEY_WARNING",
            php_runtime::api::RuntimeSeverity::Warning,
            message,
            source.map_or_else(php_runtime::api::RuntimeSourceSpan::default, |source| {
                php_runtime::api::RuntimeSourceSpan {
                    file: context
                        .unit
                        .files
                        .get(source.span.file.index())
                        .map(|file| file.path.clone()),
                    start: source.span.start,
                    end: source.span.end,
                }
            }),
            Vec::new(),
            None,
        ));
    }
    match context.encode(found.unwrap_or(Value::Null)) {
        Ok(value) if write_native_value(out, value) => 0,
        _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
    }
}

pub(in crate::vm) extern "C" fn jit_native_array_fetch_abi(
    quiet: u32,
    array: i64,
    key: i64,
    out: *mut i64,
) -> i32 {
    with_native_context_for("array_fetch", |context| {
        let (quiet, source_function, source) = if quiet & 0x8000_0000 != 0 {
            let function = (quiet >> 1) & 0x3ff;
            let continuation = (quiet >> 11) & 0x0f_ffff;
            (
                quiet & 1,
                Some(function),
                context
                    .instruction_for_continuation(function, continuation),
            )
        } else {
            (quiet, None, None)
        };
        if quiet > 1 {
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        }
        let encoded_array = array;
        let Ok(key) = context.decode(key) else {
            record_native_helper_failure(
                context,
                "array fetch could not decode its key".to_owned(),
            );
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let key = dereference_native_dimension_value(key);
        if let Some(index) = context.plain_array_storage_index(array) {
            if emit_native_array_dimension_conversion_diagnostic(
                context,
                &key,
                source.as_deref(),
                NativeDimensionOperation::Fetch { quiet: quiet == 1 },
            )
            .is_err()
            {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
            let Some(key) = php_runtime::api::ArrayKey::from_value(&key) else {
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            };
            let found = match context.array_at(index) {
                Ok(array) => array.get(&key).cloned(),
                Err(error) => {
                    record_native_helper_failure(context, error);
                    return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                }
            };
            return finish_native_array_fetch(
                context,
                found,
                &key,
                quiet,
                true,
                source.as_deref(),
                out,
            );
        }
        let Ok(mut array) = context.decode(array) else {
            record_native_helper_failure(context, "array fetch could not decode its target".to_owned());
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let reference_target = matches!(array, Value::Reference(_));
        for _ in 0..16 {
            let Value::Reference(reference) = array else {
                break;
            };
            array = reference.get();
        }
        if reference_target && array == Value::Uninitialized {
            if quiet == 0 && let (Some(function), Some(source)) = (source_function, source.as_ref()) {
                let function = context.unit.functions.get(function as usize);
                let local = match source.kind {
                    php_ir::InstructionKind::FetchDim {
                        array: php_ir::Operand::Local(local),
                        ..
                    } => Some(local),
                    php_ir::InstructionKind::FetchDim {
                        array: php_ir::Operand::Register(register),
                        ..
                    } => function.and_then(|function| {
                        function.blocks.iter().find_map(|block| {
                            block.instructions.iter().find_map(|instruction| {
                                match instruction.kind {
                                    php_ir::InstructionKind::LoadLocal { dst, local }
                                    | php_ir::InstructionKind::LoadLocalQuiet { dst, local }
                                        if dst == register =>
                                    {
                                        Some(local)
                                    }
                                    _ => None,
                                }
                            })
                        })
                    }),
                    _ => None,
                };
                if let Some(name) = local
                    .and_then(|local| function.and_then(|function| function.locals.get(local.index())))
                    .cloned()
                {
                    emit_native_undefined_variable_warning(
                        context,
                        &name,
                        i64::try_from(source.span.file.index()).unwrap_or(-1),
                        i64::from(source.span.start),
                    );
                }
            }
            array = Value::Null;
        }
        if emit_native_dimension_conversion_diagnostic(
            context,
            &array,
            &key,
            source.as_deref(),
            NativeDimensionOperation::Fetch { quiet: quiet == 1 },
        )
        .is_err()
        {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        }
        let Some(key) = php_runtime::api::ArrayKey::from_value(&key) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let array_target = matches!(&array, Value::Array(_));
        let found = match array {
            Value::Array(array) => array.get(&key).cloned(),
            Value::String(string) => {
                let php_runtime::api::ArrayKey::Int(index) = key else {
                    if quiet == 1 {
                        return match context.encode(Value::Null) {
                            Ok(value) if write_native_value(out, value) => 0,
                            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                        };
                    }
                    let message = "Cannot access offset of type string on string";
                    let throwable = match source.as_ref() {
                        Some(source) => encode_native_throwable_at(
                            context,
                            "TypeError",
                            message,
                            source.span,
                        ),
                        None => encode_native_throwable(context, "TypeError", message),
                    };
                    return match throwable {
                        Ok(value) if write_native_value(out, value) => {
                            php_jit::JitCallStatus::THROW.0 as i32
                        }
                        _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                    };
                };
                let bytes = string.as_bytes();
                let index = if index < 0 {
                    i64::try_from(bytes.len()).unwrap_or(i64::MAX) + index
                } else {
                    index
                };
                usize::try_from(index).ok().and_then(|index| {
                    bytes
                        .get(index)
                        .map(|byte| Value::String(PhpString::from_bytes(vec![*byte])))
                })
            }
            Value::Object(object) if native_simple_xml_dimension(&object, &key).is_some() => {
                native_simple_xml_dimension(&object, &key)
            }
            Value::Object(object) if object.class_name().eq_ignore_ascii_case("arrayobject") => {
                let name = match &key {
                    php_runtime::api::ArrayKey::Int(key) => key.to_string(),
                    php_runtime::api::ArrayKey::String(key) => key.to_string_lossy(),
                };
                object.get_property(&name)
            }
            Value::Object(object) => {
                let class_name = object.class_name();
                let local_method =
                    native_method_in_hierarchy(context, &class_name, "offsetGet");
                let external_method = local_method
                    .is_none()
                    .then(|| native_external_method(context, &class_name, "offsetGet"))
                    .flatten();
                if local_method.is_none() && external_method.is_none() {
                    record_native_helper_failure(
                        context,
                        format!("array fetch on {class_name} could not resolve offsetGet()"),
                    );
                    return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                }
                let key = match &key {
                    php_runtime::api::ArrayKey::Int(key) => Value::Int(*key),
                    php_runtime::api::ArrayKey::String(key) => Value::String(key.clone()),
                };
                let Ok(key) = context.encode(key) else {
                    record_native_helper_failure(
                        context,
                        format!("array fetch on {class_name} could not encode its key"),
                    );
                    return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                };
                let invoked = if let Some(function) = local_method {
                    invoke_native_method(context, function, &[encoded_array, key])
                } else if let Some((function, _)) = external_method {
                    invoke_native_external_function(
                        context,
                        function,
                        &[encoded_array, key],
                        Some(class_name.clone()),
                        context.unit.strict_types,
                    )
                } else {
                    record_native_helper_failure(
                        context,
                        format!("array fetch on {class_name} lost its resolved offsetGet()"),
                    );
                    return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                };
                let value = match invoked {
                    Ok(value) => value,
                    Err(error) => {
                        record_native_helper_failure(
                            context,
                            format!("array fetch on {class_name} failed in offsetGet(): {error}"),
                        );
                        return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                    }
                };
                match context.decode(value) {
                    Ok(value) => Some(value),
                    Err(error) => {
                        record_native_helper_failure(
                            context,
                            format!(
                                "array fetch on {class_name} could not decode offsetGet() result: {error}"
                            ),
                        );
                        return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                    }
                }
            }
            Value::Null | Value::Uninitialized => None,
            _ if quiet == 1 => None,
            _ => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        };
        finish_native_array_fetch(
            context,
            found,
            &key,
            quiet,
            array_target,
            source.as_deref(),
            out,
        )
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_array_unset_abi(
    op: u32,
    array: i64,
    key: i64,
    out: *mut i64,
) -> i32 {
    with_native_context_for("array_unset", |context| {
        let (op, source) = if op & 0x8000_0000 != 0 {
            let function = (op >> 1) & 0x3ff;
            let continuation = (op >> 11) & 0x0f_ffff;
            (
                op & 1,
                context.instruction_for_continuation(function, continuation),
            )
        } else {
            (op, None)
        };
        if op != 0 {
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        }
        let Ok(key) = context.decode(key) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let key = dereference_native_dimension_value(key);
        let Ok(target) = context.decode(array) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        if emit_native_dimension_conversion_diagnostic(
            context,
            &target,
            &key,
            source.as_deref(),
            NativeDimensionOperation::Unset,
        )
        .is_err()
        {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        }
        let Some(key) = php_runtime::api::ArrayKey::from_value(&key) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        if context
            .mutate_array(array, |target| {
                target.remove(&key);
            })
            .is_err()
        {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        }
        if write_native_value(out, array) {
            0
        } else {
            php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_array_spread_abi(
    op: u32,
    array: i64,
    source: i64,
    out: *mut i64,
) -> i32 {
    if op != 0 {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    with_native_context_for("array_spread", |context| {
        let Ok(Value::Array(source)) = context.decode(source) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let entries = source
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>();
        if context
            .mutate_array(array, |target| {
                for (key, value) in entries {
                    match key {
                        php_runtime::api::ArrayKey::Int(_) => {
                            target.append(value);
                        }
                        php_runtime::api::ArrayKey::String(_) => {
                            target.insert(key, value);
                        }
                    }
                }
            })
            .is_err()
        {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        }
        if write_native_value(out, array) {
            0
        } else {
            php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_foreach_init_abi(
    op: u32,
    source: i64,
    function: i64,
    local: i64,
    out: *mut i64,
) -> i32 {
    if op > 1 {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    with_native_context_for("foreach_init", |context| {
        let iterator = if op == 1 {
            let entries = match context.mutate_array_with(source, |source| {
                let snapshot = source
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect::<Vec<_>>();
                let mut entries = Vec::with_capacity(snapshot.len());
                for (key, value) in snapshot {
                    let reference = match value {
                        Value::Reference(reference) => reference,
                        value => php_runtime::api::ReferenceCell::new(value),
                    };
                    source.insert(key.clone(), Value::Reference(reference.clone()));
                    let key = match key {
                        php_runtime::api::ArrayKey::Int(key) => Value::Int(key),
                        php_runtime::api::ArrayKey::String(key) => Value::String(key),
                    };
                    entries.push((key, Value::Reference(reference)));
                }
                entries
            }) {
                Ok(entries) => entries,
                Err(_) => {
                    return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                }
            };
            let live_global = usize::try_from(function)
                .ok()
                .and_then(|function| context.unit.functions.get(function))
                .filter(|function| function.flags.is_top_level)
                .and_then(|function| {
                    usize::try_from(local)
                        .ok()
                        .and_then(|local| function.locals.get(local))
                })
                .cloned();
            context.encode_iterator(entries, Some(source), live_global, None, None)
        } else {
            let mut decoded = match context.decode(source) {
                Ok(value) => value,
                Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            };
            for _ in 0..16 {
                let Value::Reference(reference) = decoded else {
                    break;
                };
                decoded = reference.get();
            }
            match decoded {
                Value::Array(source) => context.encode_array_iterator(source),
                Value::Generator(generator) => context.encode_generator_iterator(generator),
                Value::Object(object) if native_spl_iterator_entries(&object).is_some() => {
                    let entries = native_spl_iterator_entries(&object).unwrap_or_default();
                    context.encode_iterator(entries, None, None, None, None)
                }
                Value::Object(object) if native_dom_collection_entries(&object).is_some() => {
                    let source = native_dom_collection_entries(&object).unwrap_or_default();
                    let entries = source
                        .iter()
                        .map(|(key, value)| {
                            let key = match key {
                                php_runtime::api::ArrayKey::Int(key) => Value::Int(key),
                                php_runtime::api::ArrayKey::String(key) => {
                                    Value::String(key.clone())
                                }
                            };
                            (key, value.clone())
                        })
                        .collect();
                    context.encode_iterator(entries, None, None, None, None)
                }
                Value::Object(object) if native_simple_xml_entries(&object).is_some() => {
                    let entries = native_simple_xml_entries(&object).unwrap_or_default();
                    context.encode_iterator(entries, None, None, None, None)
                }
                Value::Object(object) => {
                    let mut iterable = object;
                    let source_class = iterable.class_name();
                    let get_iterator_result = if let Some(get_iterator) =
                        native_method_in_hierarchy(context, &source_class, "getIterator")
                    {
                        let receiver = match context.encode(Value::Object(iterable.clone())) {
                            Ok(receiver) => receiver,
                            Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                        };
                        Some(invoke_native_method(context, get_iterator, &[receiver]))
                    } else if let Some((get_iterator, _)) =
                        native_external_method(context, &source_class, "getIterator")
                    {
                        let receiver = match context.encode(Value::Object(iterable.clone())) {
                            Ok(receiver) => receiver,
                            Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                        };
                        Some(invoke_native_external_function(
                            context,
                            get_iterator,
                            &[receiver],
                            Some(source_class.clone()),
                            context.unit.strict_types,
                        ))
                    } else {
                        None
                    };
                    if let Some(result) = get_iterator_result {
                        let result = match result {
                            Ok(result) => result,
                            Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                        };
                        let Ok(Value::Object(iterator)) = context.decode(result) else {
                            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                        };
                        iterable = iterator;
                    }
                    if let Some(entries) = native_spl_iterator_entries(&iterable) {
                        return match context.encode_iterator(entries, None, None, None, None) {
                            Ok(iterator) if write_native_value(out, iterator) => 0,
                            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                        };
                    }
                    let iterable_class = iterable.class_name();
                    let is_external_iterator = ["rewind", "valid", "current", "key", "next"]
                        .iter()
                        .all(|method| {
                            native_external_method(context, &iterable_class, method).is_some()
                        });
                    if is_external_iterator {
                        let invoke = |context: &mut NativeExecutionContext<'_>, method: &str| {
                            let encoded = invoke_native_bound_method(
                                context,
                                &php_runtime::api::CallableMethodTarget::Object(iterable.clone()),
                                method,
                                &[],
                                None,
                                context.unit.strict_types,
                                None,
                            )?;
                            context.decode(encoded)
                        };
                        if invoke(context, "rewind").is_err() {
                            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                        }
                        let mut entries = Vec::new();
                        loop {
                            let valid = match invoke(context, "valid") {
                                Ok(valid) => valid,
                                Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                            };
                            if !native_property_truthy(&valid) {
                                break;
                            }
                            let key = match invoke(context, "key") {
                                Ok(key) => key,
                                Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                            };
                            let value = match invoke(context, "current") {
                                Ok(value) => value,
                                Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                            };
                            entries.push((key, value));
                            if entries.len() >= 1_000_000 || invoke(context, "next").is_err() {
                                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                            }
                        }
                        return match context.encode_iterator(entries, None, None, None, None) {
                            Ok(iterator) if write_native_value(out, iterator) => 0,
                            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                        };
                    }
                    let rewind = native_method_in_hierarchy(context, &iterable_class, "rewind");
                    let is_user_iterator =
                        ["valid", "current", "key", "next"].iter().all(|method| {
                            native_method_in_hierarchy(context, &iterable_class, method).is_some()
                        });
                    if is_user_iterator && let Some(rewind) = rewind {
                        let receiver = match context.encode(Value::Object(iterable.clone())) {
                            Ok(receiver) => receiver,
                            Err(_) => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
                        };
                        if invoke_native_method(context, rewind, &[receiver]).is_err() {
                            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                        }
                        context.encode_iterator(Vec::new(), None, None, None, Some(iterable))
                    } else {
                        let class_name = normalize_class_name(&iterable.class_name());
                        let class = context
                            .unit
                            .classes
                            .iter()
                            .find(|class| class.name == class_name);
                        let mut names = class
                            .map(|class| {
                                class
                                    .properties
                                    .iter()
                                    .filter(|property| {
                                        !property.flags.is_private
                                            && !property.flags.is_protected
                                            && !property.flags.is_static
                                    })
                                    .map(|property| property.name.clone())
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        for (name, _) in iterable.properties_snapshot() {
                            if !names.iter().any(|candidate| candidate == &name)
                                && class.is_none_or(|class| {
                                    !class.properties.iter().any(|property| {
                                        property.name == name
                                            && (property.flags.is_private
                                                || property.flags.is_protected
                                                || property.flags.is_static)
                                    })
                                })
                            {
                                names.push(name);
                            }
                        }
                        let entries = names
                            .into_iter()
                            .map(|name| {
                                (
                                    Value::String(PhpString::from_bytes(name.into_bytes())),
                                    Value::Null,
                                )
                            })
                            .collect();
                        context.encode_iterator(entries, None, None, Some(iterable), None)
                    }
                }
                _ => return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
            }
        };
        match iterator {
            Ok(iterator) if write_native_value(out, iterator) => 0,
            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_foreach_next_abi(
    iterator: i64,
    key_out: *mut i64,
    value_out: *mut i64,
    has_out: *mut i64,
) -> i32 {
    with_native_context_for("foreach_next", |context| {
        let Ok(entry) = context.iterator_next(iterator) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let (key, value, has_value) = match entry {
            Some((key, value)) => {
                let (Ok(key), Ok(value)) = (context.encode(key), context.encode(value)) else {
                    return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
                };
                (key, value, 1)
            }
            None => (
                php_jit::jit_encode_constant(u32::MAX),
                php_jit::jit_encode_constant(u32::MAX),
                0,
            ),
        };
        if write_native_value(key_out, key)
            && write_native_value(value_out, value)
            && write_native_value(has_out, has_value)
        {
            0
        } else {
            php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_foreach_cleanup_abi(iterator: i64) -> i32 {
    with_native_context_for("foreach_cleanup", |context| {
        match context.close_iterator(iterator) {
            Ok(()) => 0,
            Err(_) => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_constant_fetch_abi(
    op: u32,
    function: i64,
    continuation: i64,
    out: *mut i64,
) -> i32 {
    if op != 0 {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    with_native_context_for("constant_fetch", |context| {
        let (Ok(function), Ok(continuation)) =
            (u32::try_from(function), u32::try_from(continuation))
        else {
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        let Some(instruction) = context.instruction_for_continuation(function, continuation) else {
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        let php_ir::InstructionKind::FetchConst { name, fallback, .. } = &instruction.kind else {
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        let value = context.lookup_constant(name).or_else(|error| {
            fallback
                .as_deref()
                .map_or(Err(error), |fallback| context.lookup_constant(fallback))
        });
        let Ok(value) = value else {
            context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
                "E_PHP_RUNTIME_UNDEFINED_CONSTANT",
                php_runtime::api::RuntimeSeverity::FatalError,
                format!("Undefined constant \"{name}\""),
                php_runtime::api::RuntimeSourceSpan {
                    file: context
                        .unit
                        .files
                        .get(instruction.span.file.index())
                        .map(|file| file.path.clone()),
                    start: instruction.span.start,
                    end: instruction.span.end,
                },
                Vec::new(),
                None,
            ));
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        match context.encode(value) {
            Ok(value) if write_native_value(out, value) => 0,
            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(in crate::vm) extern "C" fn jit_native_truthy_abi(src: i64, out: *mut i64) -> i32 {
    if let Some(value) = fast_native_truthy(src) {
        return if write_native_value(out, i64::from(value)) {
            0
        } else {
            php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
        };
    }
    with_native_context_for("truthy", |context| {
        context.attribute_active_helper("truthy", None);
        let Ok(src) = context.decode(src) else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        context.record_truthy_class(match &src {
            Value::Uninitialized => "uninitialized",
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
            Value::Reference(_) => "reference",
            Value::Callable(_) => "callable",
            Value::Resource(_) => "resource",
            Value::Generator(_) => "generator",
            Value::Fiber(_) => "fiber",
        });
        let mut operation = php_runtime::api::NativeOperationContext::default();
        let mut result = Value::Null;
        let status = php_runtime::api::native_cast(
            &mut operation,
            php_runtime::api::NativeCastOp::Bool,
            &src,
            &mut result,
        );
        let Value::Bool(result) = result else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        if status == php_runtime::api::NativeOperationStatus::Ok
            && write_native_value(out, i64::from(result))
        {
            0
        } else {
            helper_status(status)
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

// SAFETY: audited native ABI pointer boundary; see the function-local safety notes.
#[allow(unsafe_code)]
pub(in crate::vm) extern "C" fn jit_native_type_predicate_abi(
    op: u32,
    src: i64,
    out: *mut i64,
) -> i32 {
    let operation = match op {
        2 => php_jit::JitNativeTypePredicate::Int,
        3 => php_jit::JitNativeTypePredicate::Float,
        4 => php_jit::JitNativeTypePredicate::String,
        1 => php_jit::JitNativeTypePredicate::Bool,
        0 => php_jit::JitNativeTypePredicate::Null,
        5 => php_jit::JitNativeTypePredicate::Array,
        6 => php_jit::JitNativeTypePredicate::Object,
        7 => php_jit::JitNativeTypePredicate::Resource,
        8 => php_jit::JitNativeTypePredicate::Scalar,
        _ => return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32,
    };
    with_native_context_for("type_predicate", |context| {
        match super::native_builtins::execute_native_type_predicate_operation(
            context, src, operation,
        ) {
            Ok(value) if write_native_value(out, value) => 0,
            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_stable_length_abi(
    op: u32,
    src: i64,
    function: i64,
    continuation: i64,
    out: *mut i64,
) -> i32 {
    if op > 1 {
        return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
    }
    with_native_context_for("stable_length", |context| {
        let Some(source) = u32::try_from(function)
            .ok()
            .zip(u32::try_from(continuation).ok())
            .and_then(|(function, continuation)| {
                context.instruction_for_continuation(function, continuation)
            })
        else {
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        let name = if op == 0 { "strlen" } else { "count" };
        context.attribute_active_helper(name, u32::try_from(function).ok());
        match execute_native_builtin(context, name, &[src], &source, None, None) {
            Ok(value) if write_native_value(out, value) => 0,
            _ => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
        }
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}

pub(in crate::vm) extern "C" fn jit_native_runtime_fatal_abi(
    function: u32,
    continuation: u32,
) -> i32 {
    with_native_context_for("runtime_fatal", |context| {
        let Some(instruction) = context
            .instruction_for_continuation(function, continuation)
        else {
            return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
        };
        let php_ir::InstructionKind::RuntimeError {
            diagnostic_id,
            message,
        } = &instruction.kind
        else {
            if let php_ir::InstructionKind::CallFunction { name, args, .. } = &instruction.kind
                && let Some(target) = context
                    .function_id(name)
                    .and_then(|function| context.unit.functions.get(function.index()))
            {
                let required = target
                    .params
                    .iter()
                    .take_while(|parameter| parameter.required)
                    .count();
                let path = context
                    .unit
                    .files
                    .get(instruction.span.file.index())
                    .map_or("<unknown>", |file| file.path.as_str());
                let line = std::fs::read(path).ok().map_or(1, |bytes| {
                    bytes
                        .iter()
                        .take(instruction.span.start as usize)
                        .filter(|byte| **byte == b'\n')
                        .count()
                        + 1
                });
                let message = format!(
                    "Too few arguments to function {name}(), {} passed in {path} on line {line} and exactly {required} expected",
                    args.len()
                );
                context.output.write_slices(&[
                    b"\nFatal error: Uncaught ArgumentCountError: ",
                    message.as_bytes(),
                    b"\n",
                ]);
                context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
                    "E_PHP_VM_ARGUMENT_COUNT",
                    php_runtime::api::RuntimeSeverity::FatalError,
                    message,
                    php_runtime::api::RuntimeSourceSpan {
                        file: Some(path.to_owned()),
                        start: instruction.span.start,
                        end: instruction.span.end,
                    },
                    Vec::new(),
                    None,
                ));
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
            if let php_ir::InstructionKind::FetchStaticProperty {
                class_name,
                property,
                ..
            } = &instruction.kind
            {
                let normalized = normalize_class_name(class_name);
                let display_name = context
                    .unit
                    .classes
                    .iter()
                    .find(|class| class.name == normalized)
                    .map_or(class_name.as_str(), |class| class.display_name.as_str());
                let message =
                    format!("Access to undeclared static property {display_name}::${property}");
                context.output.write_slices(&[
                    b"\nFatal error: Uncaught Error: ",
                    message.as_bytes(),
                    b"\n",
                ]);
                context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
                    "E_UNDECLARED_STATIC_PROPERTY",
                    php_runtime::api::RuntimeSeverity::FatalError,
                    message,
                    php_runtime::api::RuntimeSourceSpan {
                        file: context
                            .unit
                            .files
                            .get(instruction.span.file.index())
                            .map(|file| file.path.clone()),
                        start: instruction.span.start,
                        end: instruction.span.end,
                    },
                    Vec::new(),
                    None,
                ));
                return php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32;
            }
            return php_jit::JitCallStatus::ABI_MISMATCH.0 as i32;
        };
        let class = if diagnostic_id == "E_PHP_VM_UNHANDLED_MATCH" {
            "UnhandledMatchError"
        } else {
            "Error"
        };
        context.output.write_slices(&[
            b"\nFatal error: Uncaught ",
            class.as_bytes(),
            b": ",
            message.as_bytes(),
            b"\n",
        ]);
        context.diagnostic = Some(php_runtime::api::RuntimeDiagnostic::new(
            diagnostic_id,
            php_runtime::api::RuntimeSeverity::FatalError,
            message,
            php_runtime::api::RuntimeSourceSpan {
                file: context
                    .unit
                    .files
                    .get(instruction.span.file.index())
                    .map(|file| file.path.clone()),
                start: instruction.span.start,
                end: instruction.span.end,
            },
            Vec::new(),
            None,
        ));
        php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
    })
    .unwrap_or(php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32)
}
