//! Backend-neutral typed operations shared by native compiler tiers.

use crate::convert::{
    NumericValue, compare, equal, identical, php_float_to_int, to_array_php, to_bool, to_float,
    to_int, to_number, to_object_php, to_string,
};
use crate::{OutputBuffer, PhpString, Value};

/// ABI version for typed runtime operations.
pub const NATIVE_OPERATION_ABI_VERSION: u32 = 1;
/// Stable hash over versioned operation IDs and signatures.
pub const NATIVE_OPERATION_ABI_HASH: u64 = 0x6e61_7469_7665_0001;

/// Stable helper ID shared by every native compiler backend.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct JitHelperId(pub u32);

/// Typed unary-operation helper ID.
pub const JIT_HELPER_SCALAR_UNARY: JitHelperId = JitHelperId(1010);
/// Typed binary-operation helper ID.
pub const JIT_HELPER_SCALAR_BINARY: JitHelperId = JitHelperId(1011);
/// Typed comparison helper ID.
pub const JIT_HELPER_SCALAR_COMPARE: JitHelperId = JitHelperId(1012);
/// Typed cast helper ID.
pub const JIT_HELPER_SCALAR_CAST: JitHelperId = JitHelperId(1013);
/// Typed echo helper ID.
pub const JIT_HELPER_ECHO_VALUE: JitHelperId = JitHelperId(1020);

/// ABI-level argument and result types.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeAbiType {
    Context,
    Frame,
    Value,
    ValueOut,
    Bool,
    I64,
    U64,
    StringId,
    LocalId,
    FunctionId,
    ClassId,
    ArgumentSlice,
    UnaryOp,
    BinaryOp,
    CompareOp,
    CastOp,
    OutputBuffer,
    Status,
    Void,
}

/// Ownership contract for operands crossing the helper boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeOwnership {
    BorrowedRooted,
    CallerOwnedOutSlot,
    RuntimeOwnedHandle,
    ScalarByValue,
}

/// Stable explicit status returned by safe typed operations.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeOperationStatus {
    Ok = 0,
    RuntimeError = 1,
    Throw = 2,
    CallUserland = 3,
    Suspend = 4,
    Unsupported = 5,
}

/// Backend-neutral operation family audit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeOperationFamily {
    ConstantsDeclarations,
    GlobalsStatics,
    ReferencesLvalues,
    ScalarOperators,
    OutputDiagnostics,
    CallsArguments,
    ObjectsClone,
    Properties,
    StaticProperties,
    ClassConstantsInstanceOf,
    Arrays,
    Foreach,
    IncludeRequire,
    Eval,
    ExceptionsFinally,
    GeneratorsFibers,
    Autoload,
    Destructors,
    GcSafepoints,
    Resources,
    RuntimeErrors,
}

/// Complete audit row for one typed operation boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeOperationDescriptor {
    pub id: JitHelperId,
    pub name: &'static str,
    pub family: NativeOperationFamily,
    pub signature_version: u16,
    pub args: &'static [NativeAbiType],
    pub result: NativeAbiType,
    pub ownership: NativeOwnership,
    pub direct_callers: &'static [&'static str],
    pub native_callers: &'static [&'static str],
    pub may_call_user_code: bool,
    pub may_allocate: bool,
    pub may_throw: bool,
    pub may_diagnose: bool,
    pub may_suspend: bool,
    pub gc_safepoint: bool,
    /// Concrete symbol for native-callable entries; stable owner contract otherwise.
    pub implementation: &'static str,
    /// True only when baseline/optimized code may call the operation directly.
    pub native_callable: bool,
}

