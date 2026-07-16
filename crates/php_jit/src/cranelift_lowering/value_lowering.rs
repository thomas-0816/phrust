//! Direct CLIF lowering for proven scalar PHP values.

use super::*;

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
