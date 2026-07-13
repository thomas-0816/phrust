use super::*;

fn run_test_pass(
    pass: &dyn OptimizerPass,
    unit: &mut IrUnit,
    context: &PassContext,
) -> Result<PassReport, PassError> {
    let mut transaction = PassTransaction::new(unit);
    let mut report = OptimizerPass::run(pass, &mut transaction, context)?;
    report.changed = transaction.changed();
    transaction.add_instrumentation(&mut report);
    if report.changed {
        verify_unit(transaction.unit()).map_err(|errors| PassError::Verification {
            phase: pass.phase(),
            errors,
        })?;
        report.stats.insert("verifier_calls", 1);
    } else {
        report.stats.insert("verifier_calls", 0);
    }
    transaction.commit();
    Ok(report)
}

#[cfg(test)]
macro_rules! test_pass_run_adapter {
    ($($pass:ty),+ $(,)?) => {
        $(impl $pass {
            pub(super) fn run(
                &self,
                unit: &mut IrUnit,
                context: &PassContext,
            ) -> Result<PassReport, PassError> {
                run_test_pass(self, unit, context)
            }
        })+
    };
}

test_pass_run_adapter!(
    NoopPass,
    ConstantFoldingPass,
    LiteralCompactionPass,
    CopyPropagationPass,
    PeepholeSimplify,
    BranchSimplify,
);