const UNARY_VALUE_OUT: &[NativeAbiType] = &[
    NativeAbiType::Context,
    NativeAbiType::UnaryOp,
    NativeAbiType::Value,
    NativeAbiType::ValueOut,
];
const BINARY_VALUE_VALUE_OUT: &[NativeAbiType] = &[
    NativeAbiType::Context,
    NativeAbiType::BinaryOp,
    NativeAbiType::Value,
    NativeAbiType::Value,
    NativeAbiType::ValueOut,
];
const COMPARE_VALUE_VALUE_OUT: &[NativeAbiType] = &[
    NativeAbiType::Context,
    NativeAbiType::CompareOp,
    NativeAbiType::Value,
    NativeAbiType::Value,
    NativeAbiType::ValueOut,
];
const CAST_VALUE_OUT: &[NativeAbiType] = &[
    NativeAbiType::Context,
    NativeAbiType::CastOp,
    NativeAbiType::Value,
    NativeAbiType::ValueOut,
];
const ECHO_VALUE: &[NativeAbiType] = &[
    NativeAbiType::Context,
    NativeAbiType::OutputBuffer,
    NativeAbiType::Value,
];
const CONTEXT_FRAME: &[NativeAbiType] = &[NativeAbiType::Context, NativeAbiType::Frame];
const CONTEXT_FRAME_VALUE_OUT: &[NativeAbiType] = &[
    NativeAbiType::Context,
    NativeAbiType::Frame,
    NativeAbiType::ValueOut,
];
const CONTEXT_FRAME_ARGS_OUT: &[NativeAbiType] = &[
    NativeAbiType::Context,
    NativeAbiType::Frame,
    NativeAbiType::ArgumentSlice,
    NativeAbiType::ValueOut,
];
const NATIVE_CALLERS: &[&str] = &["baseline", "optimizing"];
const RUNTIME_AND_NATIVE: &[&str] = &["php_runtime", "php_vm"];

macro_rules! operation_impl {
    ($id:literal, $name:literal, $family:ident, $args:expr, $result:ident, $ownership:ident,
     $user:literal, $allocate:literal, $throw:literal, $diagnose:literal, $suspend:literal, $gc:literal,
     $implementation:expr, $native_callable:literal) => {
        NativeOperationDescriptor {
            id: JitHelperId($id),
            name: $name,
            family: NativeOperationFamily::$family,
            signature_version: 1,
            args: $args,
            result: NativeAbiType::$result,
            ownership: NativeOwnership::$ownership,
            direct_callers: RUNTIME_AND_NATIVE,
            native_callers: NATIVE_CALLERS,
            may_call_user_code: $user,
            may_allocate: $allocate,
            may_throw: $throw,
            may_diagnose: $diagnose,
            may_suspend: $suspend,
            gc_safepoint: $gc,
            implementation: $implementation,
            native_callable: $native_callable,
        }
    };
}

macro_rules! operation {
    ($id:literal, $name:literal, $family:ident, $args:expr, $result:ident, $ownership:ident,
     $user:literal, $allocate:literal, $throw:literal, $diagnose:literal, $suspend:literal, $gc:literal) => {
        operation_impl!(
            $id,
            $name,
            $family,
            $args,
            $result,
            $ownership,
            $user,
            $allocate,
            $throw,
            $diagnose,
            $suspend,
            $gc,
            concat!("php_runtime::native_ops::contract::", $name),
            false
        )
    };
}

macro_rules! native_helper {
    ($id:literal, $name:literal, $family:ident, $args:expr, $result:ident, $ownership:ident,
     $user:literal, $allocate:literal, $throw:literal, $diagnose:literal, $suspend:literal, $gc:literal,
     $implementation:expr) => {
        operation_impl!(
            $id,
            $name,
            $family,
            $args,
            $result,
            $ownership,
            $user,
            $allocate,
            $throw,
            $diagnose,
            $suspend,
            $gc,
            $implementation,
            true
        )
    };
}

