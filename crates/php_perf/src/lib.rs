//! performance performance measurement data model.
//!
//! This crate only defines stable report types. It does not run benchmarks,
//! collect VM counters, or choose performance budgets.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Stable schema version for performance performance reports.
pub const PERF_REPORT_SCHEMA_VERSION: u32 = 1;

/// Stable schema version for Cranelift big-win JIT reports.
pub const CRANELIFT_JIT_REPORT_SCHEMA_VERSION: u32 = 1;

/// Stable schema version for PHP VM internal phase timing sidecars.
pub const PHASE_TIMING_REPORT_SCHEMA_VERSION: u32 = 1;

/// Internal phase timing sidecar for one `php-vm` command invocation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhaseTimingReport {
    /// Report schema version.
    pub schema_version: u32,
    /// Command name, for example `run` or `compile`.
    pub command: String,
    /// Input path exactly as normalized by the caller.
    pub path: String,
    /// Total measured internal command time in milliseconds.
    pub total_internal_ms: f64,
    /// Per-phase milliseconds sorted by phase key.
    pub phases: BTreeMap<String, f64>,
    /// Cheap counts available from already-built structures.
    pub counts: BTreeMap<String, u64>,
    /// Stable command/runtime flags that affect the measured path.
    pub flags: BTreeMap<String, String>,
}

impl PhaseTimingReport {
    /// Creates an empty timing report for a command and input path.
    #[must_use]
    pub fn new(command: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            schema_version: PHASE_TIMING_REPORT_SCHEMA_VERSION,
            command: command.into(),
            path: path.into(),
            total_internal_ms: 0.0,
            phases: BTreeMap::new(),
            counts: BTreeMap::new(),
            flags: BTreeMap::new(),
        }
    }

    /// Serializes this report to normalized pretty JSON with a trailing newline.
    pub fn to_stable_json(&self) -> serde_json::Result<String> {
        let mut json = serde_json::to_string_pretty(self)?;
        json.push('\n');
        Ok(json)
    }
}

/// Stable identifier for one benchmark/report run.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PerfRunId(String);

impl PerfRunId {
    /// Creates a run id from a caller-provided deterministic string.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the run id as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Benchmark scenario metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PerfScenario {
    /// Stable scenario id, for example `performance.perf_smoke.echo_loop`.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Scenario group such as `vm`, `array`, `call`, or `composer`.
    pub group: String,
    /// Optional fixture path or synthetic scenario source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixture: Option<String>,
}

impl PerfScenario {
    /// Creates a scenario with no fixture path.
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, group: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            group: group.into(),
            fixture: None,
        }
    }

    /// Adds a fixture path.
    #[must_use]
    pub fn with_fixture(mut self, fixture: impl Into<String>) -> Self {
        self.fixture = Some(fixture.into());
        self
    }
}

/// Metric metadata for one measurement value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PerfMetric {
    /// Stable metric name such as `wall_time_ms`.
    pub name: String,
    /// Unit such as `ms`, `count`, or `bytes`.
    pub unit: String,
    /// Numeric value for the metric.
    pub value: f64,
    /// Whether lower values are better for this metric.
    pub lower_is_better: bool,
}

impl PerfMetric {
    /// Creates a metric value.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        unit: impl Into<String>,
        value: f64,
        lower_is_better: bool,
    ) -> Self {
        Self {
            name: name.into(),
            unit: unit.into(),
            value,
            lower_is_better,
        }
    }
}

/// Environment metadata shared by all measurements in a report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PerfEnvironment {
    /// Engine version or semantic build label.
    pub engine_version: String,
    /// Git commit when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    /// Rust target triple used by the benchmark binary.
    pub rust_target_triple: String,
    /// Optimization flags and engine switches used for the run.
    pub opt_flags: Vec<String>,
    /// Feature flags captured in sorted order for stable JSON.
    pub feature_flags: BTreeMap<String, bool>,
    /// Additional normalized environment values.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
}

