use php_ir::{LoweringOptions, lower_frontend_result, verify_unit};
use php_optimizer::{OptimizationLevel, PassContext, PassPipeline};
use php_semantics::{FrontendResult, analyze_source};
use php_source::SourceText;

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
    let frontend = analyze_source(source);
    let mut lowering = lower_frontend_result(
        &frontend,
        LoweringOptions {
            source_path: source_path.to_string(),
            source_text: Some(source.to_string()),
            ..LoweringOptions::default()
        },
    );
    if !frontend.has_errors() && lowering.verification.is_ok() {
        verify_unit(&lowering.unit).map_err(|errors| {
            format!(
                "{source_path}: IR verification failed: {} error(s)",
                errors.len()
            )
        })?;
        lowering.verification = verify_unit(&lowering.unit);
    }
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
) -> Result<(), String> {
    if !pipeline.ok() || !optimization_level.runs_pipeline() {
        return Ok(());
    }
    PassPipeline::performance()
        .run(
            &mut pipeline.lowering.unit,
            &PassContext::new(optimization_level),
        )
        .map(|_| ())
        .map_err(|error| format!("{}: optimizer failed: {error}", pipeline.path))
}