/// Required helper-family audit. Each row is a single typed semantic operation.
pub const NATIVE_OPERATION_REGISTRY: &[NativeOperationDescriptor] = &[
    operation!(
        1001,
        "constant_fetch",
        ConstantsDeclarations,
        CONTEXT_FRAME_VALUE_OUT,
        Status,
        CallerOwnedOutSlot,
        true,
        false,
        true,
        true,
        false,
        true
    ),
    operation!(
        1002,
        "declaration_publish",
        ConstantsDeclarations,
        CONTEXT_FRAME,
        Status,
        BorrowedRooted,
        true,
        true,
        true,
        true,
        false,
        true
    ),
    operation!(
        1003,
        "global_bind",
        GlobalsStatics,
        CONTEXT_FRAME,
        Status,
        RuntimeOwnedHandle,
        false,
        true,
        true,
        true,
        false,
        true
    ),
    operation!(
        1004,
        "reference_bind_lvalue",
        ReferencesLvalues,
        CONTEXT_FRAME,
        Status,
        RuntimeOwnedHandle,
        true,
        true,
        true,
        true,
        false,
        true
    ),
    native_helper!(
        1010,
        "scalar_unary",
        ScalarOperators,
        UNARY_VALUE_OUT,
        Status,
        CallerOwnedOutSlot,
        false,
        true,
        false,
        true,
        false,
        true,
        "php_runtime::api::native_unary"
    ),
    native_helper!(
        1011,
        "scalar_binary",
        ScalarOperators,
        BINARY_VALUE_VALUE_OUT,
        Status,
        CallerOwnedOutSlot,
        true,
        true,
        true,
        true,
        false,
        true,
        "php_runtime::api::native_binary"
    ),
    native_helper!(
        1012,
        "scalar_compare",
        ScalarOperators,
        COMPARE_VALUE_VALUE_OUT,
        Status,
        CallerOwnedOutSlot,
        true,
        false,
        true,
        true,
        false,
        true,
        "php_runtime::api::native_compare"
    ),
    native_helper!(
        1013,
        "scalar_cast",
        ScalarOperators,
        CAST_VALUE_OUT,
        Status,
        CallerOwnedOutSlot,
        true,
        true,
        true,
        true,
        false,
        true,
        "php_runtime::api::native_cast"
    ),
    native_helper!(
        1020,
        "echo_value",
        OutputDiagnostics,
        ECHO_VALUE,
        Status,
        BorrowedRooted,
        true,
        true,
        true,
        true,
        false,
        true,
        "php_runtime::api::native_echo"
    ),
    operation!(
        1021,
        "emit_diagnostic",
        OutputDiagnostics,
        CONTEXT_FRAME,
        Status,
        BorrowedRooted,
        true,
        true,
        true,
        true,
        false,
        true
    ),
    operation!(
        1030,
        "bind_call_arguments",
        CallsArguments,
        CONTEXT_FRAME_ARGS_OUT,
        Status,
        CallerOwnedOutSlot,
        true,
        true,
        true,
        true,
        false,
        true
    ),
    operation!(
        1031,
        "native_call_trampoline",
        CallsArguments,
        CONTEXT_FRAME_ARGS_OUT,
        Status,
        BorrowedRooted,
        true,
        true,
        true,
        true,
        true,
        true
    ),
    operation!(
        1040,
        "object_construct_clone",
        ObjectsClone,
        CONTEXT_FRAME_ARGS_OUT,
        Status,
        CallerOwnedOutSlot,
        true,
        true,
        true,
        true,
        false,
        true
    ),
    operation!(
        1041,
        "property_operation",
        Properties,
        CONTEXT_FRAME_VALUE_OUT,
        Status,
        CallerOwnedOutSlot,
        true,
        true,
        true,
        true,
        false,
        true
    ),
    operation!(
        1042,
        "static_property_operation",
        StaticProperties,
        CONTEXT_FRAME_VALUE_OUT,
        Status,
        CallerOwnedOutSlot,
        true,
        true,
        true,
        true,
        false,
        true
    ),
    operation!(
        1043,
        "class_constant_instanceof",
        ClassConstantsInstanceOf,
        CONTEXT_FRAME_VALUE_OUT,
        Status,
        CallerOwnedOutSlot,
        true,
        true,
        true,
        true,
        false,
        true
    ),
    operation!(
        1050,
        "array_operation",
        Arrays,
        CONTEXT_FRAME_VALUE_OUT,
        Status,
        CallerOwnedOutSlot,
        true,
        true,
        true,
        true,
        false,
        true
    ),
    operation!(
        1051,
        "foreach_operation",
        Foreach,
        CONTEXT_FRAME_VALUE_OUT,
        Status,
        RuntimeOwnedHandle,
        true,
        true,
        true,
        true,
        false,
        true
    ),
    operation!(
        1060,
        "include_require",
        IncludeRequire,
        CONTEXT_FRAME_VALUE_OUT,
        Status,
        CallerOwnedOutSlot,
        true,
        true,
        true,
        true,
        false,
        true
    ),
    operation!(
        1061,
        "eval_source",
        Eval,
        CONTEXT_FRAME_VALUE_OUT,
        Status,
        CallerOwnedOutSlot,
        true,
        true,
        true,
        true,
        false,
        true
    ),
    operation!(
        1070,
        "exception_finally",
        ExceptionsFinally,
        CONTEXT_FRAME,
        Status,
        RuntimeOwnedHandle,
        true,
        true,
        true,
        true,
        false,
        true
    ),
    operation!(
        1071,
        "generator_fiber_transition",
        GeneratorsFibers,
        CONTEXT_FRAME_VALUE_OUT,
        Status,
        RuntimeOwnedHandle,
        true,
        true,
        true,
        true,
        true,
        true
    ),
    operation!(
        1080,
        "autoload_resolve",
        Autoload,
        CONTEXT_FRAME_VALUE_OUT,
        Status,
        CallerOwnedOutSlot,
        true,
        true,
        true,
        true,
        false,
        true
    ),
    operation!(
        1081,
        "destructor_run",
        Destructors,
        CONTEXT_FRAME,
        Status,
        RuntimeOwnedHandle,
        true,
        true,
        true,
        true,
        false,
        true
    ),
    operation!(
        1082,
        "gc_safepoint",
        GcSafepoints,
        CONTEXT_FRAME,
        Status,
        BorrowedRooted,
        true,
        true,
        true,
        true,
        false,
        true
    ),
    operation!(
        1090,
        "resource_operation",
        Resources,
        CONTEXT_FRAME_VALUE_OUT,
        Status,
        CallerOwnedOutSlot,
        true,
        true,
        true,
        true,
        false,
        true
    ),
    operation!(
        1091,
        "runtime_fatal",
        RuntimeErrors,
        CONTEXT_FRAME,
        Status,
        BorrowedRooted,
        false,
        false,
        false,
        true,
        false,
        true
    ),
];

