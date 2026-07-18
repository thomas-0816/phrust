use super::NativeExecutionContext;

const HELPER_OTHER: usize = 39;
const HELPER_NAMES: [&str; 40] = [
    "execution_poll",
    "unary",
    "binary",
    "compare",
    "cast",
    "echo",
    "local_fetch",
    "local_store",
    "value_retain",
    "value_release",
    "reference_bind",
    "return_check",
    "exception_new",
    "array_new",
    "array_insert",
    "array_fetch",
    "array_unset",
    "array_spread",
    "object_new",
    "property_fetch",
    "property_assign",
    "object_clone",
    "object_clone_with",
    "foreach_init",
    "foreach_next",
    "foreach_cleanup",
    "constant_fetch",
    "truthy",
    "type_predicate",
    "stable_length",
    "runtime_fatal",
    "dynamic_code",
    "call_function",
    "call_method",
    "call_static_method",
    "call_callable",
    "call_constructor",
    "call_runtime_intrinsic",
    "native_transition",
    "other",
];
const LOCAL_REASONS: [&str; 8] = [
    "plain_initialized_local",
    "uninitialized_warning",
    "reference_dereference",
    "top_level_global",
    "superglobal",
    "GLOBALS",
    "synthetic_compiler_local",
    "unknown",
];
const VALUE_CLASSES: [&str; 14] = [
    "uninitialized",
    "null",
    "bool",
    "int",
    "float",
    "string",
    "array",
    "object",
    "reference",
    "callable",
    "resource",
    "generator",
    "fiber",
    "mixed",
];
const LIFECYCLE_REASONS: [&str; 9] = [
    "copy",
    "store",
    "argument",
    "return",
    "temporary",
    "branch_merge",
    "frame_cleanup",
    "exception_cleanup",
    "helper_result",
];
const ROOT_REASONS: [&str; 10] = [
    "global_or_static",
    "session",
    "callback_or_handler",
    "pending_throwable",
    "enum_or_static_object",
    "native_frame",
    "suspension",
    "resource_owned",
    "rooted_container",
    "call_arguments",
];
const IR_OPERATIONS: [&str; 42] = [
    "unary_plus",
    "unary_minus",
    "unary_not",
    "unary_bit_not",
    "binary_add",
    "binary_sub",
    "binary_mul",
    "binary_div",
    "binary_mod",
    "binary_concat",
    "binary_pow",
    "binary_bit_and",
    "binary_bit_or",
    "binary_bit_xor",
    "binary_shift_left",
    "binary_shift_right",
    "compare_equal",
    "compare_not_equal",
    "compare_identical",
    "compare_not_identical",
    "compare_less",
    "compare_less_equal",
    "compare_greater",
    "compare_greater_equal",
    "compare_spaceship",
    "cast_bool",
    "cast_int",
    "cast_float",
    "cast_string",
    "cast_array",
    "cast_object",
    "cast_void",
    "load_local",
    "store_local",
    "value_lifecycle",
    "truthy",
    "reference_bind_value",
    "reference_bind_dimension",
    "reference_bind_static_local",
    "reference_bind_property",
    "reference_publish_local",
    "reference_bind_static_property",
];
const SLOW_PATH_REASONS: [&str; 8] = [
    "numeric_or_coercion",
    "comparison_or_magic",
    "cast_or_magic",
    "uninitialized_or_observable_local",
    "reference_or_global_store",
    "unknown_truthiness",
    "ownership_boundary",
    "other_runtime_semantics",
];

fn helper_index(helper_id: &str) -> usize {
    HELPER_NAMES
        .iter()
        .position(|name| *name == helper_id)
        .unwrap_or(HELPER_OTHER)
}

pub(super) struct NativeRuntimeTelemetry {
    pub(super) counters: crate::counters::VmCounters,
    pub(super) helper_timing_stack: Vec<NativeHelperTimingFrame>,
    helper_calls: [u64; HELPER_NAMES.len()],
    helper_time_nanos: [u64; HELPER_NAMES.len()],
    local_reads: [u64; LOCAL_REASONS.len()],
    local_stores: [u64; LOCAL_REASONS.len()],
    reference_read_classes: [u64; VALUE_CLASSES.len()],
    truthy_classes: [u64; VALUE_CLASSES.len()],
    retains: [u64; LIFECYCLE_REASONS.len()],
    releases: [u64; LIFECYCLE_REASONS.len()],
    root_rebuilds: [u64; ROOT_REASONS.len()],
    operation_calls: [u64; IR_OPERATIONS.len()],
    operation_time_nanos: [u64; IR_OPERATIONS.len()],
    function_calls: Vec<u64>,
    function_time_nanos: Vec<u64>,
    slow_paths: [u64; SLOW_PATH_REASONS.len()],
}

