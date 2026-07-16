//! Region IR optimization analyses and executable transformations.

pub mod cfg;
pub mod dominators;
pub mod executable;
pub mod gcm;
pub mod loops;
pub mod report;
pub mod sccp;

use crate::region_ir::{OptimizerRegionGraph, dump_region_graph};

pub use cfg::{RegionCfg, build_cfg};
pub use dominators::{DominatorTree, compute_dominators};
pub use executable::{ExecutableOptReport, optimize_executable_region};
pub use gcm::{GcmReport, run_gcm};
pub use loops::{LoopInfo, RegionLoop, detect_loops};
pub use report::{RegionOptReport, RegionScheduleDecision};
pub use sccp::{SccpReport, SccpValue, run_sccp};

/// Diagnostic optimization analysis result for the compact report graph.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionOptResult {
    /// SCCP result.
    pub sccp: SccpReport,
    /// Dominator tree.
    pub dominators: DominatorTree,
    /// Loop nesting data.
    pub loops: LoopInfo,
    /// GCM placement result.
    pub gcm: GcmReport,
}

/// Runs the diagnostic SCCP/GCM analysis used by stable reports. Production
/// lowering applies the corresponding transformations through
/// [`optimize_executable_region`].
#[must_use]
pub fn analyze_region_graph(graph: &OptimizerRegionGraph) -> RegionOptResult {
    let sccp = run_sccp(graph);
    let dominators = compute_dominators(&sccp.cfg);
    let loops = detect_loops(&sccp.cfg, &dominators);
    let gcm = run_gcm(graph, &sccp.cfg, &dominators, &loops);

    RegionOptResult {
        sccp,
        dominators,
        loops,
        gcm,
    }
}

/// Stable before/after dump for optimization reports and tests.
#[must_use]
pub fn dump_region_optimization(graph: &OptimizerRegionGraph, result: &RegionOptResult) -> String {
    let mut out = String::new();
    out.push_str("before:\n");
    out.push_str(&dump_region_graph(graph));
    out.push_str("after:\n");

    out.push_str("sccp-values:\n");
    for (index, value) in result.sccp.values.iter().enumerate() {
        out.push_str("  n");
        out.push_str(&index.to_string());
        out.push_str(" = ");
        out.push_str(&value.label());
        out.push('\n');
    }

    out.push_str("executable-control-edges:\n");
    for (from, to) in &result.sccp.executable_control_edges {
        out.push_str("  n");
        out.push_str(&from.raw().to_string());
        out.push_str(" -> n");
        out.push_str(&to.raw().to_string());
        out.push('\n');
    }

    out.push_str("unreachable-control:\n");
    for node in &result.sccp.unreachable_control_nodes {
        out.push_str("  n");
        out.push_str(&node.raw().to_string());
        out.push('\n');
    }

    out.push_str("loops:\n");
    for natural_loop in &result.loops.loops {
        out.push_str("  header=n");
        out.push_str(&natural_loop.header.raw().to_string());
        out.push_str(" backedge=n");
        out.push_str(&natural_loop.backedge.raw().to_string());
        out.push_str(" body=[");
        for (index, node) in natural_loop.body.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push('n');
            out.push_str(&node.raw().to_string());
        }
        out.push_str("]\n");
    }

    out.push_str("gcm-decisions:\n");
    for decision in &result.gcm.decisions {
        out.push_str("  n");
        out.push_str(&decision.node.raw().to_string());
        out.push(' ');
        out.push_str(decision.label);
        if let Some(anchor) = decision.anchor {
            out.push_str(" anchor=n");
            out.push_str(&anchor.raw().to_string());
        }
        out.push_str(" reason=");
        out.push_str(decision.reason);
        out.push('\n');
    }

    let counters = &result.gcm.counters;
    out.push_str("report:\n");
    out.push_str(&format!(
        "  nodes_considered={}\n",
        counters.nodes_considered
    ));
    out.push_str(&format!(
        "  nodes_scheduled_early={}\n",
        counters.nodes_scheduled_early
    ));
    out.push_str(&format!(
        "  nodes_scheduled_late={}\n",
        counters.nodes_scheduled_late
    ));
    out.push_str(&format!(
        "  nodes_kept_pinned={}\n",
        counters.nodes_kept_pinned
    ));
    out.push_str(&format!(
        "  nodes_rejected_by_effects={}\n",
        counters.nodes_rejected_by_effects
    ));
    out.push_str(&format!("  loops_detected={}\n", counters.loops_detected));

    out
}

#[cfg(test)]
mod tests {
    use crate::region_ir::{
        NodeId, OptimizerRegionGraph, RegionBuilder, RegionConst, RegionEffects, RegionId,
        RegionNode, RegionNodeKind, RegionPlacement, RegionValueType, SnapshotEntry, VmSlotId,
    };

    use super::{analyze_region_graph, dump_region_optimization};