/// Looks up one stable runtime operation by its shared native helper ID.
#[must_use]
pub fn lookup_native_operation(id: JitHelperId) -> Option<&'static NativeOperationDescriptor> {
    NATIVE_OPERATION_REGISTRY
        .iter()
        .find(|operation| operation.id == id)
}

/// Validates ordering, uniqueness, ABI metadata, and native-callable ownership.
#[must_use]
pub fn native_operation_registry_is_stable() -> bool {
    NATIVE_OPERATION_REGISTRY
        .windows(2)
        .all(|pair| pair[0].id < pair[1].id)
        && NATIVE_OPERATION_REGISTRY.iter().all(|operation| {
            operation.signature_version > 0
                && !operation.args.is_empty()
                && !operation.direct_callers.is_empty()
                && !operation.native_callers.is_empty()
                && !operation.implementation.is_empty()
                && (!operation.native_callable
                    || operation
                        .implementation
                        .starts_with("php_runtime::api::native_"))
                && (!(operation.may_allocate
                    || operation.may_throw
                    || operation.may_call_user_code
                    || operation.may_diagnose
                    || operation.may_suspend)
                    || operation.gc_safepoint)
        })
}

/// Request-owned state updated by typed operations on failure or re-entry.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NativeOperationContext {
    pub diagnostic_id: Option<&'static str>,
    pub message: Option<String>,
    pub userland_call_requested: bool,
    pub safepoints: u64,
}

/// Typed unary operation, deliberately independent of IR/compiler enums.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeUnaryOp {
    Plus,
    Minus,
    Not,
    BitNot,
}

/// Typed binary operation, deliberately independent of IR/compiler enums.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Concat,
    Pow,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
}

