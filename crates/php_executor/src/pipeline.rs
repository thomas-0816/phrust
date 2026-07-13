use php_ir::{LoweringOptions, lower_frontend_result, verify_unit};
use php_optimizer::{OptimizationLevel, PassContext, PassPipeline};
use php_semantics::{FrontendResult, analyze_source};
use php_source::SourceText;
use std::collections::BTreeMap;
use std::time::Instant;

#[derive(Clone, Debug, Default)]
pub struct CompilePhaseTimings {
    pub(crate) phases: BTreeMap<String, f64>,
}

impl CompilePhaseTimings {
    #[must_use]
    pub fn phases(&self) -> &BTreeMap<String, f64> {
        &self.phases
    }

    fn record(&mut self, phase: &'static str, started: Instant) {
        self.phases
            .insert(phase.to_string(), started.elapsed().as_secs_f64() * 1000.0);
    }
}

#[derive(Debug)]
pub(crate) enum CompileTimingCollector {
    Disabled,
    Enabled(CompilePhaseTimings),
}

impl CompileTimingCollector {
    pub(crate) fn disabled() -> Self {
        Self::Disabled
    }

    pub(crate) fn enabled() -> Self {
        Self::Enabled(CompilePhaseTimings::default())
    }

    fn measure<T>(&mut self, phase: &'static str, operation: impl FnOnce() -> T) -> T {
        match self {
            Self::Disabled => operation(),
            Self::Enabled(timings) => {
                let started = Instant::now();
                let result = operation();
                timings.record(phase, started);
                result
            }
        }
    }

    pub(crate) fn finish(self) -> Option<CompilePhaseTimings> {
        match self {
            Self::Disabled => None,
            Self::Enabled(timings) => Some(timings),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Pipeline {
    pub(crate) path: String,
    pub(crate) source: SourceText,
    pub(crate) frontend: FrontendResult,
    pub(crate) lowering: php_ir::LoweringResult,
}

impl Pipeline {
    pub(crate) fn ok(&self) -> bool {
        !self.frontend.has_errors()
            && self.lowering.diagnostics.is_empty()
            && self.lowering.verification.is_ok()
    }
}

pub(crate) fn compile_source(
    source: &str,
    source_path: &str,
    timings: &mut CompileTimingCollector,
) -> Result<Pipeline, String> {
    let frontend = timings.measure("frontend_analyze_ms", || analyze_source(source));
    let lowering = timings.measure("ir_lower_ms", || {
        lower_frontend_result(
            &frontend,
            LoweringOptions {
                source_path: source_path.to_string(),
                source_text: Some(source.to_string()),
                ..LoweringOptions::default()
            },
        )
    });
    Ok(Pipeline {
        path: source_path.to_string(),
        source: SourceText::new(source),
        frontend,
        lowering,
    })
}

pub(crate) fn apply_optimization(
    pipeline: &mut Pipeline,
    optimization_level: OptimizationLevel,
    timings: &mut CompileTimingCollector,
) -> Result<(), String> {
    if !pipeline.ok() || !optimization_level.runs_pipeline() {
        return Ok(());
    }
    timings
        .measure("optimizer_ms", || {
            PassPipeline::performance().run(
                &mut pipeline.lowering.unit,
                &PassContext::new(optimization_level),
            )
        })
        .map_err(|error| format!("{}: optimizer failed: {error}", pipeline.path))?;
    pipeline.lowering.verification =
        timings.measure("ir_verify_ms", || verify_unit(&pipeline.lowering.unit));
    Ok(())
}