impl PerfEnvironment {
    /// Creates an environment record.
    #[must_use]
    pub fn new(engine_version: impl Into<String>, rust_target_triple: impl Into<String>) -> Self {
        Self {
            engine_version: engine_version.into(),
            git_commit: None,
            rust_target_triple: rust_target_triple.into(),
            opt_flags: Vec::new(),
            feature_flags: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }

    /// Adds a git commit.
    #[must_use]
    pub fn with_git_commit(mut self, git_commit: impl Into<String>) -> Self {
        self.git_commit = Some(git_commit.into());
        self
    }

    /// Adds optimization flags in caller-defined order.
    #[must_use]
    pub fn with_opt_flags(
        mut self,
        opt_flags: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.opt_flags = opt_flags.into_iter().map(Into::into).collect();
        self
    }

    /// Adds or updates a feature flag.
    #[must_use]
    pub fn with_feature_flag(mut self, name: impl Into<String>, enabled: bool) -> Self {
        self.feature_flags.insert(name.into(), enabled);
        self
    }

    /// Adds a normalized environment field.
    #[must_use]
    pub fn with_extra(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra.insert(name.into(), value.into());
        self
    }
}

/// One scenario measurement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PerfMeasurement {
    /// Benchmark scenario.
    pub scenario: PerfScenario,
    /// Number of benchmark iterations.
    pub iterations: u64,
    /// Primary and secondary metrics.
    pub metrics: Vec<PerfMetric>,
    /// Optional wall time in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_time_ms: Option<f64>,
    /// Optional VM counters sorted by key for stable JSON.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vm_counters: BTreeMap<String, u64>,
}

impl PerfMeasurement {
    /// Creates an empty measurement for a scenario.
    #[must_use]
    pub fn new(scenario: PerfScenario, iterations: u64) -> Self {
        Self {
            scenario,
            iterations,
            metrics: Vec::new(),
            wall_time_ms: None,
            vm_counters: BTreeMap::new(),
        }
    }

    /// Adds a metric.
    #[must_use]
    pub fn with_metric(mut self, metric: PerfMetric) -> Self {
        self.metrics.push(metric);
        self
    }

    /// Adds wall-clock time in milliseconds.
    #[must_use]
    pub fn with_wall_time_ms(mut self, wall_time_ms: f64) -> Self {
        self.wall_time_ms = Some(wall_time_ms);
        self
    }

    /// Adds a VM counter.
    #[must_use]
    pub fn with_vm_counter(mut self, name: impl Into<String>, value: u64) -> Self {
        self.vm_counters.insert(name.into(), value);
        self
    }
}

/// A complete performance report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PerfReport {
    /// Report schema version.
    pub schema_version: u32,
    /// Run identifier.
    pub run_id: PerfRunId,
    /// Shared environment metadata.
    pub environment: PerfEnvironment,
    /// Scenario measurements.
    pub measurements: Vec<PerfMeasurement>,
}

impl PerfReport {
    /// Creates an empty report.
    #[must_use]
    pub fn new(run_id: PerfRunId, environment: PerfEnvironment) -> Self {
        Self {
            schema_version: PERF_REPORT_SCHEMA_VERSION,
            run_id,
            environment,
            measurements: Vec::new(),
        }
    }

    /// Adds a measurement.
    #[must_use]
    pub fn with_measurement(mut self, measurement: PerfMeasurement) -> Self {
        self.measurements.push(measurement);
        self
    }

    /// Serializes this report to normalized pretty JSON with a trailing newline.
    pub fn to_stable_json(&self) -> serde_json::Result<String> {
        let mut json = serde_json::to_string_pretty(self)?;
        json.push('\n');
        Ok(json)
    }
}

/// Correctness result for one JIT comparison row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JitCorrectnessStatus {
    /// JIT-off and JIT-on behavior matched.
    Pass,
    /// JIT-off and JIT-on behavior diverged.
    Fail,
    /// Row was skipped before comparison.
    Skipped,
}

/// Runtime JIT status for one report row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JitExecutionStatus {
    /// Region executed through the JIT path.
    Executed,
    /// Region fell back before native entry.
    Fallback,
    /// Region entered and returned through a side exit.
    SideExit,
    /// Region was skipped because feature, platform, or fixture support was absent.
    Skipped,
}

