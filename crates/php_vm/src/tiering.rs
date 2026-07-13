//! Request-local tiering policy and stats for performance adaptive execution.

use std::collections::BTreeMap;

use php_ir::ids::{BlockId, FunctionId};

use crate::{
    ExitCounterKey, ExitCounterTable, ExitPolicyThresholds, GuardKind, GuardedTier,
    InlineCacheObservation, JitMode, QuickeningMode, QuickeningObservation,
    exit_policy::inline_cache_guard_kind,
};

/// Runtime tier selected by the policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionTier {
    /// Baseline interpreter.
    Interpreter,
    /// Quickened interpreter.
    Quickened,
    /// Experimental feature-gated JIT.
    Jit,
}

/// Configurable tiering thresholds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TieringOptions {
    /// Disable all adaptive tiering when false.
    pub enabled: bool,
    /// Collect request-local tiering stats.
    pub collect_stats: bool,
    /// Function entries required before the policy considers Tier 1.
    pub function_entry_threshold: u64,
    /// Loop backedges required before the policy considers Tier 1.
    pub loop_backedge_threshold: u64,
    /// IC hit score required before the policy considers a site stable.
    pub ic_stability_threshold: i64,
    /// Guard failures after which a site is treated as unstable.
    pub guard_failure_threshold: u64,
    /// Side exits after which the exit policy treats a site as unstable.
    pub side_exit_threshold: u64,
    /// Megamorphic transitions after which the exit policy keeps a site generic.
    pub megamorphic_threshold: u64,
    /// Guard failures after which request-local blacklisting is recommended.
    pub blacklist_threshold: u64,
    /// Exits after which a future recompile is reported as a candidate.
    pub recompile_candidate_threshold: u64,
    /// Compile immediately when JIT execution is enabled; intended for tests.
    pub jit_eager: bool,
    /// Maximum native compile time budget for one request, in microseconds.
    /// `u64::MAX` means no practical budget limit.
    pub jit_max_compile_us: u64,
    /// Maximum number of functions that may be compiled in one request.
    /// `u64::MAX` means no practical budget limit.
    pub jit_max_functions: u64,
}

impl Default for TieringOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            collect_stats: false,
            function_entry_threshold: 8,
            loop_backedge_threshold: 8,
            ic_stability_threshold: 4,
            guard_failure_threshold: 2,
            side_exit_threshold: 2,
            megamorphic_threshold: 1,
            blacklist_threshold: 3,
            recompile_candidate_threshold: 4,
            jit_eager: false,
            jit_max_compile_us: u64::MAX,
            jit_max_functions: u64::MAX,
        }
    }
}

impl TieringOptions {
    #[must_use]
    pub const fn exit_policy_thresholds(&self) -> ExitPolicyThresholds {
        ExitPolicyThresholds {
            guard_failure_threshold: self.guard_failure_threshold,
            side_exit_threshold: self.side_exit_threshold,
            megamorphic_threshold: self.megamorphic_threshold,
            blacklist_threshold: self.blacklist_threshold,
            recompile_candidate_threshold: self.recompile_candidate_threshold,
        }
    }
}

/// Visible request-local tiering stats.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TieringStats {
    pub function_entry_count: u64,
    pub loop_backedge_count: u64,
    pub ic_stability_score: i64,
    pub guard_failure_score: u64,
    pub tier0_interpreter_entries: u64,
    pub tier1_quickened_entries: u64,
    pub tier2_jit_candidates: u64,
    pub tiering_disabled_entries: u64,
    pub jit_cold_entries: u64,
    pub jit_eager_candidates: u64,
    pub jit_threshold_candidates: u64,
    pub jit_blacklist_rejections: u64,
    pub jit_compile_budget_rejections: u64,
    pub jit_compile_budget_used_us: u64,
    pub jit_compiled_functions: u64,
    pub exit_policy: ExitCounterTable,
}

