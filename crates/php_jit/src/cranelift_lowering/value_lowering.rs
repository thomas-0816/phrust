//! Direct CLIF lowering for proven scalar PHP values.

use super::*;

pub(super) const NATIVE_EXACT_ARITHMETIC_COUNT: usize = 11;
pub(super) const NATIVE_EXACT_ARITHMETIC_OPERATIONS: [RegionBinaryOp;
    NATIVE_EXACT_ARITHMETIC_COUNT] = [
    RegionBinaryOp::Add,
    RegionBinaryOp::Sub,
    RegionBinaryOp::Mul,
    RegionBinaryOp::Div,
    RegionBinaryOp::Mod,
    RegionBinaryOp::Pow,
    RegionBinaryOp::BitAnd,
    RegionBinaryOp::BitOr,
    RegionBinaryOp::BitXor,
    RegionBinaryOp::ShiftLeft,
    RegionBinaryOp::ShiftRight,
];

pub(super) fn native_exact_arithmetic_index(operation: RegionBinaryOp) -> usize {
    match operation {
        RegionBinaryOp::Add => 0,
        RegionBinaryOp::Sub => 1,
        RegionBinaryOp::Mul => 2,
        RegionBinaryOp::Div => 3,
        RegionBinaryOp::Mod => 4,
        RegionBinaryOp::Pow => 5,
        RegionBinaryOp::BitAnd => 6,
        RegionBinaryOp::BitOr => 7,
        RegionBinaryOp::BitXor => 8,
        RegionBinaryOp::ShiftLeft => 9,
        RegionBinaryOp::ShiftRight => 10,
        RegionBinaryOp::Concat => unreachable!("concatenation has its own exact leaf"),
    }
}

pub(super) const fn native_exact_arithmetic_symbol(operation: RegionBinaryOp) -> &'static str {
    match operation {
        RegionBinaryOp::Add => "phrust_native_add",
        RegionBinaryOp::Sub => "phrust_native_subtract",
        RegionBinaryOp::Mul => "phrust_native_multiply",
        RegionBinaryOp::Div => "phrust_native_divide",
        RegionBinaryOp::Mod => "phrust_native_modulo",
        RegionBinaryOp::Pow => "phrust_native_power",
        RegionBinaryOp::BitAnd => "phrust_native_exact_bit_and",
        RegionBinaryOp::BitOr => "phrust_native_exact_bit_or",
        RegionBinaryOp::BitXor => "phrust_native_exact_bit_xor",
        RegionBinaryOp::ShiftLeft => "phrust_native_shift_left",
        RegionBinaryOp::ShiftRight => "phrust_native_shift_right",
        RegionBinaryOp::Concat => "phrust_unreachable_arithmetic",
    }
}

/// Publishes one immutable constant from its explicit unit-local index.
///
/// The index is producer metadata; it is never recovered from the value bits.
/// Dynamic SSA values are already authoritative native encodings, and after a
/// linked call the active literal table can belong to a different unit than a
/// value's producer. The returned flag is true when the result borrows the
/// published literal slot's owner.
pub(super) fn lower_native_literal_value(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    constant_index: u32,
    deopt_out: ir::Value,
) -> (ir::Value, ir::Value) {
    if constant_index >= crate::JIT_VALUE_TRUE {
        let not_borrowed = builder.ins().iconst(types::I8, 0);
        return (value, not_borrowed);
    }
    // The index is embedded by this compiled unit and the table is published
    // before its entrypoint becomes callable. Named/class constants keep an
    // empty slot, selected branchlessly back to their original encoding;
    // immutable literals load the request-owned value.
    let pointer_type = builder.func.dfg.value_type(deopt_out);
    let view = lower_active_runtime_view(builder, deopt_out);
    let slots = builder.ins().load(
        pointer_type,
        MemFlagsData::new(),
        view,
        std::mem::offset_of!(crate::JitNativeRuntimeView, trusted_literal_slots) as i32,
    );
    let slot = builder.ins().iadd_imm(
        slots,
        i64::from(constant_index)
            * i64::try_from(std::mem::size_of::<crate::JitNativeTrustedLiteralSlot>())
                .unwrap_or(i64::MAX),
    );
    let state = builder.ins().load(
        types::I32,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeTrustedLiteralSlot, state) as i32,
    );
    let ready = builder.ins().icmp_imm(
        IntCC::Equal,
        state,
        i64::from(crate::JIT_NATIVE_TRUSTED_LITERAL_PUBLISHED),
    );
    let native = builder.ins().load(
        types::I64,
        MemFlagsData::new(),
        slot,
        std::mem::offset_of!(crate::JitNativeTrustedLiteralSlot, value) as i32,
    );
    let selected = builder.ins().select(ready, native, value);
    (selected, ready)
}

pub(super) fn native_callable_return_type_is_releasable_scalar(
    type_: &php_ir::IrReturnType,
) -> bool {
    use php_ir::IrReturnType as Type;
    match type_ {
        Type::Int
        | Type::Float
        | Type::String
        | Type::Bool
        | Type::Null
        | Type::False
        | Type::True
        | Type::Void
        | Type::Never => true,
        Type::Nullable { inner } => native_callable_return_type_is_releasable_scalar(inner),
        Type::Union { members } => {
            !members.is_empty()
                && members
                    .iter()
                    .all(native_callable_return_type_is_releasable_scalar)
        }
        Type::Array
        | Type::Callable
        | Type::Iterable
        | Type::Object
        | Type::Mixed
        | Type::Class { .. }
        | Type::Intersection { .. }
        | Type::Dnf { .. } => false,
    }
}

