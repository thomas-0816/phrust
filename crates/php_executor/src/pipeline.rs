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

pub(crate) fn compile_source(source: &str, source_path: &str) -> Result<Pipeline, String> {
    compile_source_with_timings(source, source_path).map(|(pipeline, _)| pipeline)
}

pub(crate) fn compile_source_with_timings(
    source: &str,
    source_path: &str,
) -> Result<(Pipeline, CompilePhaseTimings), String> {
    let mut timings = CompilePhaseTimings::default();
    let started = Instant::now();
    let frontend = analyze_source(source);
    timings.record("frontend_analyze_ms", started);
    let started = Instant::now();
    let lowering = lower_frontend_result(
        &frontend,
        LoweringOptions {
            source_path: source_path.to_string(),
            source_text: Some(source.to_string()),
            ..LoweringOptions::default()
        },
    );
    timings.record("ir_lower_ms", started);
    Ok((
        Pipeline {
            path: source_path.to_string(),
            source: SourceText::new(source),
            frontend,
            lowering,
        },
        timings,
    ))
}

pub(crate) fn apply_optimization(
    pipeline: &mut Pipeline,
    optimization_level: OptimizationLevel,
) -> Result<(), String> {
    apply_optimization_with_timings(pipeline, optimization_level).map(|_| ())
}

pub(crate) fn apply_optimization_with_timings(
    pipeline: &mut Pipeline,
    optimization_level: OptimizationLevel,
) -> Result<CompilePhaseTimings, String> {
    let mut timings = CompilePhaseTimings::default();
    if !pipeline.ok() || !optimization_level.runs_pipeline() {
        return Ok(timings);
    }
    let started = Instant::now();
    PassPipeline::performance()
        .run(
            &mut pipeline.lowering.unit,
            &PassContext::new(optimization_level),
        )
        .map_err(|error| format!("{}: optimizer failed: {error}", pipeline.path))?;
    timings.record("optimizer_ms", started);
    let started = Instant::now();
    pipeline.lowering.verification = verify_unit(&pipeline.lowering.unit);
    timings.record("ir_verify_ms", started);
    Ok(timings)
}