impl TieringStats {
    #[must_use]
    pub fn to_json(&self) -> String {
        format!(
            concat!(
                "{{\n",
                "  \"schema_version\": 2,\n",
                "  \"function_entry_count\": {},\n",
                "  \"loop_backedge_count\": {},\n",
                "  \"ic_stability_score\": {},\n",
                "  \"guard_failure_score\": {},\n",
                "  \"tier0_interpreter_entries\": {},\n",
                "  \"tier1_quickened_entries\": {},\n",
                "  \"tier2_jit_candidates\": {},\n",
                "  \"tiering_disabled_entries\": {},\n",
                "  \"jit_cold_entries\": {},\n",
                "  \"jit_eager_candidates\": {},\n",
                "  \"jit_threshold_candidates\": {},\n",
                "  \"jit_blacklist_rejections\": {},\n",
                "  \"jit_compile_budget_rejections\": {},\n",
                "  \"jit_compile_budget_used_us\": {},\n",
                "  \"jit_compiled_functions\": {},\n",
                "  \"exit_policy\": {}\n",
                "}}\n"
            ),
            self.function_entry_count,
            self.loop_backedge_count,
            self.ic_stability_score,
            self.guard_failure_score,
            self.tier0_interpreter_entries,
            self.tier1_quickened_entries,
            self.tier2_jit_candidates,
            self.tiering_disabled_entries,
            self.jit_cold_entries,
            self.jit_eager_candidates,
            self.jit_threshold_candidates,
            self.jit_blacklist_rejections,
            self.jit_compile_budget_rejections,
            self.jit_compile_budget_used_us,
            self.jit_compiled_functions,
            self.exit_policy.to_json()
        )
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct FunctionHotness {
    entries: u64,
    backedges: u64,
}

/// Request-local tiering state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TieringState {
    options: TieringOptions,
    stats: TieringStats,
    functions: BTreeMap<(u64, u32), FunctionHotness>,
}

impl TieringState {
    #[must_use]
    pub fn new(options: TieringOptions) -> Self {
        let exit_policy_thresholds = options.exit_policy_thresholds();
        Self {
            options,
            stats: TieringStats {
                exit_policy: ExitCounterTable::new(exit_policy_thresholds),
                ..TieringStats::default()
            },
            functions: BTreeMap::new(),
        }
    }

    /// Starts request-local accounting without discarding worker hotness.
    pub fn begin_request(&mut self, options: TieringOptions) {
        let exit_policy_thresholds = options.exit_policy_thresholds();
        self.options = options;
        self.stats = TieringStats {
            exit_policy: ExitCounterTable::new(exit_policy_thresholds),
            ..TieringStats::default()
        };
    }

    #[must_use]
    pub fn stats(&self) -> TieringStats {
        self.stats.clone()
    }

    pub fn record_function_entry(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        quickening: QuickeningMode,
        jit: JitMode,
    ) -> ExecutionTier {
        if !self.options.enabled {
            self.stats.tiering_disabled_entries =
                self.stats.tiering_disabled_entries.saturating_add(1);
            return ExecutionTier::Interpreter;
        }

        self.stats.function_entry_count = self.stats.function_entry_count.saturating_add(1);
        let hotness = self
            .functions
            .entry((unit_key, function.raw()))
            .or_default();
        hotness.entries = hotness.entries.saturating_add(1);

        let jit_enabled = matches!(jit, JitMode::Cranelift);
        let hot_by_entry = hotness.entries >= self.options.function_entry_threshold;
        let hot_by_backedge = hotness.backedges >= self.options.loop_backedge_threshold;
        let guards_stable = self.stats.guard_failure_score < self.options.guard_failure_threshold;
        if jit_enabled
            && guards_stable
            && (self.options.jit_eager || hot_by_entry || hot_by_backedge)
        {
            self.stats.tier2_jit_candidates = self.stats.tier2_jit_candidates.saturating_add(1);
            if self.options.jit_eager {
                self.stats.jit_eager_candidates = self.stats.jit_eager_candidates.saturating_add(1);
            } else {
                self.stats.jit_threshold_candidates =
                    self.stats.jit_threshold_candidates.saturating_add(1);
            }
            ExecutionTier::Jit
        } else if quickening.enabled() && (hot_by_entry || hot_by_backedge) && guards_stable {
            self.stats.tier1_quickened_entries =
                self.stats.tier1_quickened_entries.saturating_add(1);
            ExecutionTier::Quickened
        } else {
            if jit_enabled && guards_stable && !self.options.jit_eager {
                self.stats.jit_cold_entries = self.stats.jit_cold_entries.saturating_add(1);
            }
            self.stats.tier0_interpreter_entries =
                self.stats.tier0_interpreter_entries.saturating_add(1);
            ExecutionTier::Interpreter
        }
    }

