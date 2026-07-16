//! Dispatch for typed Region semantic operations.
//!
//! Selection is driven exclusively by the append-only operation ID carried in
//! the native call frame. Source IR remains available to the individual typed
//! handler for PHP names and diagnostics; it is no longer searched to infer
//! which semantic operation was requested.

use super::*;

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
    context: &mut NativeExecutionContext<'_>,
    operation: php_jit::region_ir::RegionSemanticOperationId,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
    caller_function: u32,
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
        | Id::PropertyDimUnset => {
            execute_native_property_instruction(context, instruction, arguments, caller_function)
        }
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
    context: &mut NativeExecutionContext<'_>,
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
