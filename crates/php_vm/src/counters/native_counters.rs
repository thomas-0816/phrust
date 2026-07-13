//! Native-tier and JIT counter updates.

use super::{JitCompileDescriptor, VmCounters};

impl VmCounters {
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

    #[cfg_attr(
        not(all(feature = "jit-copy-patch", unix, target_arch = "aarch64")),
        allow(dead_code)
    )]
    pub(crate) fn record_copy_patch_executed(&mut self) {
        self.copy_patch_executed += 1;
        self.native_executions += 1;
    }

    #[cfg_attr(
        not(all(feature = "jit-copy-patch", unix, target_arch = "aarch64")),
        allow(dead_code)
    )]
    pub(crate) fn record_native_leaf_cache_lookup(&mut self, positive: bool) {
        if positive {
            self.native_leaf_cache_positive_hits += 1;
        } else {
            self.native_leaf_cache_negative_hits += 1;
        }
    }

    #[cfg_attr(
        not(all(feature = "jit-copy-patch", unix, target_arch = "aarch64")),
        allow(dead_code)
    )]
    pub(crate) fn record_native_leaf_prewarm(
        &mut self,
        attempts: u64,
        compiled: u64,
        rejected: u64,
        code_bytes: u64,
        compile_time_nanos: u64,
        rejections_by_shape: &std::collections::BTreeMap<String, u64>,
    ) {
        self.native_leaf_prewarm_attempts += attempts;
        self.native_leaf_prewarm_compiled += compiled;
        self.native_leaf_prewarm_rejected += rejected;
        self.native_leaf_prewarm_code_bytes += code_bytes;
        self.native_leaf_prewarm_compile_time_nanos += compile_time_nanos;
        for (shape, count) in rejections_by_shape {
            *self
                .native_leaf_rejections_by_shape
                .entry(shape.clone())
                .or_default() += count;
        }
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

    #[allow(dead_code)]
    pub(crate) fn record_jit_helper_call(&mut self) {
        self.jit_helper_calls += 1;
    }

    #[allow(dead_code)]
    pub(crate) fn record_jit_fast_path_hit(&mut self) {
        self.jit_fast_path_hits += 1;
    }
}