    pub fn record_loop_backedge(
        &mut self,
        unit_key: u64,
        function: FunctionId,
        current: BlockId,
        target: BlockId,
    ) {
        if !self.options.enabled || target.raw() > current.raw() {
            return;
        }
        self.stats.loop_backedge_count = self.stats.loop_backedge_count.saturating_add(1);
        let hotness = self
            .functions
            .entry((unit_key, function.raw()))
            .or_default();
        hotness.backedges = hotness.backedges.saturating_add(1);
    }

    pub fn record_quickening(&mut self, observation: QuickeningObservation) {
        if !self.options.enabled {
            return;
        }
        if observation.guard_hit || observation.specialized {
            self.stats.ic_stability_score = self.stats.ic_stability_score.saturating_add(1);
        }
        if observation.guard_failure {
            self.stats.guard_failure_score = self.stats.guard_failure_score.saturating_add(1);
        }
    }

    /// Returns true when per-site exit-policy bookkeeping has a consumer.
    ///
    /// The exit-policy table only feeds native-tier compile/blacklist
    /// decisions (jit-cranelift builds) and optional tiering stats. Without
    /// either, the per-site `ExitCounterKey` allocation and map updates are
    /// write-only work on the dispatch hot path. Aggregate quickening/IC
    /// stats (including `guard_failure_score`, which tier decisions read)
    /// are recorded unconditionally by the callers below.
    #[inline]
    fn exit_policy_recording_active(&self) -> bool {
        cfg!(feature = "jit-cranelift") || self.options.collect_stats
    }

    pub fn record_quickening_site(
        &mut self,
        function: FunctionId,
        bytecode_offset: u32,
        observation: QuickeningObservation,
    ) {
        self.record_quickening(observation);
        if !self.options.enabled || !self.exit_policy_recording_active() {
            return;
        }
        let key = ExitCounterKey::bytecode(
            function.raw(),
            bytecode_offset,
            GuardedTier::Quickening,
            if observation.dequickened {
                "type_flip"
            } else {
                "quickening_guard"
            },
            Some(GuardKind::QuickeningType),
        );
        if observation.guard_hit || observation.specialized {
            self.stats.exit_policy.record_stable_hit(key.clone());
        }
        if observation.guard_failure {
            self.stats.exit_policy.record_guard_failure(key);
        }
    }

    pub fn record_inline_cache(&mut self, observation: InlineCacheObservation) {
        if !self.options.enabled {
            return;
        }
        if observation.hit {
            self.stats.ic_stability_score = self.stats.ic_stability_score.saturating_add(1);
        }
        if observation.guard_failure {
            self.stats.guard_failure_score = self.stats.guard_failure_score.saturating_add(1);
        }
    }

    pub fn record_inline_cache_site(
        &mut self,
        function: FunctionId,
        bytecode_offset: u32,
        observation: InlineCacheObservation,
    ) {
        self.record_inline_cache(observation);
        if !self.options.enabled || !self.exit_policy_recording_active() {
            return;
        }
        let guard_kind = inline_cache_guard_kind(observation.kind);
        let key = ExitCounterKey::bytecode(
            function.raw(),
            bytecode_offset,
            GuardedTier::InlineCache,
            inline_cache_exit_reason(observation),
            Some(guard_kind),
        );
        if observation.hit {
            self.stats.exit_policy.record_stable_hit(key.clone());
        }
        if observation.guard_failure {
            self.stats.exit_policy.record_guard_failure(key.clone());
        }
        if observation.megamorphic {
            self.stats.exit_policy.record_megamorphic(key.clone());
        }
        if observation.miss && !observation.guard_failure && !observation.megamorphic {
            self.stats.exit_policy.record_side_exit(key);
        }
    }