    #[test]
    fn opt_dump_marks_constant_branch_and_dead_edge() {
        let mut graph = OptimizerRegionGraph::new(RegionId::new(330), "constant-branch");
        let start = control_node(&mut graph, RegionNodeKind::Start, None);
        let constant = graph.add_constant(RegionConst::Bool(true));
        let condition = graph.add_node(RegionNode::new(
            RegionNodeKind::Const(constant),
            Vec::new(),
            None,
            RegionValueType::Bool,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        ));
        let branch = graph.add_node(RegionNode::new(
            RegionNodeKind::If,
            vec![condition],
            Some(start),
            RegionValueType::Control,
            RegionPlacement::ControlOnly,
            RegionEffects::PURE,
        ));
        let if_true = control_node(&mut graph, RegionNodeKind::IfTrue, Some(branch));
        let if_false = control_node(&mut graph, RegionNodeKind::IfFalse, Some(branch));
        control_node(&mut graph, RegionNodeKind::Return, Some(if_true));
        control_node(&mut graph, RegionNodeKind::Return, Some(if_false));

        let result = analyze_region_graph(&graph);
        let dump = dump_region_optimization(&graph, &result);

        assert!(dump.contains("before:\nregion r330 constant-branch\n"));
        assert!(dump.contains("  n1 = const bool true\n"));
        assert!(dump.contains("  n2 -> n3\n"));
        assert!(!dump.contains("  n2 -> n4\n"));
        assert!(dump.contains("unreachable-control:\n  n4\n"));
    }

    #[test]
    fn gcm_keeps_loop_invariant_add_at_shallow_anchor() {
        let mut graph = OptimizerRegionGraph::new(RegionId::new(331), "loop-invariant");
        let start = control_node(&mut graph, RegionNodeKind::Start, None);
        let loop_header = graph.add_node(RegionNode::new(
            RegionNodeKind::LoopBegin,
            Vec::new(),
            Some(NodeId::new(6)),
            RegionValueType::Control,
            RegionPlacement::ControlOnly,
            RegionEffects::PURE,
        ));
        let one = const_i64(&mut graph, 1);
        let two = const_i64(&mut graph, 2);
        let add = graph.add_node(RegionNode::new(
            RegionNodeKind::Add,
            vec![one, two],
            None,
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        ));
        let _entry_edge = control_node(&mut graph, RegionNodeKind::Begin, Some(start));
        let loop_end = control_node(&mut graph, RegionNodeKind::LoopEnd, Some(loop_header));
        let _use_in_loop = graph.add_node(RegionNode::new(
            RegionNodeKind::Call,
            vec![add],
            Some(loop_end),
            RegionValueType::MixedValue,
            RegionPlacement::Pinned,
            RegionEffects {
                may_call: true,
                ..RegionEffects::PURE
            },
        ));

        let result = analyze_region_graph(&graph);
        let dump = dump_region_optimization(&graph, &result);

        assert!(dump.contains("loops_detected=1"));
        assert!(dump.contains(&format!(
            "  n{} late anchor=n0 reason=pure-scalar",
            add.raw()
        )));
    }

    #[test]
    fn gcm_pins_effectful_guard_snapshot_and_memory_nodes() {
        let mut builder = RegionBuilder::new(RegionId::new(332), "pinned-semantics");
        let start = builder.start();
        let condition = builder.const_bool(true);
        let snapshot = builder.add_snapshot(vec![SnapshotEntry {
            slot: VmSlotId::new(0),
            value_type: RegionValueType::I64,
        }]);
        builder.emit_guard(snapshot, start, condition);
        let mut graph = builder.finish();
        graph.add_node(RegionNode::new(
            RegionNodeKind::Call,
            Vec::new(),
            Some(start),
            RegionValueType::MixedValue,
            RegionPlacement::Pinned,
            RegionEffects {
                may_call: true,
                may_throw: true,
                ..RegionEffects::PURE
            },
        ));
        graph.add_node(RegionNode::new(
            RegionNodeKind::Load,
            Vec::new(),
            Some(start),
            RegionValueType::Memory,
            RegionPlacement::Pinned,
            RegionEffects {
                reads_memory: true,
                ..RegionEffects::PURE
            },
        ));
        graph.add_node(RegionNode::new(
            RegionNodeKind::Snapshot(snapshot),
            Vec::new(),
            Some(start),
            RegionValueType::MixedValue,
            RegionPlacement::Pinned,
            RegionEffects::MAY_DEOPT,
        ));

        let result = analyze_region_graph(&graph);
        let dump = dump_region_optimization(&graph, &result);

        assert!(dump.contains("n2 pinned anchor=n0 reason=not-floating"));
        assert!(dump.contains("n3 pinned anchor=n0 reason=not-floating"));
        assert!(dump.contains("n4 pinned anchor=n0 reason=not-floating"));
        assert!(dump.contains("n5 pinned anchor=n0 reason=not-floating"));
    }

    fn control_node(
        graph: &mut OptimizerRegionGraph,
        kind: RegionNodeKind,
        control: Option<NodeId>,
    ) -> NodeId {
        graph.add_node(RegionNode::new(
            kind,
            Vec::new(),
            control,
            RegionValueType::Control,
            RegionPlacement::ControlOnly,
            RegionEffects::PURE,
        ))
    }

    fn const_i64(graph: &mut OptimizerRegionGraph, value: i64) -> NodeId {
        let constant = graph.add_constant(RegionConst::I64(value));
        graph.add_node(RegionNode::new(
            RegionNodeKind::Const(constant),
            Vec::new(),
            None,
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        ))
    }
}
