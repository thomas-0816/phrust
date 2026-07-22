//! Dispatch for typed Region semantic operations.
//!
//! Selection is driven exclusively by the append-only operation ID carried in
//! the native call frame. Source IR remains available to the individual typed
//! handler for PHP names and diagnostics; it is no longer searched to infer
//! which semantic operation was requested.

use super::*;

// SAFETY: generated code passes immutable numeric callsite metadata and a
// caller-owned packed operand/result area for the synchronous helper call.
#[allow(unsafe_code)]
pub(in crate::vm) extern "C" fn jit_native_semantic_dispatch_abi(
    runtime: *mut NativeRequestFastState,
    unit_identity: u64,
    function: u32,
    continuation: u32,
    operation: u32,
    operands: *const i64,
    operand_count: u32,
    out: *mut php_jit::JitCallResult,
) -> i32 {
    // SAFETY: this is the production instantiation; publication validated the
    // callsite and generated operand shape.
    unsafe {
        jit_native_semantic_dispatch_impl::<false>(
            runtime,
            unit_identity,
            function,
            continuation,
            operation,
            operands,
            operand_count,
            out,
        )
    }
}

// SAFETY: same internal ABI as the production entry. This separately
// published function adds diagnostic accounting so production code contains
// no telemetry branch.
#[allow(unsafe_code)]
pub(in crate::vm) extern "C" fn jit_native_semantic_dispatch_diagnostic_abi(
    runtime: *mut NativeRequestFastState,
    unit_identity: u64,
    function: u32,
    continuation: u32,
    operation: u32,
    operands: *const i64,
    operand_count: u32,
    out: *mut php_jit::JitCallResult,
) -> i32 {
    // SAFETY: diagnostic publication validates the same generated ABI.
    unsafe {
        jit_native_semantic_dispatch_impl::<true>(
            runtime,
            unit_identity,
            function,
            continuation,
            operation,
            operands,
            operand_count,
            out,
        )
    }
}

#[allow(unsafe_code)]
unsafe fn jit_native_semantic_dispatch_impl<const DIAGNOSTIC: bool>(
    runtime: *mut NativeRequestFastState,
    _unit_identity: u64,
    function: u32,
    continuation: u32,
    operation: u32,
    operands: *const i64,
    operand_count: u32,
    out: *mut php_jit::JitCallResult,
) -> i32 {
    debug_assert!(!runtime.is_null());
    debug_assert!(!out.is_null());
    debug_assert!(operand_count == 0 || !operands.is_null());
    // SAFETY: the published callsite owns exactly `operand_count` packed i64
    // operands for this synchronous invocation.
    let operands = if operand_count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(operands, operand_count as usize) }
    };
    // SAFETY: every native entry receives the live request state directly and
    // helpers cannot outlive the synchronous request invocation.
    let context = unsafe { native_cold_context(runtime) };
    // SAFETY: function/continuation are constants emitted together with this
    // immutable callsite table. Artifact publication rejects mismatched code.
    let descriptor = unsafe {
        &*context
            .prepared_native_callsite(function, continuation)
            .unwrap_unchecked()
    };
    debug_assert!(matches!(
        descriptor.kind,
        crate::compiled_unit::NativeCallSiteKind::Semantic
    ));
    // SAFETY: RegionSemanticOperationId is append-only and the compiler emits
    // only a validated enum discriminant from the prepared Region IR.
    let operation = unsafe {
        std::mem::transmute::<u32, php_jit::region_ir::RegionSemanticOperationId>(operation)
    };
    let helper_id = semantic_operation_helper_id(operation);
    if DIAGNOSTIC {
        context.enter_runtime_helper(helper_id);
    }
    let instruction = descriptor.semantic_instruction();
    let outcome = execute_native_semantic_operation(
        context,
        operation,
        instruction,
        operands,
        function,
        continuation,
    );
    let outcome = match outcome {
        Ok(encoded) if operation == php_jit::region_ir::RegionSemanticOperationId::BindGlobal => {
            let php_ir::InstructionKind::BindGlobal { name, .. } = &instruction.kind else {
                unreachable!("validated BindGlobal callsite must retain its source name")
            };
            context
                .publish_native_global_reference(function, continuation, name, encoded)
                .map(|()| encoded)
        }
        outcome => outcome,
    };
    if DIAGNOSTIC {
        context.exit_runtime_helper(helper_id);
    }
    super::call_dispatch::finish_native_dispatch_outcome(
        runtime,
        Some(outcome),
        Some(descriptor.span),
        out,
    )
}

