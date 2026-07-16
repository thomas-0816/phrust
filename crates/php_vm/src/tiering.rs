//! Native-version compilation budgets and statistics.

/// Runtime native version selected by the coordinator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionTier {
    Baseline,
    Optimized,
}

/// Configurable native compilation thresholds and budgets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TieringOptions {
    pub enabled: bool,
    pub collect_stats: bool,
    pub function_entry_threshold: u64,
    pub loop_backedge_threshold: u64,
    pub ic_stability_threshold: i64,
    pub guard_failure_threshold: u64,
    pub side_exit_threshold: u64,
    pub megamorphic_threshold: u64,
    pub blacklist_threshold: u64,
    pub recompile_candidate_threshold: u64,
    pub native_eager: bool,
    pub native_max_compile_us: u64,
    pub native_max_functions: u64,
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
            native_eager: false,
            native_max_compile_us: u64::MAX,
            native_max_functions: u64::MAX,
        }
    }
}

/// Visible native compilation and transition statistics.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TieringStats {
    pub function_entry_count: u64,
    pub loop_backedge_count: u64,
    pub baseline_entries: u64,
    pub optimized_candidates: u64,
    pub native_guard_exits: u64,
    pub native_blacklist_rejections: u64,
    pub native_compile_budget_rejections: u64,
    pub native_compile_budget_used_us: u64,
    pub native_compiled_functions: u64,
}

impl TieringStats {
    #[must_use]
    pub fn to_json(&self) -> String {
        format!(
            concat!(
                "{{\n",
                "  \"schema_version\": 3,\n",
                "  \"function_entry_count\": {},\n",
                "  \"loop_backedge_count\": {},\n",
                "  \"baseline_entries\": {},\n",
                "  \"optimized_candidates\": {},\n",
                "  \"native_guard_exits\": {},\n",
                "  \"native_blacklist_rejections\": {},\n",
                "  \"native_compile_budget_rejections\": {},\n",
                "  \"native_compile_budget_used_us\": {},\n",
                "  \"native_compiled_functions\": {}\n",
                "}}\n"
            ),
            self.function_entry_count,
            self.loop_backedge_count,
            self.baseline_entries,
            self.optimized_candidates,
            self.native_guard_exits,
            self.native_blacklist_rejections,
            self.native_compile_budget_rejections,
            self.native_compile_budget_used_us,
            self.native_compiled_functions,
        )
    }
}
