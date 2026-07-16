//! Conservative Global Code Motion placement prototype.

use crate::region_ir::{
    NodeId, OptimizerRegionGraph, RegionEffects, RegionNodeKind, RegionPlacement, RegionValueType,
};

use super::{
    cfg::RegionCfg,
    dominators::DominatorTree,
    loops::LoopInfo,
    report::{RegionOptReport, RegionScheduleDecision},
};

/// GCM placement result.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GcmReport {
    /// Aggregate counters.
    pub counters: RegionOptReport,
    /// Stable per-node decisions.
    pub decisions: Vec<RegionScheduleDecision>,
}

/// Runs a conservative GCM classifier over the region graph.
#[must_use]
pub fn run_gcm(
    graph: &OptimizerRegionGraph,
    cfg: &RegionCfg,
    dominators: &DominatorTree,
    loops: &LoopInfo,
) -> GcmReport {
    let mut counters = RegionOptReport {
        loops_detected: loops.loops.len() as u64,
        ..RegionOptReport::default()
    };
    let mut decisions = Vec::new();

    for (index, node) in graph.nodes().iter().enumerate() {
        let id = NodeId::new(index as u32);
        if node.placement != RegionPlacement::Floating
            || node.value_type == RegionValueType::Control
        {
            counters.nodes_kept_pinned += 1;
            decisions.push(RegionScheduleDecision::new(
                id,
                "pinned",
                node.control,
                "not-floating",
            ));
            continue;
        }

        if !is_gcm_movable(&node.kind, node.value_type, node.effects) {
            counters.nodes_rejected_by_effects += 1;
            decisions.push(RegionScheduleDecision::new(
                id,
                "rejected",
                node.control,
                "php-visible-effects",
            ));
            continue;
        }

        counters.nodes_considered += 1;
        let early = schedule_early_anchor(graph, cfg, dominators, id);
        counters.nodes_scheduled_early += 1;

        let mut late = schedule_late_anchor(graph, cfg, dominators, id).or(early);
        if let (Some(early_anchor), Some(late_anchor)) = (early, late)
            && loops.depth(late_anchor) > loops.depth(early_anchor)
        {
            late = Some(early_anchor);
        }

        counters.nodes_scheduled_late += 1;
        decisions.push(RegionScheduleDecision::new(id, "late", late, "pure-scalar"));
    }

    GcmReport {
        counters,
        decisions,
    }
}

fn is_gcm_movable(
    kind: &RegionNodeKind,
    value_type: RegionValueType,
    effects: RegionEffects,
) -> bool {
    effects.is_pure()
        && matches!(value_type, RegionValueType::Bool | RegionValueType::I64)
        && matches!(
            kind,
            RegionNodeKind::Param { .. }
                | RegionNodeKind::Const(_)
                | RegionNodeKind::Copy
                | RegionNodeKind::Phi
                | RegionNodeKind::Add
                | RegionNodeKind::Sub
                | RegionNodeKind::Mul
                | RegionNodeKind::AndBool
                | RegionNodeKind::OrBool
                | RegionNodeKind::Compare(_)
        )
}

fn schedule_early_anchor(
    graph: &OptimizerRegionGraph,
    cfg: &RegionCfg,
    dominators: &DominatorTree,
    id: NodeId,
) -> Option<NodeId> {
    let node = graph.node(id)?;
    let anchors: Vec<NodeId> = node
        .inputs
        .iter()
        .filter_map(|input| control_anchor(graph, cfg, *input))
        .collect();
    dominators
        .common_dominator(&anchors)
        .or_else(|| anchors.first().copied())
        .or(cfg.entry)
}

fn schedule_late_anchor(
    graph: &OptimizerRegionGraph,
    cfg: &RegionCfg,
    dominators: &DominatorTree,
    id: NodeId,
) -> Option<NodeId> {
    let anchors: Vec<NodeId> = graph
        .def_use()
        .uses(id)
        .iter()
        .filter_map(|use_node| control_anchor(graph, cfg, *use_node))
        .collect();
    dominators
        .common_dominator(&anchors)
        .or_else(|| anchors.first().copied())
}

fn control_anchor(graph: &OptimizerRegionGraph, cfg: &RegionCfg, id: NodeId) -> Option<NodeId> {
    let node = graph.node(id)?;
    if node.value_type == RegionValueType::Control || node.kind.is_control() {
        Some(id)
    } else {
        node.control.or(cfg.entry)
    }
}
