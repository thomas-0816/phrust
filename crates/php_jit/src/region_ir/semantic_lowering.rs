//! Construction-time checks for typed semantic native calls.

use super::{NativeCompileError, RegionCallTarget, RegionNativeCall, RegionSemanticContext};

pub(super) fn validate_semantic_call(
    call: &RegionNativeCall,
    expected_context: RegionSemanticContext,
) -> Result<(), NativeCompileError> {
    let RegionCallTarget::Semantic { operation } = &call.target else {
        return Ok(());
    };
    if operation.context() != expected_context {
        return Err(NativeCompileError::new(
            "JIT_REGION_REJECT_SEMANTIC_CONTEXT",
            format!(
                "semantic operation {} carries {:?}, expected {:?}",
                operation.operation_id().raw(),
                operation.context(),
                expected_context
            ),
        ));
    }
    if !call.args.is_empty() || call.direct_arity.is_some() || call.variadic {
        return Err(NativeCompileError::new(
            "JIT_REGION_REJECT_SEMANTIC_CALL_SHAPE",
            format!(
                "semantic operation {} must not use PHP call binding metadata",
                operation.operation_id().raw()
            ),
        ));
    }
    let expected_operands = operation.materialized_operand_count();
    if call.operands.len() != expected_operands || call.operands.iter().any(Option::is_none) {
        return Err(NativeCompileError::new(
            "JIT_REGION_REJECT_SEMANTIC_OPERANDS",
            format!(
                "semantic operation {} requires {expected_operands} materialized operands, got {}",
                operation.operation_id().raw(),
                call.operands.len()
            ),
        ));
    }
    Ok(())
}
