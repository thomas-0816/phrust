use php_optimizer::OptimizationReport;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
pub(super) struct OptimizerReportJson<'a> {
    level: &'a str,
    enabled_pass_count: usize,
    passes: Vec<OptimizerPassJson<'a>>,
}

impl<'a> From<&'a OptimizationReport> for OptimizerReportJson<'a> {
    fn from(report: &'a OptimizationReport) -> Self {
        Self {
            level: report.level.as_str(),
            enabled_pass_count: report.enabled_pass_count(),
            passes: report
                .passes
                .iter()
                .map(|pass| OptimizerPassJson {
                    name: pass.name,
                    phase: pass.phase.as_str(),
                    enabled: pass.enabled,
                    changed: pass.changed,
                    source_spans_preserved: pass.source_spans_preserved,
                    rolled_back: pass.rolled_back,
                    scope: OptimizerPassScopeJson {
                        functions: &pass.scope.functions,
                        blocks: &pass.scope.blocks,
                        constants: pass.scope.constants,
                        metadata: &pass.scope.metadata,
                        source_mappings_may_change: pass.scope.source_mappings_may_change,
                    },
                    stats: &pass.stats,
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct OptimizerPassJson<'a> {
    name: &'a str,
    phase: &'a str,
    enabled: bool,
    changed: bool,
    source_spans_preserved: bool,
    rolled_back: bool,
    scope: OptimizerPassScopeJson<'a>,
    stats: &'a BTreeMap<&'static str, u64>,
}

#[derive(Serialize)]
struct OptimizerPassScopeJson<'a> {
    functions: &'a [usize],
    blocks: &'a [(usize, usize)],
    constants: bool,
    metadata: &'a [&'static str],
    source_mappings_may_change: bool,
}