pub(super) fn encode_native_bool(
    builder: &mut FunctionBuilder<'_>,
    condition: ir::Value,
) -> ir::Value {
    let false_value = builder.ins().iconst(
        types::I64,
        crate::jit_encode_constant(crate::JIT_VALUE_FALSE),
    );
    let true_value = builder.ins().iconst(
        types::I64,
        crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
    );
    builder.ins().select(condition, true_value, false_value)
}

pub(super) fn scalar_truthy(
    builder: &mut FunctionBuilder<'_>,
    value: ir::Value,
    class: SsaValueClass,
) -> Option<ir::Value> {
    match class {
        SsaValueClass::Null => Some(builder.ins().icmp(IntCC::NotEqual, value, value)),
        SsaValueClass::Bool => Some(builder.ins().icmp_imm(
            IntCC::Equal,
            value,
            crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
        )),
        SsaValueClass::Int => Some(builder.ins().icmp_imm(IntCC::NotEqual, value, 0)),
        _ => None,
    }
}

pub(super) fn lower_direct_compare(
    builder: &mut FunctionBuilder<'_>,
    op: RegionCompareOpCode,
    lhs: ir::Value,
    rhs: ir::Value,
    lhs_class: SsaValueClass,
    rhs_class: SsaValueClass,
) -> Option<ir::Value> {
    // Mixed handles carry no identity semantics. Equal strings and floats can
    // live in different request slots, while arrays require structural
    // identity and references require dereferencing. Let the tier-specific
    // exact lowering classify their stable native representation instead of
    // comparing encoded arena indexes.
    if lhs_class == SsaValueClass::MixedHandle || rhs_class == SsaValueClass::MixedHandle {
        return None;
    }
    if lhs_class == SsaValueClass::Int && rhs_class == SsaValueClass::Int {
        if op == RegionCompareOpCode::Spaceship {
            let less = builder.ins().icmp(IntCC::SignedLessThan, lhs, rhs);
            let greater = builder.ins().icmp(IntCC::SignedGreaterThan, lhs, rhs);
            let less = builder.ins().uextend(types::I64, less);
            let greater = builder.ins().uextend(types::I64, greater);
            return Some(builder.ins().isub(greater, less));
        }
        let condition = builder.ins().icmp(region_compare_intcc(op), lhs, rhs);
        return Some(encode_native_bool(builder, condition));
    }
    if lhs_class != rhs_class {
        if matches!(
            op,
            RegionCompareOpCode::Identical | RegionCompareOpCode::NotIdentical
        ) {
            let different = op == RegionCompareOpCode::NotIdentical;
            let condition = builder.ins().icmp(
                if different {
                    IntCC::Equal
                } else {
                    IntCC::NotEqual
                },
                lhs,
                lhs,
            );
            return Some(encode_native_bool(builder, condition));
        }
        return None;
    }
    if matches!(lhs_class, SsaValueClass::Bool | SsaValueClass::Null) {
        let (lhs, rhs) = if lhs_class == SsaValueClass::Bool {
            let true_value = crate::jit_encode_constant(crate::JIT_VALUE_TRUE);
            let lhs = builder.ins().icmp_imm(IntCC::Equal, lhs, true_value);
            let rhs = builder.ins().icmp_imm(IntCC::Equal, rhs, true_value);
            (
                builder.ins().uextend(types::I64, lhs),
                builder.ins().uextend(types::I64, rhs),
            )
        } else {
            let zero = builder.ins().iconst(types::I64, 0);
            (zero, zero)
        };
        if op == RegionCompareOpCode::Spaceship {
            let less = builder.ins().icmp(IntCC::SignedLessThan, lhs, rhs);
            let greater = builder.ins().icmp(IntCC::SignedGreaterThan, lhs, rhs);
            let less = builder.ins().uextend(types::I64, less);
            let greater = builder.ins().uextend(types::I64, greater);
            return Some(builder.ins().isub(greater, less));
        }
        let condition = builder.ins().icmp(region_compare_intcc(op), lhs, rhs);
        return Some(encode_native_bool(builder, condition));
    }
    None
}

pub(super) fn lower_direct_cast(
    builder: &mut FunctionBuilder<'_>,
    op: RegionCastOp,
    value: ir::Value,
    class: SsaValueClass,
) -> Option<ir::Value> {
    match op {
        RegionCastOp::Bool => scalar_truthy(builder, value, class)
            .map(|condition| encode_native_bool(builder, condition)),
        RegionCastOp::Int => match class {
            SsaValueClass::Int => Some(value),
            SsaValueClass::Null => Some(builder.ins().iconst(types::I64, 0)),
            SsaValueClass::Bool => {
                let condition = builder.ins().icmp_imm(
                    IntCC::Equal,
                    value,
                    crate::jit_encode_constant(crate::JIT_VALUE_TRUE),
                );
                Some(builder.ins().uextend(types::I64, condition))
            }
            _ => None,
        },
        RegionCastOp::Void => Some(
            builder
                .ins()
                .iconst(types::I64, crate::jit_encode_constant(u32::MAX)),
        ),
        RegionCastOp::Float | RegionCastOp::String | RegionCastOp::Array | RegionCastOp::Object => {
            None
        }
    }
}