impl Default for NativeRuntimeTelemetry {
    fn default() -> Self {
        Self {
            counters: crate::counters::VmCounters::default(),
            helper_timing_stack: Vec::new(),
            helper_calls: [0; HELPER_NAMES.len()],
            helper_time_nanos: [0; HELPER_NAMES.len()],
            local_reads: [0; LOCAL_REASONS.len()],
            local_stores: [0; LOCAL_REASONS.len()],
            reference_read_classes: [0; VALUE_CLASSES.len()],
            truthy_classes: [0; VALUE_CLASSES.len()],
            retains: [0; LIFECYCLE_REASONS.len()],
            releases: [0; LIFECYCLE_REASONS.len()],
            root_rebuilds: [0; ROOT_REASONS.len()],
            operation_calls: [0; IR_OPERATIONS.len()],
            operation_time_nanos: [0; IR_OPERATIONS.len()],
            function_calls: Vec::new(),
            function_time_nanos: Vec::new(),
            slow_paths: [0; SLOW_PATH_REASONS.len()],
        }
    }
}

pub(super) struct NativeHelperTimingFrame {
    helper_index: usize,
    started_at: std::time::Instant,
    pub(super) child_time_nanos: u64,
    operation_index: Option<usize>,
    function_index: Option<usize>,
}

impl NativeRuntimeTelemetry {
    fn enter_helper(&mut self, helper_id: &'static str) {
        self.counters.runtime_helper_calls = self.counters.runtime_helper_calls.saturating_add(1);
        if matches!(
            helper_id,
            "call_function"
                | "call_method"
                | "call_static_method"
                | "call_callable"
                | "call_constructor"
                | "native_transition"
        ) {
            self.counters.native_ownership_escapes =
                self.counters.native_ownership_escapes.saturating_add(1);
        }
        let helper_index = helper_index(helper_id);
        self.helper_calls[helper_index] = self.helper_calls[helper_index].saturating_add(1);
        self.helper_timing_stack.push(NativeHelperTimingFrame {
            helper_index,
            started_at: std::time::Instant::now(),
            child_time_nanos: 0,
            operation_index: None,
            function_index: None,
        });
    }

    fn exit_helper(&mut self, helper_id: &'static str) {
        let Some(frame) = self.helper_timing_stack.pop() else {
            return;
        };
        let helper_index = helper_index(helper_id);
        debug_assert_eq!(frame.helper_index, helper_index);
        let elapsed = frame
            .started_at
            .elapsed()
            .as_nanos()
            .min(u128::from(u64::MAX)) as u64;
        let exclusive = elapsed.saturating_sub(frame.child_time_nanos);
        self.counters.runtime_helper_time_nanos = self
            .counters
            .runtime_helper_time_nanos
            .saturating_add(exclusive);
        self.helper_time_nanos[helper_index] =
            self.helper_time_nanos[helper_index].saturating_add(exclusive);
        if let Some(index) = frame.operation_index {
            self.operation_time_nanos[index] =
                self.operation_time_nanos[index].saturating_add(exclusive);
        }
        if let Some(index) = frame.function_index {
            if self.function_time_nanos.len() <= index {
                self.function_time_nanos.resize(index + 1, 0);
            }
            self.function_time_nanos[index] =
                self.function_time_nanos[index].saturating_add(exclusive);
        }
        let inclusive_time = self
            .counters
            .runtime_helper_inclusive_time_nanos_by_id
            .entry(helper_id.to_owned())
            .or_default();
        *inclusive_time = inclusive_time.saturating_add(elapsed);
        if let Some(parent) = self.helper_timing_stack.last_mut() {
            parent.child_time_nanos = parent.child_time_nanos.saturating_add(elapsed);
        }
    }
}