/// Typed comparison operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeCompareOp {
    Equal,
    NotEqual,
    Identical,
    NotIdentical,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Spaceship,
}

/// Typed cast operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeCastOp {
    Bool,
    Int,
    Float,
    String,
    Array,
    Object,
    Void,
}

fn fail(
    context: &mut NativeOperationContext,
    id: &'static str,
    message: String,
) -> NativeOperationStatus {
    context.diagnostic_id = Some(id);
    context.message = Some(message);
    NativeOperationStatus::RuntimeError
}

fn publish(
    context: &mut NativeOperationContext,
    result: Result<Value, String>,
    out: &mut Value,
) -> NativeOperationStatus {
    context.safepoints = context.safepoints.saturating_add(1);
    match result {
        Ok(value) => {
            *out = value;
            NativeOperationStatus::Ok
        }
        Err(message) => fail(context, "E_NATIVE_RUNTIME_OPERATION", message),
    }
}

fn publish_operation(
    context: &mut NativeOperationContext,
    compute: impl FnOnce() -> Result<Value, String>,
    out: &mut Value,
) -> NativeOperationStatus {
    publish(context, compute(), out)
}

/// Executes one unary semantic operation into a caller-owned result slot.
pub fn native_unary(
    context: &mut NativeOperationContext,
    op: NativeUnaryOp,
    src: &Value,
    out: &mut Value,
) -> NativeOperationStatus {
    publish_operation(
        context,
        || match op {
            NativeUnaryOp::Plus => to_number(src).map(number_value),
            NativeUnaryOp::Minus => to_number(src).map(|number| match number {
                NumericValue::Int(value) => value
                    .checked_neg()
                    .map(Value::Int)
                    .unwrap_or_else(|| Value::float(-(value as f64))),
                NumericValue::Float(value) => Value::float(-value),
            }),
            NativeUnaryOp::Not => to_bool(src).map(|value| Value::Bool(!value)),
            NativeUnaryOp::BitNot => match effective_value(src) {
                Value::Int(value) => Ok(Value::Int(!value)),
                Value::String(value) => Ok(Value::String(PhpString::from_bytes(
                    value
                        .as_bytes()
                        .iter()
                        .map(|byte| !byte)
                        .collect::<Vec<_>>(),
                ))),
                Value::Float(value) => Ok(Value::Int(!php_float_to_int(value.to_f64()))),
                _ => Err("bitwise not requires int or string".to_owned()),
            },
        },
        out,
    )
}

/// Executes one binary semantic operation into a caller-owned result slot.
pub fn native_binary(
    context: &mut NativeOperationContext,
    op: NativeBinaryOp,
    lhs: &Value,
    rhs: &Value,
    out: &mut Value,
) -> NativeOperationStatus {
    publish_operation(
        context,
        || match op {
            NativeBinaryOp::Concat => match (to_string(lhs), to_string(rhs)) {
                (Ok(lhs), Ok(rhs)) => Ok(Value::String(PhpString::from_parts(&[
                    lhs.as_bytes(),
                    rhs.as_bytes(),
                ]))),
                (Err(error), _) | (_, Err(error)) => Err(error),
            },
            NativeBinaryOp::BitAnd
            | NativeBinaryOp::BitOr
            | NativeBinaryOp::BitXor
            | NativeBinaryOp::ShiftLeft
            | NativeBinaryOp::ShiftRight => native_bitwise(op, lhs, rhs),
            NativeBinaryOp::Pow => match (to_number(lhs), to_number(rhs)) {
                (Ok(lhs), Ok(rhs)) => Ok(native_power(lhs, rhs)),
                (Err(error), _) | (_, Err(error)) => Err(error),
            },
            NativeBinaryOp::Add
            | NativeBinaryOp::Sub
            | NativeBinaryOp::Mul
            | NativeBinaryOp::Div
            | NativeBinaryOp::Mod => {
                if matches!(op, NativeBinaryOp::Add)
                    && let Some(union) = array_union(lhs, rhs)
                {
                    Ok(union)
                } else {
                    match (to_number(lhs), to_number(rhs)) {
                        (Ok(lhs), Ok(rhs)) => native_arithmetic(op, lhs, rhs),
                        (Err(error), _) | (_, Err(error)) => Err(error),
                    }
                }
            }
        },
        out,
    )
}

