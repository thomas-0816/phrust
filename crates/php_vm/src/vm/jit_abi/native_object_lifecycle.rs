//! Exact object-lifecycle preparation for the generated release spine.
//!
//! These leaves resolve immutable destructor metadata and retire native object
//! storage. They never invoke a PHP body and never recursively release a PHP
//! value; generated code owns both operations.

use super::*;

fn write_action(
    output: *mut php_jit::JitNativeDestructorAction,
    action: php_jit::JitNativeDestructorAction,
) -> Result<(), String> {
    if output.is_null() {
        return Err("generated destructor action output is null".to_owned());
    }
    // SAFETY: generated code supplies one live stack-owned output record.
    #[allow(unsafe_code)]
    unsafe {
        output.write(action);
    }
    Ok(())
}

fn write_children(
    output: *mut php_jit::JitNativeReleaseChildren,
    children: Vec<i64>,
) -> Result<(), String> {
    if output.is_null() {
        return Err("generated release-child output is null".to_owned());
    }
    let owner = Box::new(children);
    let action = php_jit::JitNativeReleaseChildren {
        values: owner.as_ptr() as usize as u64,
        count: u32::try_from(owner.len())
            .map_err(|_| "generated release-child count overflow".to_owned())?,
        token: Box::into_raw(owner) as usize as u64,
        reserved: 0,
    };
    // SAFETY: generated code supplies one live stack-owned output record.
    #[allow(unsafe_code)]
    unsafe {
        output.write(action);
    }
    Ok(())
}

pub(in crate::vm) extern "C" fn jit_native_object_release_prepare_abi(
    runtime: *mut NativeRequestFastState,
    receiver: i64,
    output: *mut php_jit::JitNativeDestructorAction,
) -> i32 {
    let result = with_baseline_native_context_for(runtime, "object_release_prepare", |context| {
        let index = NativeRequestColdState::direct_value_index(receiver)
            .ok_or_else(|| "generated destructor received a non-direct receiver".to_owned())?;
        let slot = context
            .direct_value_slots
            .get(index)
            .copied()
            .ok_or_else(|| "generated destructor receiver slot is unavailable".to_owned())?;
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT || slot.refcount != 1 {
            return Err("generated destructor requires one final direct object owner".to_owned());
        }
        let object = context
            .direct_object(index)
            .ok_or_else(|| "generated destructor receiver identity is unavailable".to_owned())?;
        let mut action = php_jit::JitNativeDestructorAction::default();

        // A genuine cold alias still owns the object. Its direct descriptor is
        // retired by the finalizer, but destruction remains deferred until the
        // outer alias becomes unreachable.
        if object.gc_refcount_estimate() > 2
            || context
                .destroyed_objects
                .get(&object.id())
                .is_some_and(WeakObjectHandle::is_alive)
        {
            return write_action(output, action);
        }

        // SAFETY: the request fast state is the live prefix of this same
        // synchronous context. Only immutable symbol/publication capabilities
        // are read while the cold owner remains mutably borrowed.
        #[allow(unsafe_code)]
        let fast = unsafe { &mut *runtime };
        let Some(plan) = fast
            .symbol_query
            .destructor_callable_plan(&object.class_name())
        else {
            return write_action(output, action);
        };
        let runtime_view = if plan.runtime_view != 0 {
            plan.runtime_view
        } else if fast.header.runtime_view_pointer != 0 {
            fast.header.runtime_view_pointer
        } else {
            std::ptr::from_ref(&fast.header.runtime_view) as usize as u64
        };
        // SAFETY: callable publication returns a request-stable immutable view
        // and validates the function index before exposing the plan.
        #[allow(unsafe_code)]
        let view = unsafe { &*(runtime_view as usize as *const php_jit::JitNativeRuntimeView) };
        if plan.function.index() >= view.trusted_preferred_function_entry_count as usize
            || plan.function.index() >= view.trusted_function_contract_count as usize
        {
            return Err("generated destructor publication index is out of range".to_owned());
        }
        let entries = view.trusted_preferred_function_entries as usize
            as *const std::sync::atomic::AtomicUsize;
        let contracts = view.trusted_function_contracts as usize
            as *const php_jit::JitNativeFunctionContractView;
        if entries.is_null() || contracts.is_null() {
            return Err("generated destructor publication tables are unavailable".to_owned());
        }
        // SAFETY: both indices were checked against immutable published table
        // lengths and the tables remain live for the request.
        #[allow(unsafe_code)]
        let entry = unsafe {
            (*entries.add(plan.function.index())).load(std::sync::atomic::Ordering::Acquire)
        };
        if entry == 0 {
            return Err("generated destructor entry is unpublished".to_owned());
        }
        #[allow(unsafe_code)]
        let trace_metadata = unsafe { (*contracts.add(plan.function.index())).trace_metadata };
        context
            .destroyed_objects
            .insert(object.id(), object.weak_handle());
        action = php_jit::JitNativeDestructorAction {
            entry: entry as u64,
            runtime_view,
            trace_metadata,
            function_id: plan.function.raw(),
            reserved: 0,
        };
        write_action(output, action)
    });
    match result {
        Some(Ok(())) => 0,
        Some(Err(error)) => {
            // SAFETY: this exact leaf runs synchronously under the request.
            #[allow(unsafe_code)]
            let context = unsafe { active_baseline_cold_context() };
            record_native_helper_failure(context, error);
            php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
        }
        None => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
    }
}

pub(in crate::vm) extern "C" fn jit_native_object_release_finalize_abi(
    runtime: *mut NativeRequestFastState,
    receiver: i64,
    output: *mut php_jit::JitNativeReleaseChildren,
) -> i32 {
    let result = with_baseline_native_context_for(runtime, "object_release_finalize", |context| {
        let children = context.finalize_generated_object_release(receiver)?;
        write_children(output, children)
    });
    match result {
        Some(Ok(())) => 0,
        Some(Err(error)) => {
            // SAFETY: this exact leaf runs synchronously under the request.
            #[allow(unsafe_code)]
            let context = unsafe { active_baseline_cold_context() };
            record_native_helper_failure(context, error);
            php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
        }
        None => php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32,
    }
}

pub(in crate::vm) extern "C" fn jit_native_object_release_children_drop_abi(
    _runtime: *mut NativeRequestFastState,
    token: u64,
) -> i32 {
    if token != 0 {
        // SAFETY: finalize transfers exactly one Box<Vec<i64>> token and the
        // generated release spine calls this leaf exactly once.
        #[allow(unsafe_code)]
        unsafe {
            drop(Box::from_raw(token as usize as *mut Vec<i64>));
        }
    }
    0
}