/// Stable JIT counter bundle used by Cranelift reports.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct JitCounterSnapshot {
    /// Compile attempts observed.
    pub compile_attempts: u64,
    /// Regions successfully compiled.
    pub compiled_regions: u64,
    /// Regions executed through the JIT path.
    pub executed_regions: u64,
    /// Bailouts before or during execution.
    pub bailouts: u64,
    /// Native code bytes generated by accepted JIT compiles.
    pub code_bytes: u64,
    /// Native compile time accumulated for accepted JIT compiles.
    pub compile_time_nanos: u64,
    /// Structured side exits.
    pub side_exits: u64,
    /// Guard failures.
    pub guard_failures: u64,
    /// Regions disabled after repeated failures.
    pub blacklisted_regions: u64,
    /// Function entries kept cold by the tiering threshold.
    pub tiering_cold_functions: u64,
    /// Function entries admitted by the tiering threshold.
    pub tiering_hot_functions: u64,
    /// Function entries admitted by test-only eager mode.
    pub tiering_eager_functions: u64,
    /// Function entries rejected because the region was blacklisted.
    pub tiering_blacklist_rejections: u64,
    /// Function entries rejected by the request compile budget.
    pub tiering_budget_rejections: u64,
    /// Runtime helper calls made by JIT code.
    pub helper_calls: u64,
    /// Inline fast-path operations completed by JIT code.
    pub fast_path_hits: u64,
    /// Helper-assisted packed-array int-index fetches completed by JIT code.
    pub packed_fetch_fast_hits: u64,
    /// Packed-array fetch exits caused by negative or out-of-bounds indexes.
    pub packed_fetch_bounds_exits: u64,
    /// Packed-array fetch exits caused by layout, element, or reference guards.
    pub packed_fetch_layout_exits: u64,
    /// Packed-array foreach int-sum native loop invocations.
    pub packed_foreach_sum_fast_hits: u64,
    /// Packed-array foreach int-sum exits caused by layout or element guards.
    pub packed_foreach_sum_layout_exits: u64,
    /// Packed-array foreach int-sum exits caused by checked-add overflow.
    pub packed_foreach_sum_overflow_exits: u64,
    /// Guarded known internal calls completed by native code.
    pub known_call_fast_hits: u64,
    /// Known internal-call exits caused by string/array guard misses.
    pub known_call_guard_exits: u64,
    /// Slow path calls after known internal-call guard exits.
    pub known_call_slow_calls: u64,
    /// Guarded monomorphic method calls completed by the direct VM/JIT dispatch helper.
    pub direct_call_hits: u64,
    /// Generic method-call fallbacks after direct-call misses or guard failures.
    pub direct_call_fallbacks: u64,
    /// Guarded monomorphic property loads completed by native code.
    pub property_load_fast_hits: u64,
    /// Property-load exits caused by class, hook, magic, or storage guards.
    pub property_load_guard_exits: u64,
    /// Property-load exits caused by stale class layout metadata.
    pub property_load_layout_exits: u64,
    /// Property-load exits caused by uninitialized typed properties.
    pub property_load_uninitialized_exits: u64,
    /// Slow path calls after property-load guard exits.
    pub property_load_slow_calls: u64,
    /// Guarded string/string concatenations completed by native code.
    pub string_concat_fast_path_hits: u64,
    /// String-concat guard exits resumed through generic concat semantics.
    pub string_concat_fast_path_misses: u64,
    /// Inline arithmetic overflow exits observed.
    pub overflow_exits: u64,
    /// Slow path calls caused by JIT fallback/resume.
    pub slow_path_calls: u64,
    /// Process-local compile-cache hits.
    pub compile_cache_hits: u64,
    /// Process-local compile-cache misses.
    pub compile_cache_misses: u64,
    /// Process-local compile-cache invalidations.
    pub compile_cache_invalidations: u64,
}

impl JitCounterSnapshot {
    /// Creates an empty JIT counter snapshot.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            compile_attempts: 0,
            compiled_regions: 0,
            executed_regions: 0,
            bailouts: 0,
            code_bytes: 0,
            compile_time_nanos: 0,
            side_exits: 0,
            guard_failures: 0,
            blacklisted_regions: 0,
            tiering_cold_functions: 0,
            tiering_hot_functions: 0,
            tiering_eager_functions: 0,
            tiering_blacklist_rejections: 0,
            tiering_budget_rejections: 0,
            helper_calls: 0,
            fast_path_hits: 0,
            packed_fetch_fast_hits: 0,
            packed_fetch_bounds_exits: 0,
            packed_fetch_layout_exits: 0,
            packed_foreach_sum_fast_hits: 0,
            packed_foreach_sum_layout_exits: 0,
            packed_foreach_sum_overflow_exits: 0,
            known_call_fast_hits: 0,
            known_call_guard_exits: 0,
            known_call_slow_calls: 0,
            direct_call_hits: 0,
            direct_call_fallbacks: 0,
            property_load_fast_hits: 0,
            property_load_guard_exits: 0,
            property_load_layout_exits: 0,
            property_load_uninitialized_exits: 0,
            property_load_slow_calls: 0,
            string_concat_fast_path_hits: 0,
            string_concat_fast_path_misses: 0,
            overflow_exits: 0,
            slow_path_calls: 0,
            compile_cache_hits: 0,
            compile_cache_misses: 0,
            compile_cache_invalidations: 0,
        }
    }
}