/// Executes one comparison semantic operation into a caller-owned result slot.
pub fn native_compare(
    context: &mut NativeOperationContext,
    op: NativeCompareOp,
    lhs: &Value,
    rhs: &Value,
    out: &mut Value,
) -> NativeOperationStatus {
    publish_operation(
        context,
        || match op {
            NativeCompareOp::Identical => Ok(Value::Bool(identical(lhs, rhs))),
            NativeCompareOp::NotIdentical => Ok(Value::Bool(!identical(lhs, rhs))),
            NativeCompareOp::Equal => equal(lhs, rhs).map(Value::Bool),
            NativeCompareOp::NotEqual => equal(lhs, rhs).map(|value| Value::Bool(!value)),
            NativeCompareOp::Less => compare(lhs, rhs).map(|order| Value::Bool(order.is_lt())),
            NativeCompareOp::LessEqual => {
                compare(lhs, rhs).map(|order| Value::Bool(!order.is_gt()))
            }
            NativeCompareOp::Greater => compare(lhs, rhs).map(|order| Value::Bool(order.is_gt())),
            NativeCompareOp::GreaterEqual => {
                compare(lhs, rhs).map(|order| Value::Bool(!order.is_lt()))
            }
            NativeCompareOp::Spaceship => compare(lhs, rhs).map(|order| {
                Value::Int(match order {
                    std::cmp::Ordering::Less => -1,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => 1,
                })
            }),
        },
        out,
    )
}

/// Executes one cast semantic operation into a caller-owned result slot.
pub fn native_cast(
    context: &mut NativeOperationContext,
    op: NativeCastOp,
    src: &Value,
    out: &mut Value,
) -> NativeOperationStatus {
    publish_operation(
        context,
        || match op {
            NativeCastOp::Bool => to_bool(src).map(Value::Bool),
            NativeCastOp::Int => to_int(src).map(Value::Int),
            NativeCastOp::Float => to_float(src).map(Value::float),
            NativeCastOp::String => to_string(src).map(Value::String),
            NativeCastOp::Array => to_array_php(src).map(Value::Array),
            NativeCastOp::Object => to_object_php(src),
            NativeCastOp::Void => Ok(Value::Null),
        },
        out,
    )
}

/// Emits one value without allocating a boxed transfer result.
pub fn native_echo(
    context: &mut NativeOperationContext,
    output: &mut OutputBuffer,
    value: &Value,
) -> NativeOperationStatus {
    context.safepoints = context.safepoints.saturating_add(1);
    match to_string(value) {
        Ok(value) => {
            output.write_php_string(&value);
            NativeOperationStatus::Ok
        }
        Err(message) => fail(context, "E_NATIVE_ECHO", message),
    }
}

fn effective_value(value: &Value) -> Value {
    match value {
        Value::Reference(cell) => effective_value(&cell.get()),
        value => value.clone(),
    }
}

fn number_value(value: NumericValue) -> Value {
    match value {
        NumericValue::Int(value) => Value::Int(value),
        NumericValue::Float(value) => Value::float(value),
    }
}

