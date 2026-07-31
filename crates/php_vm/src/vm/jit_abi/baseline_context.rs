//! Baseline-only activation and cold-context reentry.
//!
//! Generated optimizing code receives only `NativeRequestFastState`.
//! This module owns the synchronous TLS bridge used by explicit baseline
//! continuations, diagnostics, and outer request activation.

use super::*;
use std::cell::Cell;

#[derive(Clone, Copy)]
pub(super) struct ActiveBaselineContext {
    pub(super) cold: *mut std::ffi::c_void,
}

impl ActiveBaselineContext {
    const EMPTY: Self = Self {
        cold: std::ptr::null_mut(),
    };
}

thread_local! {
    /// Baseline/cold execution is synchronous with one native activation.
    ///
    /// Keeping this pointer in activation state rather than adjacent to
    /// `NativeRequestFastState` prevents an exact handler from recovering the
    /// Rust coordinator by casting its ordinary runtime capability.
    pub(super) static ACTIVE_BASELINE_CONTEXT: Cell<ActiveBaselineContext> =
        const { Cell::new(ActiveBaselineContext::EMPTY) };
}

pub(in crate::vm) struct NativeRequestActivationGuard {
    pub(super) _runtime_view: php_jit::JitNativeRuntimeViewGuard,
    pub(super) fast_state: *mut NativeRequestFastState,
    pub(super) previous_header: php_jit::JitNativeFastStateHeader,
    pub(super) previous_execution_scope: *const NativeExecutionScope,
    pub(super) previous_baseline_context: ActiveBaselineContext,
}

impl Drop for NativeRequestActivationGuard {
    fn drop(&mut self) {
        // SAFETY: the request owner keeps the separately allocated fast state
        // stable for the complete synchronous activation. Nested unit
        // activations overwrite only this request-owned header and unwind in
        // strict stack order, so restoring the captured header returns direct
        // callees to the outer unit's dense publication tables.
        #[allow(unsafe_code)]
        unsafe {
            (*self.fast_state).header = self.previous_header;
            (*self.fast_state).execution_scope = self.previous_execution_scope;
        }
        ACTIVE_BASELINE_CONTEXT.with(|active| {
            active.set(self.previous_baseline_context);
        });
    }
}