/// One Cranelift big-win report row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CraneliftJitReportRow {
    /// Stable big-win target id.
    pub target: String,
    /// Fixture or scenario path.
    pub fixture: String,
    /// Correctness comparison result.
    pub correctness: JitCorrectnessStatus,
    /// JIT execution status.
    pub jit_status: JitExecutionStatus,
    /// Iterations run for this row.
    pub iterations: u64,
    /// Optional wall-clock timing in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_time_seconds: Option<f64>,
    /// Optional total elapsed timing in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_time_seconds: Option<f64>,
    /// Optional native compile timing in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compile_time_seconds: Option<f64>,
    /// Optional execution timing in seconds, excluding known native compile time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_time_seconds: Option<f64>,
    /// Optional instruction count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<u64>,
    /// Stable JIT counters.
    pub counters: JitCounterSnapshot,
    /// Additional VM counters sorted by key.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vm_counters: BTreeMap<String, u64>,
    /// Known-gap or skip identifiers relevant to this row.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_gaps: Vec<String>,
}

impl CraneliftJitReportRow {
    /// Creates a report row with no timing data.
    #[must_use]
    pub fn new(
        target: impl Into<String>,
        fixture: impl Into<String>,
        correctness: JitCorrectnessStatus,
        jit_status: JitExecutionStatus,
        iterations: u64,
    ) -> Self {
        Self {
            target: target.into(),
            fixture: fixture.into(),
            correctness,
            jit_status,
            iterations,
            wall_time_seconds: None,
            total_time_seconds: None,
            compile_time_seconds: None,
            execution_time_seconds: None,
            instructions: None,
            counters: JitCounterSnapshot::new(),
            vm_counters: BTreeMap::new(),
            known_gaps: Vec::new(),
        }
    }

    /// Adds a stable JIT counter snapshot.
    #[must_use]
    pub fn with_counters(mut self, counters: JitCounterSnapshot) -> Self {
        self.counters = counters;
        self
    }

    /// Adds separated wall-clock timing fields in seconds.
    #[must_use]
    pub fn with_timings_seconds(
        mut self,
        total_time_seconds: f64,
        compile_time_seconds: f64,
        execution_time_seconds: f64,
    ) -> Self {
        self.wall_time_seconds = Some(total_time_seconds);
        self.total_time_seconds = Some(total_time_seconds);
        self.compile_time_seconds = Some(compile_time_seconds);
        self.execution_time_seconds = Some(execution_time_seconds);
        self
    }

    /// Adds an extra VM counter.
    #[must_use]
    pub fn with_vm_counter(mut self, name: impl Into<String>, value: u64) -> Self {
        self.vm_counters.insert(name.into(), value);
        self
    }

    /// Adds a known-gap identifier.
    #[must_use]
    pub fn with_known_gap(mut self, gap: impl Into<String>) -> Self {
        self.known_gaps.push(gap.into());
        self
    }
}

/// Complete Cranelift big-win JIT report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CraneliftJitReport {
    /// Report schema version.
    pub schema_version: u32,
    /// Run identifier.
    pub run_id: PerfRunId,
    /// Shared environment metadata.
    pub environment: PerfEnvironment,
    /// Big-win rows.
    pub rows: Vec<CraneliftJitReportRow>,
}

impl CraneliftJitReport {
    /// Creates an empty Cranelift JIT report.
    #[must_use]
    pub fn new(run_id: PerfRunId, environment: PerfEnvironment) -> Self {
        Self {
            schema_version: CRANELIFT_JIT_REPORT_SCHEMA_VERSION,
            run_id,
            environment,
            rows: Vec::new(),
        }
    }

    /// Adds a row.
    #[must_use]
    pub fn with_row(mut self, row: CraneliftJitReportRow) -> Self {
        self.rows.push(row);
        self
    }

    /// Serializes this report to normalized pretty JSON with a trailing newline.
    pub fn to_stable_json(&self) -> serde_json::Result<String> {
        let mut json = serde_json::to_string_pretty(self)?;
        json.push('\n');
        Ok(json)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CraneliftJitReport, CraneliftJitReportRow, JitCorrectnessStatus, JitCounterSnapshot,
        JitExecutionStatus, PerfEnvironment, PerfMeasurement, PerfMetric, PerfReport, PerfRunId,
        PerfScenario, PhaseTimingReport,
    };

