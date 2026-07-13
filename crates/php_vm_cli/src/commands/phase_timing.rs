use php_perf::PhaseTimingReport;
use std::time::Instant;

#[derive(Debug)]
pub(super) struct PhaseTimingCollector {
    report: PhaseTimingReport,
    started: Instant,
}

impl PhaseTimingCollector {
    pub(super) fn new(command: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            report: PhaseTimingReport::new(command, path),
            started: Instant::now(),
        }
    }

    pub(super) fn record_phase(&mut self, name: impl Into<String>, started: Instant) {
        self.report
            .phases
            .insert(name.into(), started.elapsed().as_secs_f64() * 1000.0);
    }

    pub(super) fn add_phase_ms(&mut self, name: impl Into<String>, elapsed_ms: f64) {
        self.report.phases.insert(name.into(), elapsed_ms);
    }

    pub(super) fn count(&mut self, name: impl Into<String>, value: u64) {
        self.report.counts.insert(name.into(), value);
    }

    pub(super) fn flag(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.report.flags.insert(name.into(), value.into());
    }

    pub(super) fn finish(mut self) -> PhaseTimingReport {
        self.report.total_internal_ms = self.started.elapsed().as_secs_f64() * 1000.0;
        self.report
    }
}