impl NativeExecutionContext<'_> {
    pub(in crate::vm) fn runtime_counters(&self) -> crate::counters::VmCounters {
        let telemetry = self.runtime_telemetry.borrow();
        let mut counters = telemetry.counters.clone();
        counters.runtime_helper_calls_by_id = HELPER_NAMES
            .iter()
            .zip(telemetry.helper_calls)
            .filter(|(_, count)| *count != 0)
            .map(|(name, count)| ((*name).to_owned(), count))
            .collect();
        counters.runtime_helper_time_nanos_by_id = HELPER_NAMES
            .iter()
            .zip(telemetry.helper_time_nanos)
            .filter(|(_, time)| *time != 0)
            .map(|(name, time)| ((*name).to_owned(), time))
            .collect();
        counters.runtime_helper_calls_by_ir_operation =
            named_counters(&IR_OPERATIONS, telemetry.operation_calls);
        counters.runtime_helper_time_nanos_by_ir_operation =
            named_counters(&IR_OPERATIONS, telemetry.operation_time_nanos);
        counters.runtime_helper_calls_by_function =
            self.named_function_counters(&telemetry.function_calls);
        counters.runtime_helper_time_nanos_by_function =
            self.named_function_counters(&telemetry.function_time_nanos);
        counters.runtime_helper_local_read_by_reason =
            named_counters(&LOCAL_REASONS, telemetry.local_reads);
        counters.runtime_helper_local_store_by_reason =
            named_counters(&LOCAL_REASONS, telemetry.local_stores);
        counters.runtime_helper_reference_read_by_value_class =
            named_counters(&VALUE_CLASSES, telemetry.reference_read_classes);
        counters.runtime_helper_truthy_by_value_class =
            named_counters(&VALUE_CLASSES, telemetry.truthy_classes);
        counters.runtime_helper_retain_by_reason =
            named_counters(&LIFECYCLE_REASONS, telemetry.retains);
        counters.runtime_helper_release_by_reason =
            named_counters(&LIFECYCLE_REASONS, telemetry.releases);
        counters.runtime_helper_object_release_root_scans_by_reason =
            named_counters(&ROOT_REASONS, telemetry.root_rebuilds);
        counters
            .runtime_helper_object_release_root_scans_by_reason
            .insert(
                "request_membership_traversal".to_owned(),
                self.root_index.membership_traversals(),
            );
        let (call_traversals, call_cache_hits, call_cache_misses) =
            self.call_root_index.telemetry();
        counters
            .runtime_helper_object_release_root_scans_by_reason
            .insert("call_membership_traversal".to_owned(), call_traversals);
        counters
            .runtime_helper_object_release_root_scans_by_reason
            .insert("call_membership_cache_hit".to_owned(), call_cache_hits);
        counters
            .runtime_helper_object_release_root_scans_by_reason
            .insert("call_membership_cache_miss".to_owned(), call_cache_misses);
        counters.native_slow_path_entries_by_reason =
            named_counters(&SLOW_PATH_REASONS, telemetry.slow_paths);
        let mut seen_regions = std::collections::BTreeSet::new();
        for handle in self.native_entries.values().chain(
            self.dynamic_units
                .iter()
                .flat_map(|unit| unit.native_entries.values()),
        ) {
            if !seen_regions.insert(handle.region_id.clone()) {
                continue;
            }
            let (locals, registers, moves) = handle.ssa_metrics();
            counters.native_ssa_promoted_locals =
                counters.native_ssa_promoted_locals.saturating_add(locals);
            counters.native_ssa_promoted_registers = counters
                .native_ssa_promoted_registers
                .saturating_add(registers);
            counters.native_ownership_moves = counters.native_ownership_moves.saturating_add(moves);
        }
        counters.native_frame_arena_capacity_bytes =
            self.native_frame_arena.capacity_bytes() as u64;
        counters.native_frame_arena_high_water_bytes =
            self.native_frame_arena.high_water_bytes() as u64;
        if self.options.collect_counters {
            let (stack_virtual, stack_committed) = current_php_worker_stack_bytes();
            counters.native_worker_stack_virtual_bytes = stack_virtual;
            counters.native_worker_stack_committed_bytes = stack_committed;
        }
        counters
    }

    pub(super) fn enter_runtime_helper(&self, helper_id: &'static str) {
        self.runtime_telemetry.borrow_mut().enter_helper(helper_id);
    }

    pub(super) fn exit_runtime_helper(&self, helper_id: &'static str) {
        self.runtime_telemetry.borrow_mut().exit_helper(helper_id);
    }

    pub(super) fn attribute_active_helper(&self, operation: &'static str, function: Option<u32>) {
        if !self.options.collect_counters {
            return;
        }
        let Some(operation_index) = IR_OPERATIONS.iter().position(|name| *name == operation) else {
            return;
        };
        let mut telemetry = self.runtime_telemetry.borrow_mut();
        telemetry.operation_calls[operation_index] =
            telemetry.operation_calls[operation_index].saturating_add(1);
        let slow_reason = if operation.starts_with("binary_") || operation.starts_with("unary_") {
            "numeric_or_coercion"
        } else if operation.starts_with("compare_") {
            "comparison_or_magic"
        } else if operation.starts_with("cast_") {
            "cast_or_magic"
        } else if operation == "load_local" {
            "uninitialized_or_observable_local"
        } else if operation == "store_local" {
            "reference_or_global_store"
        } else if operation == "truthy" {
            "unknown_truthiness"
        } else if operation == "value_lifecycle" {
            "ownership_boundary"
        } else {
            "other_runtime_semantics"
        };
        record_scratch(&mut telemetry.slow_paths, &SLOW_PATH_REASONS, slow_reason);
        let function_index = function.map(|function| function as usize);
        if let Some(index) = function_index {
            if telemetry.function_calls.len() <= index {
                telemetry.function_calls.resize(index + 1, 0);
            }
            telemetry.function_calls[index] = telemetry.function_calls[index].saturating_add(1);
        }
        if let Some(frame) = telemetry.helper_timing_stack.last_mut() {
            frame.operation_index = Some(operation_index);
            frame.function_index = function_index;
        }
    }

    fn named_function_counters(&self, values: &[u64]) -> std::collections::BTreeMap<String, u64> {
        values
            .iter()
            .enumerate()
            .filter(|(_, value)| **value != 0)
            .map(|(index, value)| {
                let name = self
                    .unit
                    .functions
                    .get(index)
                    .map_or("<unknown>", |function| function.name.as_str());
                (format!("{index}:{name}"), *value)
            })
            .collect()
    }

    pub(in crate::vm) fn record_native_direct_calls(&self, handle: &php_jit::JitFunctionHandle) {
        if !self.options.collect_counters {
            return;
        }
        let calls = handle.compiled_to_compiled_calls_per_invocation();
        let method_calls = handle.compiled_method_calls_per_invocation();
        let inlined_calls = handle.inlined_calls_per_invocation();
        let stable_calls = calls.saturating_add(inlined_calls);
        let mut telemetry = self.runtime_telemetry.borrow_mut();
        if let Some(metadata) = handle.region_state_metadata() {
            let mut unit_code_bytes = 0_u64;
            for entry in &metadata.function_entries {
                let key = format!("{}:{}", self.unit_identity, entry.function.raw());
                telemetry
                    .counters
                    .native_code_bytes_by_function
                    .insert(key.clone(), entry.code_bytes);
                telemetry
                    .counters
                    .native_stack_bytes_by_function
                    .insert(key, u64::from(entry.native_stack_bytes));
                unit_code_bytes = unit_code_bytes.saturating_add(entry.code_bytes);
            }
            telemetry
                .counters
                .native_code_bytes_by_unit
                .insert(self.unit_identity.to_string(), unit_code_bytes);
        }
        telemetry.counters.native_call_direct = telemetry
            .counters
            .native_call_direct
            .saturating_add(stable_calls);
        telemetry.counters.native_callsite_total = telemetry
            .counters
            .native_callsite_total
            .saturating_add(stable_calls);
        telemetry.counters.native_same_unit_direct_eligible = telemetry
            .counters
            .native_same_unit_direct_eligible
            .saturating_add(stable_calls);
        telemetry.counters.native_same_unit_direct_executed = telemetry
            .counters
            .native_same_unit_direct_executed
            .saturating_add(stable_calls);
        telemetry.counters.native_method_monomorphic_eligible = telemetry
            .counters
            .native_method_monomorphic_eligible
            .saturating_add(method_calls);
        telemetry.counters.native_method_monomorphic_executed = telemetry
            .counters
            .native_method_monomorphic_executed
            .saturating_add(method_calls);
        telemetry.counters.native_inlined_calls = telemetry
            .counters
            .native_inlined_calls
            .saturating_add(inlined_calls);
        telemetry.counters.native_inline_calls_removed = telemetry
            .counters
            .native_inline_calls_removed
            .saturating_add(inlined_calls);
        telemetry.counters.native_inline_bytes_added = telemetry
            .counters
            .native_inline_bytes_added
            .saturating_add(handle.inline_bytes_added_per_invocation());
        telemetry.counters.native_tail_calls = telemetry
            .counters
            .native_tail_calls
            .saturating_add(handle.tail_calls_per_invocation());
        if let Some(metadata) = handle.region_state_metadata() {
            merge_counter_map(
                &mut telemetry.counters.native_inline_rejected_by_reason,
                &metadata.inline_rejected_by_reason,
            );
        }
    }

    pub(super) fn record_native_method_pic(&self, executed: bool) {
        if !self.options.collect_counters {
            return;
        }
        let mut telemetry = self.runtime_telemetry.borrow_mut();
        telemetry.counters.native_method_monomorphic_eligible = telemetry
            .counters
            .native_method_monomorphic_eligible
            .saturating_add(1);
        if !executed {
            return;
        }
        telemetry.counters.native_method_monomorphic_executed = telemetry
            .counters
            .native_method_monomorphic_executed
            .saturating_add(1);
        telemetry.counters.native_call_dynamic =
            telemetry.counters.native_call_dynamic.saturating_sub(1);
        telemetry.counters.native_call_direct =
            telemetry.counters.native_call_direct.saturating_add(1);
        if let Some(count) = telemetry
            .counters
            .native_call_dynamic_by_reason
            .get_mut("method polymorphism")
        {
            *count = count.saturating_sub(1);
        }
    }

    pub(super) fn merge_nested_runtime_counters(
        &self,
        nested: &crate::counters::VmCounters,
        nested_elapsed: std::time::Duration,
    ) {
        let mut telemetry = self.runtime_telemetry.borrow_mut();
        let counters = &mut telemetry.counters;
        counters.native_execution_entries = counters
            .native_execution_entries
            .saturating_add(nested.native_execution_entries);
        counters.native_region_entries = counters
            .native_region_entries
            .saturating_add(nested.native_region_entries);
        counters.native_region_side_exits = counters
            .native_region_side_exits
            .saturating_add(nested.native_region_side_exits);
        counters.native_call_direct = counters
            .native_call_direct
            .saturating_add(nested.native_call_direct);
        counters.native_call_dynamic = counters
            .native_call_dynamic
            .saturating_add(nested.native_call_dynamic);
        counters.native_callsite_total = counters
            .native_callsite_total
            .saturating_add(nested.native_callsite_total);
        counters.native_same_unit_direct_eligible = counters
            .native_same_unit_direct_eligible
            .saturating_add(nested.native_same_unit_direct_eligible);
        counters.native_same_unit_direct_executed = counters
            .native_same_unit_direct_executed
            .saturating_add(nested.native_same_unit_direct_executed);
        counters.native_cross_unit_direct_eligible = counters
            .native_cross_unit_direct_eligible
            .saturating_add(nested.native_cross_unit_direct_eligible);
        counters.native_cross_unit_direct_executed = counters
            .native_cross_unit_direct_executed
            .saturating_add(nested.native_cross_unit_direct_executed);
        counters.native_method_monomorphic_eligible = counters
            .native_method_monomorphic_eligible
            .saturating_add(nested.native_method_monomorphic_eligible);
        counters.native_method_monomorphic_executed = counters
            .native_method_monomorphic_executed
            .saturating_add(nested.native_method_monomorphic_executed);
        counters.native_builtin_direct_eligible = counters
            .native_builtin_direct_eligible
            .saturating_add(nested.native_builtin_direct_eligible);
        counters.native_builtin_direct_executed = counters
            .native_builtin_direct_executed
            .saturating_add(nested.native_builtin_direct_executed);
        merge_counter_map(
            &mut counters.native_builtin_calls_by_name,
            &nested.native_builtin_calls_by_name,
        );
        merge_counter_map(
            &mut counters.native_builtin_time_nanos_by_name,
            &nested.native_builtin_time_nanos_by_name,
        );
        counters.native_call_argument_allocation_bytes = counters
            .native_call_argument_allocation_bytes
            .saturating_add(nested.native_call_argument_allocation_bytes);
        counters.native_call_frame_bytes = counters
            .native_call_frame_bytes
            .saturating_add(nested.native_call_frame_bytes);
        counters.native_inlined_calls = counters
            .native_inlined_calls
            .saturating_add(nested.native_inlined_calls);
        counters.native_inline_bytes_added = counters
            .native_inline_bytes_added
            .saturating_add(nested.native_inline_bytes_added);
        counters.native_inline_calls_removed = counters
            .native_inline_calls_removed
            .saturating_add(nested.native_inline_calls_removed);
        counters.native_tail_calls = counters
            .native_tail_calls
            .saturating_add(nested.native_tail_calls);
        counters.native_transition_count = counters
            .native_transition_count
            .saturating_add(nested.native_transition_count);
        counters.native_transition_time_nanos = counters
            .native_transition_time_nanos
            .saturating_add(nested.native_transition_time_nanos);
        counters.runtime_helper_calls = counters
            .runtime_helper_calls
            .saturating_add(nested.runtime_helper_calls);
        counters.runtime_helper_time_nanos = counters
            .runtime_helper_time_nanos
            .saturating_add(nested.runtime_helper_time_nanos);
        counters.runtime_helper_object_release_fast_paths = counters
            .runtime_helper_object_release_fast_paths
            .saturating_add(nested.runtime_helper_object_release_fast_paths);
        counters.runtime_helper_object_release_root_scans = counters
            .runtime_helper_object_release_root_scans
            .saturating_add(nested.runtime_helper_object_release_root_scans);
        counters.runtime_helper_release_to_zero = counters
            .runtime_helper_release_to_zero
            .saturating_add(nested.runtime_helper_release_to_zero);
        counters.native_value_encodes = counters
            .native_value_encodes
            .saturating_add(nested.native_value_encodes);
        counters.native_value_decodes = counters
            .native_value_decodes
            .saturating_add(nested.native_value_decodes);
        counters.native_value_table_allocations = counters
            .native_value_table_allocations
            .saturating_add(nested.native_value_table_allocations);
        counters.native_value_table_reuses = counters
            .native_value_table_reuses
            .saturating_add(nested.native_value_table_reuses);
        counters.native_value_table_high_water = counters
            .native_value_table_high_water
            .max(nested.native_value_table_high_water);
        counters.native_ssa_promoted_locals = counters
            .native_ssa_promoted_locals
            .saturating_add(nested.native_ssa_promoted_locals);
        counters.native_ssa_promoted_registers = counters
            .native_ssa_promoted_registers
            .saturating_add(nested.native_ssa_promoted_registers);
        counters.native_ownership_moves = counters
            .native_ownership_moves
            .saturating_add(nested.native_ownership_moves);
        counters.native_ownership_clones = counters
            .native_ownership_clones
            .saturating_add(nested.native_ownership_clones);
        counters.native_ownership_escapes = counters
            .native_ownership_escapes
            .saturating_add(nested.native_ownership_escapes);
        counters.gc_safepoint_polls = counters
            .gc_safepoint_polls
            .saturating_add(nested.gc_safepoint_polls);
        counters.gc_safepoint_collections = counters
            .gc_safepoint_collections
            .saturating_add(nested.gc_safepoint_collections);
        merge_counter_map(
            &mut counters.native_region_side_exits_by_reason,
            &nested.native_region_side_exits_by_reason,
        );
        merge_counter_map(
            &mut counters.native_transition_by_reason,
            &nested.native_transition_by_reason,
        );
        merge_counter_map(
            &mut counters.native_call_dynamic_by_reason,
            &nested.native_call_dynamic_by_reason,
        );
        merge_counter_map(
            &mut counters.native_call_dynamic_by_target,
            &nested.native_call_dynamic_by_target,
        );
        merge_gauge_map_max(
            &mut counters.native_code_bytes_by_function,
            &nested.native_code_bytes_by_function,
        );
        merge_gauge_map_max(
            &mut counters.native_code_bytes_by_unit,
            &nested.native_code_bytes_by_unit,
        );
        merge_gauge_map_max(
            &mut counters.native_stack_bytes_by_function,
            &nested.native_stack_bytes_by_function,
        );
        merge_counter_map(
            &mut counters.native_inline_rejected_by_reason,
            &nested.native_inline_rejected_by_reason,
        );
        merge_counter_map(
            &mut counters.native_callsite_calls_by_id,
            &nested.native_callsite_calls_by_id,
        );
        merge_counter_map(
            &mut counters.native_callsite_inclusive_time_nanos_by_id,
            &nested.native_callsite_inclusive_time_nanos_by_id,
        );
        merge_counter_map(
            &mut counters.native_callsite_exclusive_time_nanos_by_id,
            &nested.native_callsite_exclusive_time_nanos_by_id,
        );
        merge_counter_map(
            &mut counters.native_transition_time_nanos_by_reason,
            &nested.native_transition_time_nanos_by_reason,
        );
        merge_counter_map(
            &mut counters.runtime_helper_inclusive_time_nanos_by_id,
            &nested.runtime_helper_inclusive_time_nanos_by_id,
        );
        for (name, value) in &nested.runtime_helper_calls_by_id {
            let index = helper_index(name);
            telemetry.helper_calls[index] = telemetry.helper_calls[index].saturating_add(*value);
        }
        for (name, value) in &nested.runtime_helper_time_nanos_by_id {
            let index = helper_index(name);
            telemetry.helper_time_nanos[index] =
                telemetry.helper_time_nanos[index].saturating_add(*value);
        }
        merge_named_scratch(
            &mut telemetry.operation_calls,
            &IR_OPERATIONS,
            &nested.runtime_helper_calls_by_ir_operation,
        );
        merge_named_scratch(
            &mut telemetry.operation_time_nanos,
            &IR_OPERATIONS,
            &nested.runtime_helper_time_nanos_by_ir_operation,
        );
        merge_function_scratch(
            &mut telemetry.function_calls,
            &nested.runtime_helper_calls_by_function,
        );
        merge_function_scratch(
            &mut telemetry.function_time_nanos,
            &nested.runtime_helper_time_nanos_by_function,
        );
        merge_named_scratch(
            &mut telemetry.local_reads,
            &LOCAL_REASONS,
            &nested.runtime_helper_local_read_by_reason,
        );
        merge_named_scratch(
            &mut telemetry.local_stores,
            &LOCAL_REASONS,
            &nested.runtime_helper_local_store_by_reason,
        );
        merge_named_scratch(
            &mut telemetry.reference_read_classes,
            &VALUE_CLASSES,
            &nested.runtime_helper_reference_read_by_value_class,
        );
        merge_named_scratch(
            &mut telemetry.truthy_classes,
            &VALUE_CLASSES,
            &nested.runtime_helper_truthy_by_value_class,
        );
        merge_named_scratch(
            &mut telemetry.retains,
            &LIFECYCLE_REASONS,
            &nested.runtime_helper_retain_by_reason,
        );
        merge_named_scratch(
            &mut telemetry.releases,
            &LIFECYCLE_REASONS,
            &nested.runtime_helper_release_by_reason,
        );
        merge_named_scratch(
            &mut telemetry.root_rebuilds,
            &ROOT_REASONS,
            &nested.runtime_helper_object_release_root_scans_by_reason,
        );
        merge_named_scratch(
            &mut telemetry.slow_paths,
            &SLOW_PATH_REASONS,
            &nested.native_slow_path_entries_by_reason,
        );
        if let Some(parent) = telemetry.helper_timing_stack.last_mut() {
            let nested_elapsed = nested_elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
            parent.child_time_nanos = parent.child_time_nanos.saturating_add(nested_elapsed);
        }
    }

    pub(super) fn active_helper_child_time_nanos(&self) -> u64 {
        self.runtime_telemetry
            .borrow()
            .helper_timing_stack
            .last()
            .map_or(0, |frame| frame.child_time_nanos)
    }

    pub(super) fn record_native_callsite_timing(
        &self,
        function: u32,
        block: u32,
        instruction: u32,
        inclusive_nanos: u64,
        child_nanos: u64,
    ) {
        let id = format!("{function}:{block}:{instruction}");
        let mut telemetry = self.runtime_telemetry.borrow_mut();
        let calls = telemetry
            .counters
            .native_callsite_calls_by_id
            .entry(id.clone())
            .or_default();
        *calls = calls.saturating_add(1);
        let inclusive = telemetry
            .counters
            .native_callsite_inclusive_time_nanos_by_id
            .entry(id.clone())
            .or_default();
        *inclusive = inclusive.saturating_add(inclusive_nanos);
        let exclusive = telemetry
            .counters
            .native_callsite_exclusive_time_nanos_by_id
            .entry(id)
            .or_default();
        *exclusive = exclusive.saturating_add(inclusive_nanos.saturating_sub(child_nanos));
    }

    pub(super) fn record_object_release_root_check(&self, fast_path: bool) {
        if !self.options.collect_counters {
            return;
        }
        let mut telemetry = self.runtime_telemetry.borrow_mut();
        if fast_path {
            telemetry.counters.runtime_helper_object_release_fast_paths = telemetry
                .counters
                .runtime_helper_object_release_fast_paths
                .saturating_add(1);
        } else {
            telemetry.counters.runtime_helper_object_release_root_scans = telemetry
                .counters
                .runtime_helper_object_release_root_scans
                .saturating_add(1);
        }
    }

    pub(super) fn record_local_read_reason(&self, reason: &'static str) {
        if !self.options.collect_counters {
            return;
        }
        record_scratch(
            &mut self.runtime_telemetry.borrow_mut().local_reads,
            &LOCAL_REASONS,
            reason,
        );
    }

    pub(super) fn record_local_store_reason(&self, reason: &'static str) {
        if !self.options.collect_counters {
            return;
        }
        record_scratch(
            &mut self.runtime_telemetry.borrow_mut().local_stores,
            &LOCAL_REASONS,
            reason,
        );
    }

    pub(super) fn record_reference_read_class(&self, class: &'static str) {
        if !self.options.collect_counters {
            return;
        }
        record_scratch(
            &mut self.runtime_telemetry.borrow_mut().reference_read_classes,
            &VALUE_CLASSES,
            class,
        );
    }

    pub(super) fn record_truthy_class(&self, class: &'static str) {
        if !self.options.collect_counters {
            return;
        }
        record_scratch(
            &mut self.runtime_telemetry.borrow_mut().truthy_classes,
            &VALUE_CLASSES,
            class,
        );
    }

    pub(super) fn record_lifecycle_reason(&self, retain: bool, reason: &'static str) {
        if !self.options.collect_counters {
            return;
        }
        let mut telemetry = self.runtime_telemetry.borrow_mut();
        let target = if retain {
            &mut telemetry.retains
        } else {
            &mut telemetry.releases
        };
        record_scratch(target, &LIFECYCLE_REASONS, reason);
    }

    pub(super) fn record_release_to_zero(&self) {
        if self.options.collect_counters {
            let mut telemetry = self.runtime_telemetry.borrow_mut();
            telemetry.counters.runtime_helper_release_to_zero = telemetry
                .counters
                .runtime_helper_release_to_zero
                .saturating_add(1);
        }
    }

    pub(super) fn record_ownership_clone(&self) {
        if self.options.collect_counters {
            let mut telemetry = self.runtime_telemetry.borrow_mut();
            telemetry.counters.native_ownership_clones =
                telemetry.counters.native_ownership_clones.saturating_add(1);
        }
    }

    pub(super) fn record_root_rebuild_reason(&self, reason: &'static str) {
        if !self.options.collect_counters {
            return;
        }
        record_scratch(
            &mut self.runtime_telemetry.borrow_mut().root_rebuilds,
            &ROOT_REASONS,
            reason,
        );
    }

    pub(super) fn record_value_table_allocation(&self, high_water: usize) {
        if self.options.collect_counters {
            let mut telemetry = self.runtime_telemetry.borrow_mut();
            telemetry.counters.native_value_table_allocations = telemetry
                .counters
                .native_value_table_allocations
                .saturating_add(1);
            telemetry.counters.native_value_table_high_water = telemetry
                .counters
                .native_value_table_high_water
                .max(high_water as u64);
        }
    }

    pub(super) fn record_value_encode(&self) {
        if self.options.collect_counters {
            let mut telemetry = self.runtime_telemetry.borrow_mut();
            telemetry.counters.native_value_encodes =
                telemetry.counters.native_value_encodes.saturating_add(1);
        }
    }

    pub(super) fn record_value_decode(&self) {
        if self.options.collect_counters {
            let mut telemetry = self.runtime_telemetry.borrow_mut();
            telemetry.counters.native_value_decodes =
                telemetry.counters.native_value_decodes.saturating_add(1);
        }
    }

    pub(super) fn record_value_table_reuse(&self) {
        if self.options.collect_counters {
            let mut telemetry = self.runtime_telemetry.borrow_mut();
            telemetry.counters.native_value_table_reuses = telemetry
                .counters
                .native_value_table_reuses
                .saturating_add(1);
        }
    }

    pub(super) fn record_native_transition(
        &self,
        reason: &'static str,
        elapsed: std::time::Duration,
        nested_helper_time_nanos: u64,
    ) {
        let elapsed_nanos = elapsed.as_nanos().min(u128::from(u64::MAX)) as u64;
        let mut telemetry = self.runtime_telemetry.borrow_mut();
        telemetry.counters.native_transition_count =
            telemetry.counters.native_transition_count.saturating_add(1);
        telemetry.counters.native_transition_time_nanos = telemetry
            .counters
            .native_transition_time_nanos
            .saturating_add(elapsed_nanos);
        let count = telemetry
            .counters
            .native_transition_by_reason
            .entry(reason.to_owned())
            .or_default();
        *count = count.saturating_add(1);
        let time = telemetry
            .counters
            .native_transition_time_nanos_by_reason
            .entry(reason.to_owned())
            .or_default();
        *time = time.saturating_add(elapsed_nanos);
        if let Some(parent) = telemetry.helper_timing_stack.last_mut() {
            parent.child_time_nanos = parent
                .child_time_nanos
                .saturating_add(elapsed_nanos.saturating_sub(nested_helper_time_nanos));
        }
    }
}