    #[test]
    fn phase_timing_report_serializes_stable_pretty_json() {
        let mut report = PhaseTimingReport::new("run", "fixtures/runtime/valid/hello.php");
        report.total_internal_ms = 1.25;
        report.phases.insert("execute_ms".to_string(), 0.75);
        report.counts.insert("source_bytes".to_string(), 42);
        report
            .flags
            .insert("bytecode_cache".to_string(), "off".to_string());

        let json = report.to_stable_json().expect("serialize report");

        assert!(json.ends_with('\n'));
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"execute_ms\": 0.75"));
        assert!(json.contains("\"source_bytes\": 42"));
    }

    #[test]
    fn perf_report_json_is_stable_and_sorted() {
        let environment = PerfEnvironment::new("phrust-0.0.0", "aarch64-apple-darwin")
            .with_git_commit("abc1234")
            .with_opt_flags(["--opt-level=0", "--quickening=off"])
            .with_feature_flag("jit-cranelift", false)
            .with_feature_flag("bytecode-cache", true)
            .with_extra("TZ", "UTC")
            .with_extra("LC_ALL", "C");
        let scenario = PerfScenario::new("performance.echo-loop", "Echo loop", "vm")
            .with_fixture("tests/fixtures/performance/perf_smoke/echo_loop.php");
        let measurement = PerfMeasurement::new(scenario, 10)
            .with_wall_time_ms(12.5)
            .with_metric(PerfMetric::new("dispatches", "count", 42.0, true))
            .with_vm_counter("op_echo", 10)
            .with_vm_counter("op_load_const", 32);
        let report = PerfReport::new(PerfRunId::new("performance-test-run"), environment)
            .with_measurement(measurement);

        let json = report.to_stable_json().expect("serialize report");

        assert_eq!(
            json,
            r#"{
  "schema_version": 1,
  "run_id": "performance-test-run",
  "environment": {
    "engine_version": "phrust-0.0.0",
    "git_commit": "abc1234",
    "rust_target_triple": "aarch64-apple-darwin",
    "opt_flags": [
      "--opt-level=0",
      "--quickening=off"
    ],
    "feature_flags": {
      "bytecode-cache": true,
      "jit-cranelift": false
    },
    "extra": {
      "LC_ALL": "C",
      "TZ": "UTC"
    }
  },
  "measurements": [
    {
      "scenario": {
        "id": "performance.echo-loop",
        "name": "Echo loop",
        "group": "vm",
        "fixture": "tests/fixtures/performance/perf_smoke/echo_loop.php"
      },
      "iterations": 10,
      "metrics": [
        {
          "name": "dispatches",
          "unit": "count",
          "value": 42.0,
          "lower_is_better": true
        }
      ],
      "wall_time_ms": 12.5,
      "vm_counters": {
        "op_echo": 10,
        "op_load_const": 32
      }
    }
  ]
}
"#
        );
    }

    #[test]
    fn optional_fields_are_omitted_when_empty() {
        let report = PerfReport::new(
            PerfRunId::new("minimal"),
            PerfEnvironment::new("phrust-0.0.0", "unknown-target"),
        )
        .with_measurement(PerfMeasurement::new(
            PerfScenario::new("performance.minimal", "Minimal", "smoke"),
            1,
        ));

        let json = report.to_stable_json().expect("serialize report");

        assert!(!json.contains("git_commit"));
        assert!(!json.contains("wall_time_ms"));
        assert!(!json.contains("vm_counters"));
        assert!(!json.contains("extra"));
    }

    #[test]
    fn cranelift_jit_report_json_is_stable_and_sorted() {
        let environment = PerfEnvironment::new("phrust-0.0.0", "aarch64-apple-darwin")
            .with_git_commit("abc1234")
            .with_opt_flags(["--jit=cranelift", "--jit-backend=cranelift"])
            .with_feature_flag("jit-cranelift", true);
        let counters = JitCounterSnapshot {
            compile_attempts: 1,
            compiled_regions: 1,
            executed_regions: 1,
            bailouts: 0,
            code_bytes: 64,
            compile_time_nanos: 50_000,
            side_exits: 0,
            guard_failures: 0,
            blacklisted_regions: 0,
            tiering_cold_functions: 0,
            tiering_hot_functions: 1,
            tiering_eager_functions: 0,
            tiering_blacklist_rejections: 0,
            tiering_budget_rejections: 0,
            helper_calls: 2,
            fast_path_hits: 2,
            packed_fetch_fast_hits: 1,
            packed_fetch_bounds_exits: 0,
            packed_fetch_layout_exits: 0,
            packed_foreach_sum_fast_hits: 0,
            packed_foreach_sum_layout_exits: 0,
            packed_foreach_sum_overflow_exits: 0,
            known_call_fast_hits: 0,
            known_call_guard_exits: 0,
            known_call_slow_calls: 0,
            direct_call_hits: 0,
            direct_call_fallbacks: 0,
            property_load_fast_hits: 0,
            property_load_guard_exits: 0,
            property_load_layout_exits: 0,
            property_load_uninitialized_exits: 0,
            property_load_slow_calls: 0,
            string_concat_fast_path_hits: 0,
            string_concat_fast_path_misses: 0,
            overflow_exits: 0,
            slow_path_calls: 0,
            compile_cache_hits: 0,
            compile_cache_misses: 1,
            compile_cache_invalidations: 0,
        };
        let row = CraneliftJitReportRow::new(
            "integer_arithmetic_leaf",
            "tests/fixtures/performance/cranelift/int_leaf.php",
            JitCorrectnessStatus::Pass,
            JitExecutionStatus::Executed,
            1,
        )
        .with_counters(counters)
        .with_timings_seconds(0.00125, 0.00005, 0.00120)
        .with_vm_counter("jit_compile_attempts", 1)
        .with_vm_counter("jit_executed", 1);
        let report =
            CraneliftJitReport::new(PerfRunId::new("performance-cranelift-test"), environment)
                .with_row(row);

        let json = report.to_stable_json().expect("serialize report");

        assert_eq!(
            json,
            r#"{
  "schema_version": 1,
  "run_id": "performance-cranelift-test",
  "environment": {
    "engine_version": "phrust-0.0.0",
    "git_commit": "abc1234",
    "rust_target_triple": "aarch64-apple-darwin",
    "opt_flags": [
      "--jit=cranelift",
      "--jit-backend=cranelift"
    ],
    "feature_flags": {
      "jit-cranelift": true
    }
  },
  "rows": [
    {
      "target": "integer_arithmetic_leaf",
      "fixture": "tests/fixtures/performance/cranelift/int_leaf.php",
      "correctness": "pass",
      "jit_status": "executed",
      "iterations": 1,
      "wall_time_seconds": 0.00125,
      "total_time_seconds": 0.00125,
      "compile_time_seconds": 0.00005,
      "execution_time_seconds": 0.0012,
      "counters": {
        "compile_attempts": 1,
        "compiled_regions": 1,
        "executed_regions": 1,
        "bailouts": 0,
        "code_bytes": 64,
        "compile_time_nanos": 50000,
        "side_exits": 0,
        "guard_failures": 0,
        "blacklisted_regions": 0,
        "tiering_cold_functions": 0,
        "tiering_hot_functions": 1,
        "tiering_eager_functions": 0,
        "tiering_blacklist_rejections": 0,
        "tiering_budget_rejections": 0,
        "helper_calls": 2,
        "fast_path_hits": 2,
        "packed_fetch_fast_hits": 1,
        "packed_fetch_bounds_exits": 0,
        "packed_fetch_layout_exits": 0,
        "packed_foreach_sum_fast_hits": 0,
        "packed_foreach_sum_layout_exits": 0,
        "packed_foreach_sum_overflow_exits": 0,
        "known_call_fast_hits": 0,
        "known_call_guard_exits": 0,
        "known_call_slow_calls": 0,
        "direct_call_hits": 0,
        "direct_call_fallbacks": 0,
        "property_load_fast_hits": 0,
        "property_load_guard_exits": 0,
        "property_load_layout_exits": 0,
        "property_load_uninitialized_exits": 0,
        "property_load_slow_calls": 0,
        "string_concat_fast_path_hits": 0,
        "string_concat_fast_path_misses": 0,
        "overflow_exits": 0,
        "slow_path_calls": 0,
        "compile_cache_hits": 0,
        "compile_cache_misses": 1,
        "compile_cache_invalidations": 0
      },
      "vm_counters": {
        "jit_compile_attempts": 1,
        "jit_executed": 1
      }
    }
  ]
}
"#
        );
    }
}