    pub fn record_jit_side_exit(
        &mut self,
        function: FunctionId,
        region_id: impl Into<String>,
        reason: impl Into<String>,
        guard_kind: Option<GuardKind>,
    ) {
        if !self.options.enabled {
            return;
        }
        self.stats
            .exit_policy
            .record_side_exit(ExitCounterKey::region(
                function.raw(),
                region_id,
                GuardedTier::Cranelift,
                reason,
                guard_kind,
            ));
    }

    pub fn record_jit_blacklist_rejection(&mut self) {
        self.stats.jit_blacklist_rejections = self.stats.jit_blacklist_rejections.saturating_add(1);
    }

    pub fn record_jit_compile_budget_rejection(&mut self) {
        self.stats.jit_compile_budget_rejections =
            self.stats.jit_compile_budget_rejections.saturating_add(1);
    }

    pub fn record_jit_compiled_function(&mut self, compile_time_nanos: u64) {
        self.stats.jit_compiled_functions = self.stats.jit_compiled_functions.saturating_add(1);
        let micros = compile_time_nanos.saturating_add(999) / 1_000;
        self.stats.jit_compile_budget_used_us =
            self.stats.jit_compile_budget_used_us.saturating_add(micros);
    }

    #[must_use]
    pub fn jit_compile_budget_used_us(&self) -> u64 {
        self.stats.jit_compile_budget_used_us
    }

    #[must_use]
    pub fn jit_compiled_functions(&self) -> u64 {
        self.stats.jit_compiled_functions
    }
}

fn inline_cache_exit_reason(observation: InlineCacheObservation) -> &'static str {
    if observation.megamorphic {
        "megamorphic_site"
    } else if observation.guard_failure {
        match observation.kind {
            Some(crate::InlineCacheKind::MethodCall | crate::InlineCacheKind::PropertyFetch) => {
                "wrong_class_shape"
            }
            Some(crate::InlineCacheKind::DimFetch) => "packed_to_mixed_layout",
            Some(crate::InlineCacheKind::FunctionCall) => "builtin_or_function_shape",
            _ => "inline_cache_guard",
        }
    } else if observation.miss {
        match observation.kind {
            Some(crate::InlineCacheKind::DimFetch) => "packed_to_mixed_layout",
            Some(crate::InlineCacheKind::FunctionCall) => "builtin_stub_fallback",
            _ => "inline_cache_miss",
        }
    } else {
        "stable"
    }
}

impl Default for TieringState {
    fn default() -> Self {
        Self::new(TieringOptions::default())
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionTier, TieringOptions, TieringState};
    use crate::{InlineCacheObservation, JitMode, QuickeningMode, QuickeningObservation};
    use php_ir::ids::{BlockId, FunctionId};

    #[test]
    fn policy_promotes_quickening_after_entry_threshold() {
        let mut state = TieringState::new(TieringOptions {
            function_entry_threshold: 2,
            ..TieringOptions::default()
        });

        assert_eq!(
            state.record_function_entry(7, FunctionId::new(1), QuickeningMode::On, JitMode::Off),
            ExecutionTier::Interpreter
        );
        assert_eq!(
            state.record_function_entry(7, FunctionId::new(1), QuickeningMode::On, JitMode::Off),
            ExecutionTier::Quickened
        );
        assert_eq!(state.stats().tier1_quickened_entries, 1);
    }

    #[test]
    fn disabled_policy_stays_interpreter() {
        let mut state = TieringState::new(TieringOptions {
            enabled: false,
            function_entry_threshold: 1,
            ..TieringOptions::default()
        });

        assert_eq!(
            state.record_function_entry(
                7,
                FunctionId::new(1),
                QuickeningMode::On,
                JitMode::Cranelift,
            ),
            ExecutionTier::Interpreter
        );
        assert_eq!(state.stats().tiering_disabled_entries, 1);
        assert_eq!(state.stats().tier2_jit_candidates, 0);
    }

