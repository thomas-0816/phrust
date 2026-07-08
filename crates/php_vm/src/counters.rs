//! Optional VM/runtime counters for performance performance instrumentation.

use std::collections::{BTreeMap, BTreeSet};

use php_ir::instruction::{BinaryOp, InstructionKind};
use php_runtime::{OutputStats, PhpArrayShapeKind, PhpArrayShapeLookupFallback, Slot, TempValue};

use crate::aliasing::{AliasState, alias_transition_key};
use crate::bytecode::DenseOpcode;
use crate::inline_cache::POLYMORPHIC_INLINE_CACHE_LIMIT;
use crate::{InlineCacheKind, InlineCacheObservation};

/// O(1) per-opcode execution counts for the rich-IR interpreter, indexed by
/// `ir_opcode_index`. Boxed so `VmCounters` stays cheap to move.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct IrOpcodeCounts(pub(crate) Box<[u64; IR_OPCODE_COUNT]>);

impl Default for IrOpcodeCounts {
    fn default() -> Self {
        Self(Box::new([0; IR_OPCODE_COUNT]))
    }
}

/// O(1) per-opcode execution counts for dense bytecode, indexed by the
/// `DenseOpcode` discriminant (max 66 today; 128 leaves headroom).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DenseOpcodeCounts(pub(crate) Box<[u64; DENSE_OPCODE_SLOTS]>);

pub(crate) const DENSE_OPCODE_SLOTS: usize = 128;

impl Default for DenseOpcodeCounts {
    fn default() -> Self {
        Self(Box::new([0; DENSE_OPCODE_SLOTS]))
    }
}

/// One runtime observation for a property-fetch callsite profile.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct PropertyFetchProfileObservation {
    pub callsite: String,
    pub property: String,
    pub receiver_class: String,
    pub class_id: u32,
    pub declared_property_name: Option<String>,
    pub visibility_context: Option<String>,
    pub property_slot_index: Option<usize>,
    pub class_layout_version: u64,
    pub has_magic_get: bool,
    pub has_property_hook: bool,
    pub dynamic_property_fallback: bool,
    pub declared_visible_property: bool,
    pub uninitialized_typed_property: bool,
    pub non_eligible_reasons: Vec<&'static str>,
}

/// Aggregated metadata for one property-fetch callsite.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PropertyFetchProfile {
    pub callsite: String,
    pub property: String,
    pub observations: u64,
    pub receiver_classes: BTreeSet<String>,
    pub class_ids: BTreeSet<u32>,
    pub declared_property_names: BTreeSet<String>,
    pub visibility_contexts: BTreeSet<String>,
    pub property_slot_indexes: BTreeSet<usize>,
    pub class_layout_versions: BTreeSet<u64>,
    pub has_magic_get: bool,
    pub has_property_hook: bool,
    pub dynamic_property_fallback: bool,
    pub saw_declared_visible_property: bool,
    pub saw_uninitialized_typed_property: bool,
    pub non_eligible_reasons: BTreeSet<String>,
}

/// One runtime observation for a method-call callsite profile.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MethodCallProfileObservation {
    pub callsite: String,
    pub method: String,
    pub receiver_class: String,
    pub class_id: u32,
    pub declaring_class: Option<String>,
    pub method_id: Option<u32>,
    pub method_slot_index: Option<usize>,
    pub visibility_context: Option<String>,
    pub override_layout_version: u64,
    pub method_is_final: bool,
    pub method_is_private: bool,
    pub method_is_static: bool,
    pub has_magic_call: bool,
    pub magic_call_fallback: bool,
    pub simple_positional_arguments: bool,
    pub has_by_ref_argument: bool,
    pub callee_jit_eligible: bool,
    pub direct_vm_call_helper_available: bool,
    pub non_eligible_reasons: Vec<&'static str>,
}

/// Aggregated metadata for one method-call callsite.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MethodCallProfile {
    pub callsite: String,
    pub method: String,
    pub observations: u64,
    pub receiver_classes: BTreeSet<String>,
    pub class_ids: BTreeSet<u32>,
    pub declaring_classes: BTreeSet<String>,
    pub method_ids: BTreeSet<u32>,
    pub method_slot_indexes: BTreeSet<usize>,
    pub visibility_contexts: BTreeSet<String>,
    pub override_layout_versions: BTreeSet<u64>,
    pub saw_final_method: bool,
    pub saw_private_method: bool,
    pub saw_static_method: bool,
    pub has_magic_call: bool,
    pub magic_call_fallback: bool,
    pub simple_positional_arguments: bool,
    pub saw_by_ref_argument: bool,
    pub saw_callee_jit_eligible: bool,
    pub saw_direct_vm_call_helper: bool,
    pub non_eligible_reasons: BTreeSet<String>,
}

/// Aggregated request-profile timing for one VM execution boundary.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BoundaryProfile {
    pub count: u64,
    pub inclusive_nanos: u64,
    pub exclusive_nanos: u64,
    pub rich_instructions: u64,
    pub dense_instructions: u64,
}

/// Aggregated request-profile timing for one operation family.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OperationProfile {
    pub count: u64,
    pub inclusive_nanos: u64,
}

/// Lightweight counters collected only when explicitly enabled.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VmCounters {
    pub jit_mode: String,
    pub jit_threshold: u64,
    pub instructions_executed: u64,
    pub bytecode_lower_attempts: u64,
    pub bytecode_lower_successes: u64,
    pub dense_execution_plan_cache_hits: u64,
    pub dense_execution_plan_cache_misses: u64,
    pub bytecode_unsupported_fallbacks: u64,
    pub bytecode_unsupported_reasons: BTreeMap<String, u64>,
    pub bytecode_auto_fallback_reasons: BTreeMap<String, u64>,
    pub bytecode_lowered_by_family: BTreeMap<String, u64>,
    pub bytecode_executed_by_family: BTreeMap<String, u64>,
    pub bytecode_instructions_executed: u64,
    pub entry_rich_instructions_executed: u64,
    pub include_rich_instructions_executed: u64,
    pub entry_bytecode_instructions_executed: u64,
    pub include_bytecode_instructions_executed: u64,
    pub include_profiles_by_path: BTreeMap<String, BoundaryProfile>,
    pub dense_include_entry_attempts: u64,
    pub dense_include_entry_successes: u64,
    pub dense_include_entry_fallbacks: u64,
    pub dense_include_entry_fallback_by_reason: BTreeMap<String, u64>,
    pub dense_include_entry_fallback_by_path: BTreeMap<String, u64>,
    pub dense_functions_planned: u64,
    pub dense_functions_executed: u64,
    pub rich_fallback_functions_planned: u64,
    pub rich_fallback_functions_executed: u64,
    pub dense_function_fallback_by_reason: BTreeMap<String, u64>,
    pub rich_fallback_functions_by_name: BTreeMap<String, u64>,
    pub dense_instruction_families_executed: BTreeMap<String, u64>,
    pub dense_property_fetch_hits: u64,
    pub dense_property_assignment_hits: u64,
    pub dense_property_fallback_by_reason: BTreeMap<String, u64>,
    pub dense_property_ic_reuse: u64,
    pub dense_direct_call_hits: u64,
    pub dense_method_call_hits: u64,
    pub dense_static_call_hits: u64,
    pub dense_callable_call_hits: u64,
    pub dense_call_ic_hits: u64,
    pub dense_call_ic_misses: u64,
    pub dense_call_fallback_by_reason: BTreeMap<String, u64>,
    pub dense_branch_executions: u64,
    pub dense_branch_true: u64,
    pub dense_branch_false: u64,
    pub dense_branch_fallthrough_chosen: u64,
    pub dense_block_entries: u64,
    pub dense_block_entry_counts: BTreeMap<String, u64>,
    pub dense_branch_edge_counts: BTreeMap<String, u64>,
    pub superinstruction_candidates: u64,
    pub superinstruction_candidates_by_kind: BTreeMap<String, u64>,
    pub superinstructions_emitted: u64,
    pub superinstructions_emitted_by_kind: BTreeMap<String, u64>,
    pub superinstructions_executed: BTreeMap<String, u64>,
    /// Scratch accumulators for hot per-instruction recording: opcode counts
    /// are O(1) enum-indexed arrays, block/branch keys are integer tuples, so
    /// recording never allocates or compares strings. Everything is folded
    /// into the public string-keyed maps by `fold_scratch_counters` before
    /// any snapshot is taken.
    pub(crate) ir_opcode_counts: IrOpcodeCounts,
    pub(crate) dense_opcode_counts: DenseOpcodeCounts,
    pub(crate) dense_opcode_seen: Vec<DenseOpcode>,
    pub(crate) dense_block_entry_scratch: BTreeMap<(u32, u32), u64>,
    pub(crate) dense_branch_edge_scratch: BTreeMap<(u32, u32, u32), u64>,
    pub(crate) superinstruction_scratch: BTreeMap<&'static str, u64>,
    pub superinstruction_deopt_or_fallbacks: u64,
    pub superinstruction_deopt_or_fallback_by_reason: BTreeMap<String, u64>,
    pub superinstruction_skipped_by_reason: BTreeMap<String, u64>,
    pub optimized_exit_snapshots_created: u64,
    pub optimized_exit_snapshots_materialized: u64,
    pub optimized_exits_by_reason: BTreeMap<String, u64>,
    pub snapshot_rejection_by_missing_state_family: BTreeMap<String, u64>,
    pub fallback_resume_successes: u64,
    pub opcodes: BTreeMap<String, u64>,
    pub function_calls: u64,
    pub method_calls: u64,
    pub function_profiles_by_name: BTreeMap<String, BoundaryProfile>,
    pub method_profiles_by_name: BTreeMap<String, BoundaryProfile>,
    pub builtin_profiles_by_name: BTreeMap<String, BoundaryProfile>,
    pub frame_allocations: u64,
    pub frame_reuses: u64,
    pub frames_allocated: u64,
    pub frames_reused: u64,
    pub register_files_allocated: u64,
    pub register_files_reused: u64,
    pub frame_reuse_blocked_by_reason: BTreeMap<String, u64>,
    pub call_frame_layout_observed: BTreeMap<String, u64>,
    pub tiny_frame_candidates: u64,
    pub specialized_frame_hits: u64,
    pub generic_frame_fallback_by_reason: BTreeMap<String, u64>,
    pub arg_array_avoided: u64,
    pub heap_frame_avoided: u64,
    pub frame_alias_state: BTreeMap<String, u64>,
    pub alias_state_transitions: BTreeMap<String, u64>,
    pub fast_path_disabled_by_reference: u64,
    pub dequickened_by_reference: u64,
    pub ic_invalidated_by_reference: u64,
    pub dense_bytecode_fallback_by_reference: u64,
    pub request_arena_allocations: u64,
    pub request_arena_bytes: u64,
    pub request_pool_resets: u64,
    pub persistent_engine_allocations: u64,
    pub persistent_engine_bytes: u64,
    pub arena_fallback_allocations_by_reason: BTreeMap<String, u64>,
    pub destructor_sensitive_arena_blocks: u64,
    pub value_clones: u64,
    pub string_allocations: u64,
    pub array_handle_clones: u64,
    pub cow_separations: u64,
    pub reference_cell_creations: u64,
    pub value_clone_by_source_family: BTreeMap<String, u64>,
    pub array_handle_clone_by_source_family: BTreeMap<String, u64>,
    pub cow_separation_by_source_family: BTreeMap<String, u64>,
    pub reference_cell_creation_by_source_family: BTreeMap<String, u64>,
    pub object_allocations: u64,
    pub array_packed_direct_gets: u64,
    pub array_mixed_indexed_gets: u64,
    pub array_linear_scan_fallbacks: u64,
    pub array_metadata_recomputes: u64,
    pub symbol_map_lookups: u64,
    pub symbol_linear_fallbacks: u64,
    pub symbol_intern_hits: u64,
    pub symbol_intern_misses: u64,
    pub string_hash_cache_hits: u64,
    pub string_hash_cache_misses: u64,
    pub symbol_eq_fast_hits: u64,
    pub symbol_eq_byte_fallbacks: u64,
    pub object_declared_slot_reads: u64,
    pub object_declared_slot_writes: u64,
    pub object_dynamic_property_map_reads: u64,
    pub object_dynamic_property_map_writes: u64,
    pub packed_values_storage_arrays: u64,
    pub packed_values_storage_reads: u64,
    pub packed_values_storage_appends: u64,
    pub packed_virtual_key_iterations: u64,
    pub packed_to_mixed_by_reason: BTreeMap<String, u64>,
    pub record_storage_arrays: u64,
    pub record_slot_reads: u64,
    pub record_slot_writes: u64,
    pub record_shape_promotions: u64,
    pub record_key_symbol_hits: u64,
    pub record_to_mixed_by_reason: BTreeMap<String, u64>,
    pub foreach_no_clone_hits: u64,
    pub foreach_clone_required_by_reason: BTreeMap<String, u64>,
    pub array_read_borrow_hits: u64,
    pub direct_arg_frame_hits: u64,
    pub direct_method_frame_hits: u64,
    pub direct_closure_frame_hits: u64,
    pub direct_constructor_frame_hits: u64,
    pub argument_vector_allocations_avoided: u64,
    pub direct_frame_fallback_by_reason: BTreeMap<String, u64>,
    pub symbolized_call_name_hits: u64,
    pub symbolized_method_name_hits: u64,
    pub symbolized_property_name_hits: u64,
    pub symbolized_array_key_hits: u64,
    pub symbolized_name_fallbacks_by_reason: BTreeMap<String, u64>,
    pub array_dim_fetches: u64,
    pub packed_dim_fast_path_hits: u64,
    pub packed_dim_fast_path_misses: u64,
    pub array_packed_append_fast_path_hits: u64,
    pub array_packed_read_fast_path_hits: u64,
    pub array_sequential_foreach_fast_path_hits: u64,
    pub array_fast_path_hits_by_family: BTreeMap<String, u64>,
    pub array_fast_path_fallback_by_reason: BTreeMap<String, u64>,
    pub array_shape_observed_by_kind: BTreeMap<String, u64>,
    pub record_shape_hits: u64,
    pub record_shape_misses: u64,
    pub small_map_hits: u64,
    pub small_map_misses: u64,
    pub key_coercion_fallbacks: u64,
    pub order_semantics_fallbacks: u64,
    pub packed_append_fast_hits: u64,
    pub packed_foreach_fast_hits: u64,
    pub cow_or_reference_fallbacks: u64,
    pub array_count_fast_path_hits: u64,
    pub array_packed_to_mixed_transitions: u64,
    pub numeric_string_classify_calls: u64,
    pub numeric_string_cache_hits: u64,
    pub numeric_string_cache_misses: u64,
    pub numeric_string_specialization_hits: u64,
    pub numeric_string_warning_sensitive_fallbacks: u64,
    pub numeric_string_overflow_precision_fallbacks: u64,
    pub array_operation_profiles_by_family: BTreeMap<String, OperationProfile>,
    pub typecheck_fast_path_hits: u64,
    pub typecheck_fast_path_misses: u64,
    pub output_bytes: u64,
    pub output_buffer_appends: u64,
    pub output_buffer_batch_writes: u64,
    pub output_batched_appends: u64,
    pub output_batch_bytes: u64,
    pub output_buffer_flushes: u64,
    pub output_fast_appends: u64,
    pub output_slow_appends_by_reason: BTreeMap<String, u64>,
    pub output_operation_profiles_by_family: BTreeMap<String, OperationProfile>,
    pub internal_function_dispatches: u64,
    pub internal_function_dispatch_cache_hits: u64,
    pub internal_function_dispatch_cache_misses: u64,
    pub internal_count_array_direct_fast_path_hits: u64,
    pub function_call_ic_hits: u64,
    pub function_call_ic_misses: u64,
    pub builtin_call_ic_hits: u64,
    pub builtin_call_ic_misses: u64,
    pub builtin_fast_stub_hits: BTreeMap<String, u64>,
    pub builtin_fast_stub_misses: BTreeMap<String, u64>,
    pub builtin_fast_stub_fallback_by_reason: BTreeMap<String, u64>,
    pub builtin_intrinsic_candidates: u64,
    pub intrinsic_hits: BTreeMap<String, u64>,
    pub intrinsic_misses: BTreeMap<String, u64>,
    pub intrinsic_fallback_by_reason: BTreeMap<String, u64>,
    pub json_encode_fast_path_hits: u64,
    pub json_encode_fast_path_bytes: u64,
    pub json_encode_generic_fallback_by_reason: BTreeMap<String, u64>,
    pub array_slice_packed_fast_hits: u64,
    pub count_array_shape_fast_hits: u64,
    pub map_update_slot_fast_hits: u64,
    pub property_dim_assign_in_place_hits: u64,
    pub property_dim_assign_generic_by_reason: BTreeMap<String, u64>,
    pub property_dim_probe_borrowed_hits: u64,
    pub cufa_owned_argument_moves: u64,
    pub cufa_shared_argument_clones: u64,
    pub array_builtin_fast_fallback_by_reason: BTreeMap<String, u64>,
    pub specialized_builtin_opcode_hits: BTreeMap<String, u64>,
    pub slow_path_calls_by_reason: BTreeMap<String, u64>,
    pub value_clone_by_reason: BTreeMap<String, u64>,
    pub by_ref_arg_location_binding_attempts: u64,
    pub by_ref_arg_location_bindings: u64,
    pub by_ref_arg_value_materializations: u64,
    pub by_ref_arg_register_pins: u64,
    pub by_ref_arg_cow_separations: u64,
    pub by_ref_arg_cow_separations_avoided: u64,
    pub by_ref_arg_fallback_by_reason: BTreeMap<String, u64>,
    pub dense_method_dispatch_attempts: u64,
    pub dense_method_dispatch_hits: u64,
    pub dense_method_dispatch_fallbacks: u64,
    pub dense_method_dispatch_fallback_by_reason: BTreeMap<String, u64>,
    pub rich_method_calls_from_dense_callers: u64,
    pub dense_jump_threading_trampoline_blocks: u64,
    pub dense_jump_threading_threaded_edges: u64,
    pub dense_jump_threading_rollbacks: u64,
    pub call_ic_megamorphic_fallbacks: u64,
    pub local_slot_fast_path_hits: u64,
    pub local_slot_fast_path_misses: u64,
    pub property_fetches: u64,
    pub property_accesses: u64,
    pub type_checks: u64,
    pub includes: u64,
    pub autoloads: u64,
    pub include_resolution_hits: u64,
    pub include_resolution_misses: u64,
    pub include_compile_hits: u64,
    pub include_compile_misses: u64,
    pub include_once_skips: u64,
    pub include_fallback_by_reason: BTreeMap<String, u64>,
    pub include_stale_invalidation_by_reason: BTreeMap<String, u64>,
    pub include_graph_hits: u64,
    pub include_graph_misses: u64,
    pub autoload_graph_hits: u64,
    pub autoload_graph_misses: u64,
    pub negative_lookup_hits: u64,
    pub invalidations_by_reason: BTreeMap<String, u64>,
    pub fallback_by_path_semantics: BTreeMap<String, u64>,
    pub string_concats: u64,
    pub string_concat_fast_path_hits: u64,
    pub string_concat_fast_path_misses: u64,
    pub concat_prealloc_hits: u64,
    pub concat_fallback_by_reason: BTreeMap<String, u64>,
    pub guard_failures: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub literal_intern_hits: u64,
    pub literal_intern_misses: u64,
    pub quickening_attempts: u64,
    pub quickening_candidates_by_family: BTreeMap<String, u64>,
    pub quickening_specialized: u64,
    pub quickening_applied_by_family: BTreeMap<String, u64>,
    pub quickened_executions_by_family: BTreeMap<String, u64>,
    pub quickening_guard_hits: u64,
    pub quickening_guard_misses: u64,
    pub quickening_guard_failures: u64,
    pub quickening_guard_failures_by_family: BTreeMap<String, u64>,
    pub quickening_fallback_calls: u64,
    pub quickening_dequickens: u64,
    pub quickening_dequickened_by_reason: BTreeMap<String, u64>,
    pub quickening_megamorphic: u64,
    pub quickening_disabled: u64,
    pub adaptive_tiny_unit_setup_skips: u64,
    pub native_candidates: u64,
    pub native_compiled_regions: u64,
    pub native_executions: u64,
    pub native_compile_budget_rejections: u64,
    pub native_eligibility_rejections_by_reason: BTreeMap<String, u64>,
    pub native_side_exits_by_reason: BTreeMap<String, u64>,
    pub native_blacklist_suppression_by_unstable_region: BTreeMap<String, u64>,
    pub native_platform_unavailable: u64,
    pub jit_compile_attempts: u64,
    pub jit_compiled: u64,
    pub jit_executed: u64,
    pub jit_bailouts: u64,
    pub jit_code_bytes: u64,
    pub jit_compile_time_nanos: u64,
    pub jit_side_exits: u64,
    pub jit_side_exit_reasons: BTreeMap<String, u64>,
    pub jit_guard_failures: u64,
    pub jit_blacklisted_regions: u64,
    pub jit_blacklist_reasons: BTreeMap<String, u64>,
    pub jit_tiering_cold_functions: u64,
    pub jit_tiering_hot_functions: u64,
    pub jit_tiering_eager_functions: u64,
    pub jit_tiering_blacklist_rejections: u64,
    pub jit_tiering_budget_rejections: u64,
    pub jit_helper_calls: u64,
    pub jit_fast_path_hits: u64,
    pub packed_fetch_fast_hits: u64,
    pub record_lookup_fast_hits: u64,
    pub record_lookup_key_miss_exits: u64,
    pub record_lookup_layout_exits: u64,
    pub packed_fetch_bounds_exits: u64,
    pub packed_fetch_layout_exits: u64,
    pub packed_fetch_bounds_fallbacks: u64,
    pub packed_fetch_layout_fallbacks: u64,
    pub packed_foreach_sum_fast_hits: u64,
    pub packed_foreach_sum_layout_exits: u64,
    pub packed_foreach_sum_overflow_exits: u64,
    pub known_call_fast_hits: u64,
    pub known_call_guard_exits: u64,
    pub known_call_slow_calls: u64,
    pub direct_call_hits: u64,
    pub direct_call_fallbacks: u64,
    pub property_load_fast_hits: u64,
    pub property_load_guard_exits: u64,
    pub property_load_layout_exits: u64,
    pub property_load_uninitialized_exits: u64,
    pub property_load_slow_calls: u64,
    pub jit_overflow_exits: u64,
    pub jit_slow_path_calls: u64,
    pub jit_compile_cache_hits: u64,
    pub jit_compile_cache_misses: u64,
    pub jit_compile_cache_invalidations: u64,
    pub jit_compile_descriptors: Vec<JitCompileDescriptor>,
    pub inline_cache_observations: u64,
    pub inline_cache_slots: u64,
    pub inline_cache_function_slots: u64,
    pub inline_cache_method_slots: u64,
    pub inline_cache_property_slots: u64,
    pub inline_cache_property_assign_slots: u64,
    pub inline_cache_dim_slots: u64,
    pub inline_cache_class_constant_static_property_slots: u64,
    pub inline_cache_class_relation_slots: u64,
    pub inline_cache_include_path_slots: u64,
    pub inline_cache_autoload_class_lookup_slots: u64,
    pub inline_cache_hits: u64,
    pub inline_cache_misses: u64,
    pub inline_cache_invalidations: u64,
    pub inline_cache_guard_failures: u64,
    pub inline_cache_fallback_calls: u64,
    pub inline_cache_monomorphic: u64,
    pub inline_cache_polymorphic: u64,
    pub inline_cache_megamorphic: u64,
    pub inline_cache_disabled: u64,
    pub method_ic_hits: u64,
    pub method_ic_misses: u64,
    pub method_ic_polymorphic_hits: u64,
    pub method_ic_guard_failures: u64,
    pub method_direct_dispatch_hits: u64,
    pub method_direct_dispatch_fallbacks: u64,
    pub method_tiny_inline_candidates: u64,
    pub method_inline_candidates: u64,
    pub method_inline_hits: u64,
    pub method_inline_fallback_by_reason: BTreeMap<String, u64>,
    pub constructor_inline_hits: u64,
    pub dto_array_inline_hits: u64,
    pub sort_callback_resolution_cache_hits: u64,
    pub sort_callback_direct_call_hits: u64,
    pub sort_callback_generic_fallback_by_reason: BTreeMap<String, u64>,
    pub method_tiny_inline_rejected_by_reason: BTreeMap<String, u64>,
    pub property_ic_hits: u64,
    pub property_ic_misses: u64,
    pub property_ic_guard_failures: u64,
    pub property_ic_fallback_reasons: BTreeMap<String, u64>,
    pub property_assign_ic_hits: u64,
    pub property_assign_ic_misses: u64,
    pub property_assign_ic_guard_failures: u64,
    pub property_assign_ic_shape_exits: u64,
    pub property_assign_ic_visibility_exits: u64,
    pub property_assign_ic_type_exits: u64,
    pub property_assign_ic_readonly_exits: u64,
    pub property_assign_ic_hook_magic_exits: u64,
    pub property_assign_ic_reference_exits: u64,
    pub property_assign_ic_dynamic_exits: u64,
    pub property_assign_ic_fallback_reasons: BTreeMap<String, u64>,
    pub object_operation_profiles_by_family: BTreeMap<String, OperationProfile>,
    pub class_static_ic_hits: u64,
    pub class_static_ic_misses: u64,
    pub class_static_ic_guard_failures: u64,
    pub class_relation_cache_hits: u64,
    pub class_relation_cache_misses: u64,
    pub class_relation_cache_invalidations: u64,
    pub instanceof_cache_hits: u64,
    pub instanceof_cache_misses: u64,
    pub method_override_cache_hits: u64,
    pub method_override_cache_misses: u64,
    pub include_path_ic_hits: u64,
    pub include_path_ic_misses: u64,
    pub include_path_ic_invalidations: u64,
    pub include_path_ic_guard_failures: u64,
    pub autoload_class_lookup_ic_hits: u64,
    pub autoload_class_lookup_ic_misses: u64,
    pub autoload_class_lookup_ic_invalidations: u64,
    pub autoload_class_lookup_ic_guard_failures: u64,
    pub property_fetch_profiles: BTreeMap<String, PropertyFetchProfile>,
    pub method_call_profiles: BTreeMap<String, MethodCallProfile>,
    /// Runtime lever R3: dense register reads moved instead of cloned because a
    /// conservative last-use analysis proved the read was a block-local last use.
    pub last_use_moves_applied: u64,
    /// Subset of `last_use_moves_applied` where the moved value was a refcounted
    /// heap value (array/string/object/...), so a real clone/allocation was avoided.
    pub last_use_move_clones_avoided: u64,
    /// Runtime lever R3 (array-read release): transient shared array-handle
    /// register clones dropped at a dimension fetch's block-local last use, so a
    /// following in-place write to the array's owning local skips a
    /// copy-on-write separation. Observable in the `cow_separations` drop.
    pub last_use_array_read_releases: u64,
    /// Candidate register reads left cloning, grouped by stable rejection reason.
    pub last_use_move_ineligible_by_reason: BTreeMap<String, u64>,
}

/// Diagnostic metadata for one successful Cranelift compile.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JitCompileDescriptor {
    pub function_id: u32,
    pub function_name: String,
    pub ir_fingerprint: String,
    pub code_bytes: u64,
    pub compile_time_nanos: u64,
    pub target_isa: String,
    pub abi_hash: u64,
    pub config_hash: u64,
}

impl VmCounters {
    pub(crate) fn set_jit_config(&mut self, mode: &str, threshold: u64) {
        self.jit_mode.clear();
        self.jit_mode.push_str(mode);
        self.jit_threshold = threshold;
    }