pub(in crate::vm) fn activate_native_context(
    context: &mut NativeRequestColdState<'_>,
) -> NativeRequestActivationGuard {
    let deployment = context.compiled.prepared_deployment_image();
    let (trusted_linked_functions, trusted_linked_function_count) = context
        .current_dynamic_unit
        .and_then(|unit| context.dynamic_units.get(unit))
        .map_or((0, 0), |package| {
            (
                package.linked_functions.as_ptr() as usize as u64,
                u32::try_from(package.linked_functions.len()).unwrap_or(u32::MAX),
            )
        });
    let (trusted_literal_slots, trusted_literal_slot_count) = context
        .trusted_literal_slots
        .get(&context.unit_identity)
        .map_or((0, 0), |slots| {
            (
                slots.as_ptr() as usize as u64,
                u32::try_from(slots.len()).unwrap_or(u32::MAX),
            )
        });
    let (active_call_arguments, active_call_argument_count, active_call_fixed_argument_count) =
        context.call_frames.last().map_or((0, 0, 0), |frame| {
            (
                frame.arguments.as_ptr() as usize as u64,
                u32::try_from(frame.arguments.len()).unwrap_or(u32::MAX),
                frame.fixed_argument_count,
            )
        });
    let view = php_jit::JitNativeRuntimeView {
        abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
        reserved: 0,
        direct_value_slots: context.direct_value_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(context.direct_value_next.as_mut()) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(context.direct_value_free_head.as_mut()) as usize
            as u64,
        direct_value_reused_bytes: std::ptr::from_mut(context.direct_value_reused_bytes.as_mut())
            as usize as u64,
        direct_object_owners: context.direct_object_owners.as_mut_ptr() as usize as u64,
        direct_array_states: context.direct_array_states.as_mut_ptr() as usize as u64,
        direct_array_entries: context.direct_array_entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(context.direct_array_next.as_mut()) as usize as u64,
        direct_array_free_heads: context.direct_array_free_heads.as_mut_ptr() as usize as u64,
        direct_array_reused_bytes: std::ptr::from_mut(context.direct_array_reused_bytes.as_mut())
            as usize as u64,
        direct_string_bytes: context.direct_string_bytes.as_mut_ptr() as usize as u64,
        direct_string_next: std::ptr::from_mut(context.direct_string_next.as_mut()) as usize as u64,
        direct_string_free_heads: context.direct_string_free_heads.as_mut_ptr() as usize as u64,
        direct_string_reused_bytes: std::ptr::from_mut(context.direct_string_reused_bytes.as_mut())
            as usize as u64,
        active_call_arguments,
        active_call_argument_count,
        active_call_fixed_argument_count,
        active_call_fixed_arguments: 0,
        active_call_tail_arguments: 0,
        trusted_globals_proxy: context.trusted_globals_proxy,
        trusted_request_local_function_offsets: context
            .trusted_request_local_function_offsets
            .as_ptr() as usize as u64,
        trusted_request_local_function_count: u32::try_from(
            context.trusted_request_local_function_offsets.len(),
        )
        .unwrap_or(u32::MAX),
        trusted_request_local_reserved: 0,
        trusted_request_local_slots: context.trusted_request_local_slots.as_ptr() as usize as u64,
        trusted_request_local_slot_count: u32::try_from(context.trusted_request_local_slots.len())
            .unwrap_or(u32::MAX),
        trusted_request_local_slot_reserved: 0,
        trusted_constant_views: deployment.constant_views.as_ptr() as usize as u64,
        trusted_constant_view_count: u32::try_from(deployment.constant_views.len())
            .unwrap_or(u32::MAX),
        trusted_constant_view_reserved: 0,
        trusted_literal_slots,
        trusted_literal_slot_count,
        trusted_literal_slot_reserved: 0,
        trusted_constant_slots: context.trusted_constant_slots.as_mut_ptr() as usize as u64,
        trusted_constant_slot_count: u32::try_from(context.trusted_constant_slots.len())
            .unwrap_or(u32::MAX),
        trusted_constant_slot_reserved: 0,
        trusted_class_plans: context.trusted_class_plans.as_ptr() as usize as u64,
        trusted_class_plan_count: u32::try_from(context.trusted_class_plans.len())
            .unwrap_or(u32::MAX),
        trusted_class_plan_reserved: 0,
        trusted_function_entries: deployment.native_function_entries.as_ptr() as usize as u64,
        trusted_function_entry_count: u32::try_from(deployment.native_function_entries.len())
            .unwrap_or(u32::MAX),
        trusted_function_entry_reserved: 0,
        trusted_preferred_function_entries: deployment.preferred_function_entries.as_ptr() as usize
            as u64,
        trusted_preferred_function_entry_count: u32::try_from(
            deployment.preferred_function_entries.len(),
        )
        .unwrap_or(u32::MAX),
        trusted_preferred_function_entry_reserved: 0,
        baseline_function_entry_counts: deployment.baseline_function_entry_counts.as_ptr() as usize
            as u64,
        baseline_function_entry_count: u32::try_from(
            deployment.baseline_function_entry_counts.len(),
        )
        .unwrap_or(u32::MAX),
        baseline_function_entry_reserved: 0,
        trusted_linked_functions,
        trusted_linked_function_count,
        trusted_linked_function_reserved: 0,
        fiber_suspension_states: context.fiber_suspension_states.as_mut_ptr() as usize as u64,
        fiber_suspension_next: std::ptr::from_mut(context.fiber_suspension_next.as_mut()) as usize
            as u64,
        fiber_suspension_capacity: u32::try_from(context.fiber_suspension_states.capacity())
            .unwrap_or(u32::MAX),
        fiber_execution_scope: context.current_native_execution_scope,
        poll_counter: std::ptr::from_mut(context.native_poll_counter.as_mut()) as usize as u64,
        root_mutation_pending: std::ptr::from_mut(context.native_root_mutation_pending.as_mut())
            as usize as u64,
        trusted_property_function_offsets: context.trusted_property_function_offsets.as_ptr()
            as usize as u64,
        trusted_property_function_count: u32::try_from(
            context.trusted_property_function_offsets.len(),
        )
        .unwrap_or(u32::MAX),
        trusted_property_reserved: 0,
        trusted_property_slots: context.trusted_property_slots.as_mut_ptr() as usize as u64,
        trusted_property_slot_count: u32::try_from(context.trusted_property_slots.len())
            .unwrap_or(u32::MAX),
        trusted_property_slot_reserved: 0,
        trusted_closure_plans: context.trusted_closure_plans.as_ptr() as usize as u64,
        trusted_closure_plan_count: u32::try_from(context.trusted_closure_plans.len())
            .unwrap_or(u32::MAX),
        trusted_closure_plan_reserved: 0,
        trusted_exception_plans: context.trusted_exception_plans.as_ptr() as usize as u64,
        trusted_exception_plan_count: u32::try_from(context.trusted_exception_plans.len())
            .unwrap_or(u32::MAX),
        trusted_exception_plan_reserved: 0,
        trusted_global_reference_slots: context.trusted_global_reference_slots.as_ptr() as usize
            as u64,
        trusted_global_reference_slot_count: u32::try_from(
            context.trusted_global_reference_slots.len(),
        )
        .unwrap_or(u32::MAX),
        trusted_global_reference_slot_reserved: 0,
        trusted_static_local_slots: context.trusted_static_local_slots.as_ptr() as usize as u64,
        trusted_static_local_slot_count: u32::try_from(context.trusted_static_local_slots.len())
            .unwrap_or(u32::MAX),
        trusted_static_local_slot_reserved: 0,
        static_property_slots: context.static_property_slots.as_mut_ptr() as usize as u64,
        static_property_slot_count: *context.static_property_next,
        static_property_slot_reserved: 0,
        trusted_static_property_slots: context.trusted_static_property_slots.as_mut_ptr() as usize
            as u64,
        trusted_static_property_slot_count: u32::try_from(
            context.trusted_static_property_slots.len(),
        )
        .unwrap_or(u32::MAX),
        trusted_static_property_slot_reserved: 0,
        trusted_instanceof_plans: context.trusted_instanceof_plans.as_ptr() as usize as u64,
        trusted_instanceof_plan_count: u32::try_from(context.trusted_instanceof_plans.len())
            .unwrap_or(u32::MAX),
        trusted_instanceof_plan_reserved: 0,
        trusted_instanceof_entries: context.trusted_instanceof_entries.as_ptr() as usize as u64,
        trusted_instanceof_entry_count: u32::try_from(context.trusted_instanceof_entries.len())
            .unwrap_or(u32::MAX),
        trusted_instanceof_entry_reserved: 0,
        trusted_exception_route_plans: context.trusted_exception_route_plans.as_ptr() as usize
            as u64,
        trusted_exception_route_plan_count: u32::try_from(
            context.trusted_exception_route_plans.len(),
        )
        .unwrap_or(u32::MAX),
        trusted_exception_route_plan_reserved: 0,
        trusted_exception_route_entries: context.trusted_exception_route_entries.as_ptr() as usize
            as u64,
        trusted_exception_route_entry_count: u32::try_from(
            context.trusted_exception_route_entries.len(),
        )
        .unwrap_or(u32::MAX),
        trusted_exception_route_entry_reserved: 0,
        error_reporting: std::ptr::from_mut(&mut context.error_reporting) as usize as u64,
    };
    let newly_published_unit = context.current_dynamic_unit.and_then(|unit| {
        let package = context.dynamic_units.get_mut(unit)?;
        let newly_published =
            package.published_runtime_view.abi_version != php_jit::JIT_RUNTIME_ABI_VERSION;
        *package.published_runtime_view = view;
        newly_published.then_some(unit)
    });
    if let Some(unit) = newly_published_unit {
        cold_dynamic_units::refresh_linked_function_records_for_unit(context, unit);
    }
    let execution_scope = usize::try_from(context.current_native_execution_scope)
        .ok()
        .and_then(|identity| identity.checked_sub(1))
        .and_then(|index| context.native_execution_scopes.get(index))
        .map_or(std::ptr::null(), |scope| std::ptr::from_ref(scope.as_ref()));
    // SAFETY: `NativeRequestOwner` allocates the fast state separately and
    // wires this stable pointer before exposing the cold state.
    let fast_state = context.fast_state;
    let previous_header;
    let previous_execution_scope;
    #[allow(unsafe_code)]
    unsafe {
        previous_header = (*fast_state).header;
        previous_execution_scope = (*fast_state).execution_scope;
        (*fast_state).header = php_jit::JitNativeFastStateHeader {
            abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
            flags: 0,
            runtime_view_pointer: 0,
            runtime_view: view,
        };
        (*context.fast_state).output = std::ptr::from_mut(&mut context.output);
        (*context.fast_state).json_state =
            std::ptr::from_mut(context.builtin_request_state.json_mut());
        (*context.fast_state).pcre_state =
            std::ptr::from_mut(context.builtin_request_state.pcre_mut());
        (*context.fast_state).configuration = NativeConfigurationCapability::published(context);
        (*context.fast_state).http_response = NativeHttpResponseCapability::published(context);
        (*context.fast_state).cwd = std::ptr::from_mut(&mut context.cwd);
        (*context.fast_state).filesystem_capabilities =
            std::ptr::from_ref(&context.options.runtime_context.filesystem);
        (*context.fast_state).stdin = std::ptr::from_ref(&context.options.runtime_context.stdin);
        (*context.fast_state).resources = std::ptr::from_mut(&mut context.resources);
        (*context.fast_state).upload_registry = std::ptr::from_mut(&mut context.upload_registry);
        (*context.fast_state).direct_resource_handles =
            std::ptr::from_mut(&mut context.direct_resource_handles);
        (*context.fast_state).direct_closure_handles =
            std::ptr::from_mut(&mut context.direct_closure_handles);
        (*context.fast_state).execution_scope = execution_scope;
        (*context.fast_state).request_query = NativeRequestQueryCapability::published(context);
        (*context.fast_state).execution_deadline =
            NativeExecutionDeadlineCapability::published(context);
        (*context.fast_state).frame_arena = NativeFrameArenaCapability::published(context);
    }
    let runtime_view = php_jit::activate_native_runtime_view(view);
    let baseline_context = ActiveBaselineContext {
        cold: std::ptr::from_mut(&mut *context).cast(),
    };
    let previous_baseline_context =
        ACTIVE_BASELINE_CONTEXT.with(|active| active.replace(baseline_context));
    NativeRequestActivationGuard {
        _runtime_view: runtime_view,
        fast_state,
        previous_header,
        previous_execution_scope,
        previous_baseline_context,
    }
}