fn named_counters<const N: usize>(
    names: &[&str; N],
    values: [u64; N],
) -> std::collections::BTreeMap<String, u64> {
    names
        .iter()
        .zip(values)
        .filter(|(_, value)| *value != 0)
        .map(|(name, value)| ((*name).to_owned(), value))
        .collect()
}

fn merge_function_scratch(target: &mut Vec<u64>, source: &std::collections::BTreeMap<String, u64>) {
    for (name, value) in source {
        let Some(index) = name
            .split_once(':')
            .map_or(name.as_str(), |(index, _)| index)
            .parse::<usize>()
            .ok()
        else {
            continue;
        };
        if target.len() <= index {
            target.resize(index + 1, 0);
        }
        target[index] = target[index].saturating_add(*value);
    }
}

fn record_scratch<const N: usize>(target: &mut [u64; N], names: &[&str; N], name: &str) {
    if let Some(index) = names.iter().position(|candidate| *candidate == name) {
        target[index] = target[index].saturating_add(1);
    }
}

fn merge_named_scratch<const N: usize>(
    target: &mut [u64; N],
    names: &[&str; N],
    source: &std::collections::BTreeMap<String, u64>,
) {
    for (name, value) in source {
        if let Some(index) = names.iter().position(|candidate| *candidate == name) {
            target[index] = target[index].saturating_add(*value);
        }
    }
}

