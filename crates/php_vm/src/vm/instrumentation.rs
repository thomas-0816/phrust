use super::*;

impl Vm {
    pub(super) fn record_counter_instruction(&self, kind: &InstructionKind) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_instruction(kind);
            if self.include_execution_depth.get() > 0 {
                counters.record_include_rich_instruction();
            } else {
                counters.record_entry_rich_instruction();
            }
        }
    }

    pub(super) fn current_instructions_executed(&self) -> Option<u64> {
        self.counters
            .borrow()
            .as_ref()
            .map(|counters| counters.instructions_executed)
    }

    pub(super) fn current_bytecode_instructions_executed(&self) -> Option<u64> {
        self.counters
            .borrow()
            .as_ref()
            .map(|counters| counters.bytecode_instructions_executed)
    }

    pub(super) fn record_counter_bytecode_lower_attempt(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_bytecode_lower_attempt();
        }
    }

    pub(super) fn record_counter_bytecode_lower_success(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_bytecode_lower_success();
        }
    }

    pub(super) fn record_counter_dense_execution_plan_cache_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_execution_plan_cache_hit();
        }
    }

    pub(super) fn record_counter_dense_execution_plan_cache_miss(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_execution_plan_cache_miss();
        }
    }

    pub(super) fn record_counter_dense_execution_plan(&self, plan: &DenseExecutionPlan) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_execution_plan(
                plan.dense_function_count(),
                plan.rich_fallback_function_count(),
            );
        }
    }

    pub(super) fn record_counter_dense_function_executed(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_function_executed();
        }
    }

    pub(super) fn record_counter_dense_property_fetch_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_property_fetch_hit();
        }
    }

    pub(super) fn record_counter_dense_property_assignment_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_property_assignment_hit();
        }
    }

    pub(super) fn record_counter_dense_property_ic_reuse(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_property_ic_reuse();
        }
    }

    pub(super) fn record_counter_dense_property_fallback(&self, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_property_fallback(reason);
        }
    }

    pub(super) fn record_counter_dense_direct_call_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_direct_call_hit();
        }
    }

    pub(super) fn record_counter_dense_call_bare_args_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_call_bare_args_hit();
        }
    }

    pub(super) fn record_counter_dense_method_call_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_method_call_hit();
        }
    }

    pub(super) fn record_counter_dense_callable_call_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_callable_call_hit();
        }
    }

    pub(super) fn record_counter_dense_static_call_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_static_call_hit();
        }
    }

    pub(super) fn record_counter_dense_call_ic_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_call_ic_hit();
        }
    }

    pub(super) fn record_counter_dense_call_ic_miss(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_call_ic_miss();
        }
    }

    pub(super) fn record_counter_dense_call_fallback(&self, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_call_fallback(reason);
        }
    }

    pub(super) fn record_counter_dense_method_dispatch_attempt(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.dense_method_dispatch_attempts += 1;
        }
    }

    pub(super) fn record_counter_dense_method_dispatch_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.dense_method_dispatch_hits += 1;
        }
    }

    pub(super) fn record_counter_dense_method_dispatch_fallback(&self, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_method_dispatch_fallback(reason);
        }
    }

    pub(super) fn record_counter_dense_jump_threading(
        &self,
        report: &crate::bytecode::DenseJumpThreadingReport,
        rolled_back: bool,
    ) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.dense_jump_threading_trampoline_blocks += report.trampoline_blocks;
            if rolled_back {
                counters.dense_jump_threading_rollbacks += 1;
            } else {
                counters.dense_jump_threading_threaded_edges += report.threaded_edges;
            }
        }
    }

    pub(super) fn record_counter_rich_fallback_function_executed(
        &self,
        reason: &str,
        function_name: &str,
    ) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_rich_fallback_function_executed(
                dense_bytecode_unsupported_reason(reason),
                function_name,
            );
        }
    }

    pub(super) fn record_counter_bytecode_unsupported_fallback(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_bytecode_unsupported_fallback();
        }
    }

    pub(super) fn record_counter_bytecode_unsupported_reason(&self, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_bytecode_unsupported_reason(reason);
        }
    }

    pub(super) fn record_counter_bytecode_auto_fallback_reason(&self, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_bytecode_auto_fallback_reason(reason);
        }
    }

    pub(super) fn record_counter_bytecode_instruction(&self, opcode: DenseOpcode) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_bytecode_instruction(opcode);
            if self.include_execution_depth.get() > 0 {
                counters.record_include_bytecode_instruction();
            } else {
                counters.record_entry_bytecode_instruction();
            }
        }
    }

    pub(super) fn record_counter_dense_block_entry(&self, function: u32, block: u32) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_block_entry(function, block);
        }
    }

    pub(super) fn record_counter_dense_branch(
        &self,
        function: u32,
        from_block: u32,
        to_block: u32,
        truthy: bool,
        fallthrough: bool,
    ) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_branch(function, from_block, to_block, truthy, fallthrough);
        }
    }

    pub(super) fn record_counter_bytecode_lowered_families(&self, dense: &DenseBytecodeUnit) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            for function in &dense.functions {
                for instruction in &function.instructions {
                    counters
                        .record_bytecode_lowered_family(dense_opcode_family(instruction.opcode));
                }
            }
        }
    }

    pub(super) fn record_counter_superinstruction_selection(
        &self,
        report: &SuperinstructionSelectionReport,
    ) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_superinstruction_selection(
                report.candidates,
                &report.candidates_by_kind,
                &report.emitted_by_kind,
                &report.skipped_by_reason,
            );
        }
    }

    pub(super) fn record_counter_superinstruction_executed(&self, opcode: DenseOpcode) {
        if !self.options.collect_counters || !opcode.is_superinstruction() {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_superinstruction_executed(opcode.as_str());
        }
    }

    pub(super) fn record_counter_autoload(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_autoload();
        }
    }

    pub(super) fn record_counter_negative_lookup_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_negative_lookup_hit();
        }
    }

    pub(super) fn record_counter_invalidation_by_reason(&self, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_invalidation_by_reason(reason);
        }
    }

    pub(super) fn record_counter_fallback_by_path_semantics(&self, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_fallback_by_path_semantics(reason);
            counters.record_include_fallback_by_reason(reason);
        }
    }

    /// Compares an include-path IC target's stored parent-directory version
    /// against the current one, counters only (never hit acceptance).
    pub(super) fn record_counter_directory_version_observation(
        &self,
        target: &IncludePathCacheTarget,
    ) {
        if !self.options.collect_counters {
            return;
        }
        let current = target
            .canonical_path
            .parent()
            .and_then(crate::include::include_directory_version);
        let matches = match (&target.directory_version, &current) {
            (Some(stored), Some(current)) => stored == current,
            _ => false,
        };
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            if matches {
                counters.record_directory_version_hit();
            } else {
                counters.record_directory_version_miss();
            }
        }
    }

    pub(super) fn record_counter_negative_include_cache_blocked(&self, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_negative_include_cache_blocked(reason);
        }
    }

    pub(super) fn record_counter_frame_activation(
        &self,
        reused: bool,
        register_count: u32,
        local_count: u32,
    ) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_frame_activation(reused, register_count, local_count);
            counters.record_alias_state(AliasState::NoReferencesObserved);
        }
    }

    pub(super) fn record_counter_frame_reuse_blocked(&self, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_frame_reuse_blocked(reason);
        }
    }

    pub(super) fn record_counter_call_frame_layout(&self, layout: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_call_frame_layout(layout);
        }
    }

    pub(super) fn record_counter_tiny_frame_candidate(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_tiny_frame_candidate();
        }
    }

    pub(super) fn record_counter_specialized_frame_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_specialized_frame_hit();
        }
    }

    pub(super) fn record_counter_generic_frame_fallback(&self, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_generic_frame_fallback(reason);
        }
    }

    pub(super) fn record_counter_arg_array_avoided(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_arg_array_avoided();
        }
    }

    pub(super) fn record_counter_heap_frame_avoided(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_heap_frame_avoided();
        }
    }

    pub(super) fn record_counter_alias_state(&self, state: AliasState) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_alias_state(state);
        }
    }

    pub(super) fn record_counter_alias_state_transition(&self, from: AliasState, to: AliasState) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_alias_state_transition(from, to);
        }
    }

    pub(super) fn record_counter_fast_path_disabled_by_reference(&self, state: AliasState) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_fast_path_disabled_by_reference(state);
        }
    }

    pub(super) fn record_counter_dequickened_by_reference(&self, state: AliasState) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dequickened_by_reference(state);
        }
    }

    pub(super) fn record_counter_literal_intern(&self, hit: bool) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_literal_intern(hit);
        }
    }

    pub(super) fn record_counter_quickening_site(
        &self,
        function_id: FunctionId,
        bytecode_offset: u32,
        observation: QuickeningObservation,
    ) {
        self.tiering
            .borrow_mut()
            .record_quickening_site(function_id, bytecode_offset, observation);
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_quickening(observation);
        }
    }

    pub(super) fn record_counter_inline_cache_site(
        &self,
        function_id: FunctionId,
        bytecode_offset: u32,
        observation: InlineCacheObservation,
    ) {
        self.tiering.borrow_mut().record_inline_cache_site(
            function_id,
            bytecode_offset,
            observation,
        );
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_inline_cache(observation);
        }
    }

    pub(super) fn record_counter_property_fetch_profile(
        &self,
        observation: PropertyFetchProfileObservation,
    ) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_property_fetch_profile(observation);
        }
    }

    pub(super) fn record_counter_property_ic_fallback(&self, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_property_ic_fallback(reason);
        }
    }

    pub(super) fn record_counter_property_assign_ic_fallback(&self, reason: &str) {
        if !self.options.inline_caches.enabled() {
            return;
        }
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_property_assign_ic_fallback(reason);
        }
    }

    pub(super) fn record_counter_method_call_profile(
        &self,
        observation: MethodCallProfileObservation,
    ) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_method_call_profile(observation);
        }
    }

    pub(super) fn record_counter_method_direct_dispatch_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_method_direct_dispatch_hit();
        }
    }

    pub(super) fn record_counter_method_direct_dispatch_fallback(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_method_direct_dispatch_fallback();
        }
    }

    pub(super) fn record_counter_method_tiny_inline_candidate(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_method_tiny_inline_candidate();
        }
    }

    pub(super) fn record_counter_method_tiny_inline_rejection(&self, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_method_tiny_inline_rejection(reason);
        }
    }

    pub(super) fn record_counter_class_relation_cache_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_class_relation_cache_hit();
        }
    }

    pub(super) fn record_counter_class_relation_cache_miss(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_class_relation_cache_miss();
        }
    }

    pub(super) fn record_counter_class_relation_cache_invalidation(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_class_relation_cache_invalidation();
        }
    }

    pub(super) fn record_counter_instanceof_cache_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_instanceof_cache_hit();
        }
    }

    pub(super) fn record_counter_instanceof_cache_miss(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_instanceof_cache_miss();
        }
    }

    pub(super) fn record_counter_method_override_cache_hit(&self) {
        if !self.options.collect_counters || !self.options.inline_caches.enabled() {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_method_override_cache_hit();
        }
    }

    pub(super) fn record_counter_method_override_cache_miss(&self) {
        if !self.options.collect_counters || !self.options.inline_caches.enabled() {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_method_override_cache_miss();
        }
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(super) fn record_counter_jit_compile_attempt(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_jit_compile_attempt();
        }
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(super) fn record_counter_jit_compiled(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_jit_compiled();
        }
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(super) fn record_counter_jit_compile_metadata(
        &self,
        code_bytes: u64,
        compile_time_nanos: u64,
    ) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_jit_compile_metadata(code_bytes, compile_time_nanos);
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_jit_compile_descriptor(&self, descriptor: JitCompileDescriptor) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_jit_compile_descriptor(descriptor);
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_jit_compile_cache_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_jit_compile_cache_hit();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_jit_compile_cache_miss(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_jit_compile_cache_miss();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_jit_compile_cache_invalidations(&self, count: u64) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            for _ in 0..count {
                counters.record_jit_compile_cache_invalidation();
            }
        }
    }

    pub(super) fn record_counter_jit_tiering_decision(&self, tier: ExecutionTier) {
        if !self.options.collect_counters || !matches!(self.options.jit, JitMode::Cranelift) {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            match tier {
                ExecutionTier::Interpreter => counters.record_jit_tiering_cold_function(),
                ExecutionTier::Jit if self.options.tiering.jit_eager => {
                    counters.record_jit_tiering_eager_function();
                }
                ExecutionTier::Jit => counters.record_jit_tiering_hot_function(),
                ExecutionTier::Quickened => {}
            }
        }
    }

    #[cfg_attr(feature = "jit-cranelift", allow(dead_code))]
    pub(super) fn record_counter_native_candidate(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_native_candidate();
        }
    }

    #[cfg_attr(feature = "jit-cranelift", allow(dead_code))]
    pub(super) fn record_counter_native_platform_unavailable(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_native_platform_unavailable();
        }
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(super) fn record_counter_native_eligibility_rejection(&self, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_native_eligibility_rejection(reason);
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_jit_tiering_blacklist_rejection(&self) {
        self.tiering.borrow_mut().record_jit_blacklist_rejection();
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_jit_tiering_blacklist_rejection();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_jit_tiering_budget_rejection(&self) {
        self.tiering
            .borrow_mut()
            .record_jit_compile_budget_rejection();
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_jit_tiering_budget_rejection();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn jit_compile_budget_allows_attempt(&self) -> bool {
        let tiering = self.tiering.borrow();
        tiering.jit_compiled_functions() < self.options.tiering.jit_max_functions
            && tiering.jit_compile_budget_used_us() < self.options.tiering.jit_max_compile_us
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(super) fn record_counter_jit_executed(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_jit_executed();
        }
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(super) fn record_counter_jit_bailout(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_jit_bailout();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_jit_side_exit(&self, reason: php_jit::SideExitReason) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_jit_side_exit(reason.as_str());
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_jit_guard_failure(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_jit_guard_failure();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_jit_blacklisted_region(&self, reason: JitBlacklistReason) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_jit_blacklisted_region(reason.as_str());
        }
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(super) fn record_counter_jit_helper_calls(&self, count: u64) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            for _ in 0..count {
                counters.record_jit_helper_call();
            }
        }
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(super) fn record_counter_jit_fast_path_hits(&self, count: u64) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            for _ in 0..count {
                counters.record_jit_fast_path_hit();
            }
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_jit_overflow_exit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_jit_overflow_exit();
        }
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(super) fn record_counter_jit_slow_path_call(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_jit_slow_path_call();
        }
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(super) fn record_counter_record_lookup_fast_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_record_lookup_fast_hit();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_record_lookup_key_miss_exit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_record_lookup_key_miss_exit();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_record_lookup_layout_exit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_record_lookup_layout_exit();
        }
    }

    pub(super) fn record_counter_packed_fetch_fast_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_packed_fetch_fast_hit();
        }
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(super) fn record_counter_packed_fetch_bounds_exit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_packed_fetch_bounds_exit();
        }
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(super) fn record_counter_packed_fetch_layout_exit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_packed_fetch_layout_exit();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_packed_foreach_sum_fast_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_packed_foreach_sum_fast_hit();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_packed_foreach_sum_layout_exit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_packed_foreach_sum_layout_exit();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_packed_foreach_sum_overflow_exit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_packed_foreach_sum_overflow_exit();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_known_call_fast_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_known_call_fast_hit();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_known_call_guard_exit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_known_call_guard_exit();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_known_call_slow_call(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_known_call_slow_call();
        }
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(super) fn record_counter_direct_call_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_direct_call_hit();
        }
    }

    #[cfg_attr(not(feature = "jit-cranelift"), allow(dead_code))]
    pub(super) fn record_counter_direct_call_fallback(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_direct_call_fallback();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_property_load_fast_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_property_load_fast_hit();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_property_load_guard_exit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_property_load_guard_exit();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_property_load_layout_exit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_property_load_layout_exit();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_property_load_uninitialized_exit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_property_load_uninitialized_exit();
        }
    }

    #[cfg(feature = "jit-cranelift")]
    pub(super) fn record_counter_property_load_slow_call(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_property_load_slow_call();
        }
    }

    pub(super) fn record_counter_string_concat_fast_path(&self, hit: bool) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_string_concat_fast_path(hit);
        }
    }

    pub(super) fn record_counter_concat_prealloc_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_concat_prealloc_hit();
        }
    }

    pub(super) fn record_counter_concat_fallback(&self, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_concat_fallback(reason);
        }
    }

    pub(super) fn record_counter_value_clone_reason(&self, reason: &'static str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_value_clone_by_reason(reason);
        }
    }

    pub(super) fn record_counter_last_use_move_applied(&self, clone_avoided: bool) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_last_use_move_applied(clone_avoided);
        }
    }

    pub(super) fn record_counter_last_use_array_read_release(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_last_use_array_read_release();
        }
    }

    /// Folds a freshly built plan's build-time rejection reasons into the
    /// counters exactly once (called only when a plan is first analyzed).
    pub(super) fn record_counter_last_use_move_ineligible(
        &self,
        plan: &crate::last_use::LastUseMovePlan,
    ) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            for (reason, count) in plan.ineligible_by_reason() {
                counters.record_last_use_move_ineligible(reason, *count);
            }
        }
    }

    pub(super) fn record_counter_direct_frame(
        &self,
        layout: &str,
        function: &IrFunction,
        elided: bool,
    ) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            if elided {
                counters.record_direct_frame_hit(layout, function.name.ends_with("::__construct"));
            } else {
                counters.record_direct_frame_fallback("argument_vector_observed");
            }
        }
    }

    pub(super) fn record_counter_foreach_no_clone_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_foreach_no_clone_hit();
        }
    }

    pub(super) fn record_counter_symbolized_call_name_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_symbolized_call_name_hit();
        }
    }

    pub(super) fn record_counter_symbolized_method_name_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_symbolized_method_name_hit();
        }
    }

    pub(super) fn record_counter_symbolized_property_name_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_symbolized_property_name_hit();
        }
    }

    pub(super) fn record_counter_symbolized_array_key_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_symbolized_array_key_hit();
        }
    }

    pub(super) fn record_counter_symbolized_name_fallback(&self, reason: &'static str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_symbolized_name_fallback(reason);
        }
    }

    pub(super) fn record_counter_packed_dim_fast_path(&self, hit: bool) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_packed_dim_fast_path(hit);
        }
    }

    pub(super) fn record_counter_array_packed_append_fast_path_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_array_packed_append_fast_path_hit();
        }
    }

    pub(super) fn record_counter_array_packed_read_fast_path_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_array_packed_read_fast_path_hit();
        }
    }

    pub(super) fn record_counter_array_sequential_foreach_fast_path_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_array_sequential_foreach_fast_path_hit();
        }
    }

    pub(super) fn record_counter_cow_or_reference_fallback(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_cow_or_reference_fallback();
        }
    }

    pub(super) fn record_counter_array_fast_path_fallback(&self, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_array_fast_path_fallback(reason);
        }
    }

    pub(super) fn record_counter_array_count_fast_path_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_array_count_fast_path_hit();
        }
    }

    pub(super) fn record_counter_array_packed_to_mixed_transition(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_array_packed_to_mixed_transition();
        }
    }

    pub(super) fn record_counter_array_shape_observed(&self, array: &PhpArray) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_array_shape_observed(array.shape_metadata().kind);
        }
    }

    pub(super) fn record_counter_record_shape_lookup(&self, lookup: &PhpArrayShapeLookup<'_>) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            match lookup {
                PhpArrayShapeLookup::Hit(_) => counters.record_record_shape_lookup_hit(),
                PhpArrayShapeLookup::Miss => counters.record_record_shape_lookup_miss(),
                PhpArrayShapeLookup::Fallback(fallback) => {
                    counters.record_array_shape_lookup_fallback(*fallback);
                }
            }
        }
    }

    pub(super) fn record_counter_small_map_lookup(&self, lookup: &PhpArrayShapeLookup<'_>) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            match lookup {
                PhpArrayShapeLookup::Hit(_) => counters.record_small_map_lookup_hit(),
                PhpArrayShapeLookup::Miss => counters.record_small_map_lookup_miss(),
                PhpArrayShapeLookup::Fallback(fallback) => {
                    counters.record_array_shape_lookup_fallback(*fallback);
                }
            }
        }
    }

    pub(super) fn record_counter_array_shape_lookup_fallback(
        &self,
        fallback: PhpArrayShapeLookupFallback,
    ) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_array_shape_lookup_fallback(fallback);
        }
    }

    pub(super) fn record_counter_numeric_string_specialization_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_numeric_string_specialization_hit();
        }
    }

    pub(super) fn record_counter_internal_function_dispatch(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_internal_function_dispatch();
        }
    }

    pub(super) fn record_counter_internal_function_dispatch_cache(&self, hit: bool) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_internal_function_dispatch_cache(hit);
        }
    }

    pub(super) fn record_counter_internal_count_array_direct_fast_path_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_internal_count_array_direct_fast_path_hit();
        }
    }

    pub(super) fn record_counter_function_call_ic(&self, hit: bool) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_function_call_ic(hit);
        }
    }

    pub(super) fn record_counter_builtin_call_ic(&self, hit: bool) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_builtin_call_ic(hit);
        }
    }

    pub(super) fn record_counter_builtin_fast_stub(&self, name: &str, hit: bool) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_builtin_fast_stub(name, hit);
        }
    }

    pub(super) fn record_counter_builtin_fast_stub_fallback(&self, name: &str, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_builtin_fast_stub_fallback(name, reason);
        }
    }

    pub(super) fn record_counter_builtin_intrinsic_candidate(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_builtin_intrinsic_candidate();
        }
    }

    pub(super) fn record_counter_intrinsic(&self, name: &str, hit: bool) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_intrinsic(name, hit);
        }
    }

    pub(super) fn record_counter_intrinsic_fallback(&self, name: &str, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_intrinsic_fallback(name, reason);
        }
    }

    /// Counts single-key non-append dim writes (the grouped map-update
    /// shape) as slot-fast when the container was mutated in place, and as
    /// a borrow-conflict fallback otherwise. Appends and nested writes stay
    /// outside the map-update counters.
    pub(super) fn record_counter_map_update_slot_path(
        &self,
        path: AssignDimLocalPath,
        dims: &[ArrayKey],
        append: bool,
    ) {
        if append || dims.len() != 1 || !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            match path {
                AssignDimLocalPath::InPlace => counters.record_map_update_slot_fast_hit(),
                AssignDimLocalPath::ClonedReferenceFallback => {
                    counters.record_array_builtin_fast_fallback("map_update", "borrow_conflict");
                }
            }
        }
    }

    pub(super) fn record_counter_property_dim_assign_in_place_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_property_dim_assign_in_place_hit();
        }
    }

    pub(super) fn record_counter_property_dim_assign_generic(&self, reason: &'static str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_property_dim_assign_generic(reason);
        }
    }

    pub(super) fn record_counter_property_dim_probe_borrowed_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_property_dim_probe_borrowed_hit();
        }
    }

    pub(super) fn record_counter_cufa_argument_path(&self, owned: bool) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_cufa_argument_path(owned);
        }
    }

    pub(super) fn record_counter_array_slice_packed_fast_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_array_slice_packed_fast_hit();
        }
    }

    pub(super) fn record_counter_count_array_shape_fast_hit(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_count_array_shape_fast_hit();
        }
    }

    pub(super) fn record_counter_array_builtin_fast_fallback(&self, builtin: &str, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_array_builtin_fast_fallback(builtin, reason);
        }
    }

    pub(super) fn record_counter_json_encode_fast_path(&self, bytes: usize) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_json_encode_fast_path(bytes);
        }
    }

    pub(super) fn record_counter_json_encode_generic_fallback(&self, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_json_encode_generic_fallback(reason);
        }
    }

    pub(super) fn record_counter_call_ic_megamorphic_fallback(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_call_ic_megamorphic_fallback();
        }
    }

    pub(super) fn record_counter_local_slot_fast_path(&self, hit: bool) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_local_slot_fast_path(hit);
        }
    }

    pub(super) fn record_counter_sort_callback(&self, kind: &str, reason: Option<&'static str>) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            match kind {
                "resolved" => counters.sort_callback_resolution_cache_hits += 1,
                "direct" => counters.sort_callback_direct_call_hits += 1,
                _ => {
                    if let Some(reason) = reason {
                        *counters
                            .sort_callback_generic_fallback_by_reason
                            .entry(reason.to_owned())
                            .or_default() += 1;
                    }
                }
            }
        }
    }

    pub(super) fn record_counter_method_inline(&self, kind: &str, reason: Option<&'static str>) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            match kind {
                "candidate" => counters.method_inline_candidates += 1,
                "hit" => counters.method_inline_hits += 1,
                _ => {
                    if let Some(reason) = reason {
                        *counters
                            .method_inline_fallback_by_reason
                            .entry(reason.to_owned())
                            .or_default() += 1;
                    }
                }
            }
        }
    }
}