fn native_arithmetic(
    op: NativeBinaryOp,
    lhs: NumericValue,
    rhs: NumericValue,
) -> Result<Value, String> {
    match op {
        NativeBinaryOp::Add => Ok(checked_numeric(lhs, rhs, i64::checked_add, |a, b| a + b)),
        NativeBinaryOp::Sub => Ok(checked_numeric(lhs, rhs, i64::checked_sub, |a, b| a - b)),
        NativeBinaryOp::Mul => Ok(checked_numeric(lhs, rhs, i64::checked_mul, |a, b| a * b)),
        NativeBinaryOp::Div if rhs.as_f64() == 0.0 => Err("division by zero".to_owned()),
        NativeBinaryOp::Div => {
            if let (NumericValue::Int(lhs), NumericValue::Int(rhs)) = (lhs, rhs)
                && lhs % rhs == 0
            {
                Ok(Value::Int(lhs / rhs))
            } else {
                Ok(Value::float(lhs.as_f64() / rhs.as_f64()))
            }
        }
        NativeBinaryOp::Mod => {
            let rhs = rhs.as_f64() as i64;
            if rhs == 0 {
                Err("modulo by zero".to_owned())
            } else {
                let lhs = lhs.as_f64() as i64;
                Ok(Value::Int(lhs.checked_rem(rhs).unwrap_or(0)))
            }
        }
        NativeBinaryOp::Concat
        | NativeBinaryOp::Pow
        | NativeBinaryOp::BitAnd
        | NativeBinaryOp::BitOr
        | NativeBinaryOp::BitXor
        | NativeBinaryOp::ShiftLeft
        | NativeBinaryOp::ShiftRight => Err("operation is not arithmetic".to_owned()),
    }
}

fn checked_numeric(
    lhs: NumericValue,
    rhs: NumericValue,
    integer: fn(i64, i64) -> Option<i64>,
    float: fn(f64, f64) -> f64,
) -> Value {
    if let (NumericValue::Int(lhs), NumericValue::Int(rhs)) = (lhs, rhs) {
        integer(lhs, rhs)
            .map(Value::Int)
            .unwrap_or_else(|| Value::float(float(lhs as f64, rhs as f64)))
    } else {
        Value::float(float(lhs.as_f64(), rhs.as_f64()))
    }
}

fn native_power(lhs: NumericValue, rhs: NumericValue) -> Value {
    if let (NumericValue::Int(lhs), NumericValue::Int(rhs)) = (lhs, rhs)
        && rhs >= 0
        && let Ok(exponent) = u32::try_from(rhs)
        && let Some(value) = lhs.checked_pow(exponent)
    {
        Value::Int(value)
    } else {
        Value::float(lhs.as_f64().powf(rhs.as_f64()))
    }
}

fn native_bitwise(op: NativeBinaryOp, lhs: &Value, rhs: &Value) -> Result<Value, String> {
    if let (Value::String(lhs), Value::String(rhs)) = (lhs, rhs) {
        let bytes = match op {
            NativeBinaryOp::BitAnd => lhs
                .as_bytes()
                .iter()
                .zip(rhs.as_bytes())
                .map(|(left, right)| left & right)
                .collect(),
            NativeBinaryOp::BitXor => lhs
                .as_bytes()
                .iter()
                .zip(rhs.as_bytes())
                .map(|(left, right)| left ^ right)
                .collect(),
            NativeBinaryOp::BitOr => bitwise_string_or(lhs.as_bytes(), rhs.as_bytes()),
            NativeBinaryOp::Add
            | NativeBinaryOp::Sub
            | NativeBinaryOp::Mul
            | NativeBinaryOp::Div
            | NativeBinaryOp::Mod
            | NativeBinaryOp::Concat
            | NativeBinaryOp::Pow
            | NativeBinaryOp::ShiftLeft
            | NativeBinaryOp::ShiftRight => {
                return Err("string operands require a bitwise operation".to_owned());
            }
        };
        return Ok(Value::String(PhpString::from_bytes(bytes)));
    }
    let lhs = to_int(lhs)?;
    let rhs = to_int(rhs)?;
    match op {
        NativeBinaryOp::BitAnd => Ok(Value::Int(lhs & rhs)),
        NativeBinaryOp::BitOr => Ok(Value::Int(lhs | rhs)),
        NativeBinaryOp::BitXor => Ok(Value::Int(lhs ^ rhs)),
        NativeBinaryOp::ShiftLeft if rhs < 0 => Err("bit shift by negative number".to_owned()),
        NativeBinaryOp::ShiftLeft => Ok(Value::Int(lhs.wrapping_shl(rhs as u32))),
        NativeBinaryOp::ShiftRight if rhs < 0 => Err("bit shift by negative number".to_owned()),
        NativeBinaryOp::ShiftRight => Ok(Value::Int(lhs.wrapping_shr(rhs as u32))),
        NativeBinaryOp::Add
        | NativeBinaryOp::Sub
        | NativeBinaryOp::Mul
        | NativeBinaryOp::Div
        | NativeBinaryOp::Mod
        | NativeBinaryOp::Concat
        | NativeBinaryOp::Pow => Err("operation is not bitwise".to_owned()),
    }
}