fn current_php_worker_stack_bytes() -> (u64, u64) {
    let current = std::thread::current();
    if !current
        .name()
        .is_some_and(|name| name.starts_with("php-worker-"))
    {
        return (0, 0);
    }
    let virtual_bytes = std::env::var("PHRUST_SERVER_PHP_WORKER_STACK_BYTES")
        .or_else(|_| std::env::var("PHRUST_SERVER_TOKIO_WORKER_STACK_BYTES"))
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(16 * 1024 * 1024);
    #[cfg(target_os = "linux")]
    let committed_bytes = std::fs::read_to_string("/proc/thread-self/smaps")
        .ok()
        .and_then(|smaps| {
            let mut stack = false;
            for line in smaps.lines() {
                if line.contains('-') && line.contains(' ') {
                    stack = line.ends_with("[stack]");
                } else if stack
                    && let Some(value) = line.strip_prefix("Rss:")
                    && let Some(kib) = value.split_whitespace().next()
                    && let Ok(kib) = kib.parse::<u64>()
                {
                    return Some(kib.saturating_mul(1024));
                }
            }
            None
        })
        .unwrap_or(0);
    #[cfg(not(target_os = "linux"))]
    let committed_bytes = 0;
    (virtual_bytes, committed_bytes)
}

fn merge_counter_map(
    target: &mut std::collections::BTreeMap<String, u64>,
    source: &std::collections::BTreeMap<String, u64>,
) {
    for (name, value) in source {
        let entry = target.entry(name.clone()).or_default();
        *entry = entry.saturating_add(*value);
    }
}

fn merge_gauge_map_max(
    target: &mut std::collections::BTreeMap<String, u64>,
    source: &std::collections::BTreeMap<String, u64>,
) {
    for (name, value) in source {
        let entry = target.entry(name.clone()).or_default();
        *entry = (*entry).max(*value);
    }
}
