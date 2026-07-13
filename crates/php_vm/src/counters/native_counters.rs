//! Native-tier and JIT counter updates.

use super::{JitCompileDescriptor, VmCounters};

impl VmCounters {
    pub(crate) fn record_packed_foreach_sum_fast_hit(&mut self) {
        self.packed_foreach_sum_fast_hits += 1;
        self.record_array_fast_path_hit("packed_int_sum");
    }

    pub(crate) fn record_packed_foreach_sum_layout_exit(&mut self) {
        self.packed_foreach_sum_layout_exits += 1;
        self.record_array_fast_path_fallback("layout_or_element");
    }

    pub(crate) fn record_packed_foreach_sum_overflow_exit(&mut self) {
        self.packed_foreach_sum_overflow_exits += 1;
        self.record_array_fast_path_fallback("overflow");
    }

    pub(crate) fn record_known_call_fast_hit(&mut self) {
        self.known_call_fast_hits += 1;
    }

    pub(crate) fn record_known_call_guard_exit(&mut self) {
        self.known_call_guard_exits += 1;
    }

    pub(crate) fn record_known_call_slow_call(&mut self) {
        self.known_call_slow_calls += 1;
        self.record_slow_path_call("jit.known_call");
    }

    pub(crate) fn record_property_load_fast_hit(&mut self) {
        self.property_load_fast_hits += 1;
    }

    pub(crate) fn record_property_load_guard_exit(&mut self) {
        self.property_load_guard_exits += 1;
    }

    pub(crate) fn record_property_load_layout_exit(&mut self) {
        self.property_load_layout_exits += 1;
    }

    pub(crate) fn record_property_load_uninitialized_exit(&mut self) {
        self.property_load_uninitialized_exits += 1;
    }

    pub(crate) fn record_property_load_slow_call(&mut self) {
        self.property_load_slow_calls += 1;
        self.record_slow_path_call("jit.property_load");
    }

    #[allow(dead_code)]
    pub(crate) fn record_record_lookup_fast_hit(&mut self) {
        self.record_lookup_fast_hits += 1;
    }

    pub(crate) fn record_record_lookup_key_miss_exit(&mut self) {
        self.record_lookup_key_miss_exits += 1;
    }

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

    pub(crate) fn record_compiled_to_compiled_calls(&mut self, count: u64) {
        self.compiled_to_compiled_calls = self.compiled_to_compiled_calls.saturating_add(count);
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

    pub(crate) fn record_jit_compile_cache_hit(&mut self) {
        self.jit_compile_cache_hits += 1;
    }

    pub(crate) fn record_jit_compile_cache_miss(&mut self) {
        self.jit_compile_cache_misses += 1;
    }

    pub(crate) fn record_jit_compile_cache_invalidation(&mut self) {
        self.jit_compile_cache_invalidations += 1;
    }

    pub(crate) fn record_jit_compile_attempt(&mut self) {
        self.jit_compile_attempts += 1;
    }

    pub(crate) fn record_jit_compiled(&mut self) {
        self.jit_compiled += 1;
        self.native_compiled_regions += 1;
    }

    pub(crate) fn record_jit_compile_metadata(&mut self, code_bytes: u64, compile_time_nanos: u64) {
        self.jit_code_bytes += code_bytes;
        self.jit_compile_time_nanos += compile_time_nanos;
    }

    pub(crate) fn record_jit_compile_descriptor(&mut self, descriptor: JitCompileDescriptor) {
        self.jit_compile_descriptors.push(descriptor);
    }

    pub(crate) fn record_jit_code_manager_event(
        &mut self,
        event: php_jit::CraneliftCodeManagerEvent,
    ) {
        self.jit_process_cache_hits = self
            .jit_process_cache_hits
            .saturating_add(event.process_cache_hits);
        self.jit_process_cache_misses = self
            .jit_process_cache_misses
            .saturating_add(event.process_cache_misses);
        self.jit_compile_waits = self.jit_compile_waits.saturating_add(event.compile_waits);
        self.jit_duplicate_compiles_avoided = self
            .jit_duplicate_compiles_avoided
            .saturating_add(event.duplicate_compiles_avoided);
        self.jit_code_bytes_live = event.code_bytes_live as u64;
        self.jit_code_bytes_retired = event.code_bytes_retired as u64;
        self.jit_code_generations = event.code_generations as u64;
        self.jit_evictions = self.jit_evictions.saturating_add(event.evictions);
    }

    pub(crate) fn record_jit_executed(&mut self) {
        self.jit_executed += 1;
        self.native_executions += 1;
    }

    pub(crate) fn record_jit_bailout(&mut self) {
        self.jit_bailouts += 1;
    }

    pub(crate) fn record_jit_side_exit(&mut self, reason: &str) {
        self.jit_side_exits += 1;
        *self
            .jit_side_exit_reasons
            .entry(reason.to_owned())
            .or_default() += 1;
        self.record_native_side_exit(reason);
    }

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

    pub(crate) fn record_jit_tiering_blacklist_rejection(&mut self) {
        self.jit_tiering_blacklist_rejections += 1;
    }

    pub(crate) fn record_jit_tiering_budget_rejection(&mut self) {
        self.jit_tiering_budget_rejections += 1;
        self.native_compile_budget_rejections += 1;
    }

    pub(crate) fn record_native_candidate(&mut self) {
        self.native_candidates += 1;
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

    #[allow(dead_code)]
    pub(crate) fn record_jit_helper_call(&mut self) {
        self.jit_helper_calls += 1;
    }

    #[allow(dead_code)]
    pub(crate) fn record_jit_fast_path_hit(&mut self) {
        self.jit_fast_path_hits += 1;
    }

    pub(super) fn render_call_transfer_json(&self, json: &mut String) {
        for (name, value) in [
            ("direct_call_source_reads", self.direct_call_source_reads),
            ("direct_call_moves", self.direct_call_moves),
            ("direct_call_clones", self.direct_call_clones),
            (
                "direct_call_owned_value_buffers",
                self.direct_call_owned_value_buffers,
            ),
            (
                "cranelift_prepared_arg_materializations",
                self.cranelift_prepared_arg_materializations,
            ),
            (
                "cranelift_direct_slot_marshals",
                self.cranelift_direct_slot_marshals,
            ),
            (
                "dense_activation_transfers",
                self.dense_activation_transfers,
            ),
            ("nested_vm_results_avoided", self.nested_vm_results_avoided),
            (
                "recursive_dense_calls_avoided",
                self.recursive_dense_calls_avoided,
            ),
        ] {
            super::push_field(json, name, value, true);
        }
    }

    pub(super) fn render_compiled_call_json(&self, json: &mut String) {
        super::push_field(
            json,
            "compiled_to_compiled_calls",
            self.compiled_to_compiled_calls,
            true,
        );
    }

    pub(super) fn render_process_cache_json(&self, json: &mut String) {
        for (name, value) in [
            ("jit_process_cache_hits", self.jit_process_cache_hits),
            ("jit_process_cache_misses", self.jit_process_cache_misses),
            ("jit_compile_waits", self.jit_compile_waits),
            (
                "jit_duplicate_compiles_avoided",
                self.jit_duplicate_compiles_avoided,
            ),
            ("jit_code_bytes_live", self.jit_code_bytes_live),
            ("jit_code_bytes_retired", self.jit_code_bytes_retired),
            ("jit_code_generations", self.jit_code_generations),
            ("jit_evictions", self.jit_evictions),
        ] {
            super::push_field(json, name, value, true);
        }
    }
}
