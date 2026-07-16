//! Rule-selection metadata for region IR.

use std::collections::BTreeSet;

use php_ir::rule_selection::{RuleKind, RuleSelection, RuleSelectionReport};

use super::{NodeId, OptimizerRegionGraph, RegionNodeKind, RegionValueType};

/// Selects report-only rules for the safe scalar region IR subset.
#[must_use]
pub fn select_region_rules(graph: &OptimizerRegionGraph) -> RuleSelectionReport {
    let mut report = RuleSelectionReport::default();
    let mut fused_children = BTreeSet::new();

    for (index, node) in graph.nodes().iter().enumerate() {
        let id = NodeId::new(index as u32);
        if fused_children.contains(&id) {
            continue;
        }

        if let RegionNodeKind::Compare(_) = node.kind
            && let Some(branch) = compare_branch_use(graph, id)
        {
            let rule_id = report.next_id();
            report.push(RuleSelection::selected(
                rule_id,
                RuleKind::CompareAndBranch,
                vec![id.raw(), branch.raw()],
            ));
            let child = report.next_id();
            report.push(RuleSelection::fused_child(
                child,
                rule_id,
                vec![branch.raw()],
            ));
            fused_children.insert(branch);
            continue;
        }

        let rule_id = report.next_id();
        match select_region_node_rule(graph, id) {
            Some(kind) => report.push(RuleSelection::selected(rule_id, kind, vec![id.raw()])),
            None => report.push(RuleSelection::skipped(
                rule_id,
                vec![id.raw()],
                "unsupported_or_effectful_region_shape",
            )),
        }
    }

    report
}

/// Dumps region rule metadata in the shared stable format.
#[must_use]
pub fn dump_region_rule_selection(graph: &OptimizerRegionGraph) -> String {
    select_region_rules(graph).dump_text()
}

fn select_region_node_rule(graph: &OptimizerRegionGraph, node: NodeId) -> Option<RuleKind> {
    let node = graph.node(node)?;
    if !node.effects.is_pure() {
        return None;
    }
    match node.kind {
        RegionNodeKind::Param { .. } => Some(RuleKind::Param),
        RegionNodeKind::Const(_) => Some(RuleKind::Const),
        RegionNodeKind::Copy => Some(RuleKind::Move),
        RegionNodeKind::Add | RegionNodeKind::Sub | RegionNodeKind::Mul => {
            Some(RuleKind::BinaryInt)
        }
        RegionNodeKind::AndBool | RegionNodeKind::OrBool => Some(RuleKind::BinaryInt),
        RegionNodeKind::Compare(_) => Some(RuleKind::Compare),
        RegionNodeKind::Return => Some(RuleKind::ReturnValue),
        RegionNodeKind::Start
        | RegionNodeKind::End
        | RegionNodeKind::Begin
        | RegionNodeKind::Merge
        | RegionNodeKind::LoopBegin
        | RegionNodeKind::LoopEnd
        | RegionNodeKind::If
        | RegionNodeKind::IfTrue
        | RegionNodeKind::IfFalse
        | RegionNodeKind::Entry(_)
        | RegionNodeKind::Exit(_) => Some(RuleKind::NoRule),
        _ if matches!(
            node.value_type,
            RegionValueType::Bool | RegionValueType::I64
        ) =>
        {
            Some(RuleKind::NoRule)
        }
        _ => None,
    }
}

fn compare_branch_use(graph: &OptimizerRegionGraph, compare: NodeId) -> Option<NodeId> {
    graph.def_use().uses(compare).iter().copied().find(|user| {
        graph.node(*user).is_some_and(|node| {
            matches!(node.kind, RegionNodeKind::If) && node.inputs.contains(&compare)
        })
    })
}

#[cfg(test)]
mod tests {
    use php_ir::rule_selection::RuleKind;

    use super::select_region_rules;
    use crate::region_ir::{
        OptimizerRegionGraph, RegionBuilder, RegionEffects, RegionId, RegionNode, RegionNodeKind,
        RegionPlacement, RegionValueType, VmSlotId,
    };

    #[test]
    fn region_rule_selection_covers_scalar_arithmetic_and_return() {
        let mut builder = RegionBuilder::new(RegionId::new(350), "region-rules");
        let start = builder.start();
        let left = builder.param_i64(VmSlotId::new(0));
        let right = builder.const_i64(1);
        let add = builder.emit_add_i64(left, right);
        builder.emit_return(start, add);
        let graph = builder.finish();

        let report = select_region_rules(&graph);

        assert!(
            report
                .rule_selection_by_kind
                .contains_key(&RuleKind::BinaryInt.as_str())
        );
        assert!(
            report
                .rule_selection_by_kind
                .contains_key(&RuleKind::ReturnValue.as_str())
        );
    }

    #[test]
    fn region_rule_selection_reports_compare_and_branch_fusion() {
        let mut builder = RegionBuilder::new(RegionId::new(351), "region-compare-branch");
        let start = builder.start();
        let left = builder.param_i64(VmSlotId::new(0));
        let right = builder.const_i64(1);
        let compare = builder.emit_compare_i64(crate::region_ir::RegionCompareOp::Lt, left, right);
        builder.emit_if(start, compare);
        let graph = builder.finish();

        let dump = select_region_rules(&graph).dump_text();

        assert!(dump.contains("compare_and_branch=1"));
        assert!(dump.contains("fused_into_r"));
        assert!(dump.contains("sources=["));
    }

    #[test]
    fn region_rule_selection_skips_effectful_shapes() {
        let mut graph = OptimizerRegionGraph::new(RegionId::new(352), "region-effectful");
        let start = graph.add_node(RegionNode::new(
            RegionNodeKind::Start,
            Vec::new(),
            None,
            RegionValueType::Control,
            RegionPlacement::ControlOnly,
            RegionEffects::PURE,
        ));
        graph.add_node(RegionNode::new(
            RegionNodeKind::Call,
            Vec::new(),
            Some(start),
            RegionValueType::MixedValue,
            RegionPlacement::Pinned,
            RegionEffects {
                may_call: true,
                ..RegionEffects::PURE
            },
        ));

        let dump = select_region_rules(&graph).dump_text();

        assert!(dump.contains("skipped=1"));
        assert!(dump.contains("unsupported_or_effectful_region_shape"));
    }
}