    pub(crate) fn record_instruction(&mut self, kind: &InstructionKind) {
        self.instructions_executed += 1;
        // O(1) enum-indexed count: no allocation or map walk per instruction.
        let index = ir_opcode_index(kind);
        if let Some(count) = self.ir_opcode_counts.0.get_mut(index) {
            *count += 1;
        } else {
            debug_assert!(
                false,
                "ir_opcode_index returned {index}; extend IR_OPCODE_NAMES/IR_OPCODE_COUNT \
                 alongside the ir_opcode_index match"
            );
        }
        match kind {
            InstructionKind::BindReferenceFromCall { .. }
            | InstructionKind::CallFunction { .. }
            | InstructionKind::CallClosure { .. }
            | InstructionKind::AcquireCallable { .. }
            | InstructionKind::CallCallable { .. }
            | InstructionKind::Pipe { .. } => self.function_calls += 1,
            InstructionKind::BindReferenceFromMethodCall { .. }
            | InstructionKind::CallMethod { .. }
            | InstructionKind::CallStaticMethod { .. } => self.method_calls += 1,
            InstructionKind::BindReferenceDim { .. }
            | InstructionKind::BindReferenceFromDim { .. }
            | InstructionKind::FetchDim { .. }
            | InstructionKind::ArrayGet { .. }
            | InstructionKind::IssetDim { .. }
            | InstructionKind::EmptyDim { .. }
            | InstructionKind::UnsetDim { .. } => self.array_dim_fetches += 1,
            InstructionKind::FetchProperty { .. }
            | InstructionKind::FetchDynamicProperty { .. }
            | InstructionKind::FetchStaticProperty { .. }
            | InstructionKind::FetchDynamicStaticProperty { .. } => {
                self.property_fetches += 1;
                self.property_accesses += 1;
            }
            InstructionKind::IssetProperty { .. }
            | InstructionKind::IssetDynamicProperty { .. }
            | InstructionKind::EmptyProperty { .. }
            | InstructionKind::EmptyDynamicProperty { .. }
            | InstructionKind::IssetDynamicPropertyDim { .. }
            | InstructionKind::EmptyDynamicPropertyDim { .. }
            | InstructionKind::UnsetProperty { .. }
            | InstructionKind::UnsetPropertyDim { .. }
            | InstructionKind::UnsetDynamicProperty { .. }
            | InstructionKind::AssignProperty { .. }
            | InstructionKind::AssignPropertyDim { .. }
            | InstructionKind::AssignDynamicProperty { .. }
            | InstructionKind::BindReferenceProperty { .. }
            | InstructionKind::BindReferenceStaticProperty { .. }
            | InstructionKind::BindReferenceFromProperty { .. }
            | InstructionKind::BindReferenceFromPropertyDim { .. }
            | InstructionKind::AssignStaticProperty { .. }
            | InstructionKind::AssignDynamicStaticProperty { .. } => self.property_accesses += 1,
            InstructionKind::InstanceOf { .. } | InstructionKind::DynamicInstanceOf { .. } => {
                self.type_checks += 1
            }
            InstructionKind::Include { .. } => self.includes += 1,
            InstructionKind::Binary {
                op: BinaryOp::Concat,
                ..
            } => self.string_concats += 1,
            _ => {}
        }
    }

    pub(crate) fn record_entry_rich_instruction(&mut self) {
        self.entry_rich_instructions_executed += 1;
    }

    pub(crate) fn record_include_rich_instruction(&mut self) {
        self.include_rich_instructions_executed += 1;
    }

    pub(crate) fn record_autoload(&mut self) {
        self.autoloads += 1;
    }

    pub(crate) fn record_include_resolution_hit(&mut self) {
        self.include_resolution_hits += 1;
    }

    pub(crate) fn record_include_resolution_miss(&mut self) {
        self.include_resolution_misses += 1;
    }

    pub(crate) fn record_include_compile_hit(&mut self) {
        self.include_compile_hits += 1;
    }

    pub(crate) fn record_include_compile_miss(&mut self) {
        self.include_compile_misses += 1;
    }

    pub(crate) fn record_include_once_skip(&mut self) {
        self.include_once_skips += 1;
    }

