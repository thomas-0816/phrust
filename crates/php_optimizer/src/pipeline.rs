use super::*;

/// Ordered pass pipeline.
pub struct PassPipeline {
    passes: Vec<Box<dyn OptimizerPass>>,
}

impl PassPipeline {
    /// Creates a pipeline from explicit passes.
    #[must_use]
    pub fn new(passes: Vec<Box<dyn OptimizerPass>>) -> Self {
        Self { passes }
    }

    /// conservative no-op optimizer pipeline.
    #[must_use]
    pub fn noop() -> Self {
        Self::new(vec![
            Box::new(NoopPass::new("perf_pre_verify_noop", PassPhase::PreVerify)),
            Box::new(NoopPass::new(
                "perf_post_verify_noop",
                PassPhase::PostVerify,
            )),
        ])
    }

    /// Current performance optimizer pipeline.
    #[must_use]
    pub fn performance() -> Self {
        Self::new(vec![
            Box::new(NoopPass::new("perf_pre_verify_noop", PassPhase::PreVerify)),
            Box::new(ConstantFoldingPass),
            Box::new(LiteralCompactionPass),
            Box::new(CopyPropagationPass),
            Box::new(PeepholeSimplify),
            Box::new(BranchSimplify),
            Box::new(NoopPass::new(
                "perf_post_verify_noop",
                PassPhase::PostVerify,
            )),
        ])
    }

    /// Runs passes around verifier boundaries.
    pub fn run(
        &self,
        unit: &mut IrUnit,
        context: &PassContext,
    ) -> Result<OptimizationReport, PassError> {
        let mut reports = Vec::new();
        // Validate the incoming artifact once. Every changed pass is then
        // verified exactly once before its scoped transaction commits.
        verify_unit(unit).map_err(|errors| PassError::Verification {
            phase: PassPhase::PreVerify,
            errors,
        })?;
        self.run_phase(PassPhase::PreVerify, unit, context, &mut reports)?;
        self.run_phase(PassPhase::PostVerify, unit, context, &mut reports)?;
        Ok(OptimizationReport {
            level: context.level(),
            passes: reports,
        })
    }

    fn run_phase(
        &self,
        phase: PassPhase,
        unit: &mut IrUnit,
        context: &PassContext,
        reports: &mut Vec<PassReport>,
    ) -> Result<(), PassError> {
        for pass in self.passes.iter().filter(|pass| pass.phase() == phase) {
            if context.level() < pass.min_level() || !context.is_pass_enabled(pass.name()) {
                reports.push(PassReport::skipped(pass.name(), pass.phase()));
                continue;
            }
            let mut transaction = PassTransaction::new(unit);
            let mut report = match pass.run(&mut transaction, context) {
                Ok(report) => report,
                Err(error) => {
                    transaction.rollback();
                    return Err(error);
                }
            };
            report.changed = transaction.changed();
            transaction.add_instrumentation(&mut report);
            if report.changed {
                report.stats.insert("verifier_calls", 1);
                match verify_unit(transaction.unit()) {
                    Ok(()) => {
                        transaction.commit();
                        reports.push(report);
                    }
                    Err(errors) => {
                        report.changed = false;
                        report.rolled_back = true;
                        report.stats.insert("verifier_errors", errors.len() as u64);
                        transaction.rollback();
                        reports.push(report);
                    }
                }
            } else {
                report.stats.insert("verifier_calls", 0);
                transaction.commit();
                reports.push(report);
            }
        }
        Ok(())
    }
}