    #[test]
    fn eager_policy_promotes_first_entry_for_tests() {
        let mut state = TieringState::new(TieringOptions {
            jit_eager: true,
            function_entry_threshold: 10,
            ..TieringOptions::default()
        });

        assert_eq!(
            state.record_function_entry(
                7,
                FunctionId::new(1),
                QuickeningMode::Off,
                JitMode::Cranelift
            ),
            ExecutionTier::Jit
        );
        assert_eq!(state.stats().jit_eager_candidates, 1);
        assert_eq!(state.stats().jit_cold_entries, 0);
    }

    #[test]
    fn cold_jit_entry_stays_interpreter_until_threshold() {
        let mut state = TieringState::new(TieringOptions {
            function_entry_threshold: 2,
            ..TieringOptions::default()
        });

        assert_eq!(
            state.record_function_entry(
                7,
                FunctionId::new(1),
                QuickeningMode::Off,
                JitMode::Cranelift
            ),
            ExecutionTier::Interpreter
        );
        assert_eq!(
            state.record_function_entry(
                7,
                FunctionId::new(1),
                QuickeningMode::Off,
                JitMode::Cranelift
            ),
            ExecutionTier::Jit
        );
        assert_eq!(state.stats().jit_cold_entries, 1);
        assert_eq!(state.stats().jit_threshold_candidates, 1);
    }

    #[test]
    fn backedge_hotness_is_counted() {
        let mut state = TieringState::new(TieringOptions::default());

        state.record_loop_backedge(7, FunctionId::new(1), BlockId::new(3), BlockId::new(1));
        state.record_loop_backedge(7, FunctionId::new(1), BlockId::new(1), BlockId::new(3));

        assert_eq!(state.stats().loop_backedge_count, 1);
    }

    #[test]
    fn megamorphic_guard_failures_stay_interpreter() {
        let mut state = TieringState::new(TieringOptions {
            function_entry_threshold: 1,
            guard_failure_threshold: 2,
            ..TieringOptions::default()
        });

        state.record_quickening(QuickeningObservation {
            specialization: None,
            attempt: true,
            specialized: false,
            guard_hit: false,
            guard_miss: true,
            guard_failure: true,
            fallback_call: true,
            dequickened: false,
            megamorphic: false,
            disabled: false,
            seeded: false,
        });
        state.record_inline_cache(InlineCacheObservation {
            candidate: true,
            seeded: false,
            persistent_worker: false,
            slot_allocated: true,
            kind: None,
            hit: false,
            miss: true,
            guard_failure: true,
            invalidation: false,
            fallback_call: true,
            monomorphic: false,
            polymorphic: false,
            megamorphic: true,
            disabled: false,
        });

        assert_eq!(
            state.record_function_entry(
                7,
                FunctionId::new(1),
                QuickeningMode::On,
                JitMode::Cranelift,
            ),
            ExecutionTier::Interpreter
        );
        assert_eq!(state.stats().guard_failure_score, 2);
        assert_eq!(state.stats().tier2_jit_candidates, 0);
    }

    #[test]
    fn request_stats_reset_but_unit_scoped_hotness_persists() {
        let options = TieringOptions {
            function_entry_threshold: 2,
            ..TieringOptions::default()
        };
        let mut state = TieringState::new(options.clone());

        assert_eq!(
            state.record_function_entry(7, FunctionId::new(1), QuickeningMode::On, JitMode::Off),
            ExecutionTier::Interpreter
        );
        state.begin_request(options);
        assert_eq!(state.stats().function_entry_count, 0);
        assert_eq!(
            state.record_function_entry(7, FunctionId::new(1), QuickeningMode::On, JitMode::Off),
            ExecutionTier::Quickened
        );
        assert_eq!(
            state.record_function_entry(8, FunctionId::new(1), QuickeningMode::On, JitMode::Off),
            ExecutionTier::Interpreter
        );
    }
}
