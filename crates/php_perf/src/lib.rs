//! performance performance measurement data model.
//!
//! This crate only defines stable report types. It does not run benchmarks,
//! collect VM counters, or choose performance budgets.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Stable schema version for performance performance reports.
pub const PERF_REPORT_SCHEMA_VERSION: u32 = 1;

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

#[cfg(test)]
mod tests {
    use super::{
        PerfEnvironment, PerfMeasurement, PerfMetric, PerfReport, PerfRunId, PerfScenario,
        PhaseTimingReport,
    };

    #[test]
    fn phase_timing_report_serializes_stable_pretty_json() {
        let mut report = PhaseTimingReport::new("run", "fixtures/runtime/valid/hello.php");
        report.total_internal_ms = 1.25;
        report.phases.insert("execute_ms".to_string(), 0.75);
        report.counts.insert("source_bytes".to_string(), 42);
        report
            .flags
            .insert("native_cache".to_string(), "off".to_string());

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
            .with_opt_flags(["--engine-preset=baseline"])
            .with_feature_flag("cranelift", true)
            .with_feature_flag("native-cache", true)
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
      "--engine-preset=baseline"
    ],
    "feature_flags": {
      "cranelift": true,
      "native-cache": true
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
}