pub(super) fn semantic_operation_from_frame(
    frame: &php_jit::JitNativeCallFrame,
) -> Result<Option<php_jit::region_ir::RegionSemanticOperationId>, String> {
    if frame.target.kind != php_jit::JitNativeCallKind::SEMANTIC_OPERATION {
        return Ok(None);
    }
    php_jit::region_ir::RegionSemanticOperationId::from_raw(frame.target.function_id)
        .map(Some)
        .ok_or_else(|| {
            format!(
                "JIT_NATIVE_UNKNOWN_SEMANTIC_OPERATION: {}",
                frame.target.function_id
            )
        })
}

pub(super) fn semantic_operation_helper_id(
    operation: php_jit::region_ir::RegionSemanticOperationId,
) -> &'static str {
    use php_jit::region_ir::RegionSemanticOperationId as Id;
    match operation {
        Id::StaticPropertyFetch
        | Id::StaticPropertyAssign
        | Id::StaticPropertyIsset
        | Id::StaticPropertyEmpty
        | Id::StaticPropertyDimIsset
        | Id::StaticPropertyDimEmpty
        | Id::StaticPropertyDimUnset
        | Id::StaticPropertyReference => "semantic_static_property",
        Id::ClassConstantFetch => "semantic_class_constant",
        Id::ObjectClassName => "semantic_object_class_name",
        Id::InstanceOf | Id::DynamicInstanceOf => "semantic_instanceof",
        Id::ResolveCallable => "semantic_resolve_callable",
        Id::AcquireCallable => "semantic_acquire_callable",
        Id::PropertyFetch
        | Id::PropertyAssign
        | Id::PropertyIsset
        | Id::PropertyEmpty
        | Id::PropertyUnset
        | Id::PropertyDimAssign
        | Id::PropertyDimIsset
        | Id::PropertyDimEmpty
        | Id::PropertyDimUnset => "semantic_property",
        Id::BindGlobal => "semantic_bind_global",
        Id::BoundClosureClass => "semantic_bound_closure_class",
    }
}

pub(super) fn execute_native_semantic_operation(
    context: &mut NativeRequestColdState<'_>,
    operation: php_jit::region_ir::RegionSemanticOperationId,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
    caller_function: u32,
    continuation: u32,
) -> Result<i64, String> {
    use php_jit::region_ir::RegionSemanticOperationId as Id;

    let outcome = match operation {
        Id::StaticPropertyFetch
        | Id::StaticPropertyAssign
        | Id::StaticPropertyIsset
        | Id::StaticPropertyEmpty
        | Id::StaticPropertyDimIsset
        | Id::StaticPropertyDimEmpty
        | Id::StaticPropertyDimUnset
        | Id::StaticPropertyReference => {
            execute_native_static_property(context, instruction, arguments, caller_function)
        }
        Id::ClassConstantFetch => {
            execute_native_class_constant(context, instruction, caller_function)
        }
        Id::InstanceOf | Id::DynamicInstanceOf => {
            execute_native_instanceof(context, instruction, arguments)
        }
        Id::ResolveCallable => execute_native_resolve_callable(context, instruction),
        Id::AcquireCallable => execute_native_acquire_callable(context, instruction, arguments),
        Id::PropertyFetch
        | Id::PropertyAssign
        | Id::PropertyIsset
        | Id::PropertyEmpty
        | Id::PropertyUnset
        | Id::PropertyDimAssign
        | Id::PropertyDimIsset
        | Id::PropertyDimEmpty
        | Id::PropertyDimUnset => execute_native_property_instruction(
            context,
            instruction,
            arguments,
            caller_function,
            Some(continuation),
        ),
        Id::BindGlobal => execute_native_bind_global(context, instruction),
        Id::BoundClosureClass => return execute_bound_closure_class(context, arguments),
        Id::ObjectClassName => None,
    };

    outcome.ok_or_else(|| {
        format!(
            "JIT_NATIVE_SEMANTIC_SOURCE_MISMATCH: operation={} function={} instruction={}",
            operation.raw(),
            caller_function,
            instruction.id.raw()
        )
    })?
}

fn execute_bound_closure_class(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
) -> Result<i64, String> {
    let [bound_object] = arguments else {
        return Err("bound closure class operation requires one object".to_owned());
    };
    let Value::Object(object) = context.decode(*bound_object)? else {
        return Err("bound closure class operation requires an object".to_owned());
    };
    let class_name = object.class_name();
    let display_name = context
        .unit
        .classes
        .iter()
        .find(|class| class.name == normalize_class_name(&class_name))
        .map_or(class_name, |class| class.display_name.clone());
    context.encode(Value::String(PhpString::from_bytes(
        display_name.as_bytes().to_vec(),
    )))
}