#[allow(unsafe_code)]
pub(super) fn with_baseline_native_context_for<R>(
    runtime: *mut NativeRequestFastState,
    helper_id: &'static str,
    operation: impl FnOnce(&mut NativeRequestColdState<'_>) -> R,
) -> Option<R> {
    // SAFETY: native activation publishes the synchronous baseline
    // coordinator independently of the fast-state capability.
    let context = unsafe { active_baseline_cold_context() };
    let runtime_state = (unsafe { runtime.as_ref() })?;
    let requested_entries = runtime_state
        .header
        .active_runtime_view()
        .trusted_function_entries;
    let active_entries = context
        .compiled
        .prepared_deployment_image()
        .native_function_entries
        .as_ptr() as usize as u64;
    if requested_entries == active_entries {
        let result = operation(context);
        if let Err(error) = context.restore_materialized_direct_references() {
            cold_diagnostics::record_native_helper_failure(
                context,
                format!("native {helper_id} reference restoration failed: {error}"),
            );
            return None;
        }
        return Some(result);
    }
    let Some(target_unit) = context
        .dynamic_units
        .iter()
        .enumerate()
        .find_map(|(unit, package)| {
            let entries = package
                .compiled
                .prepared_deployment_image()
                .native_function_entries
                .as_ptr() as usize as u64;
            (entries == requested_entries).then_some(unit)
        })
    else {
        cold_diagnostics::record_native_helper_failure(
            context,
            format!(
                "native {helper_id} runtime view has unknown function entries {requested_entries:#x} (active={active_entries:#x})"
            ),
        );
        return None;
    };
    let mut operation = Some(operation);
    match context.with_active_dynamic_unit(target_unit, None, |context| {
        let operation = operation
            .take()
            .expect("native cold crossing operation was already consumed");
        let result = operation(context);
        if let Err(error) = context.restore_materialized_direct_references() {
            cold_diagnostics::record_native_helper_failure(
                context,
                format!("native {helper_id} reference restoration failed: {error}"),
            );
            return None;
        }
        Some(result)
    }) {
        Ok(Some(result)) => Some(result),
        Ok(None) => None,
        Err(error) => {
            cold_diagnostics::record_native_helper_failure(
                context,
                format!("native {helper_id} runtime view activation failed: {error}"),
            );
            None
        }
    }
}

/// Enter the cold state owned by one exact compiled artifact.
///
/// Linked native calls switch the published runtime view without moving the
/// complete Rust-owned unit state. Baseline semantic continuations carry the
/// immutable artifact identity emitted with their callsite, so route that one
/// cold transition to the matching unit before resolving continuation-local
/// metadata. This is unit selection in the baseline-native tier, not a
/// per-operation artifact validation path.
#[allow(unsafe_code)]
pub(super) fn with_baseline_native_context_for_unit<R>(
    unit_identity: u64,
    operation: impl FnOnce(&mut NativeRequestColdState<'_>) -> R,
) -> Option<R> {
    // SAFETY: native activation publishes the synchronous baseline
    // coordinator independently of the fast-state capability.
    let context = unsafe { active_baseline_cold_context() };
    if context.unit_identity == unit_identity {
        let result = operation(context);
        if let Err(error) = context.restore_materialized_direct_references() {
            cold_diagnostics::record_native_helper_failure(
                context,
                format!("native unit {unit_identity:#x} reference restoration failed: {error}"),
            );
            return None;
        }
        return Some(result);
    }
    let target_unit = context
        .dynamic_units
        .iter()
        .position(|package| package.compiled.artifact_identity() == unit_identity)?;
    let mut operation = Some(operation);
    context
        .with_active_dynamic_unit(target_unit, None, |context| {
            let operation = operation
                .take()
                .expect("native cold unit operation was already consumed");
            let result = operation(context);
            if let Err(error) = context.restore_materialized_direct_references() {
                cold_diagnostics::record_native_helper_failure(
                    context,
                    format!("native unit {unit_identity:#x} reference restoration failed: {error}"),
                );
                return None;
            }
            Some(result)
        })
        .ok()
        .flatten()
}

/// Enters the cold Rust semantic coordinator from a baseline or diagnostic
/// ABI. Exact handlers receive `NativeRequestFastState` and cannot name this
/// owner-only field.
#[allow(unsafe_code)]
pub(super) unsafe fn active_baseline_cold_context<'a>() -> &'a mut NativeRequestColdState<'a> {
    // SAFETY: every baseline/diagnostic ABI is synchronous with one native
    // activation. Nested activations replace and restore this exact pointer in
    // stack order; exact handlers receive only the independent fast state.
    let cold = ACTIVE_BASELINE_CONTEXT.with(Cell::get).cold;
    debug_assert!(
        !cold.is_null(),
        "baseline ABI entered without an active cold context"
    );
    unsafe { &mut *cold.cast::<NativeRequestColdState<'a>>() }
}