    pub(crate) fn record_include_fallback_by_reason(&mut self, reason: &str) {
        *self
            .include_fallback_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_include_profile(
        &mut self,
        path: &str,
        inclusive_nanos: u64,
        exclusive_nanos: u64,
        rich_instructions: u64,
        dense_instructions: u64,
    ) {
        record_boundary_profile(
            &mut self.include_profiles_by_path,
            path,
            inclusive_nanos,
            exclusive_nanos,
            rich_instructions,
            dense_instructions,
        );
    }

    pub(crate) fn record_include_stale_invalidation_by_reason(&mut self, reason: &str) {
        *self
            .include_stale_invalidation_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_include_graph_hit(&mut self) {
        self.include_graph_hits += 1;
    }

    pub(crate) fn record_include_graph_miss(&mut self) {
        self.include_graph_misses += 1;
    }

    pub(crate) fn record_autoload_graph_hit(&mut self) {
        self.autoload_graph_hits += 1;
    }

    pub(crate) fn record_autoload_graph_miss(&mut self) {
        self.autoload_graph_misses += 1;
    }

    pub(crate) fn record_negative_lookup_hit(&mut self) {
        self.negative_lookup_hits += 1;
    }

    pub(crate) fn record_invalidation_by_reason(&mut self, reason: &str) {
        *self
            .invalidations_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_fallback_by_path_semantics(&mut self, reason: &str) {
        *self
            .fallback_by_path_semantics
            .entry(reason.to_owned())
            .or_default() += 1;
        self.record_slow_path_call(&format!("include_autoload.{reason}"));
    }

    pub(crate) fn record_slow_path_call(&mut self, reason: &str) {
        *self
            .slow_path_calls_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    /// Attributes a forced `Value` clone at a semantic funnel (call-argument
    /// snapshot, return value, foreach element, reference/COW, property read).
    pub(crate) fn record_value_clone_by_reason(&mut self, reason: &str) {
        *self
            .value_clone_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_dense_method_dispatch_fallback(&mut self, reason: &str) {
        self.dense_method_dispatch_fallbacks += 1;
        self.rich_method_calls_from_dense_callers += 1;
        *self
            .dense_method_dispatch_fallback_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    /// Attributes one by-reference argument binding fallback (why a location
    /// binding was not used or still materialized a caller value).
    pub(crate) fn record_by_ref_arg_fallback(&mut self, reason: &str) {
        *self
            .by_ref_arg_fallback_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_direct_frame_hit(&mut self, layout: &str, is_constructor: bool) {
        if is_constructor {
            self.direct_constructor_frame_hits += 1;
        } else {
            match layout {
                "known_method_frame" => self.direct_method_frame_hits += 1,
                "closure_frame" => self.direct_closure_frame_hits += 1,
                _ => self.direct_arg_frame_hits += 1,
            }
        }
        self.argument_vector_allocations_avoided += 1;
    }

    pub(crate) fn record_direct_frame_fallback(&mut self, reason: &str) {
        *self
            .direct_frame_fallback_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_foreach_no_clone_hit(&mut self) {
        self.foreach_no_clone_hits += 1;
    }

    pub(crate) fn record_array_read_borrow_hit(&mut self) {
        self.array_read_borrow_hits += 1;
    }

    pub(crate) fn record_symbolized_call_name_hit(&mut self) {
        self.symbolized_call_name_hits += 1;
    }

    pub(crate) fn record_symbolized_method_name_hit(&mut self) {
        self.symbolized_method_name_hits += 1;
    }

    pub(crate) fn record_symbolized_property_name_hit(&mut self) {
        self.symbolized_property_name_hits += 1;
    }

    pub(crate) fn record_symbolized_array_key_hit(&mut self) {
        self.symbolized_array_key_hits += 1;
    }

    pub(crate) fn record_symbolized_name_fallback(&mut self, reason: &str) {
        *self
            .symbolized_name_fallbacks_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_frame_activation(
        &mut self,
        reused: bool,
        register_count: u32,
        local_count: u32,
    ) {
        if reused {
            self.frame_reuses += 1;
            self.frames_reused += 1;
            self.register_files_reused += 1;
            self.request_pool_resets += 1;
        } else {
            self.frame_allocations += 1;
            self.frames_allocated += 1;
            self.register_files_allocated += 1;
            self.record_request_arena_allocation(request_frame_allocation_bytes(
                register_count,
                local_count,
            ));
        }
    }

    pub(crate) fn record_frame_reuse_blocked(&mut self, reason: &str) {
        *self
            .frame_reuse_blocked_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
        *self
            .arena_fallback_allocations_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
        if matches!(
            reason,
            "destructor_sensitive_value" | "try_finally" | "generator" | "fiber_continuation"
        ) {
            self.destructor_sensitive_arena_blocks += 1;
        }
        if alias_sensitive_reason(reason) {
            self.record_alias_state(AliasState::EscapedReference);
            self.fast_path_disabled_by_reference += 1;
        }
    }

    pub(crate) fn record_call_frame_layout(&mut self, layout: &str) {
        *self
            .call_frame_layout_observed
            .entry(layout.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_function_profile(
        &mut self,
        name: &str,
        is_method: bool,
        inclusive_nanos: u64,
        exclusive_nanos: u64,
        rich_instructions: u64,
        dense_instructions: u64,
    ) {
        let profiles = if is_method {
            &mut self.method_profiles_by_name
        } else {
            &mut self.function_profiles_by_name
        };
        record_boundary_profile(
            profiles,
            name,
            inclusive_nanos,
            exclusive_nanos,
            rich_instructions,
            dense_instructions,
        );
    }

    pub(crate) fn record_builtin_profile(
        &mut self,
        name: &str,
        inclusive_nanos: u64,
        exclusive_nanos: u64,
        rich_instructions: u64,
        dense_instructions: u64,
    ) {
        record_boundary_profile(
            &mut self.builtin_profiles_by_name,
            name,
            inclusive_nanos,
            exclusive_nanos,
            rich_instructions,
            dense_instructions,
        );
    }

    pub(crate) fn record_array_operation_profile(&mut self, family: &str, inclusive_nanos: u64) {
        record_operation_profile(
            &mut self.array_operation_profiles_by_family,
            family,
            inclusive_nanos,
        );
    }

    pub(crate) fn record_object_operation_profile(&mut self, family: &str, inclusive_nanos: u64) {
        record_operation_profile(
            &mut self.object_operation_profiles_by_family,
            family,
            inclusive_nanos,
        );
    }

    pub(crate) fn record_output_operation_profile(&mut self, family: &str, inclusive_nanos: u64) {
        record_operation_profile(
            &mut self.output_operation_profiles_by_family,
            family,
            inclusive_nanos,
        );
    }

    pub(crate) fn record_tiny_frame_candidate(&mut self) {
        self.tiny_frame_candidates += 1;
    }

    pub(crate) fn record_specialized_frame_hit(&mut self) {
        self.specialized_frame_hits += 1;
    }

    pub(crate) fn record_generic_frame_fallback(&mut self, reason: &str) {
        *self
            .generic_frame_fallback_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_arg_array_avoided(&mut self) {
        self.arg_array_avoided += 1;
    }

    pub(crate) fn record_heap_frame_avoided(&mut self) {
        self.heap_frame_avoided += 1;
    }

    pub(crate) fn record_alias_state(&mut self, state: AliasState) {
        *self
            .frame_alias_state
            .entry(state.as_str().to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_alias_state_transition(&mut self, from: AliasState, to: AliasState) {
        self.record_alias_state(to);
        if from != to {
            *self
                .alias_state_transitions
                .entry(alias_transition_key(from, to))
                .or_default() += 1;
        }
    }

    pub(crate) fn record_fast_path_disabled_by_reference(&mut self, state: AliasState) {
        if state.is_reference_sensitive() {
            self.record_alias_state(state);
            self.fast_path_disabled_by_reference += 1;
        }
    }

    pub(crate) fn record_dequickened_by_reference(&mut self, state: AliasState) {
        if state.is_reference_sensitive() {
            self.record_alias_state(state);
            self.dequickened_by_reference += 1;
            self.fast_path_disabled_by_reference += 1;
        }
    }

    pub(crate) fn record_ic_invalidated_by_reference(&mut self, state: AliasState) {
        if state.is_reference_sensitive() {
            self.record_alias_state(state);
            self.ic_invalidated_by_reference += 1;
            self.fast_path_disabled_by_reference += 1;
        }
    }

    pub(crate) fn record_dense_bytecode_fallback_by_reference(&mut self, state: AliasState) {
        if state.is_reference_sensitive() {
            self.record_alias_state(state);
            self.dense_bytecode_fallback_by_reference += 1;
            self.fast_path_disabled_by_reference += 1;
        }
    }

    pub(crate) fn record_request_arena_allocation(&mut self, bytes: usize) {
        self.request_arena_allocations += 1;
        self.request_arena_bytes += bytes as u64;
    }

    /// Records the persistent immutable engine-metadata heap footprint as a
    /// snapshot: `entries` interned immutable names totalling `bytes`. This is
    /// engine-owned data that survives across requests and never holds userland
    /// values, so it is the one class currently safe to account here. Setting
    /// (not accumulating) keeps the field a footprint rather than a per-call
    /// delta.
    pub(crate) fn record_persistent_engine_footprint(&mut self, entries: u64, bytes: u64) {
        self.persistent_engine_allocations = entries;
        self.persistent_engine_bytes = bytes;
    }

    pub(crate) fn record_runtime_layout_stats(
        &mut self,
        stats: php_runtime::layout_stats::RuntimeLayoutStats,
    ) {
        self.value_clones += stats.value_clones;
        self.string_allocations += stats.string_allocations;
        self.array_handle_clones += stats.array_handle_clones;
        self.cow_separations += stats.cow_separations;
        self.reference_cell_creations += stats.reference_cell_creations;
        self.object_allocations += stats.object_allocations;
        self.array_packed_direct_gets += stats.array_packed_direct_gets;
        self.array_mixed_indexed_gets += stats.array_mixed_indexed_gets;
        self.array_linear_scan_fallbacks += stats.array_linear_scan_fallbacks;
        self.array_metadata_recomputes += stats.array_metadata_recomputes;
        self.symbol_map_lookups += stats.symbol_map_lookups;
        self.symbol_linear_fallbacks += stats.symbol_linear_fallbacks;
        self.symbol_intern_hits += stats.symbol_intern_hits;
        self.symbol_intern_misses += stats.symbol_intern_misses;
        self.string_hash_cache_hits += stats.string_hash_cache_hits;
        self.string_hash_cache_misses += stats.string_hash_cache_misses;
        self.symbol_eq_fast_hits += stats.symbol_eq_fast_hits;
        self.symbol_eq_byte_fallbacks += stats.symbol_eq_byte_fallbacks;
        self.object_declared_slot_reads += stats.object_declared_slot_reads;
        self.object_declared_slot_writes += stats.object_declared_slot_writes;
        self.object_dynamic_property_map_reads += stats.object_dynamic_property_map_reads;
        self.object_dynamic_property_map_writes += stats.object_dynamic_property_map_writes;
        self.packed_values_storage_arrays += stats.packed_values_storage_arrays;
        self.packed_values_storage_reads += stats.packed_values_storage_reads;
        self.packed_values_storage_appends += stats.packed_values_storage_appends;
        self.packed_virtual_key_iterations += stats.packed_virtual_key_iterations;
        for (reason, count) in [
            ("string_key", stats.packed_to_mixed_string_key),
            (
                "non_sequential_int_key",
                stats.packed_to_mixed_non_sequential_int_key,
            ),
            ("append_key_gap", stats.packed_to_mixed_append_key_gap),
            ("unset_hole", stats.packed_to_mixed_unset_hole),
        ] {
            if count > 0 {
                *self
                    .packed_to_mixed_by_reason
                    .entry(reason.to_owned())
                    .or_default() += count;
            }
        }
        self.record_storage_arrays += stats.record_storage_arrays;
        self.record_slot_reads += stats.record_slot_reads;
        self.record_slot_writes += stats.record_slot_writes;
        self.record_shape_promotions += stats.record_shape_promotions;
        self.record_key_symbol_hits += stats.record_key_symbol_hits;
        for (reason, count) in [
            ("int_key", stats.record_to_mixed_int_key),
            ("ambiguous_key", stats.record_to_mixed_ambiguous_key),
            ("generic_mutation", stats.record_to_mixed_generic_mutation),
        ] {
            if count > 0 {
                *self
                    .record_to_mixed_by_reason
                    .entry(reason.to_owned())
                    .or_default() += count;
            }
        }
    }

    pub(crate) fn record_runtime_layout_source_stats(
        &mut self,
        stats: php_runtime::layout_stats::RuntimeLayoutSourceStats,
    ) {
        merge_static_counter_map(
            &mut self.value_clone_by_source_family,
            stats.value_clone_by_family,
        );
        merge_static_counter_map(
            &mut self.array_handle_clone_by_source_family,
            stats.array_handle_clone_by_family,
        );
        merge_static_counter_map(
            &mut self.cow_separation_by_source_family,
            stats.cow_separation_by_family,
        );
        merge_static_counter_map(
            &mut self.reference_cell_creation_by_source_family,
            stats.reference_cell_creation_by_family,
        );
    }

    pub(crate) fn record_bytecode_lower_attempt(&mut self) {
        self.bytecode_lower_attempts += 1;
    }

    pub(crate) fn record_bytecode_lower_success(&mut self) {
        self.bytecode_lower_successes += 1;
    }

    pub(crate) fn record_dense_execution_plan_cache_hit(&mut self) {
        self.dense_execution_plan_cache_hits += 1;
    }

    pub(crate) fn record_dense_execution_plan_cache_miss(&mut self) {
        self.dense_execution_plan_cache_misses += 1;
    }

    pub(crate) fn record_bytecode_unsupported_fallback(&mut self) {
        self.bytecode_unsupported_fallbacks += 1;
    }

    pub(crate) fn record_bytecode_unsupported_reason(&mut self, reason: &str) {
        *self
            .bytecode_unsupported_reasons
            .entry(reason.to_owned())
            .or_default() += 1;
        if alias_sensitive_reason(reason) {
            self.record_dense_bytecode_fallback_by_reference(AliasState::UnknownAliasing);
        }
    }

    pub(crate) fn record_bytecode_auto_fallback_reason(&mut self, reason: &str) {
        *self
            .bytecode_auto_fallback_reasons
            .entry(reason.to_owned())
            .or_default() += 1;
        if alias_sensitive_reason(reason) {
            self.record_dense_bytecode_fallback_by_reference(AliasState::UnknownAliasing);
        }
    }

    pub(crate) fn record_bytecode_lowered_family(&mut self, family: &str) {
        *self
            .bytecode_lowered_by_family
            .entry(family.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_dense_execution_plan(
        &mut self,
        dense_functions: u64,
        rich_fallback_functions: u64,
    ) {
        self.dense_functions_planned += dense_functions;
        self.rich_fallback_functions_planned += rich_fallback_functions;
    }

    pub(crate) fn record_dense_function_executed(&mut self) {
        self.dense_functions_executed += 1;
    }

    /// Records one applied last-use move; `clone_avoided` is true when the moved
    /// value was a refcounted heap value and cloning it would have allocated or
    /// bumped a refcount.
    pub(crate) fn record_last_use_move_applied(&mut self, clone_avoided: bool) {
        self.last_use_moves_applied += 1;
        if clone_avoided {
            self.last_use_move_clones_avoided += 1;
        }
    }

    /// Records one array-read register release (Runtime lever R3): a transient
    /// shared array handle dropped at a dimension fetch's last use.
    pub(crate) fn record_last_use_array_read_release(&mut self) {
        self.last_use_array_read_releases += 1;
    }

    /// Attributes one candidate register read left cloning to a stable reason.
    pub(crate) fn record_last_use_move_ineligible(&mut self, reason: &str, count: u64) {
        *self
            .last_use_move_ineligible_by_reason
            .entry(reason.to_owned())
            .or_default() += count;
    }

    pub(crate) fn record_dense_property_fetch_hit(&mut self) {
        self.dense_property_fetch_hits += 1;
    }

    pub(crate) fn record_dense_property_assignment_hit(&mut self) {
        self.dense_property_assignment_hits += 1;
    }

    pub(crate) fn record_dense_property_ic_reuse(&mut self) {
        self.dense_property_ic_reuse += 1;
    }

    pub(crate) fn record_dense_property_fallback(&mut self, reason: &str) {
        *self
            .dense_property_fallback_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_dense_direct_call_hit(&mut self) {
        self.dense_direct_call_hits += 1;
    }

    pub(crate) fn record_dense_method_call_hit(&mut self) {
        self.dense_method_call_hits += 1;
    }

    pub(crate) fn record_dense_static_call_hit(&mut self) {
        self.dense_static_call_hits += 1;
    }

    pub(crate) fn record_dense_callable_call_hit(&mut self) {
        self.dense_callable_call_hits += 1;
    }

    pub(crate) fn record_dense_call_ic_hit(&mut self) {
        self.dense_call_ic_hits += 1;
    }

    pub(crate) fn record_dense_call_ic_miss(&mut self) {
        self.dense_call_ic_misses += 1;
    }

    pub(crate) fn record_dense_call_fallback(&mut self, reason: &str) {
        *self
            .dense_call_fallback_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_rich_fallback_function_executed(&mut self, reason: &str, name: &str) {
        self.rich_fallback_functions_executed += 1;
        *self
            .dense_function_fallback_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
        *self
            .rich_fallback_functions_by_name
            .entry(name.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_bytecode_instruction(&mut self, opcode: DenseOpcode) {
        self.bytecode_instructions_executed += 1;
        if matches!(opcode, DenseOpcode::Include) {
            self.includes += 1;
        }
        // O(1) discriminant-indexed count; the `bytecode_` prefix and family
        // projections are applied once per distinct opcode at fold time.
        let index = opcode as usize;
        if let Some(count) = self.dense_opcode_counts.0.get_mut(index) {
            if *count == 0 {
                self.dense_opcode_seen.push(opcode);
            }
            *count += 1;
        } else {
            debug_assert!(
                false,
                "DenseOpcode discriminant {index} exceeds DENSE_OPCODE_SLOTS; grow the array"
            );
        }
    }

    pub(crate) fn record_entry_bytecode_instruction(&mut self) {
        self.entry_bytecode_instructions_executed += 1;
    }

    pub(crate) fn record_include_bytecode_instruction(&mut self) {
        self.include_bytecode_instructions_executed += 1;
    }

    pub(crate) fn record_dense_include_entry_attempt(&mut self) {
        self.dense_include_entry_attempts += 1;
    }

    pub(crate) fn record_dense_include_entry_success(&mut self) {
        self.dense_include_entry_successes += 1;
    }

    pub(crate) fn record_dense_include_entry_fallback(&mut self, reason: &str, path: &str) {
        self.dense_include_entry_fallbacks += 1;
        *self
            .dense_include_entry_fallback_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
        *self
            .dense_include_entry_fallback_by_path
            .entry(path.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_dense_block_entry(&mut self, function: u32, block: u32) {
        self.dense_block_entries += 1;
        *self
            .dense_block_entry_scratch
            .entry((function, block))
            .or_default() += 1;
    }

    pub(crate) fn record_dense_branch(
        &mut self,
        function: u32,
        from_block: u32,
        to_block: u32,
        truthy: bool,
        fallthrough: bool,
    ) {
        self.dense_branch_executions += 1;
        if truthy {
            self.dense_branch_true += 1;
        } else {
            self.dense_branch_false += 1;
        }
        if fallthrough {
            self.dense_branch_fallthrough_chosen += 1;
        }
        *self
            .dense_branch_edge_scratch
            .entry((function, from_block, to_block))
            .or_default() += 1;
    }

    pub(crate) fn record_superinstruction_selection(
        &mut self,
        candidates: u64,
        candidates_by_kind: &BTreeMap<String, u64>,
        emitted_by_kind: &BTreeMap<String, u64>,
        skipped_by_reason: &BTreeMap<String, u64>,
    ) {
        self.superinstruction_candidates += candidates;
        for (kind, count) in candidates_by_kind {
            *self
                .superinstruction_candidates_by_kind
                .entry(kind.to_owned())
                .or_default() += *count;
        }
        for (kind, count) in emitted_by_kind {
            self.superinstructions_emitted += *count;
            *self
                .superinstructions_emitted_by_kind
                .entry(kind.to_owned())
                .or_default() += *count;
            *self
                .opcodes
                .entry(format!("superinstruction_emitted_{kind}"))
                .or_default() += *count;
        }
        for (reason, count) in skipped_by_reason {
            *self
                .superinstruction_skipped_by_reason
                .entry(reason.to_owned())
                .or_default() += *count;
        }
    }

    pub(crate) fn record_superinstruction_executed(&mut self, kind: &'static str) {
        *self.superinstruction_scratch.entry(kind).or_default() += 1;
    }

    /// Folds the allocation-free scratch accumulators into the public
    /// string-keyed maps. Must run before a counters snapshot is cloned or
    /// serialized; the fold is idempotent because scratch is drained.
    pub(crate) fn fold_scratch_counters(&mut self) {
        for (index, name) in IR_OPCODE_NAMES.iter().enumerate() {
            let count = std::mem::take(&mut self.ir_opcode_counts.0[index]);
            if count > 0 {
                *self.opcodes.entry((*name).to_owned()).or_default() += count;
            }
        }
        for opcode in std::mem::take(&mut self.dense_opcode_seen) {
            let count = std::mem::take(&mut self.dense_opcode_counts.0[opcode as usize]);
            if count == 0 {
                continue;
            }
            let name = opcode.as_str();
            *self.opcodes.entry(format!("bytecode_{name}")).or_default() += count;
            let family = bytecode_opcode_family(name).to_owned();
            *self
                .bytecode_executed_by_family
                .entry(family.clone())
                .or_default() += count;
            *self
                .dense_instruction_families_executed
                .entry(family)
                .or_default() += count;
        }
        for ((function, block), count) in std::mem::take(&mut self.dense_block_entry_scratch) {
            *self
                .dense_block_entry_counts
                .entry(format!("f{function}:b{block}"))
                .or_default() += count;
        }
        for ((function, from_block, to_block), count) in
            std::mem::take(&mut self.dense_branch_edge_scratch)
        {
            *self
                .dense_branch_edge_counts
                .entry(format!("f{function}:b{from_block}->b{to_block}"))
                .or_default() += count;
        }
        for (kind, count) in std::mem::take(&mut self.superinstruction_scratch) {
            *self
                .superinstructions_executed
                .entry(kind.to_owned())
                .or_default() += count;
        }
    }

    pub(crate) fn record_optimized_exit_snapshot(&mut self, tier: &str, reason: &str) {
        self.optimized_exit_snapshots_created += 1;
        *self
            .optimized_exits_by_reason
            .entry(format!("{tier}.{reason}"))
            .or_default() += 1;
    }

    pub(crate) fn record_optimized_exit_materialized(&mut self) {
        self.optimized_exit_snapshots_materialized += 1;
    }

    pub fn record_snapshot_rejection_by_missing_state_family(&mut self, family: &str) {
        *self
            .snapshot_rejection_by_missing_state_family
            .entry(family.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_fallback_resume_success(&mut self) {
        self.fallback_resume_successes += 1;
    }

    pub(crate) fn record_literal_intern(&mut self, hit: bool) {
        if hit {
            self.literal_intern_hits += 1;
        } else {
            self.literal_intern_misses += 1;
        }
    }

    pub(crate) fn record_string_concat_fast_path(&mut self, hit: bool) {
        if hit {
            self.string_concat_fast_path_hits += 1;
        } else {
            self.string_concat_fast_path_misses += 1;
        }
    }

    pub(crate) fn record_concat_prealloc_hit(&mut self) {
        self.concat_prealloc_hits += 1;
    }

    pub(crate) fn record_concat_fallback(&mut self, reason: &str) {
        *self
            .concat_fallback_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
        self.record_slow_path_call(&format!("concat.{reason}"));
    }

    pub(crate) fn record_packed_dim_fast_path(&mut self, hit: bool) {
        if hit {
            self.packed_dim_fast_path_hits += 1;
        } else {
            self.packed_dim_fast_path_misses += 1;
        }
    }

    pub(crate) fn record_array_packed_append_fast_path_hit(&mut self) {
        self.array_packed_append_fast_path_hits += 1;
        self.packed_append_fast_hits += 1;
        self.record_array_fast_path_hit("packed_append");
    }

    pub(crate) fn record_array_packed_read_fast_path_hit(&mut self) {
        self.array_packed_read_fast_path_hits += 1;
    }

    pub(crate) fn record_array_sequential_foreach_fast_path_hit(&mut self) {
        self.array_sequential_foreach_fast_path_hits += 1;
        self.packed_foreach_fast_hits += 1;
        self.record_array_fast_path_hit("packed_foreach_by_value");
    }

    pub(crate) fn record_cow_or_reference_fallback(&mut self) {
        self.cow_or_reference_fallbacks += 1;
        self.record_array_fast_path_fallback("cow_or_reference");
        self.record_fast_path_disabled_by_reference(AliasState::PropertyOrArrayDimReference);
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_packed_foreach_sum_fast_hit(&mut self) {
        self.packed_foreach_sum_fast_hits += 1;
        self.record_array_fast_path_hit("packed_int_sum");
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_packed_foreach_sum_layout_exit(&mut self) {
        self.packed_foreach_sum_layout_exits += 1;
        self.record_array_fast_path_fallback("layout_or_element");
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_packed_foreach_sum_overflow_exit(&mut self) {
        self.packed_foreach_sum_overflow_exits += 1;
        self.record_array_fast_path_fallback("overflow");
    }

    pub(crate) fn record_array_fast_path_fallback(&mut self, reason: &str) {
        *self
            .array_fast_path_fallback_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
        self.record_slow_path_call(&format!("array.{reason}"));
    }

    fn record_array_fast_path_hit(&mut self, family: &str) {
        *self
            .array_fast_path_hits_by_family
            .entry(family.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_array_shape_observed(&mut self, kind: PhpArrayShapeKind) {
        *self
            .array_shape_observed_by_kind
            .entry(kind.as_str().to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_record_shape_lookup_hit(&mut self) {
        self.record_shape_hits += 1;
        self.record_array_fast_path_hit("record_shape_fetch");
    }

    pub(crate) fn record_record_shape_lookup_miss(&mut self) {
        self.record_shape_misses += 1;
    }

    pub(crate) fn record_small_map_lookup_hit(&mut self) {
        self.small_map_hits += 1;
        self.record_array_fast_path_hit("small_map_lookup");
    }

    pub(crate) fn record_small_map_lookup_miss(&mut self) {
        self.small_map_misses += 1;
    }

    pub(crate) fn record_array_shape_lookup_fallback(
        &mut self,
        fallback: PhpArrayShapeLookupFallback,
    ) {
        match fallback {
            PhpArrayShapeLookupFallback::KeyCoercion => self.key_coercion_fallbacks += 1,
            PhpArrayShapeLookupFallback::OrderSemantics => self.order_semantics_fallbacks += 1,
            PhpArrayShapeLookupFallback::CowOrReference => self.record_cow_or_reference_fallback(),
            PhpArrayShapeLookupFallback::UnsupportedShape => {}
        }
        self.record_array_fast_path_fallback(fallback.as_str());
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_known_call_fast_hit(&mut self) {
        self.known_call_fast_hits += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_known_call_guard_exit(&mut self) {
        self.known_call_guard_exits += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_known_call_slow_call(&mut self) {
        self.known_call_slow_calls += 1;
        self.record_slow_path_call("jit.known_call");
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_property_load_fast_hit(&mut self) {
        self.property_load_fast_hits += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_property_load_guard_exit(&mut self) {
        self.property_load_guard_exits += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_property_load_layout_exit(&mut self) {
        self.property_load_layout_exits += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_property_load_uninitialized_exit(&mut self) {
        self.property_load_uninitialized_exits += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_property_load_slow_call(&mut self) {
        self.property_load_slow_calls += 1;
        self.record_slow_path_call("jit.property_load");
    }

    pub(crate) fn record_array_count_fast_path_hit(&mut self) {
        self.array_count_fast_path_hits += 1;
    }

    pub(crate) fn record_array_packed_to_mixed_transition(&mut self) {
        self.array_packed_to_mixed_transitions += 1;
    }

    pub(crate) fn record_numeric_string_cache_stats(
        &mut self,
        stats: php_runtime::numeric_string::NumericStringCacheStats,
    ) {
        self.numeric_string_classify_calls += stats.classify_calls;
        self.numeric_string_cache_hits += stats.hits;
        self.numeric_string_cache_misses += stats.misses;
        self.numeric_string_warning_sensitive_fallbacks += stats.warning_sensitive_fallbacks;
        self.numeric_string_overflow_precision_fallbacks += stats.overflow_precision_fallbacks;
    }

    pub(crate) fn record_numeric_string_specialization_hit(&mut self) {
        self.numeric_string_specialization_hits += 1;
    }

    pub(crate) fn record_typecheck_fast_path(&mut self, hit: bool) {
        if hit {
            self.typecheck_fast_path_hits += 1;
        } else {
            self.typecheck_fast_path_misses += 1;
        }
    }

    pub(crate) fn record_output_stats(&mut self, final_output_bytes: usize, stats: OutputStats) {
        self.output_bytes = final_output_bytes as u64;
        self.output_buffer_appends = stats.appends;
        self.output_buffer_batch_writes = stats.batch_writes;
        self.output_batched_appends = stats.batched_appends;
        self.output_batch_bytes = stats.batch_bytes;
        self.output_buffer_flushes = stats.flushes;
        self.output_fast_appends = stats.fast_appends;
        self.output_slow_appends_by_reason = stats.slow_appends_by_reason;
        let slow_appends = self.output_slow_appends_by_reason.clone();
        for (reason, count) in slow_appends {
            *self
                .slow_path_calls_by_reason
                .entry(format!("output.{reason}"))
                .or_default() += count;
        }
    }

    pub(crate) fn record_internal_function_dispatch(&mut self) {
        self.internal_function_dispatches += 1;
    }

    pub(crate) fn record_internal_function_dispatch_cache(&mut self, hit: bool) {
        if hit {
            self.internal_function_dispatch_cache_hits += 1;
        } else {
            self.internal_function_dispatch_cache_misses += 1;
        }
    }

    pub(crate) fn record_internal_count_array_direct_fast_path_hit(&mut self) {
        self.internal_count_array_direct_fast_path_hits += 1;
    }

    pub(crate) fn record_function_call_ic(&mut self, hit: bool) {
        if hit {
            self.function_call_ic_hits += 1;
        } else {
            self.function_call_ic_misses += 1;
        }
    }

    pub(crate) fn record_builtin_call_ic(&mut self, hit: bool) {
        if hit {
            self.builtin_call_ic_hits += 1;
        } else {
            self.builtin_call_ic_misses += 1;
        }
    }

    pub(crate) fn record_builtin_fast_stub(&mut self, name: &str, hit: bool) {
        let map = if hit {
            &mut self.builtin_fast_stub_hits
        } else {
            &mut self.builtin_fast_stub_misses
        };
        *map.entry(name.to_owned()).or_default() += 1;
    }

    pub(crate) fn record_builtin_fast_stub_fallback(&mut self, name: &str, reason: &str) {
        *self
            .builtin_fast_stub_fallback_by_reason
            .entry(format!("{name}.{reason}"))
            .or_default() += 1;
        self.record_slow_path_call(&format!("builtin_stub.{name}.{reason}"));
    }

    pub(crate) fn record_builtin_intrinsic_candidate(&mut self) {
        self.builtin_intrinsic_candidates += 1;
    }

    pub(crate) fn record_intrinsic(&mut self, name: &str, hit: bool) {
        let map = if hit {
            &mut self.intrinsic_hits
        } else {
            &mut self.intrinsic_misses
        };
        *map.entry(name.to_owned()).or_default() += 1;
    }

    pub(crate) fn record_intrinsic_fallback(&mut self, name: &str, reason: &str) {
        *self
            .intrinsic_fallback_by_reason
            .entry(format!("{name}.{reason}"))
            .or_default() += 1;
        self.record_slow_path_call(&format!("builtin_intrinsic.{name}.{reason}"));
    }

    pub(crate) fn record_json_encode_fast_path(&mut self, bytes: usize) {
        self.json_encode_fast_path_hits += 1;
        self.json_encode_fast_path_bytes += bytes as u64;
    }

    pub(crate) fn record_json_encode_generic_fallback(&mut self, reason: &str) {
        *self
            .json_encode_generic_fallback_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
        self.record_slow_path_call(&format!("json_encode_generic.{reason}"));
    }

    pub(crate) fn record_array_slice_packed_fast_hit(&mut self) {
        self.array_slice_packed_fast_hits += 1;
    }

    pub(crate) fn record_count_array_shape_fast_hit(&mut self) {
        self.count_array_shape_fast_hits += 1;
    }

    pub(crate) fn record_map_update_slot_fast_hit(&mut self) {
        self.map_update_slot_fast_hits += 1;
    }

    pub(crate) fn record_property_dim_assign_in_place_hit(&mut self) {
        self.property_dim_assign_in_place_hits += 1;
    }

    pub(crate) fn record_property_dim_assign_generic(&mut self, reason: &str) {
        *self
            .property_dim_assign_generic_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_property_dim_probe_borrowed_hit(&mut self) {
        self.property_dim_probe_borrowed_hits += 1;
    }

    pub(crate) fn record_cufa_argument_path(&mut self, owned: bool) {
        if owned {
            self.cufa_owned_argument_moves += 1;
        } else {
            self.cufa_shared_argument_clones += 1;
        }
    }

    pub(crate) fn record_array_builtin_fast_fallback(&mut self, builtin: &str, reason: &str) {
        *self
            .array_builtin_fast_fallback_by_reason
            .entry(format!("{builtin}.{reason}"))
            .or_default() += 1;
    }

    pub(crate) fn record_call_ic_megamorphic_fallback(&mut self) {
        self.call_ic_megamorphic_fallbacks += 1;
    }

    pub(crate) fn record_property_ic_fallback(&mut self, reason: &str) {
        *self
            .property_ic_fallback_reasons
            .entry(reason.to_owned())
            .or_default() += 1;
        self.record_slow_path_call(&format!("property_fetch.{reason}"));
    }

    pub(crate) fn record_property_assign_ic_fallback(&mut self, reason: &str) {
        *self
            .property_assign_ic_fallback_reasons
            .entry(reason.to_owned())
            .or_default() += 1;
        self.record_slow_path_call(&format!("property_assign.{reason}"));
        match reason {
            "layout_epoch_mismatch"
            | "receiver_class_mismatch"
            | "class_id_mismatch"
            | "declaring_class_missing"
            | "property_missing"
            | "property_slot_mismatch"
            | "storage_name_mismatch"
            | "static_or_protected_property" => {
                self.property_assign_ic_shape_exits += 1;
            }
            "visibility_mismatch" | "setter_visibility_mismatch" => {
                self.property_assign_ic_visibility_exits += 1;
            }
            "type_mismatch" => self.property_assign_ic_type_exits += 1,
            "readonly_property" | "readonly_initialized" | "readonly_metadata" => {
                self.property_assign_ic_readonly_exits += 1;
            }
            "property_hook_present"
            | "property_hook_active"
            | "property_hook_metadata"
            | "magic_set_metadata" => self.property_assign_ic_hook_magic_exits += 1,
            "reference_slot" | "reference_metadata" => {
                self.property_assign_ic_reference_exits += 1;
                self.record_ic_invalidated_by_reference(AliasState::PropertyOrArrayDimReference);
            }
            "dynamic_property_fallback" | "dynamic_property_metadata" => {
                self.property_assign_ic_dynamic_exits += 1;
            }
            _ => {}
        }
    }

    pub(crate) fn record_local_slot_fast_path(&mut self, hit: bool) {
        if hit {
            self.local_slot_fast_path_hits += 1;
        } else {
            self.local_slot_fast_path_misses += 1;
        }
    }

    pub(crate) fn record_quickening(&mut self, observation: crate::QuickeningObservation) {
        let family = observation
            .specialization
            .map(quickening_specialization_family);
        if observation.attempt {
            self.quickening_attempts += 1;
        }
        if (observation.attempt || observation.specialized)
            && let Some(family) = family
        {
            *self
                .quickening_candidates_by_family
                .entry(family.to_owned())
                .or_default() += 1;
        }
        if observation.specialized {
            self.quickening_specialized += 1;
            if let Some(family) = family {
                *self
                    .quickening_applied_by_family
                    .entry(family.to_owned())
                    .or_default() += 1;
            }
        }
        if observation.guard_hit {
            self.quickening_guard_hits += 1;
            if let Some(family) = family {
                *self
                    .quickened_executions_by_family
                    .entry(family.to_owned())
                    .or_default() += 1;
            }
        }
        if observation.guard_miss {
            self.quickening_guard_misses += 1;
        }
        if observation.guard_failure {
            self.quickening_guard_failures += 1;
            if let Some(family) = family {
                *self
                    .quickening_guard_failures_by_family
                    .entry(family.to_owned())
                    .or_default() += 1;
                self.record_optimized_exit_snapshot("quickening", family);
            } else {
                self.record_optimized_exit_snapshot("quickening", "guard_failure");
            }
            self.record_optimized_exit_materialized();
            self.record_fallback_resume_success();
        }
        if observation.fallback_call {
            self.quickening_fallback_calls += 1;
        }
        if observation.dequickened {
            self.quickening_dequickens += 1;
            *self
                .quickening_dequickened_by_reason
                .entry("guard_failure_threshold".to_owned())
                .or_default() += 1;
        }
        if observation.megamorphic {
            self.quickening_megamorphic += 1;
        }
        if observation.disabled {
            self.quickening_disabled += 1;
        }
    }

    pub(crate) fn record_adaptive_tiny_unit_setup_skip(&mut self) {
        self.adaptive_tiny_unit_setup_skips += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_jit_compile_attempt(&mut self) {
        self.jit_compile_attempts += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_jit_compiled(&mut self) {
        self.jit_compiled += 1;
        self.native_compiled_regions += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_jit_compile_metadata(&mut self, code_bytes: u64, compile_time_nanos: u64) {
        self.jit_code_bytes += code_bytes;
        self.jit_compile_time_nanos += compile_time_nanos;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_jit_compile_descriptor(&mut self, descriptor: JitCompileDescriptor) {
        self.jit_compile_descriptors.push(descriptor);
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_jit_executed(&mut self) {
        self.jit_executed += 1;
        self.native_executions += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_jit_bailout(&mut self) {
        self.jit_bailouts += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_jit_side_exit(&mut self, reason: &str) {
        self.jit_side_exits += 1;
        *self
            .jit_side_exit_reasons
            .entry(reason.to_owned())
            .or_default() += 1;
        self.record_native_side_exit(reason);
    }

    // Reserved by 07.CL.04; later guard work start calling it.
    #[allow(dead_code)]
    pub(crate) fn record_jit_guard_failure(&mut self) {
        self.jit_guard_failures += 1;
    }

    #[allow(dead_code)]
    pub(crate) fn record_jit_blacklisted_region(&mut self, reason: &str) {
        self.jit_blacklisted_regions += 1;
        *self
            .jit_blacklist_reasons
            .entry(reason.to_owned())
            .or_default() += 1;
        *self
            .native_blacklist_suppression_by_unstable_region
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_jit_tiering_cold_function(&mut self) {
        self.jit_tiering_cold_functions += 1;
    }

    pub(crate) fn record_jit_tiering_hot_function(&mut self) {
        self.jit_tiering_hot_functions += 1;
    }

    pub(crate) fn record_jit_tiering_eager_function(&mut self) {
        self.jit_tiering_eager_functions += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_jit_tiering_blacklist_rejection(&mut self) {
        self.jit_tiering_blacklist_rejections += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_jit_tiering_budget_rejection(&mut self) {
        self.jit_tiering_budget_rejections += 1;
        self.native_compile_budget_rejections += 1;
    }

    pub(crate) fn record_native_candidate(&mut self) {
        self.native_candidates += 1;
    }

    pub(crate) fn record_native_platform_unavailable(&mut self) {
        self.native_platform_unavailable += 1;
    }

    pub(crate) fn record_native_eligibility_rejection(&mut self, reason: &str) {
        *self
            .native_eligibility_rejections_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_native_side_exit(&mut self, reason: &str) {
        *self
            .native_side_exits_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    // Reserved by 07.CL.04; later helper-call work start calling it.
    #[allow(dead_code)]
    pub(crate) fn record_jit_helper_call(&mut self) {
        self.jit_helper_calls += 1;
    }

    #[allow(dead_code)]
    pub(crate) fn record_jit_fast_path_hit(&mut self) {
        self.jit_fast_path_hits += 1;
    }

    #[allow(dead_code)]
    pub(crate) fn record_record_lookup_fast_hit(&mut self) {
        self.record_lookup_fast_hits += 1;
    }

    #[cfg(feature = "jit-cranelift")]
    pub(crate) fn record_record_lookup_key_miss_exit(&mut self) {
        self.record_lookup_key_miss_exits += 1;
    }

    #[cfg(feature = "jit-cranelift")]
    pub(crate) fn record_record_lookup_layout_exit(&mut self) {
        self.record_lookup_layout_exits += 1;
    }

    pub(crate) fn record_packed_fetch_fast_hit(&mut self) {
        self.packed_fetch_fast_hits += 1;
        self.record_array_fast_path_hit("packed_int_fetch");
    }

    #[allow(dead_code)]
    pub(crate) fn record_packed_fetch_bounds_exit(&mut self) {
        self.packed_fetch_bounds_exits += 1;
        self.packed_fetch_bounds_fallbacks += 1;
        self.record_array_fast_path_fallback("bounds");
    }

    #[allow(dead_code)]
    pub(crate) fn record_packed_fetch_layout_exit(&mut self) {
        self.packed_fetch_layout_exits += 1;
        self.packed_fetch_layout_fallbacks += 1;
        self.record_array_fast_path_fallback("layout_or_key");
    }

    #[allow(dead_code)]
    pub(crate) fn record_jit_overflow_exit(&mut self) {
        self.jit_overflow_exits += 1;
    }

    #[allow(dead_code)]
    pub(crate) fn record_jit_slow_path_call(&mut self) {
        self.jit_slow_path_calls += 1;
        self.record_slow_path_call("jit.generic");
    }

    #[allow(dead_code)]
    pub(crate) fn record_direct_call_hit(&mut self) {
        self.direct_call_hits += 1;
        self.jit_fast_path_hits += 1;
    }

    #[allow(dead_code)]
    pub(crate) fn record_direct_call_fallback(&mut self) {
        self.direct_call_fallbacks += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_jit_compile_cache_hit(&mut self) {
        self.jit_compile_cache_hits += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_jit_compile_cache_miss(&mut self) {
        self.jit_compile_cache_misses += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_jit_compile_cache_invalidation(&mut self) {
        self.jit_compile_cache_invalidations += 1;
    }

    pub(crate) fn record_inline_cache(&mut self, observation: InlineCacheObservation) {
        if observation.candidate {
            self.inline_cache_observations += 1;
        }
        if observation.hit {
            self.inline_cache_hits += 1;
            if observation.kind == Some(InlineCacheKind::MethodCall) {
                self.method_ic_hits += 1;
                if observation.polymorphic {
                    self.method_ic_polymorphic_hits += 1;
                }
            }
            if observation.kind == Some(InlineCacheKind::PropertyFetch) {
                self.property_ic_hits += 1;
            }
            if observation.kind == Some(InlineCacheKind::PropertyAssign) {
                self.property_assign_ic_hits += 1;
            }
            if observation.kind == Some(InlineCacheKind::ClassConstantStaticProperty) {
                self.class_static_ic_hits += 1;
            }
            if observation.kind == Some(InlineCacheKind::ClassRelation) {
                self.class_relation_cache_hits += 1;
            }
            if observation.kind == Some(InlineCacheKind::IncludePath) {
                self.include_path_ic_hits += 1;
                self.record_include_resolution_hit();
                self.record_include_graph_hit();
            }
            if observation.kind == Some(InlineCacheKind::AutoloadClassLookup) {
                self.autoload_class_lookup_ic_hits += 1;
                self.record_autoload_graph_hit();
            }
        }
        if observation.miss {
            self.inline_cache_misses += 1;
            if observation.kind == Some(InlineCacheKind::MethodCall) {
                self.method_ic_misses += 1;
            }
            if observation.kind == Some(InlineCacheKind::PropertyFetch) {
                self.property_ic_misses += 1;
            }
            if observation.kind == Some(InlineCacheKind::PropertyAssign) {
                self.property_assign_ic_misses += 1;
            }
            if observation.kind == Some(InlineCacheKind::ClassConstantStaticProperty) {
                self.class_static_ic_misses += 1;
            }
            if observation.kind == Some(InlineCacheKind::ClassRelation) {
                self.class_relation_cache_misses += 1;
            }
            if observation.kind == Some(InlineCacheKind::IncludePath) {
                self.include_path_ic_misses += 1;
                self.record_include_resolution_miss();
                self.record_include_graph_miss();
            }
            if observation.kind == Some(InlineCacheKind::AutoloadClassLookup) {
                self.autoload_class_lookup_ic_misses += 1;
                self.record_autoload_graph_miss();
            }
        }
        if observation.invalidation {
            self.inline_cache_invalidations += 1;
            if observation.kind == Some(InlineCacheKind::IncludePath) {
                self.include_path_ic_invalidations += 1;
                self.record_invalidation_by_reason("include_path_epoch_or_guard");
            }
            if observation.kind == Some(InlineCacheKind::AutoloadClassLookup) {
                self.autoload_class_lookup_ic_invalidations += 1;
                self.record_invalidation_by_reason("autoload_lookup_epoch_or_guard");
            }
            if observation.kind == Some(InlineCacheKind::ClassRelation) {
                self.class_relation_cache_invalidations += 1;
                self.record_invalidation_by_reason("class_relation_epoch_or_guard");
            }
        }
        if observation.guard_failure {
            self.inline_cache_guard_failures += 1;
            self.record_optimized_exit_snapshot(
                "inline_cache",
                observation
                    .kind
                    .map(InlineCacheKind::counter_name)
                    .unwrap_or("guard_failure"),
            );
            self.record_optimized_exit_materialized();
            self.record_fallback_resume_success();
            if observation.kind == Some(InlineCacheKind::MethodCall) {
                self.method_ic_guard_failures += 1;
            }
            if observation.kind == Some(InlineCacheKind::PropertyFetch) {
                self.property_ic_guard_failures += 1;
            }
            if observation.kind == Some(InlineCacheKind::PropertyAssign) {
                self.property_assign_ic_guard_failures += 1;
            }
            if observation.kind == Some(InlineCacheKind::ClassConstantStaticProperty) {
                self.class_static_ic_guard_failures += 1;
            }
            if observation.kind == Some(InlineCacheKind::IncludePath) {
                self.include_path_ic_guard_failures += 1;
            }
            if observation.kind == Some(InlineCacheKind::AutoloadClassLookup) {
                self.autoload_class_lookup_ic_guard_failures += 1;
            }
        }
        if observation.fallback_call {
            self.inline_cache_fallback_calls += 1;
        }
        if observation.monomorphic {
            self.inline_cache_monomorphic += 1;
        }
        if observation.polymorphic {
            self.inline_cache_polymorphic += 1;
        }
        if observation.megamorphic {
            self.inline_cache_megamorphic += 1;
        }
        if observation.disabled {
            self.inline_cache_disabled += 1;
        }
        if !observation.slot_allocated {
            return;
        }
        self.inline_cache_slots += 1;
        match observation.kind {
            Some(InlineCacheKind::FunctionCall) => self.inline_cache_function_slots += 1,
            Some(InlineCacheKind::MethodCall) => self.inline_cache_method_slots += 1,
            Some(InlineCacheKind::PropertyFetch) => self.inline_cache_property_slots += 1,
            Some(InlineCacheKind::PropertyAssign) => {
                self.inline_cache_property_assign_slots += 1;
            }
            Some(InlineCacheKind::DimFetch) => self.inline_cache_dim_slots += 1,
            Some(InlineCacheKind::ClassConstantStaticProperty) => {
                self.inline_cache_class_constant_static_property_slots += 1;
            }
            Some(InlineCacheKind::ClassRelation) => self.inline_cache_class_relation_slots += 1,
            Some(InlineCacheKind::IncludePath) => self.inline_cache_include_path_slots += 1,
            Some(InlineCacheKind::AutoloadClassLookup) => {
                self.inline_cache_autoload_class_lookup_slots += 1;
            }
            None => {}
        }
    }

    pub(crate) fn record_method_direct_dispatch_hit(&mut self) {
        self.method_direct_dispatch_hits += 1;
    }

    pub(crate) fn record_method_direct_dispatch_fallback(&mut self) {
        self.method_direct_dispatch_fallbacks += 1;
    }

    pub(crate) fn record_method_tiny_inline_candidate(&mut self) {
        self.method_tiny_inline_candidates += 1;
    }

    pub(crate) fn record_method_tiny_inline_rejection(&mut self, reason: &str) {
        *self
            .method_tiny_inline_rejected_by_reason
            .entry(reason.to_owned())
            .or_default() += 1;
    }

    pub(crate) fn record_class_relation_cache_hit(&mut self) {
        self.class_relation_cache_hits += 1;
    }

    pub(crate) fn record_class_relation_cache_miss(&mut self) {
        self.class_relation_cache_misses += 1;
    }

    pub(crate) fn record_class_relation_cache_invalidation(&mut self) {
        self.class_relation_cache_invalidations += 1;
        self.record_invalidation_by_reason("class_relation_epoch_or_guard");
    }

    pub(crate) fn record_instanceof_cache_hit(&mut self) {
        self.instanceof_cache_hits += 1;
    }

    pub(crate) fn record_instanceof_cache_miss(&mut self) {
        self.instanceof_cache_misses += 1;
    }

    pub(crate) fn record_method_override_cache_hit(&mut self) {
        self.method_override_cache_hits += 1;
    }

    pub(crate) fn record_method_override_cache_miss(&mut self) {
        self.method_override_cache_misses += 1;
    }

    pub(crate) fn record_property_fetch_profile(
        &mut self,
        observation: PropertyFetchProfileObservation,
    ) {
        let profile = self
            .property_fetch_profiles
            .entry(observation.callsite.clone())
            .or_insert_with(|| PropertyFetchProfile {
                callsite: observation.callsite.clone(),
                property: observation.property.clone(),
                ..PropertyFetchProfile::default()
            });
        profile.observations = profile.observations.saturating_add(1);
        profile.receiver_classes.insert(observation.receiver_class);
        profile.class_ids.insert(observation.class_id);
        if let Some(name) = observation.declared_property_name {
            profile.declared_property_names.insert(name);
        } else {
            profile
                .non_eligible_reasons
                .insert("missing_declared_property".to_owned());
        }
        profile.visibility_contexts.insert(
            observation
                .visibility_context
                .unwrap_or_else(|| "global".to_owned()),
        );
        if let Some(slot) = observation.property_slot_index {
            profile.property_slot_indexes.insert(slot);
        }
        profile
            .class_layout_versions
            .insert(observation.class_layout_version);
        profile.has_magic_get |= observation.has_magic_get;
        profile.has_property_hook |= observation.has_property_hook;
        profile.dynamic_property_fallback |= observation.dynamic_property_fallback;
        profile.saw_declared_visible_property |= observation.declared_visible_property;
        profile.saw_uninitialized_typed_property |= observation.uninitialized_typed_property;
        for reason in observation.non_eligible_reasons {
            profile.non_eligible_reasons.insert(reason.to_owned());
        }
        if observation.dynamic_property_fallback {
            profile
                .non_eligible_reasons
                .insert("dynamic_property_fallback".to_owned());
        }
        if observation.has_magic_get {
            profile
                .non_eligible_reasons
                .insert("magic_get_present".to_owned());
        }
        if observation.has_property_hook {
            profile
                .non_eligible_reasons
                .insert("property_hook_present".to_owned());
        }
        if observation.uninitialized_typed_property {
            profile
                .non_eligible_reasons
                .insert("uninitialized_typed_property".to_owned());
        }
        if !observation.declared_visible_property {
            profile
                .non_eligible_reasons
                .insert("not_visible".to_owned());
        }
    }

    pub(crate) fn record_method_call_profile(&mut self, observation: MethodCallProfileObservation) {
        let profile = self
            .method_call_profiles
            .entry(observation.callsite.clone())
            .or_insert_with(|| MethodCallProfile {
                callsite: observation.callsite.clone(),
                method: observation.method.clone(),
                simple_positional_arguments: true,
                ..MethodCallProfile::default()
            });
        profile.observations = profile.observations.saturating_add(1);
        profile.receiver_classes.insert(observation.receiver_class);
        profile.class_ids.insert(observation.class_id);
        if let Some(declaring_class) = observation.declaring_class {
            profile.declaring_classes.insert(declaring_class);
        } else {
            profile
                .non_eligible_reasons
                .insert("missing_declared_method".to_owned());
        }
        if let Some(method_id) = observation.method_id {
            profile.method_ids.insert(method_id);
        }
        if let Some(slot) = observation.method_slot_index {
            profile.method_slot_indexes.insert(slot);
        }
        profile.visibility_contexts.insert(
            observation
                .visibility_context
                .unwrap_or_else(|| "global".to_owned()),
        );
        profile
            .override_layout_versions
            .insert(observation.override_layout_version);
        profile.saw_final_method |= observation.method_is_final;
        profile.saw_private_method |= observation.method_is_private;
        profile.saw_static_method |= observation.method_is_static;
        profile.has_magic_call |= observation.has_magic_call;
        profile.magic_call_fallback |= observation.magic_call_fallback;
        profile.simple_positional_arguments &= observation.simple_positional_arguments;
        profile.saw_by_ref_argument |= observation.has_by_ref_argument;
        profile.saw_callee_jit_eligible |= observation.callee_jit_eligible;
        profile.saw_direct_vm_call_helper |= observation.direct_vm_call_helper_available;
        for reason in observation.non_eligible_reasons {
            profile.non_eligible_reasons.insert(reason.to_owned());
        }
        if observation.magic_call_fallback {
            profile
                .non_eligible_reasons
                .insert("magic_call_fallback".to_owned());
        }
        if observation.has_by_ref_argument {
            profile
                .non_eligible_reasons
                .insert("by_ref_argument".to_owned());
        }
        if !observation.simple_positional_arguments {
            profile
                .non_eligible_reasons
                .insert("non_positional_argument".to_owned());
        }
        if observation.method_is_static {
            profile
                .non_eligible_reasons
                .insert("static_method".to_owned());
        }
    }

    /// Serializes counters as stable JSON without adding serde to the VM crate.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut json = String::new();
        json.push_str("{\n");
        push_field(&mut json, "schema_version", 1, true);
        push_string_field(
            &mut json,
            "jit_mode",
            if self.jit_mode.is_empty() {
                "off"
            } else {
                &self.jit_mode
            },
            true,
        );
        push_field(&mut json, "jit_threshold", self.jit_threshold, true);
        push_field(
            &mut json,
            "instructions_executed",
            self.instructions_executed,
            true,
        );
        push_field(
            &mut json,
            "bytecode_lower_attempts",
            self.bytecode_lower_attempts,
            true,
        );
        push_field(
            &mut json,
            "bytecode_lower_successes",
            self.bytecode_lower_successes,
            true,
        );
        push_field(
            &mut json,
            "dense_execution_plan_cache_hits",
            self.dense_execution_plan_cache_hits,
            true,
        );
        push_field(
            &mut json,
            "dense_execution_plan_cache_misses",
            self.dense_execution_plan_cache_misses,
            true,
        );
        push_field(
            &mut json,
            "bytecode_unsupported_fallbacks",
            self.bytecode_unsupported_fallbacks,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "bytecode_unsupported_reasons",
            &self.bytecode_unsupported_reasons,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "bytecode_auto_fallback_reasons",
            &self.bytecode_auto_fallback_reasons,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "bytecode_lowered_by_family",
            &self.bytecode_lowered_by_family,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "bytecode_executed_by_family",
            &self.bytecode_executed_by_family,
            true,
        );
        push_field(
            &mut json,
            "bytecode_instructions_executed",
            self.bytecode_instructions_executed,
            true,
        );
        push_field(
            &mut json,
            "entry_rich_instructions_executed",
            self.entry_rich_instructions_executed,
            true,
        );
        push_field(
            &mut json,
            "include_rich_instructions_executed",
            self.include_rich_instructions_executed,
            true,
        );
        push_field(
            &mut json,
            "entry_bytecode_instructions_executed",
            self.entry_bytecode_instructions_executed,
            true,
        );
        push_field(
            &mut json,
            "include_bytecode_instructions_executed",
            self.include_bytecode_instructions_executed,
            true,
        );
        push_field(
            &mut json,
            "dense_include_entry_attempts",
            self.dense_include_entry_attempts,
            true,
        );
        push_field(
            &mut json,
            "dense_include_entry_successes",
            self.dense_include_entry_successes,
            true,
        );
        push_field(
            &mut json,
            "dense_include_entry_fallbacks",
            self.dense_include_entry_fallbacks,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "dense_include_entry_fallback_by_reason",
            &self.dense_include_entry_fallback_by_reason,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "dense_include_entry_fallback_by_path",
            &self.dense_include_entry_fallback_by_path,
            true,
        );
        push_field(
            &mut json,
            "dense_functions_planned",
            self.dense_functions_planned,
            true,
        );
        push_field(
            &mut json,
            "dense_functions_executed",
            self.dense_functions_executed,
            true,
        );
        push_field(
            &mut json,
            "rich_fallback_functions_planned",
            self.rich_fallback_functions_planned,
            true,
        );
        push_field(
            &mut json,
            "rich_fallback_functions_executed",
            self.rich_fallback_functions_executed,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "dense_function_fallback_by_reason",
            &self.dense_function_fallback_by_reason,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "rich_fallback_functions_by_name",
            &self.rich_fallback_functions_by_name,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "dense_instruction_families_executed",
            &self.dense_instruction_families_executed,
            true,
        );
        push_field(
            &mut json,
            "dense_property_fetch_hits",
            self.dense_property_fetch_hits,
            true,
        );
        push_field(
            &mut json,
            "dense_property_assignment_hits",
            self.dense_property_assignment_hits,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "dense_property_fallback_by_reason",
            &self.dense_property_fallback_by_reason,
            true,
        );
        push_field(
            &mut json,
            "dense_property_ic_reuse",
            self.dense_property_ic_reuse,
            true,
        );
        push_field(
            &mut json,
            "dense_direct_call_hits",
            self.dense_direct_call_hits,
            true,
        );
        push_field(
            &mut json,
            "dense_method_call_hits",
            self.dense_method_call_hits,
            true,
        );
        push_field(
            &mut json,
            "dense_static_call_hits",
            self.dense_static_call_hits,
            true,
        );
        push_field(
            &mut json,
            "dense_callable_call_hits",
            self.dense_callable_call_hits,
            true,
        );
        push_field(
            &mut json,
            "dense_call_ic_hits",
            self.dense_call_ic_hits,
            true,
        );
        push_field(
            &mut json,
            "dense_call_ic_misses",
            self.dense_call_ic_misses,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "dense_call_fallback_by_reason",
            &self.dense_call_fallback_by_reason,
            true,
        );
        push_field(
            &mut json,
            "dense_branch_executions",
            self.dense_branch_executions,
            true,
        );
        push_field(&mut json, "dense_branch_true", self.dense_branch_true, true);
        push_field(
            &mut json,
            "dense_branch_false",
            self.dense_branch_false,
            true,
        );
        push_field(
            &mut json,
            "dense_branch_fallthrough_chosen",
            self.dense_branch_fallthrough_chosen,
            true,
        );
        push_field(
            &mut json,
            "dense_block_entries",
            self.dense_block_entries,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "dense_block_entry_counts",
            &self.dense_block_entry_counts,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "dense_branch_edge_counts",
            &self.dense_branch_edge_counts,
            true,
        );
        push_field(
            &mut json,
            "superinstruction_candidates",
            self.superinstruction_candidates,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "superinstruction_candidates_by_kind",
            &self.superinstruction_candidates_by_kind,
            true,
        );
        push_field(
            &mut json,
            "superinstructions_emitted",
            self.superinstructions_emitted,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "superinstructions_emitted_by_kind",
            &self.superinstructions_emitted_by_kind,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "superinstructions_executed",
            &self.superinstructions_executed,
            true,
        );
        push_field(
            &mut json,
            "superinstruction_deopt_or_fallbacks",
            self.superinstruction_deopt_or_fallbacks,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "superinstruction_deopt_or_fallback_by_reason",
            &self.superinstruction_deopt_or_fallback_by_reason,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "superinstruction_skipped_by_reason",
            &self.superinstruction_skipped_by_reason,
            true,
        );
        push_field(
            &mut json,
            "optimized_exit_snapshots_created",
            self.optimized_exit_snapshots_created,
            true,
        );
        push_field(
            &mut json,
            "optimized_exit_snapshots_materialized",
            self.optimized_exit_snapshots_materialized,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "optimized_exits_by_reason",
            &self.optimized_exits_by_reason,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "snapshot_rejection_by_missing_state_family",
            &self.snapshot_rejection_by_missing_state_family,
            true,
        );
        push_field(
            &mut json,
            "fallback_resume_successes",
            self.fallback_resume_successes,
            true,
        );
        json.push_str("  \"opcodes\": {");
        if self.opcodes.is_empty() {
            json.push('}');
        } else {
            json.push('\n');
            for (index, (name, count)) in self.opcodes.iter().enumerate() {
                json.push_str("    ");
                json.push('"');
                json.push_str(&escape_json(name));
                json.push_str("\": ");
                json.push_str(&count.to_string());
                if index + 1 != self.opcodes.len() {
                    json.push(',');
                }
                json.push('\n');
            }
            json.push_str("  }");
        }
        json.push_str(",\n");
        push_field(&mut json, "function_calls", self.function_calls, true);
        push_field(&mut json, "method_calls", self.method_calls, true);
        push_field(&mut json, "frame_allocations", self.frame_allocations, true);
        push_field(&mut json, "frame_reuses", self.frame_reuses, true);
        push_field(&mut json, "frames_allocated", self.frames_allocated, true);
        push_field(&mut json, "frames_reused", self.frames_reused, true);
        push_field(
            &mut json,
            "register_files_allocated",
            self.register_files_allocated,
            true,
        );
        push_field(
            &mut json,
            "register_files_reused",
            self.register_files_reused,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "frame_reuse_blocked_by_reason",
            &self.frame_reuse_blocked_by_reason,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "call_frame_layout_observed",
            &self.call_frame_layout_observed,
            true,
        );
        push_field(
            &mut json,
            "tiny_frame_candidates",
            self.tiny_frame_candidates,
            true,
        );
        push_field(
            &mut json,
            "specialized_frame_hits",
            self.specialized_frame_hits,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "generic_frame_fallback_by_reason",
            &self.generic_frame_fallback_by_reason,
            true,
        );
        push_field(&mut json, "arg_array_avoided", self.arg_array_avoided, true);
        push_field(
            &mut json,
            "heap_frame_avoided",
            self.heap_frame_avoided,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "frame_alias_state",
            &self.frame_alias_state,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "alias_state_transitions",
            &self.alias_state_transitions,
            true,
        );
        push_field(
            &mut json,
            "fast_path_disabled_by_reference",
            self.fast_path_disabled_by_reference,
            true,
        );
        push_field(
            &mut json,
            "dequickened_by_reference",
            self.dequickened_by_reference,
            true,
        );
        push_field(
            &mut json,
            "IC_invalidated_by_reference",
            self.ic_invalidated_by_reference,
            true,
        );
        push_field(
            &mut json,
            "dense_bytecode_fallback_by_reference",
            self.dense_bytecode_fallback_by_reference,
            true,
        );
        push_field(
            &mut json,
            "request_arena_allocations",
            self.request_arena_allocations,
            true,
        );
        push_field(
            &mut json,
            "request_arena_bytes",
            self.request_arena_bytes,
            true,
        );
        push_field(
            &mut json,
            "request_pool_resets",
            self.request_pool_resets,
            true,
        );
        push_field(
            &mut json,
            "persistent_engine_allocations",
            self.persistent_engine_allocations,
            true,
        );
        push_field(
            &mut json,
            "persistent_engine_bytes",
            self.persistent_engine_bytes,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "arena_fallback_allocations_by_reason",
            &self.arena_fallback_allocations_by_reason,
            true,
        );
        push_field(
            &mut json,
            "destructor_sensitive_arena_blocks",
            self.destructor_sensitive_arena_blocks,
            true,
        );
        push_field(&mut json, "value_clones", self.value_clones, true);
        push_string_u64_map_field(
            &mut json,
            "value_clone_by_source_family",
            &self.value_clone_by_source_family,
            true,
        );
        push_field(
            &mut json,
            "string_allocations",
            self.string_allocations,
            true,
        );
        push_field(
            &mut json,
            "array_handle_clones",
            self.array_handle_clones,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "array_handle_clone_by_source_family",
            &self.array_handle_clone_by_source_family,
            true,
        );
        push_field(&mut json, "cow_separations", self.cow_separations, true);
        push_field(
            &mut json,
            "last_use_array_read_releases",
            self.last_use_array_read_releases,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "cow_separation_by_source_family",
            &self.cow_separation_by_source_family,
            true,
        );
        push_field(
            &mut json,
            "reference_cell_creations",
            self.reference_cell_creations,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "reference_cell_creation_by_source_family",
            &self.reference_cell_creation_by_source_family,
            true,
        );
        push_field(
            &mut json,
            "object_allocations",
            self.object_allocations,
            true,
        );
        push_field(
            &mut json,
            "array_packed_direct_gets",
            self.array_packed_direct_gets,
            true,
        );
        push_field(
            &mut json,
            "array_mixed_indexed_gets",
            self.array_mixed_indexed_gets,
            true,
        );
        push_field(
            &mut json,
            "array_linear_scan_fallbacks",
            self.array_linear_scan_fallbacks,
            true,
        );
        push_field(
            &mut json,
            "array_metadata_recomputes",
            self.array_metadata_recomputes,
            true,
        );
        push_field(
            &mut json,
            "symbol_map_lookups",
            self.symbol_map_lookups,
            true,
        );
        push_field(
            &mut json,
            "symbol_linear_fallbacks",
            self.symbol_linear_fallbacks,
            true,
        );
        push_field(
            &mut json,
            "symbol_intern_hits",
            self.symbol_intern_hits,
            true,
        );
        push_field(
            &mut json,
            "symbol_intern_misses",
            self.symbol_intern_misses,
            true,
        );
        push_field(
            &mut json,
            "string_hash_cache_hits",
            self.string_hash_cache_hits,
            true,
        );
        push_field(
            &mut json,
            "string_hash_cache_misses",
            self.string_hash_cache_misses,
            true,
        );
        push_field(
            &mut json,
            "symbol_eq_fast_hits",
            self.symbol_eq_fast_hits,
            true,
        );
        push_field(
            &mut json,
            "symbol_eq_byte_fallbacks",
            self.symbol_eq_byte_fallbacks,
            true,
        );
        push_field(
            &mut json,
            "object_declared_slot_reads",
            self.object_declared_slot_reads,
            true,
        );
        push_field(
            &mut json,
            "object_declared_slot_writes",
            self.object_declared_slot_writes,
            true,
        );
        push_field(
            &mut json,
            "object_dynamic_property_map_reads",
            self.object_dynamic_property_map_reads,
            true,
        );
        push_field(
            &mut json,
            "object_dynamic_property_map_writes",
            self.object_dynamic_property_map_writes,
            true,
        );
        push_field(
            &mut json,
            "packed_values_storage_arrays",
            self.packed_values_storage_arrays,
            true,
        );
        push_field(
            &mut json,
            "packed_values_storage_reads",
            self.packed_values_storage_reads,
            true,
        );
        push_field(
            &mut json,
            "packed_values_storage_appends",
            self.packed_values_storage_appends,
            true,
        );
        push_field(
            &mut json,
            "packed_virtual_key_iterations",
            self.packed_virtual_key_iterations,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "packed_to_mixed_by_reason",
            &self.packed_to_mixed_by_reason,
            true,
        );
        push_field(
            &mut json,
            "record_storage_arrays",
            self.record_storage_arrays,
            true,
        );
        push_field(&mut json, "record_slot_reads", self.record_slot_reads, true);
        push_field(
            &mut json,
            "record_slot_writes",
            self.record_slot_writes,
            true,
        );
        push_field(
            &mut json,
            "record_shape_promotions",
            self.record_shape_promotions,
            true,
        );
        push_field(
            &mut json,
            "record_key_symbol_hits",
            self.record_key_symbol_hits,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "record_to_mixed_by_reason",
            &self.record_to_mixed_by_reason,
            true,
        );
        push_field(
            &mut json,
            "foreach_no_clone_hits",
            self.foreach_no_clone_hits,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "foreach_clone_required_by_reason",
            &self.foreach_clone_required_by_reason,
            true,
        );
        push_field(
            &mut json,
            "array_read_borrow_hits",
            self.array_read_borrow_hits,
            true,
        );
        push_field(
            &mut json,
            "direct_arg_frame_hits",
            self.direct_arg_frame_hits,
            true,
        );
        push_field(
            &mut json,
            "direct_method_frame_hits",
            self.direct_method_frame_hits,
            true,
        );
        push_field(
            &mut json,
            "direct_closure_frame_hits",
            self.direct_closure_frame_hits,
            true,
        );
        push_field(
            &mut json,
            "direct_constructor_frame_hits",
            self.direct_constructor_frame_hits,
            true,
        );
        push_field(
            &mut json,
            "argument_vector_allocations_avoided",
            self.argument_vector_allocations_avoided,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "direct_frame_fallback_by_reason",
            &self.direct_frame_fallback_by_reason,
            true,
        );
        push_field(
            &mut json,
            "symbolized_call_name_hits",
            self.symbolized_call_name_hits,
            true,
        );
        push_field(
            &mut json,
            "symbolized_method_name_hits",
            self.symbolized_method_name_hits,
            true,
        );
        push_field(
            &mut json,
            "symbolized_property_name_hits",
            self.symbolized_property_name_hits,
            true,
        );
        push_field(
            &mut json,
            "symbolized_array_key_hits",
            self.symbolized_array_key_hits,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "symbolized_name_fallbacks_by_reason",
            &self.symbolized_name_fallbacks_by_reason,
            true,
        );
        push_field(&mut json, "array_dim_fetches", self.array_dim_fetches, true);
        push_field(
            &mut json,
            "packed_dim_fast_path_hits",
            self.packed_dim_fast_path_hits,
            true,
        );
        push_field(
            &mut json,
            "packed_dim_fast_path_misses",
            self.packed_dim_fast_path_misses,
            true,
        );
        push_field(
            &mut json,
            "array_packed_append_fast_path_hits",
            self.array_packed_append_fast_path_hits,
            true,
        );
        push_field(
            &mut json,
            "array_packed_read_fast_path_hits",
            self.array_packed_read_fast_path_hits,
            true,
        );
        push_field(
            &mut json,
            "array_sequential_foreach_fast_path_hits",
            self.array_sequential_foreach_fast_path_hits,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "array_fast_path_hits_by_family",
            &self.array_fast_path_hits_by_family,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "array_fast_path_fallback_by_reason",
            &self.array_fast_path_fallback_by_reason,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "array_shape_observed_by_kind",
            &self.array_shape_observed_by_kind,
            true,
        );
        push_field(&mut json, "record_shape_hits", self.record_shape_hits, true);
        push_field(
            &mut json,
            "record_shape_misses",
            self.record_shape_misses,
            true,
        );
        push_field(&mut json, "small_map_hits", self.small_map_hits, true);
        push_field(&mut json, "small_map_misses", self.small_map_misses, true);
        push_field(
            &mut json,
            "key_coercion_fallbacks",
            self.key_coercion_fallbacks,
            true,
        );
        push_field(
            &mut json,
            "order_semantics_fallbacks",
            self.order_semantics_fallbacks,
            true,
        );
        push_field(
            &mut json,
            "packed_append_fast_hits",
            self.packed_append_fast_hits,
            true,
        );
        push_field(
            &mut json,
            "packed_foreach_fast_hits",
            self.packed_foreach_fast_hits,
            true,
        );
        push_field(
            &mut json,
            "cow_or_reference_fallbacks",
            self.cow_or_reference_fallbacks,
            true,
        );
        push_field(
            &mut json,
            "array_count_fast_path_hits",
            self.array_count_fast_path_hits,
            true,
        );
        push_field(
            &mut json,
            "array_packed_to_mixed_transitions",
            self.array_packed_to_mixed_transitions,
            true,
        );
        push_field(
            &mut json,
            "numeric_string_classify_calls",
            self.numeric_string_classify_calls,
            true,
        );
        push_field(
            &mut json,
            "numeric_string_cache_hits",
            self.numeric_string_cache_hits,
            true,
        );
        push_field(
            &mut json,
            "numeric_string_cache_misses",
            self.numeric_string_cache_misses,
            true,
        );
        push_field(
            &mut json,
            "numeric_string_specialization_hits",
            self.numeric_string_specialization_hits,
            true,
        );
        push_field(
            &mut json,
            "numeric_string_warning_sensitive_fallbacks",
            self.numeric_string_warning_sensitive_fallbacks,
            true,
        );
        push_field(
            &mut json,
            "numeric_string_overflow_precision_fallbacks",
            self.numeric_string_overflow_precision_fallbacks,
            true,
        );
        push_field(
            &mut json,
            "typecheck_fast_path_hits",
            self.typecheck_fast_path_hits,
            true,
        );
        push_field(
            &mut json,
            "typecheck_fast_path_misses",
            self.typecheck_fast_path_misses,
            true,
        );
        push_field(&mut json, "output_bytes", self.output_bytes, true);
        push_field(
            &mut json,
            "output_buffer_appends",
            self.output_buffer_appends,
            true,
        );
        push_field(
            &mut json,
            "output_buffer_batch_writes",
            self.output_buffer_batch_writes,
            true,
        );
        push_field(
            &mut json,
            "output_batched_appends",
            self.output_batched_appends,
            true,
        );
        push_field(
            &mut json,
            "output_batch_bytes",
            self.output_batch_bytes,
            true,
        );
        push_field(
            &mut json,
            "output_buffer_flushes",
            self.output_buffer_flushes,
            true,
        );
        push_field(
            &mut json,
            "output_fast_appends",
            self.output_fast_appends,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "output_slow_appends_by_reason",
            &self.output_slow_appends_by_reason,
            true,
        );
        push_field(
            &mut json,
            "internal_function_dispatches",
            self.internal_function_dispatches,
            true,
        );
        push_field(
            &mut json,
            "internal_function_dispatch_cache_hits",
            self.internal_function_dispatch_cache_hits,
            true,
        );
        push_field(
            &mut json,
            "internal_function_dispatch_cache_misses",
            self.internal_function_dispatch_cache_misses,
            true,
        );
        push_field(
            &mut json,
            "internal_count_array_direct_fast_path_hits",
            self.internal_count_array_direct_fast_path_hits,
            true,
        );
        push_field(
            &mut json,
            "function_call_ic_hits",
            self.function_call_ic_hits,
            true,
        );
        push_field(
            &mut json,
            "function_call_ic_misses",
            self.function_call_ic_misses,
            true,
        );
        push_field(
            &mut json,
            "builtin_call_ic_hits",
            self.builtin_call_ic_hits,
            true,
        );
        push_field(
            &mut json,
            "builtin_call_ic_misses",
            self.builtin_call_ic_misses,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "builtin_fast_stub_hits",
            &self.builtin_fast_stub_hits,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "builtin_fast_stub_misses",
            &self.builtin_fast_stub_misses,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "builtin_fast_stub_fallback_by_reason",
            &self.builtin_fast_stub_fallback_by_reason,
            true,
        );
        push_field(
            &mut json,
            "builtin_intrinsic_candidates",
            self.builtin_intrinsic_candidates,
            true,
        );
        push_string_u64_map_field(&mut json, "intrinsic_hits", &self.intrinsic_hits, true);
        push_string_u64_map_field(&mut json, "intrinsic_misses", &self.intrinsic_misses, true);
        push_string_u64_map_field(
            &mut json,
            "intrinsic_fallback_by_reason",
            &self.intrinsic_fallback_by_reason,
            true,
        );
        push_field(
            &mut json,
            "json_encode_fast_path_hits",
            self.json_encode_fast_path_hits,
            true,
        );
        push_field(
            &mut json,
            "json_encode_fast_path_bytes",
            self.json_encode_fast_path_bytes,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "json_encode_generic_fallback_by_reason",
            &self.json_encode_generic_fallback_by_reason,
            true,
        );
        push_field(
            &mut json,
            "array_slice_packed_fast_hits",
            self.array_slice_packed_fast_hits,
            true,
        );
        push_field(
            &mut json,
            "count_array_shape_fast_hits",
            self.count_array_shape_fast_hits,
            true,
        );
        push_field(
            &mut json,
            "map_update_slot_fast_hits",
            self.map_update_slot_fast_hits,
            true,
        );
        push_field(
            &mut json,
            "property_dim_assign_in_place_hits",
            self.property_dim_assign_in_place_hits,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "property_dim_assign_generic_by_reason",
            &self.property_dim_assign_generic_by_reason,
            true,
        );
        push_field(
            &mut json,
            "property_dim_probe_borrowed_hits",
            self.property_dim_probe_borrowed_hits,
            true,
        );
        push_field(
            &mut json,
            "cufa_owned_argument_moves",
            self.cufa_owned_argument_moves,
            true,
        );
        push_field(
            &mut json,
            "cufa_shared_argument_clones",
            self.cufa_shared_argument_clones,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "array_builtin_fast_fallback_by_reason",
            &self.array_builtin_fast_fallback_by_reason,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "specialized_builtin_opcode_hits",
            &self.specialized_builtin_opcode_hits,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "slow_path_calls_by_reason",
            &self.slow_path_calls_by_reason,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "value_clone_by_reason",
            &self.value_clone_by_reason,
            true,
        );
        push_field(
            &mut json,
            "by_ref_arg_location_binding_attempts",
            self.by_ref_arg_location_binding_attempts,
            true,
        );
        push_field(
            &mut json,
            "by_ref_arg_location_bindings",
            self.by_ref_arg_location_bindings,
            true,
        );
        push_field(
            &mut json,
            "by_ref_arg_value_materializations",
            self.by_ref_arg_value_materializations,
            true,
        );
        push_field(
            &mut json,
            "by_ref_arg_register_pins",
            self.by_ref_arg_register_pins,
            true,
        );
        push_field(
            &mut json,
            "by_ref_arg_cow_separations",
            self.by_ref_arg_cow_separations,
            true,
        );
        push_field(
            &mut json,
            "by_ref_arg_cow_separations_avoided",
            self.by_ref_arg_cow_separations_avoided,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "by_ref_arg_fallback_by_reason",
            &self.by_ref_arg_fallback_by_reason,
            true,
        );
        push_field(
            &mut json,
            "dense_method_dispatch_attempts",
            self.dense_method_dispatch_attempts,
            true,
        );
        push_field(
            &mut json,
            "dense_method_dispatch_hits",
            self.dense_method_dispatch_hits,
            true,
        );
        push_field(
            &mut json,
            "dense_method_dispatch_fallbacks",
            self.dense_method_dispatch_fallbacks,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "dense_method_dispatch_fallback_by_reason",
            &self.dense_method_dispatch_fallback_by_reason,
            true,
        );
        push_field(
            &mut json,
            "rich_method_calls_from_dense_callers",
            self.rich_method_calls_from_dense_callers,
            true,
        );
        push_field(
            &mut json,
            "dense_jump_threading_trampoline_blocks",
            self.dense_jump_threading_trampoline_blocks,
            true,
        );
        push_field(
            &mut json,
            "dense_jump_threading_threaded_edges",
            self.dense_jump_threading_threaded_edges,
            true,
        );
        push_field(
            &mut json,
            "dense_jump_threading_rollbacks",
            self.dense_jump_threading_rollbacks,
            true,
        );
        push_field(
            &mut json,
            "call_ic_megamorphic_fallbacks",
            self.call_ic_megamorphic_fallbacks,
            true,
        );
        push_field(
            &mut json,
            "local_slot_fast_path_hits",
            self.local_slot_fast_path_hits,
            true,
        );
        push_field(
            &mut json,
            "local_slot_fast_path_misses",
            self.local_slot_fast_path_misses,
            true,
        );
        push_field(&mut json, "property_fetches", self.property_fetches, true);
        push_field(&mut json, "property_accesses", self.property_accesses, true);
        push_field(&mut json, "type_checks", self.type_checks, true);
        push_field(&mut json, "includes", self.includes, true);
        push_field(&mut json, "autoloads", self.autoloads, true);
        push_field(
            &mut json,
            "include_resolution_hits",
            self.include_resolution_hits,
            true,
        );
        push_field(
            &mut json,
            "include_resolution_misses",
            self.include_resolution_misses,
            true,
        );
        push_field(
            &mut json,
            "include_compile_hits",
            self.include_compile_hits,
            true,
        );
        push_field(
            &mut json,
            "include_compile_misses",
            self.include_compile_misses,
            true,
        );
        push_field(
            &mut json,
            "include_once_skips",
            self.include_once_skips,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "include_fallback_by_reason",
            &self.include_fallback_by_reason,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "include_stale_invalidation_by_reason",
            &self.include_stale_invalidation_by_reason,
            true,
        );
        push_field(
            &mut json,
            "include_graph_hits",
            self.include_graph_hits,
            true,
        );
        push_field(
            &mut json,
            "include_graph_misses",
            self.include_graph_misses,
            true,
        );
        push_field(
            &mut json,
            "autoload_graph_hits",
            self.autoload_graph_hits,
            true,
        );
        push_field(
            &mut json,
            "autoload_graph_misses",
            self.autoload_graph_misses,
            true,
        );
        push_field(
            &mut json,
            "negative_lookup_hits",
            self.negative_lookup_hits,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "invalidations_by_reason",
            &self.invalidations_by_reason,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "fallback_by_path_semantics",
            &self.fallback_by_path_semantics,
            true,
        );
        push_field(&mut json, "string_concats", self.string_concats, true);
        push_field(
            &mut json,
            "string_concat_fast_path_hits",
            self.string_concat_fast_path_hits,
            true,
        );
        push_field(
            &mut json,
            "string_concat_fast_path_misses",
            self.string_concat_fast_path_misses,
            true,
        );
        push_field(
            &mut json,
            "concat_prealloc_hits",
            self.concat_prealloc_hits,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "concat_fallback_by_reason",
            &self.concat_fallback_by_reason,
            true,
        );
        push_field(&mut json, "guard_failures", self.guard_failures, true);
        push_field(&mut json, "cache_hits", self.cache_hits, true);
        push_field(&mut json, "cache_misses", self.cache_misses, true);
        push_field(
            &mut json,
            "literal_intern_hits",
            self.literal_intern_hits,
            true,
        );
        push_field(
            &mut json,
            "literal_intern_misses",
            self.literal_intern_misses,
            true,
        );
        push_field(
            &mut json,
            "quickening_attempts",
            self.quickening_attempts,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "quickening_candidates_by_family",
            &self.quickening_candidates_by_family,
            true,
        );
        push_field(
            &mut json,
            "quickening_specialized",
            self.quickening_specialized,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "quickening_applied_by_family",
            &self.quickening_applied_by_family,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "quickened_executions_by_family",
            &self.quickened_executions_by_family,
            true,
        );
        push_field(
            &mut json,
            "quickening_guard_hits",
            self.quickening_guard_hits,
            true,
        );
        push_field(
            &mut json,
            "quickening_guard_misses",
            self.quickening_guard_misses,
            true,
        );
        push_field(
            &mut json,
            "quickening_guard_failures",
            self.quickening_guard_failures,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "quickening_guard_failures_by_family",
            &self.quickening_guard_failures_by_family,
            true,
        );
        push_field(
            &mut json,
            "quickening_fallback_calls",
            self.quickening_fallback_calls,
            true,
        );
        push_field(
            &mut json,
            "quickening_dequickens",
            self.quickening_dequickens,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "quickening_dequickened_by_reason",
            &self.quickening_dequickened_by_reason,
            true,
        );
        push_field(
            &mut json,
            "quickening_megamorphic",
            self.quickening_megamorphic,
            true,
        );
        push_field(
            &mut json,
            "quickening_disabled",
            self.quickening_disabled,
            true,
        );
        push_field(
            &mut json,
            "adaptive_tiny_unit_setup_skips",
            self.adaptive_tiny_unit_setup_skips,
            true,
        );
        push_field(&mut json, "native_candidates", self.native_candidates, true);
        push_field(
            &mut json,
            "native_compiled_regions",
            self.native_compiled_regions,
            true,
        );
        push_field(&mut json, "native_executions", self.native_executions, true);
        push_field(
            &mut json,
            "native_compile_budget_rejections",
            self.native_compile_budget_rejections,
            true,
        );
        json.push_str("  \"native_eligibility_rejections_by_reason\": {");
        for (index, (reason, count)) in self
            .native_eligibility_rejections_by_reason
            .iter()
            .enumerate()
        {
            if index > 0 {
                json.push_str(", ");
            }
            json.push('"');
            json.push_str(&escape_json(reason));
            json.push_str("\": ");
            json.push_str(&count.to_string());
        }
        json.push_str("},\n");
        json.push_str("  \"native_side_exits_by_reason\": {");
        for (index, (reason, count)) in self.native_side_exits_by_reason.iter().enumerate() {
            if index > 0 {
                json.push_str(", ");
            }
            json.push('"');
            json.push_str(&escape_json(reason));
            json.push_str("\": ");
            json.push_str(&count.to_string());
        }
        json.push_str("},\n");
        json.push_str("  \"native_blacklist_suppression_by_unstable_region\": {");
        for (index, (reason, count)) in self
            .native_blacklist_suppression_by_unstable_region
            .iter()
            .enumerate()
        {
            if index > 0 {
                json.push_str(", ");
            }
            json.push('"');
            json.push_str(&escape_json(reason));
            json.push_str("\": ");
            json.push_str(&count.to_string());
        }
        json.push_str("},\n");
        push_field(
            &mut json,
            "native_platform_unavailable",
            self.native_platform_unavailable,
            true,
        );
        push_field(
            &mut json,
            "jit_compile_attempts",
            self.jit_compile_attempts,
            true,
        );
        push_field(&mut json, "jit_compiled", self.jit_compiled, true);
        push_field(&mut json, "jit_executed", self.jit_executed, true);
        push_field(&mut json, "jit_bailouts", self.jit_bailouts, true);
        push_field(&mut json, "jit_code_bytes", self.jit_code_bytes, true);
        push_field(
            &mut json,
            "jit_compile_time_nanos",
            self.jit_compile_time_nanos,
            true,
        );
        push_field(&mut json, "jit_side_exits", self.jit_side_exits, true);
        json.push_str("  \"jit_side_exit_reasons\": {");
        for (index, (reason, count)) in self.jit_side_exit_reasons.iter().enumerate() {
            if index > 0 {
                json.push_str(", ");
            }
            json.push('"');
            json.push_str(&escape_json(reason));
            json.push_str("\": ");
            json.push_str(&count.to_string());
        }
        json.push_str("},\n");
        push_field(
            &mut json,
            "jit_guard_failures",
            self.jit_guard_failures,
            true,
        );
        push_field(
            &mut json,
            "jit_blacklisted_regions",
            self.jit_blacklisted_regions,
            true,
        );
        json.push_str("  \"jit_blacklist_reasons\": {");
        for (index, (reason, count)) in self.jit_blacklist_reasons.iter().enumerate() {
            if index > 0 {
                json.push_str(", ");
            }
            json.push('"');
            json.push_str(&escape_json(reason));
            json.push_str("\": ");
            json.push_str(&count.to_string());
        }
        json.push_str("},\n");
        push_field(
            &mut json,
            "jit_tiering_cold_functions",
            self.jit_tiering_cold_functions,
            true,
        );
        push_field(
            &mut json,
            "jit_tiering_hot_functions",
            self.jit_tiering_hot_functions,
            true,
        );
        push_field(
            &mut json,
            "jit_tiering_eager_functions",
            self.jit_tiering_eager_functions,
            true,
        );
        push_field(
            &mut json,
            "jit_tiering_blacklist_rejections",
            self.jit_tiering_blacklist_rejections,
            true,
        );
        push_field(
            &mut json,
            "jit_tiering_budget_rejections",
            self.jit_tiering_budget_rejections,
            true,
        );
        push_field(&mut json, "jit_helper_calls", self.jit_helper_calls, true);
        push_field(
            &mut json,
            "jit_fast_path_hits",
            self.jit_fast_path_hits,
            true,
        );
        push_field(
            &mut json,
            "packed_fetch_fast_hits",
            self.packed_fetch_fast_hits,
            true,
        );
        push_field(
            &mut json,
            "record_lookup_fast_hits",
            self.record_lookup_fast_hits,
            true,
        );
        push_field(
            &mut json,
            "record_lookup_key_miss_exits",
            self.record_lookup_key_miss_exits,
            true,
        );
        push_field(
            &mut json,
            "record_lookup_layout_exits",
            self.record_lookup_layout_exits,
            true,
        );
        push_field(
            &mut json,
            "packed_fetch_bounds_exits",
            self.packed_fetch_bounds_exits,
            true,
        );
        push_field(
            &mut json,
            "packed_fetch_layout_exits",
            self.packed_fetch_layout_exits,
            true,
        );
        push_field(
            &mut json,
            "packed_fetch_bounds_fallbacks",
            self.packed_fetch_bounds_fallbacks,
            true,
        );
        push_field(
            &mut json,
            "packed_fetch_layout_fallbacks",
            self.packed_fetch_layout_fallbacks,
            true,
        );
        push_field(
            &mut json,
            "packed_foreach_sum_fast_hits",
            self.packed_foreach_sum_fast_hits,
            true,
        );
        push_field(
            &mut json,
            "packed_foreach_sum_layout_exits",
            self.packed_foreach_sum_layout_exits,
            true,
        );
        push_field(
            &mut json,
            "packed_foreach_sum_overflow_exits",
            self.packed_foreach_sum_overflow_exits,
            true,
        );
        push_field(
            &mut json,
            "known_call_fast_hits",
            self.known_call_fast_hits,
            true,
        );
        push_field(
            &mut json,
            "known_call_guard_exits",
            self.known_call_guard_exits,
            true,
        );
        push_field(
            &mut json,
            "known_call_slow_calls",
            self.known_call_slow_calls,
            true,
        );
        push_field(&mut json, "direct_call_hits", self.direct_call_hits, true);
        push_field(
            &mut json,
            "direct_call_fallbacks",
            self.direct_call_fallbacks,
            true,
        );
        push_field(
            &mut json,
            "property_load_fast_hits",
            self.property_load_fast_hits,
            true,
        );
        push_field(
            &mut json,
            "property_load_guard_exits",
            self.property_load_guard_exits,
            true,
        );
        push_field(
            &mut json,
            "property_load_layout_exits",
            self.property_load_layout_exits,
            true,
        );
        push_field(
            &mut json,
            "property_load_uninitialized_exits",
            self.property_load_uninitialized_exits,
            true,
        );
        push_field(
            &mut json,
            "property_load_slow_calls",
            self.property_load_slow_calls,
            true,
        );
        push_field(
            &mut json,
            "jit_overflow_exits",
            self.jit_overflow_exits,
            true,
        );
        push_field(
            &mut json,
            "jit_slow_path_calls",
            self.jit_slow_path_calls,
            true,
        );
        push_field(
            &mut json,
            "jit_compile_cache_hits",
            self.jit_compile_cache_hits,
            true,
        );
        push_field(
            &mut json,
            "jit_compile_cache_misses",
            self.jit_compile_cache_misses,
            true,
        );
        push_field(
            &mut json,
            "jit_compile_cache_invalidations",
            self.jit_compile_cache_invalidations,
            true,
        );
        json.push_str("  \"jit_compile_descriptors\": [");
        for (index, descriptor) in self.jit_compile_descriptors.iter().enumerate() {
            if index > 0 {
                json.push_str(", ");
            }
            json.push('{');
            json.push_str("\"function_id\": ");
            json.push_str(&descriptor.function_id.to_string());
            json.push_str(", \"function_name\": ");
            push_json_string(&mut json, &descriptor.function_name);
            json.push_str(", \"ir_fingerprint\": ");
            push_json_string(&mut json, &descriptor.ir_fingerprint);
            json.push_str(", \"code_bytes\": ");
            json.push_str(&descriptor.code_bytes.to_string());
            json.push_str(", \"compile_time_nanos\": ");
            json.push_str(&descriptor.compile_time_nanos.to_string());
            json.push_str(", \"target_isa\": ");
            push_json_string(&mut json, &descriptor.target_isa);
            json.push_str(", \"abi_hash\": ");
            json.push_str(&descriptor.abi_hash.to_string());
            json.push_str(", \"config_hash\": ");
            json.push_str(&descriptor.config_hash.to_string());
            json.push('}');
        }
        json.push_str("],\n");
        push_field(
            &mut json,
            "inline_cache_observations",
            self.inline_cache_observations,
            true,
        );
        push_field(
            &mut json,
            "inline_cache_slots",
            self.inline_cache_slots,
            true,
        );
        push_field(
            &mut json,
            "inline_cache_function_slots",
            self.inline_cache_function_slots,
            true,
        );
        push_field(
            &mut json,
            "inline_cache_method_slots",
            self.inline_cache_method_slots,
            true,
        );
        push_field(
            &mut json,
            "inline_cache_property_slots",
            self.inline_cache_property_slots,
            true,
        );
        push_field(
            &mut json,
            "inline_cache_property_assign_slots",
            self.inline_cache_property_assign_slots,
            true,
        );
        push_field(
            &mut json,
            "inline_cache_dim_slots",
            self.inline_cache_dim_slots,
            true,
        );
        push_field(
            &mut json,
            "inline_cache_class_constant_static_property_slots",
            self.inline_cache_class_constant_static_property_slots,
            true,
        );
        push_field(
            &mut json,
            "inline_cache_class_relation_slots",
            self.inline_cache_class_relation_slots,
            true,
        );
        push_field(
            &mut json,
            "inline_cache_include_path_slots",
            self.inline_cache_include_path_slots,
            true,
        );
        push_field(
            &mut json,
            "inline_cache_autoload_class_lookup_slots",
            self.inline_cache_autoload_class_lookup_slots,
            true,
        );
        push_field(&mut json, "inline_cache_hits", self.inline_cache_hits, true);
        push_field(
            &mut json,
            "inline_cache_misses",
            self.inline_cache_misses,
            true,
        );
        push_field(
            &mut json,
            "inline_cache_invalidations",
            self.inline_cache_invalidations,
            true,
        );
        push_field(
            &mut json,
            "inline_cache_guard_failures",
            self.inline_cache_guard_failures,
            true,
        );
        push_field(
            &mut json,
            "inline_cache_fallback_calls",
            self.inline_cache_fallback_calls,
            true,
        );
        push_field(
            &mut json,
            "inline_cache_monomorphic",
            self.inline_cache_monomorphic,
            true,
        );
        push_field(
            &mut json,
            "inline_cache_polymorphic",
            self.inline_cache_polymorphic,
            true,
        );
        push_field(
            &mut json,
            "inline_cache_megamorphic",
            self.inline_cache_megamorphic,
            true,
        );
        push_field(
            &mut json,
            "inline_cache_disabled",
            self.inline_cache_disabled,
            true,
        );
        push_field(&mut json, "method_ic_hits", self.method_ic_hits, true);
        push_field(&mut json, "method_ic_misses", self.method_ic_misses, true);
        push_field(
            &mut json,
            "method_ic_polymorphic_hits",
            self.method_ic_polymorphic_hits,
            true,
        );
        push_field(
            &mut json,
            "method_ic_guard_failures",
            self.method_ic_guard_failures,
            true,
        );
        push_field(
            &mut json,
            "method_direct_dispatch_hits",
            self.method_direct_dispatch_hits,
            true,
        );
        push_field(
            &mut json,
            "method_direct_dispatch_fallbacks",
            self.method_direct_dispatch_fallbacks,
            true,
        );
        push_field(
            &mut json,
            "method_tiny_inline_candidates",
            self.method_tiny_inline_candidates,
            true,
        );
        push_field(
            &mut json,
            "method_inline_candidates",
            self.method_inline_candidates,
            true,
        );
        push_field(
            &mut json,
            "method_inline_hits",
            self.method_inline_hits,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "method_inline_fallback_by_reason",
            &self.method_inline_fallback_by_reason,
            true,
        );
        push_field(
            &mut json,
            "constructor_inline_hits",
            self.constructor_inline_hits,
            true,
        );
        push_field(
            &mut json,
            "dto_array_inline_hits",
            self.dto_array_inline_hits,
            true,
        );
        push_field(
            &mut json,
            "sort_callback_resolution_cache_hits",
            self.sort_callback_resolution_cache_hits,
            true,
        );
        push_field(
            &mut json,
            "sort_callback_direct_call_hits",
            self.sort_callback_direct_call_hits,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "sort_callback_generic_fallback_by_reason",
            &self.sort_callback_generic_fallback_by_reason,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "method_tiny_inline_rejected_by_reason",
            &self.method_tiny_inline_rejected_by_reason,
            true,
        );
        push_field(&mut json, "property_ic_hits", self.property_ic_hits, true);
        push_field(
            &mut json,
            "property_ic_misses",
            self.property_ic_misses,
            true,
        );
        push_field(
            &mut json,
            "property_ic_guard_failures",
            self.property_ic_guard_failures,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "property_ic_fallback_reasons",
            &self.property_ic_fallback_reasons,
            true,
        );
        push_field(
            &mut json,
            "property_assign_ic_hits",
            self.property_assign_ic_hits,
            true,
        );
        push_field(
            &mut json,
            "property_assign_ic_misses",
            self.property_assign_ic_misses,
            true,
        );
        push_field(
            &mut json,
            "property_assign_ic_guard_failures",
            self.property_assign_ic_guard_failures,
            true,
        );
        push_field(
            &mut json,
            "property_assign_ic_shape_exits",
            self.property_assign_ic_shape_exits,
            true,
        );
        push_field(
            &mut json,
            "property_assign_ic_visibility_exits",
            self.property_assign_ic_visibility_exits,
            true,
        );
        push_field(
            &mut json,
            "property_assign_ic_type_exits",
            self.property_assign_ic_type_exits,
            true,
        );
        push_field(
            &mut json,
            "property_assign_ic_readonly_exits",
            self.property_assign_ic_readonly_exits,
            true,
        );
        push_field(
            &mut json,
            "property_assign_ic_hook_magic_exits",
            self.property_assign_ic_hook_magic_exits,
            true,
        );
        push_field(
            &mut json,
            "property_assign_ic_reference_exits",
            self.property_assign_ic_reference_exits,
            true,
        );
        push_field(
            &mut json,
            "property_assign_ic_dynamic_exits",
            self.property_assign_ic_dynamic_exits,
            true,
        );
        push_string_u64_map_field(
            &mut json,
            "property_assign_ic_fallback_reasons",
            &self.property_assign_ic_fallback_reasons,
            true,
        );
        push_field(
            &mut json,
            "class_static_ic_hits",
            self.class_static_ic_hits,
            true,
        );
        push_field(
            &mut json,
            "class_static_ic_misses",
            self.class_static_ic_misses,
            true,
        );
        push_field(
            &mut json,
            "class_static_ic_guard_failures",
            self.class_static_ic_guard_failures,
            true,
        );
        push_field(
            &mut json,
            "class_relation_cache_hits",
            self.class_relation_cache_hits,
            true,
        );
        push_field(
            &mut json,
            "class_relation_cache_misses",
            self.class_relation_cache_misses,
            true,
        );
        push_field(
            &mut json,
            "class_relation_cache_invalidations",
            self.class_relation_cache_invalidations,
            true,
        );
        push_field(
            &mut json,
            "instanceof_cache_hits",
            self.instanceof_cache_hits,
            true,
        );
        push_field(
            &mut json,
            "instanceof_cache_misses",
            self.instanceof_cache_misses,
            true,
        );
        push_field(
            &mut json,
            "method_override_cache_hits",
            self.method_override_cache_hits,
            true,
        );
        push_field(
            &mut json,
            "method_override_cache_misses",
            self.method_override_cache_misses,
            true,
        );
        push_field(
            &mut json,
            "include_path_ic_hits",
            self.include_path_ic_hits,
            true,
        );
        push_field(
            &mut json,
            "include_path_ic_misses",
            self.include_path_ic_misses,
            true,
        );
        push_field(
            &mut json,
            "include_path_ic_invalidations",
            self.include_path_ic_invalidations,
            true,
        );
        push_field(
            &mut json,
            "include_path_ic_guard_failures",
            self.include_path_ic_guard_failures,
            true,
        );
        push_field(
            &mut json,
            "autoload_class_lookup_ic_hits",
            self.autoload_class_lookup_ic_hits,
            true,
        );
        push_field(
            &mut json,
            "autoload_class_lookup_ic_misses",
            self.autoload_class_lookup_ic_misses,
            true,
        );
        push_field(
            &mut json,
            "autoload_class_lookup_ic_invalidations",
            self.autoload_class_lookup_ic_invalidations,
            true,
        );
        push_field(
            &mut json,
            "autoload_class_lookup_ic_guard_failures",
            self.autoload_class_lookup_ic_guard_failures,
            true,
        );
        push_property_fetch_profiles(&mut json, &self.property_fetch_profiles);
        json.push(',');
        push_method_call_profiles(&mut json, &self.method_call_profiles);
        json.push_str("}\n");
        json
    }
}

fn request_frame_allocation_bytes(register_count: u32, local_count: u32) -> usize {
    register_count as usize * std::mem::size_of::<TempValue>()
        + local_count as usize * std::mem::size_of::<Slot>()
}

fn record_boundary_profile(
    profiles: &mut BTreeMap<String, BoundaryProfile>,
    name: &str,
    inclusive_nanos: u64,
    exclusive_nanos: u64,
    rich_instructions: u64,
    dense_instructions: u64,
) {
    let profile = profiles.entry(name.to_owned()).or_default();
    profile.count = profile.count.saturating_add(1);
    profile.inclusive_nanos = profile.inclusive_nanos.saturating_add(inclusive_nanos);
    profile.exclusive_nanos = profile.exclusive_nanos.saturating_add(exclusive_nanos);
    profile.rich_instructions = profile.rich_instructions.saturating_add(rich_instructions);
    profile.dense_instructions = profile
        .dense_instructions
        .saturating_add(dense_instructions);
}

fn record_operation_profile(
    profiles: &mut BTreeMap<String, OperationProfile>,
    family: &str,
    inclusive_nanos: u64,
) {
    let profile = profiles.entry(family.to_owned()).or_default();
    profile.count = profile.count.saturating_add(1);
    profile.inclusive_nanos = profile.inclusive_nanos.saturating_add(inclusive_nanos);
}

fn alias_sensitive_reason(reason: &str) -> bool {
    reason.contains("reference") || reason.contains("by_ref") || reason.contains("cow")
}

fn quickening_specialization_family(
    specialization: crate::QuickeningSpecialization,
) -> &'static str {
    match specialization {
        crate::QuickeningSpecialization::AddIntInt
        | crate::QuickeningSpecialization::SubIntInt
        | crate::QuickeningSpecialization::MulIntInt => "integer_arithmetic",
        crate::QuickeningSpecialization::ConcatStringString => "string_concat",
        crate::QuickeningSpecialization::PackedArrayIntKey => "packed_array_dim_fetch",
        crate::QuickeningSpecialization::BoolBranchCondition => "scalar_branch",
    }
}

fn push_method_call_profiles(json: &mut String, profiles: &BTreeMap<String, MethodCallProfile>) {
    json.push_str("  \"method_call_profiles\": [");
    if !profiles.is_empty() {
        json.push('\n');
    }
    for (index, profile) in profiles.values().enumerate() {
        if index > 0 {
            json.push_str(",\n");
        }
        let state = method_profile_state(profile);
        let mut reasons = method_profile_reasons(profile);
        reasons.extend(profile.non_eligible_reasons.iter().cloned());
        let fast_path_eligible = state == "monomorphic" && reasons.is_empty();
        json.push_str("    {\n");
        push_nested_string_field(json, "callsite", &profile.callsite, true);
        push_nested_string_field(json, "method", &profile.method, true);
        push_nested_field(json, "observations", profile.observations, true);
        push_nested_string_field(json, "state", state, true);
        push_nested_bool_field(json, "fast_path_eligible", fast_path_eligible, true);
        push_nested_string_array(
            json,
            "non_eligible_reasons",
            reasons.iter().map(String::as_str),
            true,
        );
        push_nested_u32_array(json, "class_ids", profile.class_ids.iter().copied(), true);
        push_nested_string_array(
            json,
            "receiver_classes",
            profile.receiver_classes.iter().map(String::as_str),
            true,
        );
        push_nested_string_array(
            json,
            "declaring_classes",
            profile.declaring_classes.iter().map(String::as_str),
            true,
        );
        push_nested_u32_array(json, "method_ids", profile.method_ids.iter().copied(), true);
        push_nested_usize_array(
            json,
            "method_slot_indexes",
            profile.method_slot_indexes.iter().copied(),
            true,
        );
        push_nested_string_array(
            json,
            "visibility_contexts",
            profile.visibility_contexts.iter().map(String::as_str),
            true,
        );
        push_nested_u64_array(
            json,
            "override_layout_versions",
            profile.override_layout_versions.iter().copied(),
            true,
        );
        push_nested_bool_field(json, "saw_final_method", profile.saw_final_method, true);
        push_nested_bool_field(json, "saw_private_method", profile.saw_private_method, true);
        push_nested_bool_field(json, "saw_static_method", profile.saw_static_method, true);
        push_nested_bool_field(json, "has_magic_call", profile.has_magic_call, true);
        push_nested_bool_field(
            json,
            "magic_call_fallback",
            profile.magic_call_fallback,
            true,
        );
        push_nested_bool_field(
            json,
            "simple_positional_arguments",
            profile.simple_positional_arguments,
            true,
        );
        push_nested_bool_field(
            json,
            "saw_by_ref_argument",
            profile.saw_by_ref_argument,
            true,
        );
        push_nested_bool_field(
            json,
            "saw_callee_jit_eligible",
            profile.saw_callee_jit_eligible,
            true,
        );
        push_nested_bool_field(
            json,
            "saw_direct_vm_call_helper",
            profile.saw_direct_vm_call_helper,
            false,
        );
        json.push_str("    }");
    }
    if !profiles.is_empty() {
        json.push('\n');
        json.push_str("  ");
    }
    json.push_str("]\n");
}

fn push_property_fetch_profiles(
    json: &mut String,
    profiles: &BTreeMap<String, PropertyFetchProfile>,
) {
    json.push_str("  \"property_fetch_profiles\": [");
    if !profiles.is_empty() {
        json.push('\n');
    }
    for (index, profile) in profiles.values().enumerate() {
        if index > 0 {
            json.push_str(",\n");
        }
        let state = property_profile_state(profile);
        let mut reasons = property_profile_reasons(profile);
        reasons.extend(profile.non_eligible_reasons.iter().cloned());
        let fast_path_eligible = state == "monomorphic" && reasons.is_empty();
        json.push_str("    {\n");
        push_nested_string_field(json, "callsite", &profile.callsite, true);
        push_nested_string_field(json, "property", &profile.property, true);
        push_nested_field(json, "observations", profile.observations, true);
        push_nested_string_field(json, "state", state, true);
        push_nested_bool_field(json, "fast_path_eligible", fast_path_eligible, true);
        push_nested_string_array(
            json,
            "non_eligible_reasons",
            reasons.iter().map(String::as_str),
            true,
        );
        push_nested_u32_array(json, "class_ids", profile.class_ids.iter().copied(), true);
        push_nested_string_array(
            json,
            "receiver_classes",
            profile.receiver_classes.iter().map(String::as_str),
            true,
        );
        push_nested_string_array(
            json,
            "declared_property_names",
            profile.declared_property_names.iter().map(String::as_str),
            true,
        );
        push_nested_string_array(
            json,
            "visibility_contexts",
            profile.visibility_contexts.iter().map(String::as_str),
            true,
        );
        push_nested_usize_array(
            json,
            "property_slot_indexes",
            profile.property_slot_indexes.iter().copied(),
            true,
        );
        push_nested_u64_array(
            json,
            "class_layout_versions",
            profile.class_layout_versions.iter().copied(),
            true,
        );
        push_nested_bool_field(json, "has_magic_get", profile.has_magic_get, true);
        push_nested_bool_field(json, "has_property_hook", profile.has_property_hook, true);
        push_nested_bool_field(
            json,
            "dynamic_property_fallback",
            profile.dynamic_property_fallback,
            true,
        );
        push_nested_bool_field(
            json,
            "saw_declared_visible_property",
            profile.saw_declared_visible_property,
            true,
        );
        push_nested_bool_field(
            json,
            "saw_uninitialized_typed_property",
            profile.saw_uninitialized_typed_property,
            false,
        );
        json.push_str("    }");
    }
    if !profiles.is_empty() {
        json.push('\n');
        json.push_str("  ");
    }
    json.push_str("]\n");
}

fn property_profile_state(profile: &PropertyFetchProfile) -> &'static str {
    match profile.receiver_classes.len() {
        0 => "cold",
        1 => "monomorphic",
        len if len <= POLYMORPHIC_INLINE_CACHE_LIMIT => "polymorphic",
        _ => "megamorphic",
    }
}

fn property_profile_reasons(profile: &PropertyFetchProfile) -> BTreeSet<String> {
    let mut reasons = BTreeSet::new();
    match profile.receiver_classes.len() {
        0 | 1 => {}
        len if len <= POLYMORPHIC_INLINE_CACHE_LIMIT => {
            reasons.insert("polymorphic_receiver".to_owned());
        }
        _ => {
            reasons.insert("megamorphic_receiver".to_owned());
        }
    }
    if profile.class_layout_versions.len() > 1 {
        reasons.insert("unstable_layout_version".to_owned());
    }
    if profile.declared_property_names.is_empty() {
        reasons.insert("missing_declared_property".to_owned());
    }
    if !profile.saw_declared_visible_property {
        reasons.insert("not_visible".to_owned());
    }
    reasons
}

fn method_profile_state(profile: &MethodCallProfile) -> &'static str {
    match profile.receiver_classes.len() {
        0 => "cold",
        1 => "monomorphic",
        len if len <= POLYMORPHIC_INLINE_CACHE_LIMIT => "polymorphic",
        _ => "megamorphic",
    }
}

fn method_profile_reasons(profile: &MethodCallProfile) -> BTreeSet<String> {
    let mut reasons = BTreeSet::new();
    match profile.receiver_classes.len() {
        0 | 1 => {}
        len if len <= POLYMORPHIC_INLINE_CACHE_LIMIT => {
            reasons.insert("polymorphic_receiver".to_owned());
        }
        _ => {
            reasons.insert("megamorphic_receiver".to_owned());
        }
    }
    if profile.method_ids.len() > 1 || profile.method_slot_indexes.len() > 1 {
        reasons.insert("unstable_method_slot".to_owned());
    }
    if profile.override_layout_versions.len() > 1 {
        reasons.insert("unstable_override_layout_version".to_owned());
    }
    if profile.method_ids.is_empty() {
        reasons.insert("missing_declared_method".to_owned());
    }
    if !(profile.saw_callee_jit_eligible || profile.saw_direct_vm_call_helper) {
        reasons.insert("callee_not_jit_eligible".to_owned());
    }
    if profile.has_magic_call || profile.magic_call_fallback {
        reasons.insert("magic_call_present".to_owned());
    }
    if !profile.simple_positional_arguments {
        reasons.insert("non_positional_argument".to_owned());
    }
    if profile.saw_by_ref_argument {
        reasons.insert("by_ref_argument".to_owned());
    }
    if profile.saw_static_method {
        reasons.insert("static_method".to_owned());
    }
    reasons
}

fn push_field(json: &mut String, name: &str, value: u64, comma: bool) {
    json.push_str("  \"");
    json.push_str(name);
    json.push_str("\": ");
    json.push_str(&value.to_string());
    if comma {
        json.push(',');
    }
    json.push('\n');
}

fn push_string_u64_map_field(
    json: &mut String,
    name: &str,
    values: &BTreeMap<String, u64>,
    comma: bool,
) {
    json.push_str("  \"");
    json.push_str(name);
    json.push_str("\": {");
    if values.is_empty() {
        json.push('}');
    } else {
        json.push('\n');
        for (index, (key, value)) in values.iter().enumerate() {
            json.push_str("    \"");
            json.push_str(&escape_json(key));
            json.push_str("\": ");
            json.push_str(&value.to_string());
            if index + 1 != values.len() {
                json.push(',');
            }
            json.push('\n');
        }
        json.push_str("  }");
    }
    if comma {
        json.push(',');
    }
    json.push('\n');
}

fn push_nested_field(json: &mut String, name: &str, value: u64, comma: bool) {
    json.push_str("      \"");
    json.push_str(name);
    json.push_str("\": ");
    json.push_str(&value.to_string());
    if comma {
        json.push(',');
    }
    json.push('\n');
}

fn push_nested_bool_field(json: &mut String, name: &str, value: bool, comma: bool) {
    json.push_str("      \"");
    json.push_str(name);
    json.push_str("\": ");
    json.push_str(if value { "true" } else { "false" });
    if comma {
        json.push(',');
    }
    json.push('\n');
}

fn push_nested_string_field(json: &mut String, name: &str, value: &str, comma: bool) {
    json.push_str("      \"");
    json.push_str(name);
    json.push_str("\": \"");
    json.push_str(&escape_json(value));
    json.push('"');
    if comma {
        json.push(',');
    }
    json.push('\n');
}

fn push_nested_string_array<'a>(
    json: &mut String,
    name: &str,
    values: impl Iterator<Item = &'a str>,
    comma: bool,
) {
    json.push_str("      \"");
    json.push_str(name);
    json.push_str("\": [");
    for (index, value) in values.enumerate() {
        if index > 0 {
            json.push_str(", ");
        }
        json.push('"');
        json.push_str(&escape_json(value));
        json.push('"');
    }
    json.push(']');
    if comma {
        json.push(',');
    }
    json.push('\n');
}

fn push_nested_u32_array(
    json: &mut String,
    name: &str,
    values: impl Iterator<Item = u32>,
    comma: bool,
) {
    json.push_str("      \"");
    json.push_str(name);
    json.push_str("\": [");
    for (index, value) in values.enumerate() {
        if index > 0 {
            json.push_str(", ");
        }
        json.push_str(&value.to_string());
    }
    json.push(']');
    if comma {
        json.push(',');
    }
    json.push('\n');
}

fn push_nested_u64_array(
    json: &mut String,
    name: &str,
    values: impl Iterator<Item = u64>,
    comma: bool,
) {
    json.push_str("      \"");
    json.push_str(name);
    json.push_str("\": [");
    for (index, value) in values.enumerate() {
        if index > 0 {
            json.push_str(", ");
        }
        json.push_str(&value.to_string());
    }
    json.push(']');
    if comma {
        json.push(',');
    }
    json.push('\n');
}

fn push_nested_usize_array(
    json: &mut String,
    name: &str,
    values: impl Iterator<Item = usize>,
    comma: bool,
) {
    json.push_str("      \"");
    json.push_str(name);
    json.push_str("\": [");
    for (index, value) in values.enumerate() {
        if index > 0 {
            json.push_str(", ");
        }
        json.push_str(&value.to_string());
    }
    json.push(']');
    if comma {
        json.push(',');
    }
    json.push('\n');
}

// Names and indexes are defined in one fixed order: `ir_opcode_index` is
// the single match over `InstructionKind` (exhaustive, so new variants
// fail to compile until both are extended) and `IR_OPCODE_NAMES` mirrors
// its arm order.
pub(crate) const IR_OPCODE_COUNT: usize = 103;

#[rustfmt::skip]
const IR_OPCODE_NAMES: [&str; IR_OPCODE_COUNT] = [
    "nop", // 0
    "load_const", // 1
    "fetch_const", // 2
    "register_constant", // 3
    "declare_function", // 4
    "declare_class", // 5
    "move", // 6
    "load_local", // 7
    "load_local_quiet", // 8
    "store_local", // 9
    "bind_reference", // 10
    "bind_global", // 11
    "bind_reference_dim", // 12
    "bind_reference_property", // 13
    "bind_reference_property_dim", // 14
    "bind_reference_dim_from_property", // 15
    "bind_reference_from_property", // 16
    "bind_reference_from_property_dim", // 17
    "bind_reference_from_dim", // 18
    "bind_reference_from_static_property_dim", // 19
    "bind_reference_from_call", // 20
    "bind_reference_from_method_call", // 21
    "init_static_local", // 22
    "binary_concat", // 23
    "binary", // 24
    "compare", // 25
    "instanceof", // 26
    "dynamic_instanceof", // 27
    "unary", // 28
    "cast", // 29
    "discard", // 30
    "echo", // 31
    "emit_diagnostic", // 32
    "yield", // 33
    "yield_from", // 34
    "call_function", // 35
    "call_method", // 36
    "call_static_method", // 37
    "clone_object", // 38
    "clone_with", // 39
    "enter_try", // 40
    "leave_try", // 41
    "end_finally", // 42
    "throw", // 43
    "make_exception", // 44
    "make_closure", // 45
    "call_closure", // 46
    "resolve_callable", // 47
    "acquire_callable", // 48
    "call_callable", // 49
    "pipe", // 50
    "include", // 51
    "eval", // 52
    "new_object", // 53
    "dynamic_new_object", // 54
    "fetch_property", // 55
    "fetch_dynamic_property", // 56
    "isset_property", // 57
    "isset_dynamic_property", // 58
    "empty_property", // 59
    "empty_dynamic_property", // 60
    "isset_dynamic_property_dim", // 61
    "empty_dynamic_property_dim", // 62
    "isset_property_dim", // 63
    "empty_property_dim", // 64
    "unset_property", // 65
    "unset_property_dim", // 66
    "unset_dynamic_property", // 67
    "fetch_static_property", // 68
    "fetch_dynamic_static_property", // 69
    "isset_static_property", // 70
    "empty_static_property", // 71
    "isset_static_property_dim", // 72
    "empty_static_property_dim", // 73
    "unset_static_property_dim", // 74
    "fetch_class_constant", // 75
    "fetch_object_class_name", // 76
    "assign_property", // 77
    "assign_property_dim", // 78
    "assign_dynamic_property", // 79
    "bind_reference_static_property", // 80
    "assign_static_property", // 81
    "assign_dynamic_static_property", // 82
    "new_array", // 83
    "array_insert", // 84
    "array_spread", // 85
    "fetch_dim", // 86
    "assign_dim", // 87
    "append_dim", // 88
    "isset_local", // 89
    "empty_local", // 90
    "unset_local", // 91
    "isset_dim", // 92
    "empty_dim", // 93
    "unset_dim", // 94
    "foreach_init", // 95
    "foreach_next", // 96
    "foreach_cleanup", // 97
    "foreach_init_ref", // 98
    "foreach_next_ref", // 99
    "array_get", // 100
    "unsupported", // 101
    "runtime_error", // 102
];

#[rustfmt::skip]
fn ir_opcode_index(kind: &InstructionKind) -> usize {
    match kind {
        InstructionKind::Nop => 0,
        InstructionKind::LoadConst { .. } => 1,
        InstructionKind::FetchConst { .. } => 2,
        InstructionKind::RegisterConstant { .. } => 3,
        InstructionKind::DeclareFunction { .. } => 4,
        InstructionKind::DeclareClass { .. } => 5,
        InstructionKind::Move { .. } => 6,
        InstructionKind::LoadLocal { .. } => 7,
        InstructionKind::LoadLocalQuiet { .. } => 8,
        InstructionKind::StoreLocal { .. } => 9,
        InstructionKind::BindReference { .. } => 10,
        InstructionKind::BindGlobal { .. } => 11,
        InstructionKind::BindReferenceDim { .. } => 12,
        InstructionKind::BindReferenceProperty { .. } => 13,
        InstructionKind::BindReferencePropertyDim { .. } => 14,
        InstructionKind::BindReferenceDimFromProperty { .. } => 15,
        InstructionKind::BindReferenceFromProperty { .. } => 16,
        InstructionKind::BindReferenceFromPropertyDim { .. } => 17,
        InstructionKind::BindReferenceFromDim { .. } => 18,
        InstructionKind::BindReferenceFromStaticPropertyDim { .. } => 19,
        InstructionKind::BindReferenceFromCall { .. } => 20,
        InstructionKind::BindReferenceFromMethodCall { .. } => 21,
        InstructionKind::InitStaticLocal { .. } => 22,
        InstructionKind::Binary { op: BinaryOp::Concat, .. } => 23,
        InstructionKind::Binary { .. } => 24,
        InstructionKind::Compare { .. } => 25,
        InstructionKind::InstanceOf { .. } => 26,
        InstructionKind::DynamicInstanceOf { .. } => 27,
        InstructionKind::Unary { .. } => 28,
        InstructionKind::Cast { .. } => 29,
        InstructionKind::Discard { .. } => 30,
        InstructionKind::Echo { .. } => 31,
        InstructionKind::EmitDiagnostic { .. } => 32,
        InstructionKind::Yield { .. } => 33,
        InstructionKind::YieldFrom { .. } => 34,
        InstructionKind::CallFunction { .. } => 35,
        InstructionKind::CallMethod { .. } => 36,
        InstructionKind::CallStaticMethod { .. } => 37,
        InstructionKind::CloneObject { .. } => 38,
        InstructionKind::CloneWith { .. } => 39,
        InstructionKind::EnterTry { .. } => 40,
        InstructionKind::LeaveTry => 41,
        InstructionKind::EndFinally { .. } => 42,
        InstructionKind::Throw { .. } => 43,
        InstructionKind::MakeException { .. } => 44,
        InstructionKind::MakeClosure { .. } => 45,
        InstructionKind::CallClosure { .. } => 46,
        InstructionKind::ResolveCallable { .. } => 47,
        InstructionKind::AcquireCallable { .. } => 48,
        InstructionKind::CallCallable { .. } => 49,
        InstructionKind::Pipe { .. } => 50,
        InstructionKind::Include { .. } => 51,
        InstructionKind::Eval { .. } => 52,
        InstructionKind::NewObject { .. } => 53,
        InstructionKind::DynamicNewObject { .. } => 54,
        InstructionKind::FetchProperty { .. } => 55,
        InstructionKind::FetchDynamicProperty { .. } => 56,
        InstructionKind::IssetProperty { .. } => 57,
        InstructionKind::IssetDynamicProperty { .. } => 58,
        InstructionKind::EmptyProperty { .. } => 59,
        InstructionKind::EmptyDynamicProperty { .. } => 60,
        InstructionKind::IssetDynamicPropertyDim { .. } => 61,
        InstructionKind::EmptyDynamicPropertyDim { .. } => 62,
        InstructionKind::IssetPropertyDim { .. } => 63,
        InstructionKind::EmptyPropertyDim { .. } => 64,
        InstructionKind::UnsetProperty { .. } => 65,
        InstructionKind::UnsetPropertyDim { .. } => 66,
        InstructionKind::UnsetDynamicProperty { .. } => 67,
        InstructionKind::FetchStaticProperty { .. } => 68,
        InstructionKind::FetchDynamicStaticProperty { .. } => 69,
        InstructionKind::IssetStaticProperty { .. } => 70,
        InstructionKind::EmptyStaticProperty { .. } => 71,
        InstructionKind::IssetStaticPropertyDim { .. } => 72,
        InstructionKind::EmptyStaticPropertyDim { .. } => 73,
        InstructionKind::UnsetStaticPropertyDim { .. } => 74,
        InstructionKind::FetchClassConstant { .. } => 75,
        InstructionKind::FetchObjectClassName { .. } => 76,
        InstructionKind::AssignProperty { .. } => 77,
        InstructionKind::AssignPropertyDim { .. } => 78,
        InstructionKind::AssignDynamicProperty { .. } => 79,
        InstructionKind::BindReferenceStaticProperty { .. } => 80,
        InstructionKind::AssignStaticProperty { .. } => 81,
        InstructionKind::AssignDynamicStaticProperty { .. } => 82,
        InstructionKind::NewArray { .. } => 83,
        InstructionKind::ArrayInsert { .. } => 84,
        InstructionKind::ArraySpread { .. } => 85,
        InstructionKind::FetchDim { .. } => 86,
        InstructionKind::AssignDim { .. } => 87,
        InstructionKind::AppendDim { .. } => 88,
        InstructionKind::IssetLocal { .. } => 89,
        InstructionKind::EmptyLocal { .. } => 90,
        InstructionKind::UnsetLocal { .. } => 91,
        InstructionKind::IssetDim { .. } => 92,
        InstructionKind::EmptyDim { .. } => 93,
        InstructionKind::UnsetDim { .. } => 94,
        InstructionKind::ForeachInit { .. } => 95,
        InstructionKind::ForeachNext { .. } => 96,
        InstructionKind::ForeachCleanup { .. } => 97,
        InstructionKind::ForeachInitRef { .. } => 98,
        InstructionKind::ForeachNextRef { .. } => 99,
        InstructionKind::ArrayGet { .. } => 100,
        InstructionKind::Unsupported { .. } => 101,
        InstructionKind::RuntimeError { .. } => 102,
    }
}

fn bytecode_opcode_family(opcode: &str) -> &'static str {
    match opcode {
        "load_const" | "load_const_echo" | "fetch_const" => "constants",
        "move" | "load_local" | "load_local_echo" | "load_local_quiet" | "store_local"
        | "unset_local" | "isset_local" | "empty_local" | "bind_global" => "locals",
        "binary_add" | "binary_sub" | "binary_mul" | "binary_div" | "binary_mod"
        | "binary_concat" | "binary_concat_echo" | "binary_pow" | "binary_bit_and"
        | "binary_bit_or" | "binary_bit_xor" | "binary_shift_left" | "binary_shift_right"
        | "cast" => "scalar_ops",
        "compare_equal"
        | "compare_not_equal"
        | "compare_identical"
        | "compare_not_identical"
        | "compare_less"
        | "compare_less_equal"
        | "compare_greater"
        | "compare_greater_equal"
        | "compare_spaceship" => "comparisons",
        "unary_plus" | "unary_minus" | "unary_not" | "unary_bit_not" => "unary_ops",
        "call_function" => "function_calls",
        "new_array" | "array_insert" | "fetch_dim" | "assign_dim" | "append_dim" | "isset_dim"
        | "empty_dim" | "unset_dim" => "arrays",
        "fetch_property" | "assign_property" => "properties",
        "foreach_init" | "foreach_next" => "foreach",
        "include" => "includes",
        "echo" => "output",
        "jump" | "jump_if_false" | "jump_if_true" | "jump_if" => "control_flow",
        "return" => "returns",
        "declare_function" | "declare_class" => "declarations",
        "assign_property_dim" => "properties",
        "fetch_class_constant" | "fetch_static_property" | "clone_object" => "objects",
        "isset_property" | "empty_property" => "properties",
        "discard" | "nop" => "bookkeeping",
        _ => "other",
    }
}

fn push_string_field(json: &mut String, name: &str, value: &str, comma: bool) {
    json.push_str("  \"");
    json.push_str(name);
    json.push_str("\": \"");
    json.push_str(&escape_json(value));
    json.push('"');
    if comma {
        json.push(',');
    }
    json.push('\n');
}

fn push_json_string(json: &mut String, value: &str) {
    json.push('"');
    json.push_str(&escape_json(value));
    json.push('"');
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}

fn merge_static_counter_map(
    target: &mut BTreeMap<String, u64>,
    source: BTreeMap<&'static str, u64>,
) {
    for (key, count) in source {
        *target.entry(key.to_owned()).or_default() += count;
    }
}

#[cfg(test)]
mod tests {
    use crate::{AliasState, InlineCacheKind, InlineCacheObservation, QuickeningObservation};
    use php_ir::ids::RegId;
    use php_ir::instruction::{BinaryOp, InstructionKind};
    use php_runtime::{PhpArrayShapeKind, PhpArrayShapeLookupFallback};
    use std::collections::BTreeMap;

    use super::{
        JitCompileDescriptor, MethodCallProfileObservation, OutputStats,
        PropertyFetchProfileObservation, VmCounters,
    };

    #[test]
    fn counters_classify_required_perf_families() {
        let mut counters = VmCounters::default();
        counters.record_instruction(&InstructionKind::Binary {
            dst: RegId::new(0),
            op: BinaryOp::Concat,
            lhs: php_ir::operand::Operand::Register(RegId::new(1)),
            rhs: php_ir::operand::Operand::Register(RegId::new(2)),
        });
        counters.record_instruction(&InstructionKind::CallFunction {
            dst: RegId::new(1),
            name: "f".to_owned(),
            args: Vec::new(),
        });
        counters.fold_scratch_counters();
        counters.record_frame_activation(false, 4, 3);
        counters.record_frame_activation(true, 4, 3);
        counters.record_frame_reuse_blocked("by_ref_param");
        counters.record_call_frame_layout("tiny_leaf_frame");
        counters.record_tiny_frame_candidate();
        counters.record_specialized_frame_hit();
        counters.record_generic_frame_fallback("class_context");
        counters.record_arg_array_avoided();
        counters.record_heap_frame_avoided();
        counters.record_alias_state(AliasState::NoReferencesObserved);
        counters.record_alias_state_transition(
            AliasState::NoReferencesObserved,
            AliasState::LocalOnlyReference,
        );
        counters.record_fast_path_disabled_by_reference(AliasState::PropertyOrArrayDimReference);
        counters.record_dequickened_by_reference(AliasState::PropertyOrArrayDimReference);
        counters.record_ic_invalidated_by_reference(AliasState::PropertyOrArrayDimReference);
        counters.record_dense_bytecode_fallback_by_reference(AliasState::UnknownAliasing);
        counters.record_runtime_layout_stats(php_runtime::layout_stats::RuntimeLayoutStats {
            value_clones: 11,
            string_allocations: 7,
            array_handle_clones: 5,
            cow_separations: 3,
            reference_cell_creations: 2,
            object_allocations: 1,
            array_packed_direct_gets: 13,
            array_mixed_indexed_gets: 17,
            array_linear_scan_fallbacks: 19,
            array_metadata_recomputes: 23,
            symbol_map_lookups: 29,
            symbol_linear_fallbacks: 31,
            symbol_intern_hits: 37,
            symbol_intern_misses: 41,
            string_hash_cache_hits: 43,
            string_hash_cache_misses: 47,
            symbol_eq_fast_hits: 53,
            symbol_eq_byte_fallbacks: 59,
            object_declared_slot_reads: 61,
            object_declared_slot_writes: 67,
            object_dynamic_property_map_reads: 71,
            object_dynamic_property_map_writes: 73,
            packed_values_storage_arrays: 79,
            packed_values_storage_reads: 83,
            packed_values_storage_appends: 89,
            packed_virtual_key_iterations: 97,
            packed_to_mixed_string_key: 101,
            packed_to_mixed_non_sequential_int_key: 103,
            packed_to_mixed_append_key_gap: 107,
            packed_to_mixed_unset_hole: 109,
            record_storage_arrays: 113,
            record_shape_promotions: 127,
            record_slot_reads: 131,
            record_slot_writes: 137,
            record_key_symbol_hits: 139,
            record_to_mixed_int_key: 149,
            record_to_mixed_ambiguous_key: 151,
            record_to_mixed_generic_mutation: 157,
        });
        counters.record_runtime_layout_source_stats(
            php_runtime::layout_stats::RuntimeLayoutSourceStats {
                value_clone_by_family: BTreeMap::from([
                    ("stack_register_local_move", 5),
                    ("return_value", 2),
                ]),
                array_handle_clone_by_family: BTreeMap::from([("array_element_read", 3)]),
                cow_separation_by_family: BTreeMap::from([("by_ref_argument_binding", 2)]),
                reference_cell_creation_by_family: BTreeMap::from([("by_ref_argument_binding", 1)]),
            },
        );
        counters.record_autoload();
        counters.record_literal_intern(false);
        counters.record_literal_intern(true);
        counters.record_string_concat_fast_path(false);
        counters.record_string_concat_fast_path(true);
        counters.record_concat_prealloc_hit();
        counters.record_concat_fallback("scalar_conversion");
        counters.record_packed_dim_fast_path(false);
        counters.record_packed_dim_fast_path(true);
        counters.record_array_packed_append_fast_path_hit();
        counters.record_array_packed_read_fast_path_hit();
        counters.record_array_sequential_foreach_fast_path_hit();
        counters.record_cow_or_reference_fallback();
        counters.record_array_count_fast_path_hit();
        counters.record_array_packed_to_mixed_transition();
        counters.record_array_shape_observed(PhpArrayShapeKind::ShapeStableRecordLike);
        counters.record_array_shape_observed(PhpArrayShapeKind::SmallInlineMap);
        counters.record_record_shape_lookup_hit();
        counters.record_record_shape_lookup_miss();
        counters.record_small_map_lookup_hit();
        counters.record_small_map_lookup_miss();
        counters.record_array_shape_lookup_fallback(PhpArrayShapeLookupFallback::KeyCoercion);
        counters.record_array_shape_lookup_fallback(PhpArrayShapeLookupFallback::OrderSemantics);
        counters.record_numeric_string_cache_stats(
            php_runtime::numeric_string::NumericStringCacheStats {
                classify_calls: 7,
                hits: 2,
                misses: 3,
                warning_sensitive_fallbacks: 4,
                overflow_precision_fallbacks: 5,
            },
        );
        counters.record_numeric_string_specialization_hit();
        counters.record_typecheck_fast_path(false);
        counters.record_typecheck_fast_path(true);
        counters.record_output_stats(
            12,
            OutputStats {
                appends: 3,
                batch_writes: 1,
                batched_appends: 1,
                batch_bytes: 7,
                flushes: 2,
                fast_appends: 2,
                slow_appends_by_reason: BTreeMap::from([("object_to_string".to_string(), 1)]),
            },
        );
        counters.record_internal_function_dispatch();
        counters.record_internal_function_dispatch_cache(false);
        counters.record_internal_function_dispatch_cache(true);
        counters.record_internal_count_array_direct_fast_path_hit();
        counters.record_function_call_ic(false);
        counters.record_function_call_ic(true);
        counters.record_builtin_call_ic(false);
        counters.record_builtin_call_ic(true);
        counters.record_builtin_fast_stub("strlen", false);
        counters.record_builtin_fast_stub("strlen", true);
        counters.record_builtin_fast_stub_fallback("strlen", "type");
        counters.record_builtin_intrinsic_candidate();
        counters.record_intrinsic("str_contains", true);
        counters.record_intrinsic("str_contains", false);
        counters.record_intrinsic_fallback("str_contains", "type");
        counters
            .specialized_builtin_opcode_hits
            .insert("strlen".to_string(), 1);
        counters.record_call_ic_megamorphic_fallback();
        counters.record_property_ic_fallback("layout_epoch_mismatch");
        counters.record_property_assign_ic_fallback("layout_epoch_mismatch");
        counters.record_property_assign_ic_fallback("visibility_mismatch");
        counters.record_property_assign_ic_fallback("type_mismatch");
        counters.record_property_assign_ic_fallback("readonly_property");
        counters.record_property_assign_ic_fallback("property_hook_present");
        counters.record_property_assign_ic_fallback("reference_slot");
        counters.record_property_assign_ic_fallback("dynamic_property_fallback");
        counters.record_local_slot_fast_path(false);
        counters.record_local_slot_fast_path(true);
        counters.record_quickening(QuickeningObservation {
            specialization: Some(crate::QuickeningSpecialization::ConcatStringString),
            attempt: true,
            specialized: true,
            guard_hit: true,
            guard_miss: true,
            guard_failure: true,
            fallback_call: true,
            dequickened: true,
            megamorphic: true,
            disabled: true,
        });
        counters.record_adaptive_tiny_unit_setup_skip();
        counters.record_native_candidate();
        counters.record_native_platform_unavailable();
        counters.record_native_eligibility_rejection("reference_arg");
        counters.record_jit_compile_attempt();
        counters.record_jit_compiled();
        counters.record_jit_compile_metadata(64, 1_500);
        counters.record_jit_executed();
        counters.record_jit_bailout();
        counters.record_jit_side_exit("helper_status");
        counters.record_jit_guard_failure();
        counters.record_jit_blacklisted_region("too_many_side_exits");
        counters.record_jit_helper_call();
        counters.record_jit_compile_cache_hit();
        counters.record_jit_compile_cache_miss();
        counters.record_jit_compile_cache_invalidation();
        counters.record_include_graph_hit();
        counters.record_include_graph_miss();
        counters.record_autoload_graph_hit();
        counters.record_autoload_graph_miss();
        counters.record_negative_lookup_hit();
        counters.record_invalidation_by_reason("file_fingerprint_changed");
        counters.record_fallback_by_path_semantics("missing_path");
        counters.record_inline_cache(InlineCacheObservation {
            candidate: true,
            slot_allocated: true,
            kind: Some(InlineCacheKind::FunctionCall),
            ..InlineCacheObservation::empty()
        });
        counters.record_inline_cache(InlineCacheObservation {
            candidate: true,
            slot_allocated: true,
            kind: Some(InlineCacheKind::MethodCall),
            hit: true,
            miss: true,
            guard_failure: true,
            fallback_call: true,
            monomorphic: true,
            polymorphic: true,
            megamorphic: true,
            disabled: true,
            ..InlineCacheObservation::empty()
        });
        counters.record_method_direct_dispatch_hit();
        counters.record_method_direct_dispatch_fallback();
        counters.record_method_tiny_inline_candidate();
        counters.record_method_tiny_inline_rejection("not_final_or_private");
        counters.record_inline_cache(InlineCacheObservation {
            candidate: true,
            slot_allocated: true,
            kind: Some(InlineCacheKind::PropertyFetch),
            hit: true,
            miss: true,
            guard_failure: true,
            polymorphic: true,
            ..InlineCacheObservation::empty()
        });
        counters.record_inline_cache(InlineCacheObservation {
            candidate: true,
            slot_allocated: true,
            kind: Some(InlineCacheKind::PropertyAssign),
            hit: true,
            miss: true,
            guard_failure: true,
            fallback_call: true,
            ..InlineCacheObservation::empty()
        });
        counters.record_inline_cache(InlineCacheObservation {
            candidate: true,
            slot_allocated: true,
            kind: Some(InlineCacheKind::DimFetch),
            ..InlineCacheObservation::empty()
        });
        counters.record_inline_cache(InlineCacheObservation {
            candidate: true,
            slot_allocated: true,
            kind: Some(InlineCacheKind::ClassConstantStaticProperty),
            hit: true,
            miss: true,
            guard_failure: true,
            ..InlineCacheObservation::empty()
        });
        counters.record_inline_cache(InlineCacheObservation {
            candidate: true,
            slot_allocated: true,
            kind: Some(InlineCacheKind::IncludePath),
            hit: true,
            miss: true,
            invalidation: true,
            guard_failure: true,
            ..InlineCacheObservation::empty()
        });
        counters.record_inline_cache(InlineCacheObservation {
            candidate: true,
            slot_allocated: true,
            kind: Some(InlineCacheKind::AutoloadClassLookup),
            hit: true,
            miss: true,
            invalidation: true,
            guard_failure: true,
            ..InlineCacheObservation::empty()
        });

        assert_eq!(counters.instructions_executed, 2);
        assert_eq!(counters.function_calls, 1);
        assert_eq!(counters.frame_allocations, 1);
        assert_eq!(counters.frame_reuses, 1);
        assert_eq!(counters.frames_allocated, 1);
        assert_eq!(counters.frames_reused, 1);
        assert_eq!(counters.register_files_allocated, 1);
        assert_eq!(counters.register_files_reused, 1);
        assert_eq!(counters.request_arena_allocations, 1);
        assert!(counters.request_arena_bytes > 0);
        assert_eq!(counters.request_pool_resets, 1);
        assert_eq!(counters.persistent_engine_allocations, 0);
        assert_eq!(counters.persistent_engine_bytes, 0);
        assert_eq!(
            counters
                .arena_fallback_allocations_by_reason
                .get("by_ref_param"),
            Some(&1)
        );
        assert_eq!(counters.destructor_sensitive_arena_blocks, 0);
        assert_eq!(
            counters.frame_reuse_blocked_by_reason.get("by_ref_param"),
            Some(&1)
        );
        assert_eq!(
            counters.call_frame_layout_observed.get("tiny_leaf_frame"),
            Some(&1)
        );
        assert_eq!(counters.tiny_frame_candidates, 1);
        assert_eq!(counters.specialized_frame_hits, 1);
        assert_eq!(
            counters
                .generic_frame_fallback_by_reason
                .get("class_context"),
            Some(&1)
        );
        assert_eq!(counters.arg_array_avoided, 1);
        assert_eq!(counters.heap_frame_avoided, 1);
        assert_eq!(
            counters.frame_alias_state.get("no_references_observed"),
            Some(&1)
        );
        assert_eq!(
            counters.frame_alias_state.get("local_only_reference"),
            Some(&1)
        );
        assert_eq!(
            counters
                .alias_state_transitions
                .get("no_references_observed->local_only_reference"),
            Some(&1)
        );
        assert_eq!(counters.fast_path_disabled_by_reference, 7);
        assert_eq!(counters.dequickened_by_reference, 1);
        assert_eq!(counters.ic_invalidated_by_reference, 2);
        assert_eq!(counters.dense_bytecode_fallback_by_reference, 1);
        assert_eq!(counters.value_clones, 11);
        assert_eq!(counters.string_allocations, 7);
        assert_eq!(counters.array_handle_clones, 5);
        assert_eq!(counters.cow_separations, 3);
        assert_eq!(counters.reference_cell_creations, 2);
        assert_eq!(
            counters
                .value_clone_by_source_family
                .get("stack_register_local_move"),
            Some(&5)
        );
        assert_eq!(
            counters.value_clone_by_source_family.get("return_value"),
            Some(&2)
        );
        assert_eq!(
            counters
                .array_handle_clone_by_source_family
                .get("array_element_read"),
            Some(&3)
        );
        assert_eq!(
            counters
                .cow_separation_by_source_family
                .get("by_ref_argument_binding"),
            Some(&2)
        );
        assert_eq!(
            counters
                .reference_cell_creation_by_source_family
                .get("by_ref_argument_binding"),
            Some(&1)
        );
        assert_eq!(counters.object_allocations, 1);
        assert_eq!(counters.array_packed_direct_gets, 13);
        assert_eq!(counters.array_mixed_indexed_gets, 17);
        assert_eq!(counters.array_linear_scan_fallbacks, 19);
        assert_eq!(counters.array_metadata_recomputes, 23);
        assert_eq!(counters.symbol_map_lookups, 29);
        assert_eq!(counters.symbol_linear_fallbacks, 31);
        assert_eq!(counters.symbol_intern_hits, 37);
        assert_eq!(counters.symbol_intern_misses, 41);
        assert_eq!(counters.string_hash_cache_hits, 43);
        assert_eq!(counters.string_hash_cache_misses, 47);
        assert_eq!(counters.symbol_eq_fast_hits, 53);
        assert_eq!(counters.symbol_eq_byte_fallbacks, 59);
        assert_eq!(counters.object_declared_slot_reads, 61);
        assert_eq!(counters.object_declared_slot_writes, 67);
        assert_eq!(counters.object_dynamic_property_map_reads, 71);
        assert_eq!(counters.object_dynamic_property_map_writes, 73);
        assert_eq!(counters.packed_values_storage_arrays, 79);
        assert_eq!(counters.packed_values_storage_reads, 83);
        assert_eq!(counters.packed_values_storage_appends, 89);
        assert_eq!(counters.packed_virtual_key_iterations, 97);
        assert_eq!(
            counters
                .packed_to_mixed_by_reason
                .get("string_key")
                .copied(),
            Some(101)
        );
        assert_eq!(
            counters
                .packed_to_mixed_by_reason
                .get("non_sequential_int_key")
                .copied(),
            Some(103)
        );
        assert_eq!(
            counters
                .packed_to_mixed_by_reason
                .get("append_key_gap")
                .copied(),
            Some(107)
        );
        assert_eq!(
            counters
                .packed_to_mixed_by_reason
                .get("unset_hole")
                .copied(),
            Some(109)
        );
        assert_eq!(counters.string_concats, 1);
        assert_eq!(counters.packed_dim_fast_path_hits, 1);
        assert_eq!(counters.packed_dim_fast_path_misses, 1);
        assert_eq!(counters.array_packed_append_fast_path_hits, 1);
        assert_eq!(counters.array_packed_read_fast_path_hits, 1);
        assert_eq!(counters.array_sequential_foreach_fast_path_hits, 1);
        assert_eq!(
            counters.array_fast_path_hits_by_family.get("packed_append"),
            Some(&1)
        );
        assert_eq!(
            counters
                .array_fast_path_hits_by_family
                .get("packed_foreach_by_value"),
            Some(&1)
        );
        assert_eq!(
            counters
                .array_fast_path_fallback_by_reason
                .get("cow_or_reference"),
            Some(&1)
        );
        assert_eq!(counters.packed_append_fast_hits, 1);
        assert_eq!(counters.packed_foreach_fast_hits, 1);
        assert_eq!(counters.cow_or_reference_fallbacks, 1);
        assert_eq!(counters.array_count_fast_path_hits, 1);
        assert_eq!(counters.array_packed_to_mixed_transitions, 1);
        assert_eq!(
            counters
                .array_shape_observed_by_kind
                .get("shape_stable_record_like"),
            Some(&1)
        );
        assert_eq!(
            counters
                .array_shape_observed_by_kind
                .get("small_inline_map"),
            Some(&1)
        );
        assert_eq!(counters.record_shape_hits, 1);
        assert_eq!(counters.record_shape_misses, 1);
        assert_eq!(counters.small_map_hits, 1);
        assert_eq!(counters.small_map_misses, 1);
        assert_eq!(counters.key_coercion_fallbacks, 1);
        assert_eq!(counters.order_semantics_fallbacks, 1);
        assert_eq!(counters.numeric_string_classify_calls, 7);
        assert_eq!(counters.numeric_string_cache_hits, 2);
        assert_eq!(counters.numeric_string_cache_misses, 3);
        assert_eq!(counters.numeric_string_specialization_hits, 1);
        assert_eq!(counters.numeric_string_warning_sensitive_fallbacks, 4);
        assert_eq!(counters.numeric_string_overflow_precision_fallbacks, 5);
        assert_eq!(counters.typecheck_fast_path_hits, 1);
        assert_eq!(counters.typecheck_fast_path_misses, 1);
        assert_eq!(counters.output_bytes, 12);
        assert_eq!(counters.output_buffer_appends, 3);
        assert_eq!(counters.output_buffer_batch_writes, 1);
        assert_eq!(counters.output_batched_appends, 1);
        assert_eq!(counters.output_batch_bytes, 7);
        assert_eq!(counters.output_buffer_flushes, 2);
        assert_eq!(counters.output_fast_appends, 2);
        assert_eq!(
            counters
                .output_slow_appends_by_reason
                .get("object_to_string"),
            Some(&1)
        );
        assert_eq!(
            counters
                .slow_path_calls_by_reason
                .get("output.object_to_string"),
            Some(&1)
        );
        assert_eq!(counters.internal_function_dispatches, 1);
        assert_eq!(counters.internal_function_dispatch_cache_hits, 1);
        assert_eq!(counters.internal_function_dispatch_cache_misses, 1);
        assert_eq!(counters.internal_count_array_direct_fast_path_hits, 1);
        assert_eq!(counters.function_call_ic_hits, 1);
        assert_eq!(counters.function_call_ic_misses, 1);
        assert_eq!(counters.builtin_call_ic_hits, 1);
        assert_eq!(counters.builtin_call_ic_misses, 1);
        assert_eq!(counters.builtin_fast_stub_hits.get("strlen"), Some(&1));
        assert_eq!(counters.builtin_fast_stub_misses.get("strlen"), Some(&1));
        assert_eq!(
            counters
                .builtin_fast_stub_fallback_by_reason
                .get("strlen.type"),
            Some(&1)
        );
        assert_eq!(counters.builtin_intrinsic_candidates, 1);
        assert_eq!(counters.intrinsic_hits.get("str_contains"), Some(&1));
        assert_eq!(counters.intrinsic_misses.get("str_contains"), Some(&1));
        assert_eq!(
            counters
                .intrinsic_fallback_by_reason
                .get("str_contains.type"),
            Some(&1)
        );
        assert_eq!(
            counters
                .slow_path_calls_by_reason
                .get("builtin_stub.strlen.type"),
            Some(&1)
        );
        assert_eq!(
            counters
                .slow_path_calls_by_reason
                .get("builtin_intrinsic.str_contains.type"),
            Some(&1)
        );
        assert_eq!(
            counters.specialized_builtin_opcode_hits.get("strlen"),
            Some(&1)
        );
        assert_eq!(counters.call_ic_megamorphic_fallbacks, 1);
        assert_eq!(
            counters
                .property_ic_fallback_reasons
                .get("layout_epoch_mismatch"),
            Some(&1)
        );
        assert_eq!(
            counters
                .slow_path_calls_by_reason
                .get("property_fetch.layout_epoch_mismatch"),
            Some(&1)
        );
        assert_eq!(counters.property_assign_ic_hits, 1);
        assert_eq!(counters.property_assign_ic_misses, 1);
        assert_eq!(counters.property_assign_ic_guard_failures, 1);
        assert_eq!(counters.property_assign_ic_shape_exits, 1);
        assert_eq!(counters.property_assign_ic_visibility_exits, 1);
        assert_eq!(counters.property_assign_ic_type_exits, 1);
        assert_eq!(counters.property_assign_ic_readonly_exits, 1);
        assert_eq!(counters.property_assign_ic_hook_magic_exits, 1);
        assert_eq!(counters.property_assign_ic_reference_exits, 1);
        assert_eq!(counters.property_assign_ic_dynamic_exits, 1);
        assert_eq!(
            counters
                .property_assign_ic_fallback_reasons
                .get("type_mismatch"),
            Some(&1)
        );
        assert_eq!(
            counters
                .slow_path_calls_by_reason
                .get("property_assign.type_mismatch"),
            Some(&1)
        );
        assert_eq!(counters.local_slot_fast_path_hits, 1);
        assert_eq!(counters.local_slot_fast_path_misses, 1);
        assert_eq!(counters.string_concat_fast_path_hits, 1);
        assert_eq!(counters.string_concat_fast_path_misses, 1);
        assert_eq!(counters.concat_prealloc_hits, 1);
        assert_eq!(
            counters.concat_fallback_by_reason.get("scalar_conversion"),
            Some(&1)
        );
        assert_eq!(
            counters
                .slow_path_calls_by_reason
                .get("concat.scalar_conversion"),
            Some(&1)
        );
        assert_eq!(counters.autoloads, 1);
        assert_eq!(counters.literal_intern_hits, 1);
        assert_eq!(counters.literal_intern_misses, 1);
        assert_eq!(counters.quickening_attempts, 1);
        assert_eq!(
            counters
                .quickening_candidates_by_family
                .get("string_concat"),
            Some(&1)
        );
        assert_eq!(counters.quickening_specialized, 1);
        assert_eq!(
            counters.quickening_applied_by_family.get("string_concat"),
            Some(&1)
        );
        assert_eq!(
            counters.quickened_executions_by_family.get("string_concat"),
            Some(&1)
        );
        assert_eq!(counters.quickening_guard_hits, 1);
        assert_eq!(counters.quickening_guard_misses, 1);
        assert_eq!(counters.quickening_guard_failures, 1);
        assert_eq!(
            counters
                .quickening_guard_failures_by_family
                .get("string_concat"),
            Some(&1)
        );
        assert_eq!(counters.quickening_fallback_calls, 1);
        assert_eq!(counters.quickening_dequickens, 1);
        assert_eq!(
            counters
                .quickening_dequickened_by_reason
                .get("guard_failure_threshold"),
            Some(&1)
        );
        assert_eq!(counters.quickening_megamorphic, 1);
        assert_eq!(counters.quickening_disabled, 1);
        assert_eq!(counters.adaptive_tiny_unit_setup_skips, 1);
        assert_eq!(counters.native_candidates, 1);
        assert_eq!(counters.native_platform_unavailable, 1);
        assert_eq!(
            counters
                .native_eligibility_rejections_by_reason
                .get("reference_arg"),
            Some(&1)
        );
        assert_eq!(counters.jit_compile_attempts, 1);
        assert_eq!(counters.jit_compiled, 1);
        assert_eq!(counters.native_compiled_regions, 1);
        assert_eq!(counters.jit_code_bytes, 64);
        assert_eq!(counters.jit_compile_time_nanos, 1_500);
        assert_eq!(counters.jit_executed, 1);
        assert_eq!(counters.native_executions, 1);
        assert_eq!(counters.jit_bailouts, 1);
        assert_eq!(counters.jit_side_exits, 1);
        assert_eq!(
            counters.jit_side_exit_reasons.get("helper_status"),
            Some(&1)
        );
        assert_eq!(
            counters.native_side_exits_by_reason.get("helper_status"),
            Some(&1)
        );
        assert_eq!(counters.jit_guard_failures, 1);
        assert_eq!(counters.jit_blacklisted_regions, 1);
        assert_eq!(
            counters.jit_blacklist_reasons.get("too_many_side_exits"),
            Some(&1)
        );
        assert_eq!(
            counters
                .native_blacklist_suppression_by_unstable_region
                .get("too_many_side_exits"),
            Some(&1)
        );
        assert_eq!(counters.jit_helper_calls, 1);
        counters.record_jit_fast_path_hit();
        counters.record_packed_fetch_fast_hit();
        counters.record_packed_fetch_bounds_exit();
        counters.record_packed_fetch_layout_exit();
        counters.record_packed_foreach_sum_fast_hit();
        counters.record_packed_foreach_sum_layout_exit();
        counters.record_packed_foreach_sum_overflow_exit();
        counters.record_known_call_fast_hit();
        counters.record_known_call_guard_exit();
        counters.record_known_call_slow_call();
        counters.record_property_load_fast_hit();
        counters.record_property_load_guard_exit();
        counters.record_property_load_layout_exit();
        counters.record_property_load_uninitialized_exit();
        counters.record_property_load_slow_call();
        counters.record_jit_overflow_exit();
        counters.record_jit_slow_path_call();
        assert_eq!(counters.jit_fast_path_hits, 1);
        assert_eq!(counters.packed_fetch_fast_hits, 1);
        assert_eq!(counters.packed_fetch_bounds_exits, 1);
        assert_eq!(counters.packed_fetch_layout_exits, 1);
        assert_eq!(counters.packed_fetch_bounds_fallbacks, 1);
        assert_eq!(counters.packed_fetch_layout_fallbacks, 1);
        assert_eq!(counters.packed_foreach_sum_fast_hits, 1);
        assert_eq!(counters.packed_foreach_sum_layout_exits, 1);
        assert_eq!(counters.packed_foreach_sum_overflow_exits, 1);
        assert_eq!(
            counters
                .array_fast_path_hits_by_family
                .get("packed_int_fetch"),
            Some(&1)
        );
        assert_eq!(
            counters
                .array_fast_path_hits_by_family
                .get("packed_int_sum"),
            Some(&1)
        );
        assert_eq!(
            counters.array_fast_path_fallback_by_reason.get("bounds"),
            Some(&1)
        );
        assert_eq!(
            counters
                .array_fast_path_fallback_by_reason
                .get("layout_or_key"),
            Some(&1)
        );
        assert_eq!(
            counters
                .array_fast_path_fallback_by_reason
                .get("layout_or_element"),
            Some(&1)
        );
        assert_eq!(
            counters.array_fast_path_fallback_by_reason.get("overflow"),
            Some(&1)
        );
        assert_eq!(
            counters.slow_path_calls_by_reason.get("array.bounds"),
            Some(&1)
        );
        assert_eq!(
            counters
                .slow_path_calls_by_reason
                .get("array.layout_or_key"),
            Some(&1)
        );
        assert_eq!(
            counters
                .slow_path_calls_by_reason
                .get("array.layout_or_element"),
            Some(&1)
        );
        assert_eq!(
            counters.slow_path_calls_by_reason.get("array.overflow"),
            Some(&1)
        );
        assert_eq!(counters.known_call_fast_hits, 1);
        assert_eq!(counters.known_call_guard_exits, 1);
        assert_eq!(counters.known_call_slow_calls, 1);
        assert_eq!(counters.property_load_fast_hits, 1);
        assert_eq!(counters.property_load_guard_exits, 1);
        assert_eq!(counters.property_load_layout_exits, 1);
        assert_eq!(counters.property_load_uninitialized_exits, 1);
        assert_eq!(counters.property_load_slow_calls, 1);
        assert_eq!(counters.jit_overflow_exits, 1);
        assert_eq!(counters.jit_slow_path_calls, 1);
        assert_eq!(
            counters.slow_path_calls_by_reason.get("jit.known_call"),
            Some(&1)
        );
        assert_eq!(
            counters.slow_path_calls_by_reason.get("jit.property_load"),
            Some(&1)
        );
        assert_eq!(
            counters.slow_path_calls_by_reason.get("jit.generic"),
            Some(&1)
        );
        assert_eq!(counters.jit_compile_cache_hits, 1);
        assert_eq!(counters.jit_compile_cache_misses, 1);
        assert_eq!(counters.jit_compile_cache_invalidations, 1);
        assert_eq!(counters.include_graph_hits, 2);
        assert_eq!(counters.include_graph_misses, 2);
        assert_eq!(counters.autoload_graph_hits, 2);
        assert_eq!(counters.autoload_graph_misses, 2);
        assert_eq!(counters.negative_lookup_hits, 1);
        assert_eq!(
            counters
                .invalidations_by_reason
                .get("file_fingerprint_changed"),
            Some(&1)
        );
        assert_eq!(
            counters.fallback_by_path_semantics.get("missing_path"),
            Some(&1)
        );
        assert_eq!(
            counters
                .slow_path_calls_by_reason
                .get("include_autoload.missing_path"),
            Some(&1)
        );
        assert_eq!(counters.inline_cache_observations, 8);
        assert_eq!(counters.inline_cache_slots, 8);
        assert_eq!(counters.inline_cache_function_slots, 1);
        assert_eq!(counters.inline_cache_method_slots, 1);
        assert_eq!(counters.inline_cache_property_slots, 1);
        assert_eq!(counters.inline_cache_property_assign_slots, 1);
        assert_eq!(counters.inline_cache_dim_slots, 1);
        assert_eq!(
            counters.inline_cache_class_constant_static_property_slots,
            1
        );
        assert_eq!(counters.inline_cache_class_relation_slots, 0);
        assert_eq!(counters.inline_cache_include_path_slots, 1);
        assert_eq!(counters.inline_cache_autoload_class_lookup_slots, 1);
        assert_eq!(counters.method_ic_hits, 1);
        assert_eq!(counters.method_ic_misses, 1);
        assert_eq!(counters.method_ic_polymorphic_hits, 1);
        assert_eq!(counters.method_ic_guard_failures, 1);
        assert_eq!(counters.method_direct_dispatch_hits, 1);
        assert_eq!(counters.method_direct_dispatch_fallbacks, 1);
        assert_eq!(counters.method_tiny_inline_candidates, 1);
        assert_eq!(
            counters
                .method_tiny_inline_rejected_by_reason
                .get("not_final_or_private"),
            Some(&1)
        );
        assert_eq!(counters.inline_cache_fallback_calls, 2);
        assert_eq!(counters.inline_cache_monomorphic, 1);
        assert_eq!(counters.inline_cache_polymorphic, 2);
        assert_eq!(counters.inline_cache_megamorphic, 1);
        assert_eq!(counters.inline_cache_disabled, 1);
        assert_eq!(counters.property_ic_hits, 1);
        assert_eq!(counters.property_ic_misses, 1);
        assert_eq!(counters.property_ic_guard_failures, 1);
        assert_eq!(counters.class_static_ic_hits, 1);
        assert_eq!(counters.class_static_ic_misses, 1);
        assert_eq!(counters.class_static_ic_guard_failures, 1);
        assert_eq!(counters.include_path_ic_hits, 1);
        assert_eq!(counters.include_path_ic_misses, 1);
        assert_eq!(counters.include_path_ic_invalidations, 1);
        assert_eq!(counters.include_path_ic_guard_failures, 1);
        assert_eq!(counters.autoload_class_lookup_ic_hits, 1);
        assert_eq!(counters.autoload_class_lookup_ic_misses, 1);
        assert_eq!(counters.autoload_class_lookup_ic_invalidations, 1);
        assert_eq!(counters.autoload_class_lookup_ic_guard_failures, 1);
        assert_eq!(counters.opcodes["binary_concat"], 1);
        assert_eq!(counters.opcodes["call_function"], 1);
    }

    #[test]
    fn counters_json_is_stable_and_parseable() {
        let mut counters = VmCounters::default();
        counters.record_instruction(&InstructionKind::Echo {
            src: php_ir::operand::Operand::Register(RegId::new(0)),
        });
        let mut candidates = BTreeMap::new();
        candidates.insert("load_const_echo".to_string(), 1);
        let mut emitted = BTreeMap::new();
        emitted.insert("load_const_echo".to_string(), 1);
        let mut skipped = BTreeMap::new();
        skipped.insert("unsupported_producer_echo_pair".to_string(), 1);
        counters.record_superinstruction_selection(1, &candidates, &emitted, &skipped);
        counters.record_superinstruction_executed("load_const_echo");
        counters.record_optimized_exit_snapshot("quickening", "int_add");
        counters.record_optimized_exit_materialized();
        counters.record_snapshot_rejection_by_missing_state_family("foreach_iterators");
        counters.record_fallback_resume_success();
        counters.fold_scratch_counters();

        let json = counters.to_json();

        assert!(json.contains("\"instructions_executed\": 1"));
        assert!(json.contains("\"superinstruction_candidates\": 1"));
        assert!(json.contains("\"superinstruction_candidates_by_kind\": {"));
        assert!(json.contains("\"superinstructions_emitted\": 1"));
        assert!(json.contains("\"superinstructions_emitted_by_kind\": {"));
        assert!(json.contains("\"superinstructions_executed\": {"));
        assert!(json.contains("\"load_const_echo\": 1"));
        assert!(json.contains("\"superinstruction_deopt_or_fallbacks\": 0"));
        assert!(json.contains("\"superinstruction_deopt_or_fallback_by_reason\": {}"));
        assert!(json.contains("\"superinstruction_skipped_by_reason\": {"));
        assert!(json.contains("\"optimized_exit_snapshots_created\": 1"));
        assert!(json.contains("\"optimized_exit_snapshots_materialized\": 1"));
        assert!(json.contains("\"optimized_exits_by_reason\": {"));
        assert!(json.contains("\"quickening.int_add\": 1"));
        assert!(json.contains("\"snapshot_rejection_by_missing_state_family\": {"));
        assert!(json.contains("\"foreach_iterators\": 1"));
        assert!(json.contains("\"fallback_resume_successes\": 1"));
        assert!(json.contains("\"guard_failures\": 0"));
        assert!(json.contains("\"frame_allocations\": 0"));
        assert!(json.contains("\"frame_reuses\": 0"));
        assert!(json.contains("\"frames_allocated\": 0"));
        assert!(json.contains("\"frames_reused\": 0"));
        assert!(json.contains("\"register_files_allocated\": 0"));
        assert!(json.contains("\"register_files_reused\": 0"));
        assert!(json.contains("\"frame_reuse_blocked_by_reason\": {}"));
        assert!(json.contains("\"call_frame_layout_observed\": {}"));
        assert!(json.contains("\"tiny_frame_candidates\": 0"));
        assert!(json.contains("\"specialized_frame_hits\": 0"));
        assert!(json.contains("\"generic_frame_fallback_by_reason\": {}"));
        assert!(json.contains("\"arg_array_avoided\": 0"));
        assert!(json.contains("\"heap_frame_avoided\": 0"));
        assert!(json.contains("\"frame_alias_state\": {}"));
        assert!(json.contains("\"alias_state_transitions\": {}"));
        assert!(json.contains("\"fast_path_disabled_by_reference\": 0"));
        assert!(json.contains("\"dequickened_by_reference\": 0"));
        assert!(json.contains("\"IC_invalidated_by_reference\": 0"));
        assert!(json.contains("\"dense_bytecode_fallback_by_reference\": 0"));
        assert!(json.contains("\"request_arena_allocations\": 0"));
        assert!(json.contains("\"request_arena_bytes\": 0"));
        assert!(json.contains("\"request_pool_resets\": 0"));
        assert!(json.contains("\"persistent_engine_allocations\": 0"));
        assert!(json.contains("\"persistent_engine_bytes\": 0"));
        assert!(json.contains("\"arena_fallback_allocations_by_reason\": {}"));
        assert!(json.contains("\"destructor_sensitive_arena_blocks\": 0"));
        assert!(json.contains("\"value_clones\": 0"));
        assert!(json.contains("\"string_allocations\": 0"));
        assert!(json.contains("\"array_handle_clones\": 0"));
        assert!(json.contains("\"cow_separations\": 0"));
        assert!(json.contains("\"reference_cell_creations\": 0"));
        assert!(json.contains("\"object_allocations\": 0"));
        assert!(json.contains("\"array_packed_direct_gets\": 0"));
        assert!(json.contains("\"array_mixed_indexed_gets\": 0"));
        assert!(json.contains("\"array_linear_scan_fallbacks\": 0"));
        assert!(json.contains("\"array_metadata_recomputes\": 0"));
        assert!(json.contains("\"symbol_map_lookups\": 0"));
        assert!(json.contains("\"symbol_linear_fallbacks\": 0"));
        assert!(json.contains("\"packed_dim_fast_path_hits\": 0"));
        assert!(json.contains("\"packed_dim_fast_path_misses\": 0"));
        assert!(json.contains("\"array_packed_append_fast_path_hits\": 0"));
        assert!(json.contains("\"array_packed_read_fast_path_hits\": 0"));
        assert!(json.contains("\"array_sequential_foreach_fast_path_hits\": 0"));
        assert!(json.contains("\"array_fast_path_hits_by_family\": {}"));
        assert!(json.contains("\"array_fast_path_fallback_by_reason\": {}"));
        assert!(json.contains("\"array_shape_observed_by_kind\": {}"));
        assert!(json.contains("\"record_shape_hits\": 0"));
        assert!(json.contains("\"record_shape_misses\": 0"));
        assert!(json.contains("\"small_map_hits\": 0"));
        assert!(json.contains("\"small_map_misses\": 0"));
        assert!(json.contains("\"key_coercion_fallbacks\": 0"));
        assert!(json.contains("\"order_semantics_fallbacks\": 0"));
        assert!(json.contains("\"packed_append_fast_hits\": 0"));
        assert!(json.contains("\"packed_foreach_fast_hits\": 0"));
        assert!(json.contains("\"cow_or_reference_fallbacks\": 0"));
        assert!(json.contains("\"array_count_fast_path_hits\": 0"));
        assert!(json.contains("\"array_packed_to_mixed_transitions\": 0"));
        assert!(json.contains("\"numeric_string_classify_calls\": 0"));
        assert!(json.contains("\"numeric_string_cache_hits\": 0"));
        assert!(json.contains("\"numeric_string_cache_misses\": 0"));
        assert!(json.contains("\"numeric_string_specialization_hits\": 0"));
        assert!(json.contains("\"numeric_string_warning_sensitive_fallbacks\": 0"));
        assert!(json.contains("\"numeric_string_overflow_precision_fallbacks\": 0"));
        assert!(json.contains("\"typecheck_fast_path_hits\": 0"));
        assert!(json.contains("\"typecheck_fast_path_misses\": 0"));
        assert!(json.contains("\"output_bytes\": 0"));
        assert!(json.contains("\"output_buffer_appends\": 0"));
        assert!(json.contains("\"output_buffer_batch_writes\": 0"));
        assert!(json.contains("\"output_batched_appends\": 0"));
        assert!(json.contains("\"output_batch_bytes\": 0"));
        assert!(json.contains("\"output_buffer_flushes\": 0"));
        assert!(json.contains("\"output_fast_appends\": 0"));
        assert!(json.contains("\"output_slow_appends_by_reason\": {}"));
        assert!(json.contains("\"internal_function_dispatches\": 0"));
        assert!(json.contains("\"internal_function_dispatch_cache_hits\": 0"));
        assert!(json.contains("\"internal_function_dispatch_cache_misses\": 0"));
        assert!(json.contains("\"internal_count_array_direct_fast_path_hits\": 0"));
        assert!(json.contains("\"function_call_ic_hits\": 0"));
        assert!(json.contains("\"function_call_ic_misses\": 0"));
        assert!(json.contains("\"builtin_call_ic_hits\": 0"));
        assert!(json.contains("\"builtin_call_ic_misses\": 0"));
        assert!(json.contains("\"builtin_fast_stub_hits\": {}"));
        assert!(json.contains("\"builtin_fast_stub_misses\": {}"));
        assert!(json.contains("\"builtin_fast_stub_fallback_by_reason\": {}"));
        assert!(json.contains("\"builtin_intrinsic_candidates\": 0"));
        assert!(json.contains("\"intrinsic_hits\": {}"));
        assert!(json.contains("\"intrinsic_misses\": {}"));
        assert!(json.contains("\"intrinsic_fallback_by_reason\": {}"));
        assert!(json.contains("\"specialized_builtin_opcode_hits\": {}"));
        assert!(json.contains("\"slow_path_calls_by_reason\": {}"));
        assert!(json.contains("\"call_ic_megamorphic_fallbacks\": 0"));
        assert!(json.contains("\"local_slot_fast_path_hits\": 0"));
        assert!(json.contains("\"local_slot_fast_path_misses\": 0"));
        assert!(json.contains("\"literal_intern_hits\": 0"));
        assert!(json.contains("\"literal_intern_misses\": 0"));
        assert!(json.contains("\"string_concat_fast_path_hits\": 0"));
        assert!(json.contains("\"string_concat_fast_path_misses\": 0"));
        assert!(json.contains("\"concat_prealloc_hits\": 0"));
        assert!(json.contains("\"concat_fallback_by_reason\": {}"));
        assert!(json.contains("\"quickening_attempts\": 0"));
        assert!(json.contains("\"quickening_candidates_by_family\": {}"));
        assert!(json.contains("\"quickening_specialized\": 0"));
        assert!(json.contains("\"quickening_applied_by_family\": {}"));
        assert!(json.contains("\"quickened_executions_by_family\": {}"));
        assert!(json.contains("\"quickening_guard_hits\": 0"));
        assert!(json.contains("\"quickening_guard_misses\": 0"));
        assert!(json.contains("\"quickening_guard_failures\": 0"));
        assert!(json.contains("\"quickening_guard_failures_by_family\": {}"));
        assert!(json.contains("\"quickening_fallback_calls\": 0"));
        assert!(json.contains("\"quickening_dequickens\": 0"));
        assert!(json.contains("\"quickening_dequickened_by_reason\": {}"));
        assert!(json.contains("\"quickening_megamorphic\": 0"));
        assert!(json.contains("\"quickening_disabled\": 0"));
        assert!(json.contains("\"adaptive_tiny_unit_setup_skips\": 0"));
        assert!(json.contains("\"native_candidates\": 0"));
        assert!(json.contains("\"native_compiled_regions\": 0"));
        assert!(json.contains("\"native_executions\": 0"));
        assert!(json.contains("\"native_compile_budget_rejections\": 0"));
        assert!(json.contains("\"native_eligibility_rejections_by_reason\": {}"));
        assert!(json.contains("\"native_side_exits_by_reason\": {}"));
        assert!(json.contains("\"native_blacklist_suppression_by_unstable_region\": {}"));
        assert!(json.contains("\"native_platform_unavailable\": 0"));
        assert!(json.contains("\"jit_compile_attempts\": 0"));
        assert!(json.contains("\"jit_compiled\": 0"));
        assert!(json.contains("\"jit_executed\": 0"));
        assert!(json.contains("\"jit_bailouts\": 0"));
        assert!(json.contains("\"jit_code_bytes\": 0"));
        assert!(json.contains("\"jit_compile_time_nanos\": 0"));
        assert!(json.contains("\"jit_side_exits\": 0"));
        assert!(json.contains("\"jit_side_exit_reasons\": {}"));
        assert!(json.contains("\"jit_guard_failures\": 0"));
        assert!(json.contains("\"jit_blacklisted_regions\": 0"));
        assert!(json.contains("\"jit_blacklist_reasons\": {}"));
        assert!(json.contains("\"jit_tiering_cold_functions\": 0"));
        assert!(json.contains("\"jit_tiering_hot_functions\": 0"));
        assert!(json.contains("\"jit_tiering_eager_functions\": 0"));
        assert!(json.contains("\"jit_tiering_blacklist_rejections\": 0"));
        assert!(json.contains("\"jit_tiering_budget_rejections\": 0"));
        assert!(json.contains("\"jit_helper_calls\": 0"));
        assert!(json.contains("\"jit_fast_path_hits\": 0"));
        assert!(json.contains("\"packed_fetch_fast_hits\": 0"));
        assert!(json.contains("\"packed_fetch_bounds_exits\": 0"));
        assert!(json.contains("\"packed_fetch_layout_exits\": 0"));
        assert!(json.contains("\"packed_fetch_bounds_fallbacks\": 0"));
        assert!(json.contains("\"packed_fetch_layout_fallbacks\": 0"));
        assert!(json.contains("\"packed_foreach_sum_fast_hits\": 0"));
        assert!(json.contains("\"packed_foreach_sum_layout_exits\": 0"));
        assert!(json.contains("\"packed_foreach_sum_overflow_exits\": 0"));
        assert!(json.contains("\"known_call_fast_hits\": 0"));
        assert!(json.contains("\"known_call_guard_exits\": 0"));
        assert!(json.contains("\"known_call_slow_calls\": 0"));
        assert!(json.contains("\"direct_call_hits\": 0"));
        assert!(json.contains("\"direct_call_fallbacks\": 0"));
        assert!(json.contains("\"property_load_fast_hits\": 0"));
        assert!(json.contains("\"property_load_guard_exits\": 0"));
        assert!(json.contains("\"property_load_layout_exits\": 0"));
        assert!(json.contains("\"property_load_uninitialized_exits\": 0"));
        assert!(json.contains("\"property_load_slow_calls\": 0"));
        assert!(json.contains("\"jit_overflow_exits\": 0"));
        assert!(json.contains("\"jit_slow_path_calls\": 0"));
        assert!(json.contains("\"jit_compile_cache_hits\": 0"));
        assert!(json.contains("\"jit_compile_cache_misses\": 0"));
        assert!(json.contains("\"jit_compile_cache_invalidations\": 0"));
        assert!(json.contains("\"jit_compile_descriptors\": []"));
        assert!(json.contains("\"inline_cache_observations\": 0"));
        assert!(json.contains("\"inline_cache_slots\": 0"));
        assert!(json.contains("\"inline_cache_function_slots\": 0"));
        assert!(json.contains("\"inline_cache_method_slots\": 0"));
        assert!(json.contains("\"inline_cache_property_slots\": 0"));
        assert!(json.contains("\"inline_cache_property_assign_slots\": 0"));
        assert!(json.contains("\"inline_cache_dim_slots\": 0"));
        assert!(json.contains("\"inline_cache_class_constant_static_property_slots\": 0"));
        assert!(json.contains("\"inline_cache_class_relation_slots\": 0"));
        assert!(json.contains("\"inline_cache_include_path_slots\": 0"));
        assert!(json.contains("\"inline_cache_autoload_class_lookup_slots\": 0"));
        assert!(json.contains("\"inline_cache_hits\": 0"));
        assert!(json.contains("\"inline_cache_misses\": 0"));
        assert!(json.contains("\"inline_cache_invalidations\": 0"));
        assert!(json.contains("\"inline_cache_guard_failures\": 0"));
        assert!(json.contains("\"inline_cache_fallback_calls\": 0"));
        assert!(json.contains("\"inline_cache_monomorphic\": 0"));
        assert!(json.contains("\"inline_cache_polymorphic\": 0"));
        assert!(json.contains("\"inline_cache_megamorphic\": 0"));
        assert!(json.contains("\"inline_cache_disabled\": 0"));
        assert!(json.contains("\"include_graph_hits\": 0"));
        assert!(json.contains("\"include_graph_misses\": 0"));
        assert!(json.contains("\"include_resolution_hits\": 0"));
        assert!(json.contains("\"include_resolution_misses\": 0"));
        assert!(json.contains("\"include_compile_hits\": 0"));
        assert!(json.contains("\"include_compile_misses\": 0"));
        assert!(json.contains("\"include_once_skips\": 0"));
        assert!(json.contains("\"include_fallback_by_reason\": {}"));
        assert!(json.contains("\"include_stale_invalidation_by_reason\": {}"));
        assert!(json.contains("\"autoload_graph_hits\": 0"));
        assert!(json.contains("\"autoload_graph_misses\": 0"));
        assert!(json.contains("\"negative_lookup_hits\": 0"));
        assert!(json.contains("\"invalidations_by_reason\": {}"));
        assert!(json.contains("\"fallback_by_path_semantics\": {}"));
        assert!(json.contains("\"method_ic_hits\": 0"));
        assert!(json.contains("\"method_ic_misses\": 0"));
        assert!(json.contains("\"method_ic_polymorphic_hits\": 0"));
        assert!(json.contains("\"method_ic_guard_failures\": 0"));
        assert!(json.contains("\"method_direct_dispatch_hits\": 0"));
        assert!(json.contains("\"method_direct_dispatch_fallbacks\": 0"));
        assert!(json.contains("\"method_tiny_inline_candidates\": 0"));
        assert!(json.contains("\"method_tiny_inline_rejected_by_reason\": {}"));
        assert!(json.contains("\"class_relation_cache_hits\": 0"));
        assert!(json.contains("\"class_relation_cache_misses\": 0"));
        assert!(json.contains("\"class_relation_cache_invalidations\": 0"));
        assert!(json.contains("\"instanceof_cache_hits\": 0"));
        assert!(json.contains("\"instanceof_cache_misses\": 0"));
        assert!(json.contains("\"method_override_cache_hits\": 0"));
        assert!(json.contains("\"method_override_cache_misses\": 0"));
        assert!(json.contains("\"property_ic_hits\": 0"));
        assert!(json.contains("\"property_ic_misses\": 0"));
        assert!(json.contains("\"property_ic_guard_failures\": 0"));
        assert!(json.contains("\"property_ic_fallback_reasons\": {}"));
        assert!(json.contains("\"property_assign_ic_hits\": 0"));
        assert!(json.contains("\"property_assign_ic_misses\": 0"));
        assert!(json.contains("\"property_assign_ic_guard_failures\": 0"));
        assert!(json.contains("\"property_assign_ic_shape_exits\": 0"));
        assert!(json.contains("\"property_assign_ic_visibility_exits\": 0"));
        assert!(json.contains("\"property_assign_ic_type_exits\": 0"));
        assert!(json.contains("\"property_assign_ic_readonly_exits\": 0"));
        assert!(json.contains("\"property_assign_ic_hook_magic_exits\": 0"));
        assert!(json.contains("\"property_assign_ic_reference_exits\": 0"));
        assert!(json.contains("\"property_assign_ic_dynamic_exits\": 0"));
        assert!(json.contains("\"property_assign_ic_fallback_reasons\": {}"));
        assert!(json.contains("\"class_static_ic_hits\": 0"));
        assert!(json.contains("\"class_static_ic_misses\": 0"));
        assert!(json.contains("\"class_static_ic_guard_failures\": 0"));
        assert!(json.contains("\"include_path_ic_hits\": 0"));
        assert!(json.contains("\"include_path_ic_misses\": 0"));
        assert!(json.contains("\"include_path_ic_invalidations\": 0"));
        assert!(json.contains("\"include_path_ic_guard_failures\": 0"));
        assert!(json.contains("\"autoload_class_lookup_ic_hits\": 0"));
        assert!(json.contains("\"autoload_class_lookup_ic_misses\": 0"));
        assert!(json.contains("\"autoload_class_lookup_ic_invalidations\": 0"));
        assert!(json.contains("\"autoload_class_lookup_ic_guard_failures\": 0"));
        assert!(json.contains("\"property_fetch_profiles\": []"));
        assert!(json.contains("\"method_call_profiles\": []"));
        assert!(json.ends_with('\n'));
    }

    #[test]
    fn jit_compile_descriptors_are_reported_in_counter_json() {
        let mut counters = VmCounters::default();
        counters.record_jit_compile_descriptor(JitCompileDescriptor {
            function_id: 7,
            function_name: "hot\"leaf".to_owned(),
            ir_fingerprint: "00000000000000ab".to_owned(),
            code_bytes: 64,
            compile_time_nanos: 1_500,
            target_isa: "aarch64-darwin".to_owned(),
            abi_hash: 42,
            config_hash: 99,
        });

        let json = counters.to_json();

        assert!(json.contains("\"jit_compile_descriptors\": [{"));
        assert!(json.contains("\"function_id\": 7"));
        assert!(json.contains("\"function_name\": \"hot\\\"leaf\""));
        assert!(json.contains("\"ir_fingerprint\": \"00000000000000ab\""));
        assert!(json.contains("\"code_bytes\": 64"));
        assert!(json.contains("\"target_isa\": \"aarch64-darwin\""));
        assert!(json.contains("\"abi_hash\": 42"));
        assert!(json.contains("\"config_hash\": 99"));
    }

    #[test]
    fn property_fetch_profile_json_records_metadata_and_reasons() {
        let mut counters = VmCounters::default();
        counters.record_property_fetch_profile(PropertyFetchProfileObservation {
            callsite: "unit0:read:b0:i7".to_owned(),
            property: "value".to_owned(),
            receiver_class: "performancea".to_owned(),
            class_id: 1,
            declared_property_name: Some("value".to_owned()),
            visibility_context: Some("performancereader".to_owned()),
            property_slot_index: Some(0),
            class_layout_version: 1,
            declared_visible_property: true,
            ..PropertyFetchProfileObservation::default()
        });

        let mono_json = counters.to_json();
        assert!(mono_json.contains("\"state\": \"monomorphic\""));
        assert!(mono_json.contains("\"fast_path_eligible\": true"));
        assert!(mono_json.contains("\"class_ids\": [1]"));
        assert!(mono_json.contains("\"declared_property_names\": [\"value\"]"));
        assert!(mono_json.contains("\"visibility_contexts\": [\"performancereader\"]"));
        assert!(mono_json.contains("\"property_slot_indexes\": [0]"));
        assert!(mono_json.contains("\"class_layout_versions\": [1]"));

        counters.record_property_fetch_profile(PropertyFetchProfileObservation {
            callsite: "unit0:read:b0:i7".to_owned(),
            property: "value".to_owned(),
            receiver_class: "performanceb".to_owned(),
            class_id: 2,
            declared_property_name: Some("value".to_owned()),
            visibility_context: Some("performancereader".to_owned()),
            property_slot_index: Some(1),
            class_layout_version: 2,
            has_magic_get: true,
            has_property_hook: true,
            dynamic_property_fallback: true,
            uninitialized_typed_property: true,
            non_eligible_reasons: vec!["not_visible"],
            ..PropertyFetchProfileObservation::default()
        });

        let poly_json = counters.to_json();
        assert!(poly_json.contains("\"state\": \"polymorphic\""));
        assert!(poly_json.contains("\"fast_path_eligible\": false"));
        assert!(poly_json.contains("\"non_eligible_reasons\": ["));
        assert!(poly_json.contains("\"dynamic_property_fallback\""));
        assert!(poly_json.contains("\"magic_get_present\""));
        assert!(poly_json.contains("\"not_visible\""));
        assert!(poly_json.contains("\"polymorphic_receiver\""));
        assert!(poly_json.contains("\"property_hook_present\""));
        assert!(poly_json.contains("\"unstable_layout_version\""));
        assert!(poly_json.contains("\"uninitialized_typed_property\""));
        assert!(poly_json.contains("\"class_ids\": [1, 2]"));
        assert!(poly_json.contains("\"receiver_classes\": [\"performancea\", \"performanceb\"]"));
        assert!(poly_json.contains("\"property_slot_indexes\": [0, 1]"));
        assert!(poly_json.contains("\"class_layout_versions\": [1, 2]"));
        assert!(poly_json.contains("\"has_magic_get\": true"));
        assert!(poly_json.contains("\"has_property_hook\": true"));
        assert!(poly_json.contains("\"dynamic_property_fallback\": true"));
        assert!(poly_json.contains("\"saw_uninitialized_typed_property\": true"));
    }

    #[test]
    fn method_call_profile_json_records_metadata_and_reasons() {
        let mut counters = VmCounters::default();
        counters.record_method_call_profile(MethodCallProfileObservation {
            callsite: "unit0:call:b0:i9".to_owned(),
            method: "value".to_owned(),
            receiver_class: "performancemethoda".to_owned(),
            class_id: 7,
            declaring_class: Some("performancemethoda".to_owned()),
            method_id: Some(11),
            method_slot_index: Some(0),
            visibility_context: Some("performancecaller".to_owned()),
            override_layout_version: 3,
            method_is_final: true,
            simple_positional_arguments: true,
            callee_jit_eligible: true,
            ..MethodCallProfileObservation::default()
        });

        let mono_json = counters.to_json();
        assert!(mono_json.contains("\"method_call_profiles\": ["));
        assert!(mono_json.contains("\"state\": \"monomorphic\""));
        assert!(mono_json.contains("\"fast_path_eligible\": true"));
        assert!(mono_json.contains("\"class_ids\": [7]"));
        assert!(mono_json.contains("\"receiver_classes\": [\"performancemethoda\"]"));
        assert!(mono_json.contains("\"declaring_classes\": [\"performancemethoda\"]"));
        assert!(mono_json.contains("\"method_ids\": [11]"));
        assert!(mono_json.contains("\"method_slot_indexes\": [0]"));
        assert!(mono_json.contains("\"visibility_contexts\": [\"performancecaller\"]"));
        assert!(mono_json.contains("\"override_layout_versions\": [3]"));
        assert!(mono_json.contains("\"saw_final_method\": true"));
        assert!(mono_json.contains("\"saw_callee_jit_eligible\": true"));

        counters.record_method_call_profile(MethodCallProfileObservation {
            callsite: "unit0:call:b0:i9".to_owned(),
            method: "value".to_owned(),
            receiver_class: "performancemethodb".to_owned(),
            class_id: 8,
            declaring_class: Some("performancemethodb".to_owned()),
            method_id: Some(12),
            method_slot_index: Some(1),
            visibility_context: Some("performancecaller".to_owned()),
            override_layout_version: 4,
            method_is_private: true,
            method_is_static: true,
            has_magic_call: true,
            magic_call_fallback: true,
            simple_positional_arguments: false,
            has_by_ref_argument: true,
            non_eligible_reasons: vec!["visibility_context_mismatch"],
            ..MethodCallProfileObservation::default()
        });

        let poly_json = counters.to_json();
        assert!(poly_json.contains("\"state\": \"polymorphic\""));
        assert!(poly_json.contains("\"fast_path_eligible\": false"));
        assert!(poly_json.contains("\"by_ref_argument\""));
        assert!(poly_json.contains("\"magic_call_fallback\""));
        assert!(poly_json.contains("\"magic_call_present\""));
        assert!(poly_json.contains("\"non_positional_argument\""));
        assert!(poly_json.contains("\"polymorphic_receiver\""));
        assert!(poly_json.contains("\"static_method\""));
        assert!(poly_json.contains("\"unstable_method_slot\""));
        assert!(poly_json.contains("\"unstable_override_layout_version\""));
        assert!(poly_json.contains("\"visibility_context_mismatch\""));
        assert!(poly_json.contains("\"class_ids\": [7, 8]"));
        assert!(poly_json.contains("\"method_ids\": [11, 12]"));
        assert!(poly_json.contains("\"method_slot_indexes\": [0, 1]"));
        assert!(poly_json.contains("\"override_layout_versions\": [3, 4]"));
        assert!(poly_json.contains("\"saw_private_method\": true"));
        assert!(poly_json.contains("\"saw_static_method\": true"));
        assert!(poly_json.contains("\"has_magic_call\": true"));
        assert!(poly_json.contains("\"magic_call_fallback\": true"));
        assert!(poly_json.contains("\"simple_positional_arguments\": false"));
        assert!(poly_json.contains("\"saw_by_ref_argument\": true"));
    }
}