fn bitwise_string_or(lhs: &[u8], rhs: &[u8]) -> Vec<u8> {
    let length = lhs.len().max(rhs.len());
    let mut bytes = Vec::with_capacity(length);
    for index in 0..length {
        match (lhs.get(index), rhs.get(index)) {
            (Some(lhs), Some(rhs)) => bytes.push(lhs | rhs),
            (Some(lhs), None) => bytes.push(*lhs),
            (None, Some(rhs)) => bytes.push(*rhs),
            (None, None) => break,
        }
    }
    bytes
}

fn array_union(lhs: &Value, rhs: &Value) -> Option<Value> {
    let Value::Array(lhs) = effective_value(lhs) else {
        return None;
    };
    let Value::Array(rhs) = effective_value(rhs) else {
        return None;
    };
    let mut result = lhs;
    for (key, value) in rhs.iter() {
        if result.get(&key).is_none() {
            result.insert(key.clone(), effective_value(value));
        }
    }
    Some(Value::Array(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_scalar_operations_write_caller_slots_and_status() {
        let mut context = NativeOperationContext::default();
        let mut out = Value::Uninitialized;
        assert_eq!(
            native_binary(
                &mut context,
                NativeBinaryOp::Add,
                &Value::Int(20),
                &Value::Int(22),
                &mut out,
            ),
            NativeOperationStatus::Ok
        );
        assert_eq!(out, Value::Int(42));
        assert_eq!(context.safepoints, 1);
    }

    #[test]
    fn modulo_minimum_integer_by_negative_one_does_not_panic() {
        assert_eq!(
            native_arithmetic(
                NativeBinaryOp::Mod,
                NumericValue::Int(i64::MIN),
                NumericValue::Int(-1),
            ),
            Ok(Value::Int(0))
        );
    }

    #[test]
    fn registry_has_every_required_family_and_unique_ids() {
        assert!(native_operation_registry_is_stable());
        let mut ids = NATIVE_OPERATION_REGISTRY
            .iter()
            .map(|operation| operation.id)
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), NATIVE_OPERATION_REGISTRY.len());
        for family in [
            NativeOperationFamily::ConstantsDeclarations,
            NativeOperationFamily::GlobalsStatics,
            NativeOperationFamily::ReferencesLvalues,
            NativeOperationFamily::ScalarOperators,
            NativeOperationFamily::OutputDiagnostics,
            NativeOperationFamily::CallsArguments,
            NativeOperationFamily::ObjectsClone,
            NativeOperationFamily::Properties,
            NativeOperationFamily::StaticProperties,
            NativeOperationFamily::ClassConstantsInstanceOf,
            NativeOperationFamily::Arrays,
            NativeOperationFamily::Foreach,
            NativeOperationFamily::IncludeRequire,
            NativeOperationFamily::Eval,
            NativeOperationFamily::ExceptionsFinally,
            NativeOperationFamily::GeneratorsFibers,
            NativeOperationFamily::Autoload,
            NativeOperationFamily::Destructors,
            NativeOperationFamily::GcSafepoints,
            NativeOperationFamily::Resources,
            NativeOperationFamily::RuntimeErrors,
        ] {
            assert!(
                NATIVE_OPERATION_REGISTRY
                    .iter()
                    .any(|operation| operation.family == family)
            );
        }

        let native_ids = NATIVE_OPERATION_REGISTRY
            .iter()
            .filter(|operation| operation.native_callable)
            .map(|operation| operation.id.0)
            .collect::<Vec<_>>();
        assert_eq!(native_ids, [1010, 1011, 1012, 1013, 1020]);
    }
}
