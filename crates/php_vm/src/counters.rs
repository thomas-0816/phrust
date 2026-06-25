//! Optional VM/runtime counters for performance performance instrumentation.

use std::collections::{BTreeMap, BTreeSet};

use php_ir::instruction::{BinaryOp, InstructionKind};
use php_runtime::OutputStats;

use crate::inline_cache::POLYMORPHIC_INLINE_CACHE_LIMIT;
use crate::{InlineCacheKind, InlineCacheObservation};

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

/// Lightweight counters collected only when explicitly enabled.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VmCounters {
    pub jit_mode: String,
    pub jit_threshold: u64,
    pub instructions_executed: u64,
    pub opcodes: BTreeMap<String, u64>,
    pub function_calls: u64,
    pub method_calls: u64,
    pub frame_allocations: u64,
    pub frame_reuses: u64,
    pub array_dim_fetches: u64,
    pub packed_dim_fast_path_hits: u64,
    pub packed_dim_fast_path_misses: u64,
    pub array_packed_append_fast_path_hits: u64,
    pub array_packed_read_fast_path_hits: u64,
    pub array_sequential_foreach_fast_path_hits: u64,
    pub array_count_fast_path_hits: u64,
    pub array_packed_to_mixed_transitions: u64,
    pub numeric_string_cache_hits: u64,
    pub numeric_string_cache_misses: u64,
    pub typecheck_fast_path_hits: u64,
    pub typecheck_fast_path_misses: u64,
    pub output_bytes: u64,
    pub output_buffer_appends: u64,
    pub output_buffer_batch_writes: u64,
    pub output_buffer_flushes: u64,
    pub internal_function_dispatches: u64,
    pub internal_function_dispatch_cache_hits: u64,
    pub internal_function_dispatch_cache_misses: u64,
    pub internal_count_array_direct_fast_path_hits: u64,
    pub local_slot_fast_path_hits: u64,
    pub local_slot_fast_path_misses: u64,
    pub property_fetches: u64,
    pub property_accesses: u64,
    pub type_checks: u64,
    pub includes: u64,
    pub autoloads: u64,
    pub string_concats: u64,
    pub string_concat_fast_path_hits: u64,
    pub string_concat_fast_path_misses: u64,
    pub guard_failures: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub literal_intern_hits: u64,
    pub literal_intern_misses: u64,
    pub quickening_attempts: u64,
    pub quickening_specialized: u64,
    pub quickening_guard_hits: u64,
    pub quickening_guard_misses: u64,
    pub quickening_guard_failures: u64,
    pub quickening_fallback_calls: u64,
    pub quickening_dequickens: u64,
    pub quickening_megamorphic: u64,
    pub quickening_disabled: u64,
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
    pub packed_fetch_bounds_exits: u64,
    pub packed_fetch_layout_exits: u64,
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
    pub inline_cache_dim_slots: u64,
    pub inline_cache_class_constant_static_property_slots: u64,
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
    pub method_ic_guard_failures: u64,
    pub property_ic_hits: u64,
    pub property_ic_misses: u64,
    pub property_ic_guard_failures: u64,
    pub class_static_ic_hits: u64,
    pub class_static_ic_misses: u64,
    pub class_static_ic_guard_failures: u64,
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
        *self
            .opcodes
            .entry(opcode_name(kind).to_owned())
            .or_default() += 1;
        match kind {
            InstructionKind::BindReferenceFromCall { .. }
            | InstructionKind::CallFunction { .. }
            | InstructionKind::CallClosure { .. }
            | InstructionKind::CallCallable { .. }
            | InstructionKind::Pipe { .. } => self.function_calls += 1,
            InstructionKind::CallMethod { .. } | InstructionKind::CallStaticMethod { .. } => {
                self.method_calls += 1;
            }
            InstructionKind::BindReferenceDim { .. }
            | InstructionKind::BindReferenceFromDim { .. }
            | InstructionKind::FetchDim { .. }
            | InstructionKind::ArrayGet { .. }
            | InstructionKind::IssetDim { .. }
            | InstructionKind::EmptyDim { .. }
            | InstructionKind::UnsetDim { .. } => self.array_dim_fetches += 1,
            InstructionKind::FetchProperty { .. } | InstructionKind::FetchStaticProperty { .. } => {
                self.property_fetches += 1;
                self.property_accesses += 1;
            }
            InstructionKind::IssetProperty { .. }
            | InstructionKind::EmptyProperty { .. }
            | InstructionKind::UnsetProperty { .. }
            | InstructionKind::AssignProperty { .. }
            | InstructionKind::AssignStaticProperty { .. } => self.property_accesses += 1,
            InstructionKind::InstanceOf { .. } => self.type_checks += 1,
            InstructionKind::Include { .. } => self.includes += 1,
            InstructionKind::Binary {
                op: BinaryOp::Concat,
                ..
            } => self.string_concats += 1,
            _ => {}
        }
    }

    pub(crate) fn record_autoload(&mut self) {
        self.autoloads += 1;
    }

    pub(crate) fn record_frame_activation(&mut self, reused: bool) {
        if reused {
            self.frame_reuses += 1;
        } else {
            self.frame_allocations += 1;
        }
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

    pub(crate) fn record_packed_dim_fast_path(&mut self, hit: bool) {
        if hit {
            self.packed_dim_fast_path_hits += 1;
        } else {
            self.packed_dim_fast_path_misses += 1;
        }
    }

    pub(crate) fn record_array_packed_append_fast_path_hit(&mut self) {
        self.array_packed_append_fast_path_hits += 1;
    }

    pub(crate) fn record_array_packed_read_fast_path_hit(&mut self) {
        self.array_packed_read_fast_path_hits += 1;
    }

    pub(crate) fn record_array_sequential_foreach_fast_path_hit(&mut self) {
        self.array_sequential_foreach_fast_path_hits += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_packed_foreach_sum_fast_hit(&mut self) {
        self.packed_foreach_sum_fast_hits += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_packed_foreach_sum_layout_exit(&mut self) {
        self.packed_foreach_sum_layout_exits += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_packed_foreach_sum_overflow_exit(&mut self) {
        self.packed_foreach_sum_overflow_exits += 1;
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
        self.numeric_string_cache_hits += stats.hits;
        self.numeric_string_cache_misses += stats.misses;
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
        self.output_buffer_flushes = stats.flushes;
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

    pub(crate) fn record_local_slot_fast_path(&mut self, hit: bool) {
        if hit {
            self.local_slot_fast_path_hits += 1;
        } else {
            self.local_slot_fast_path_misses += 1;
        }
    }

    pub(crate) fn record_quickening(&mut self, observation: crate::QuickeningObservation) {
        if observation.attempt {
            self.quickening_attempts += 1;
        }
        if observation.specialized {
            self.quickening_specialized += 1;
        }
        if observation.guard_hit {
            self.quickening_guard_hits += 1;
        }
        if observation.guard_miss {
            self.quickening_guard_misses += 1;
        }
        if observation.guard_failure {
            self.quickening_guard_failures += 1;
        }
        if observation.fallback_call {
            self.quickening_fallback_calls += 1;
        }
        if observation.dequickened {
            self.quickening_dequickens += 1;
        }
        if observation.megamorphic {
            self.quickening_megamorphic += 1;
        }
        if observation.disabled {
            self.quickening_disabled += 1;
        }
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_jit_compile_attempt(&mut self) {
        self.jit_compile_attempts += 1;
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(crate) fn record_jit_compiled(&mut self) {
        self.jit_compiled += 1;
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
    pub(crate) fn record_packed_fetch_fast_hit(&mut self) {
        self.packed_fetch_fast_hits += 1;
    }

    #[allow(dead_code)]
    pub(crate) fn record_packed_fetch_bounds_exit(&mut self) {
        self.packed_fetch_bounds_exits += 1;
    }

    #[allow(dead_code)]
    pub(crate) fn record_packed_fetch_layout_exit(&mut self) {
        self.packed_fetch_layout_exits += 1;
    }

    #[allow(dead_code)]
    pub(crate) fn record_jit_overflow_exit(&mut self) {
        self.jit_overflow_exits += 1;
    }

    #[allow(dead_code)]
    pub(crate) fn record_jit_slow_path_call(&mut self) {
        self.jit_slow_path_calls += 1;
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
            }
            if observation.kind == Some(InlineCacheKind::PropertyFetch) {
                self.property_ic_hits += 1;
            }
            if observation.kind == Some(InlineCacheKind::ClassConstantStaticProperty) {
                self.class_static_ic_hits += 1;
            }
            if observation.kind == Some(InlineCacheKind::IncludePath) {
                self.include_path_ic_hits += 1;
            }
            if observation.kind == Some(InlineCacheKind::AutoloadClassLookup) {
                self.autoload_class_lookup_ic_hits += 1;
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
            if observation.kind == Some(InlineCacheKind::ClassConstantStaticProperty) {
                self.class_static_ic_misses += 1;
            }
            if observation.kind == Some(InlineCacheKind::IncludePath) {
                self.include_path_ic_misses += 1;
            }
            if observation.kind == Some(InlineCacheKind::AutoloadClassLookup) {
                self.autoload_class_lookup_ic_misses += 1;
            }
        }
        if observation.invalidation {
            self.inline_cache_invalidations += 1;
            if observation.kind == Some(InlineCacheKind::IncludePath) {
                self.include_path_ic_invalidations += 1;
            }
            if observation.kind == Some(InlineCacheKind::AutoloadClassLookup) {
                self.autoload_class_lookup_ic_invalidations += 1;
            }
        }
        if observation.guard_failure {
            self.inline_cache_guard_failures += 1;
            if observation.kind == Some(InlineCacheKind::MethodCall) {
                self.method_ic_guard_failures += 1;
            }
            if observation.kind == Some(InlineCacheKind::PropertyFetch) {
                self.property_ic_guard_failures += 1;
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
            Some(InlineCacheKind::DimFetch) => self.inline_cache_dim_slots += 1,
            Some(InlineCacheKind::ClassConstantStaticProperty) => {
                self.inline_cache_class_constant_static_property_slots += 1;
            }
            Some(InlineCacheKind::IncludePath) => self.inline_cache_include_path_slots += 1,
            Some(InlineCacheKind::AutoloadClassLookup) => {
                self.inline_cache_autoload_class_lookup_slots += 1;
            }
            None => {}
        }
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
            "output_buffer_flushes",
            self.output_buffer_flushes,
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
        push_field(
            &mut json,
            "quickening_specialized",
            self.quickening_specialized,
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
            "method_ic_guard_failures",
            self.method_ic_guard_failures,
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

fn opcode_name(kind: &InstructionKind) -> &'static str {
    match kind {
        InstructionKind::Nop => "nop",
        InstructionKind::LoadConst { .. } => "load_const",
        InstructionKind::FetchConst { .. } => "fetch_const",
        InstructionKind::Move { .. } => "move",
        InstructionKind::LoadLocal { .. } => "load_local",
        InstructionKind::LoadLocalQuiet { .. } => "load_local_quiet",
        InstructionKind::StoreLocal { .. } => "store_local",
        InstructionKind::BindReference { .. } => "bind_reference",
        InstructionKind::BindGlobal { .. } => "bind_global",
        InstructionKind::BindReferenceDim { .. } => "bind_reference_dim",
        InstructionKind::BindReferenceFromDim { .. } => "bind_reference_from_dim",
        InstructionKind::BindReferenceFromCall { .. } => "bind_reference_from_call",
        InstructionKind::InitStaticLocal { .. } => "init_static_local",
        InstructionKind::Binary {
            op: BinaryOp::Concat,
            ..
        } => "binary_concat",
        InstructionKind::Binary { .. } => "binary",
        InstructionKind::Compare { .. } => "compare",
        InstructionKind::InstanceOf { .. } => "instanceof",
        InstructionKind::Unary { .. } => "unary",
        InstructionKind::Cast { .. } => "cast",
        InstructionKind::Discard { .. } => "discard",
        InstructionKind::Echo { .. } => "echo",
        InstructionKind::EmitDiagnostic { .. } => "emit_diagnostic",
        InstructionKind::Yield { .. } => "yield",
        InstructionKind::YieldFrom { .. } => "yield_from",
        InstructionKind::CallFunction { .. } => "call_function",
        InstructionKind::CallMethod { .. } => "call_method",
        InstructionKind::CallStaticMethod { .. } => "call_static_method",
        InstructionKind::CloneObject { .. } => "clone_object",
        InstructionKind::CloneWith { .. } => "clone_with",
        InstructionKind::EnterTry { .. } => "enter_try",
        InstructionKind::LeaveTry => "leave_try",
        InstructionKind::EndFinally { .. } => "end_finally",
        InstructionKind::Throw { .. } => "throw",
        InstructionKind::MakeException { .. } => "make_exception",
        InstructionKind::MakeClosure { .. } => "make_closure",
        InstructionKind::CallClosure { .. } => "call_closure",
        InstructionKind::ResolveCallable { .. } => "resolve_callable",
        InstructionKind::CallCallable { .. } => "call_callable",
        InstructionKind::Pipe { .. } => "pipe",
        InstructionKind::Include { .. } => "include",
        InstructionKind::Eval { .. } => "eval",
        InstructionKind::NewObject { .. } => "new_object",
        InstructionKind::DynamicNewObject { .. } => "dynamic_new_object",
        InstructionKind::FetchProperty { .. } => "fetch_property",
        InstructionKind::IssetProperty { .. } => "isset_property",
        InstructionKind::EmptyProperty { .. } => "empty_property",
        InstructionKind::UnsetProperty { .. } => "unset_property",
        InstructionKind::FetchStaticProperty { .. } => "fetch_static_property",
        InstructionKind::FetchClassConstant { .. } => "fetch_class_constant",
        InstructionKind::AssignProperty { .. } => "assign_property",
        InstructionKind::AssignStaticProperty { .. } => "assign_static_property",
        InstructionKind::NewArray { .. } => "new_array",
        InstructionKind::ArrayInsert { .. } => "array_insert",
        InstructionKind::FetchDim { .. } => "fetch_dim",
        InstructionKind::AssignDim { .. } => "assign_dim",
        InstructionKind::AppendDim { .. } => "append_dim",
        InstructionKind::IssetLocal { .. } => "isset_local",
        InstructionKind::EmptyLocal { .. } => "empty_local",
        InstructionKind::UnsetLocal { .. } => "unset_local",
        InstructionKind::IssetDim { .. } => "isset_dim",
        InstructionKind::EmptyDim { .. } => "empty_dim",
        InstructionKind::UnsetDim { .. } => "unset_dim",
        InstructionKind::ForeachInit { .. } => "foreach_init",
        InstructionKind::ForeachNext { .. } => "foreach_next",
        InstructionKind::ForeachInitRef { .. } => "foreach_init_ref",
        InstructionKind::ForeachNextRef { .. } => "foreach_next_ref",
        InstructionKind::ArrayGet { .. } => "array_get",
        InstructionKind::Unsupported { .. } => "unsupported",
        InstructionKind::RuntimeError { .. } => "runtime_error",
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

#[cfg(test)]
mod tests {
    use crate::{InlineCacheKind, InlineCacheObservation, QuickeningObservation};
    use php_ir::ids::RegId;
    use php_ir::instruction::{BinaryOp, InstructionKind};

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
        counters.record_frame_activation(false);
        counters.record_frame_activation(true);
        counters.record_autoload();
        counters.record_literal_intern(false);
        counters.record_literal_intern(true);
        counters.record_string_concat_fast_path(false);
        counters.record_string_concat_fast_path(true);
        counters.record_packed_dim_fast_path(false);
        counters.record_packed_dim_fast_path(true);
        counters.record_array_packed_append_fast_path_hit();
        counters.record_array_packed_read_fast_path_hit();
        counters.record_array_sequential_foreach_fast_path_hit();
        counters.record_array_count_fast_path_hit();
        counters.record_array_packed_to_mixed_transition();
        counters.record_numeric_string_cache_stats(
            php_runtime::numeric_string::NumericStringCacheStats { hits: 2, misses: 3 },
        );
        counters.record_typecheck_fast_path(false);
        counters.record_typecheck_fast_path(true);
        counters.record_output_stats(
            12,
            OutputStats {
                appends: 3,
                batch_writes: 1,
                flushes: 2,
            },
        );
        counters.record_internal_function_dispatch();
        counters.record_internal_function_dispatch_cache(false);
        counters.record_internal_function_dispatch_cache(true);
        counters.record_internal_count_array_direct_fast_path_hit();
        counters.record_local_slot_fast_path(false);
        counters.record_local_slot_fast_path(true);
        counters.record_quickening(QuickeningObservation {
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
            megamorphic: true,
            disabled: true,
            ..InlineCacheObservation::empty()
        });
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
        assert_eq!(counters.string_concats, 1);
        assert_eq!(counters.packed_dim_fast_path_hits, 1);
        assert_eq!(counters.packed_dim_fast_path_misses, 1);
        assert_eq!(counters.array_packed_append_fast_path_hits, 1);
        assert_eq!(counters.array_packed_read_fast_path_hits, 1);
        assert_eq!(counters.array_sequential_foreach_fast_path_hits, 1);
        assert_eq!(counters.array_count_fast_path_hits, 1);
        assert_eq!(counters.array_packed_to_mixed_transitions, 1);
        assert_eq!(counters.numeric_string_cache_hits, 2);
        assert_eq!(counters.numeric_string_cache_misses, 3);
        assert_eq!(counters.typecheck_fast_path_hits, 1);
        assert_eq!(counters.typecheck_fast_path_misses, 1);
        assert_eq!(counters.output_bytes, 12);
        assert_eq!(counters.output_buffer_appends, 3);
        assert_eq!(counters.output_buffer_batch_writes, 1);
        assert_eq!(counters.output_buffer_flushes, 2);
        assert_eq!(counters.internal_function_dispatches, 1);
        assert_eq!(counters.internal_function_dispatch_cache_hits, 1);
        assert_eq!(counters.internal_function_dispatch_cache_misses, 1);
        assert_eq!(counters.internal_count_array_direct_fast_path_hits, 1);
        assert_eq!(counters.local_slot_fast_path_hits, 1);
        assert_eq!(counters.local_slot_fast_path_misses, 1);
        assert_eq!(counters.string_concat_fast_path_hits, 1);
        assert_eq!(counters.string_concat_fast_path_misses, 1);
        assert_eq!(counters.autoloads, 1);
        assert_eq!(counters.literal_intern_hits, 1);
        assert_eq!(counters.literal_intern_misses, 1);
        assert_eq!(counters.quickening_attempts, 1);
        assert_eq!(counters.quickening_specialized, 1);
        assert_eq!(counters.quickening_guard_hits, 1);
        assert_eq!(counters.quickening_guard_misses, 1);
        assert_eq!(counters.quickening_guard_failures, 1);
        assert_eq!(counters.quickening_fallback_calls, 1);
        assert_eq!(counters.quickening_dequickens, 1);
        assert_eq!(counters.quickening_megamorphic, 1);
        assert_eq!(counters.quickening_disabled, 1);
        assert_eq!(counters.jit_compile_attempts, 1);
        assert_eq!(counters.jit_compiled, 1);
        assert_eq!(counters.jit_code_bytes, 64);
        assert_eq!(counters.jit_compile_time_nanos, 1_500);
        assert_eq!(counters.jit_executed, 1);
        assert_eq!(counters.jit_bailouts, 1);
        assert_eq!(counters.jit_side_exits, 1);
        assert_eq!(
            counters.jit_side_exit_reasons.get("helper_status"),
            Some(&1)
        );
        assert_eq!(counters.jit_guard_failures, 1);
        assert_eq!(counters.jit_blacklisted_regions, 1);
        assert_eq!(
            counters.jit_blacklist_reasons.get("too_many_side_exits"),
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
        assert_eq!(counters.packed_foreach_sum_fast_hits, 1);
        assert_eq!(counters.packed_foreach_sum_layout_exits, 1);
        assert_eq!(counters.packed_foreach_sum_overflow_exits, 1);
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
        assert_eq!(counters.jit_compile_cache_hits, 1);
        assert_eq!(counters.jit_compile_cache_misses, 1);
        assert_eq!(counters.jit_compile_cache_invalidations, 1);
        assert_eq!(counters.inline_cache_observations, 7);
        assert_eq!(counters.inline_cache_slots, 7);
        assert_eq!(counters.inline_cache_function_slots, 1);
        assert_eq!(counters.inline_cache_method_slots, 1);
        assert_eq!(counters.inline_cache_property_slots, 1);
        assert_eq!(counters.inline_cache_dim_slots, 1);
        assert_eq!(
            counters.inline_cache_class_constant_static_property_slots,
            1
        );
        assert_eq!(counters.inline_cache_include_path_slots, 1);
        assert_eq!(counters.inline_cache_autoload_class_lookup_slots, 1);
        assert_eq!(counters.method_ic_hits, 1);
        assert_eq!(counters.method_ic_misses, 1);
        assert_eq!(counters.method_ic_guard_failures, 1);
        assert_eq!(counters.inline_cache_fallback_calls, 1);
        assert_eq!(counters.inline_cache_monomorphic, 1);
        assert_eq!(counters.inline_cache_polymorphic, 1);
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

        let json = counters.to_json();

        assert!(json.contains("\"instructions_executed\": 1"));
        assert!(json.contains("\"guard_failures\": 0"));
        assert!(json.contains("\"frame_allocations\": 0"));
        assert!(json.contains("\"frame_reuses\": 0"));
        assert!(json.contains("\"packed_dim_fast_path_hits\": 0"));
        assert!(json.contains("\"packed_dim_fast_path_misses\": 0"));
        assert!(json.contains("\"array_packed_append_fast_path_hits\": 0"));
        assert!(json.contains("\"array_packed_read_fast_path_hits\": 0"));
        assert!(json.contains("\"array_sequential_foreach_fast_path_hits\": 0"));
        assert!(json.contains("\"array_count_fast_path_hits\": 0"));
        assert!(json.contains("\"array_packed_to_mixed_transitions\": 0"));
        assert!(json.contains("\"numeric_string_cache_hits\": 0"));
        assert!(json.contains("\"numeric_string_cache_misses\": 0"));
        assert!(json.contains("\"typecheck_fast_path_hits\": 0"));
        assert!(json.contains("\"typecheck_fast_path_misses\": 0"));
        assert!(json.contains("\"output_bytes\": 0"));
        assert!(json.contains("\"output_buffer_appends\": 0"));
        assert!(json.contains("\"output_buffer_batch_writes\": 0"));
        assert!(json.contains("\"output_buffer_flushes\": 0"));
        assert!(json.contains("\"internal_function_dispatches\": 0"));
        assert!(json.contains("\"internal_function_dispatch_cache_hits\": 0"));
        assert!(json.contains("\"internal_function_dispatch_cache_misses\": 0"));
        assert!(json.contains("\"internal_count_array_direct_fast_path_hits\": 0"));
        assert!(json.contains("\"local_slot_fast_path_hits\": 0"));
        assert!(json.contains("\"local_slot_fast_path_misses\": 0"));
        assert!(json.contains("\"literal_intern_hits\": 0"));
        assert!(json.contains("\"literal_intern_misses\": 0"));
        assert!(json.contains("\"string_concat_fast_path_hits\": 0"));
        assert!(json.contains("\"string_concat_fast_path_misses\": 0"));
        assert!(json.contains("\"quickening_attempts\": 0"));
        assert!(json.contains("\"quickening_specialized\": 0"));
        assert!(json.contains("\"quickening_guard_hits\": 0"));
        assert!(json.contains("\"quickening_guard_misses\": 0"));
        assert!(json.contains("\"quickening_guard_failures\": 0"));
        assert!(json.contains("\"quickening_fallback_calls\": 0"));
        assert!(json.contains("\"quickening_dequickens\": 0"));
        assert!(json.contains("\"quickening_megamorphic\": 0"));
        assert!(json.contains("\"quickening_disabled\": 0"));
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
        assert!(json.contains("\"inline_cache_dim_slots\": 0"));
        assert!(json.contains("\"inline_cache_class_constant_static_property_slots\": 0"));
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
        assert!(json.contains("\"method_ic_hits\": 0"));
        assert!(json.contains("\"method_ic_misses\": 0"));
        assert!(json.contains("\"method_ic_guard_failures\": 0"));
        assert!(json.contains("\"property_ic_hits\": 0"));
        assert!(json.contains("\"property_ic_misses\": 0"));
        assert!(json.contains("\"property_ic_guard_failures\": 0"));
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
