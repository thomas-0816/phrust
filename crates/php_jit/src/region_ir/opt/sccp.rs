//! Minimal SCCP lattice and worklist reporting for region IR.

use crate::region_ir::{
    NodeId, OptimizerRegionGraph, RegionCompareOp, RegionConst, RegionNodeKind,
};

use super::cfg::{RegionCfg, build_cfg};

/// Minimal SCCP lattice.
#[derive(Clone, Debug, PartialEq)]
pub enum SccpValue {
    /// Top / unknown.
    Top,
    /// Known constant.
    Const(RegionConst),
    /// Bottom / not constant.
    Bottom,
}

impl SccpValue {
    /// Stable report label.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Top => "top".to_string(),
            Self::Const(RegionConst::I64(value)) => format!("const i64 {value}"),
            Self::Const(RegionConst::Bool(value)) => format!("const bool {value}"),
            Self::Const(RegionConst::F64(value)) => format!("const f64 {value}"),
            Self::Const(RegionConst::StringHandle(value)) => format!("const string-handle {value}"),
            Self::Bottom => "bottom".to_string(),
        }
    }
}

/// SCCP analysis report.
#[derive(Clone, Debug, PartialEq)]
pub struct SccpReport {
    /// Lattice value for every node in table order.
    pub values: Vec<SccpValue>,
    /// Executable control edges after constant branch pruning.
    pub executable_control_edges: Vec<(NodeId, NodeId)>,
    /// SSA dependency worklist visited by the prototype.
    pub ssa_worklist: Vec<NodeId>,
    /// Control nodes marked unreachable by constant branch pruning.
    pub unreachable_control_nodes: Vec<NodeId>,
    /// Reconstructed CFG used by the report.
    pub cfg: RegionCfg,
}

/// Runs the minimal scalar SCCP prototype.
#[must_use]
pub fn run_sccp(graph: &OptimizerRegionGraph) -> SccpReport {
    let cfg = build_cfg(graph);
    let mut values = vec![SccpValue::Top; graph.nodes().len()];
    let mut changed = true;

    while changed {
        changed = false;
        for (index, node) in graph.nodes().iter().enumerate() {
            let id = NodeId::new(index as u32);
            let next = evaluate_node(graph, &values, id, &node.kind);
            if values[index] != next {
                values[index] = next;
                changed = true;
            }
        }
    }

    let (executable_control_edges, unreachable_control_nodes) =
        executable_edges(graph, &cfg, &values);
    let mut ssa_worklist: Vec<NodeId> = (0..graph.nodes().len())
        .map(|index| NodeId::new(index as u32))
        .filter(|node| !graph.def_use().uses(*node).is_empty())
        .collect();
    ssa_worklist.sort_unstable();
    ssa_worklist.dedup();

    SccpReport {
        values,
        executable_control_edges,
        ssa_worklist,
        unreachable_control_nodes,
        cfg,
    }
}

fn evaluate_node(
    graph: &OptimizerRegionGraph,
    values: &[SccpValue],
    id: NodeId,
    kind: &RegionNodeKind,
) -> SccpValue {
    let Some(node) = graph.node(id) else {
        return SccpValue::Bottom;
    };

    match kind {
        RegionNodeKind::Const(constant) => graph
            .constant(*constant)
            .cloned()
            .map(SccpValue::Const)
            .unwrap_or(SccpValue::Bottom),
        RegionNodeKind::Copy => node
            .inputs
            .first()
            .and_then(|input| values.get(input.index()))
            .cloned()
            .unwrap_or(SccpValue::Top),
        RegionNodeKind::Phi => merge_phi(
            node.inputs
                .iter()
                .filter_map(|input| values.get(input.index())),
        ),
        RegionNodeKind::Add | RegionNodeKind::Sub | RegionNodeKind::Mul => {
            evaluate_i64_binary(kind, node.inputs.as_slice(), values)
        }
        RegionNodeKind::AndBool | RegionNodeKind::OrBool => {
            evaluate_bool_binary(kind, node.inputs.as_slice(), values)
        }
        RegionNodeKind::Compare(op) => evaluate_compare(*op, node.inputs.as_slice(), values),
        _ if node.effects.is_pure() => SccpValue::Top,
        _ => SccpValue::Bottom,
    }
}

fn merge_phi<'a>(inputs: impl Iterator<Item = &'a SccpValue>) -> SccpValue {
    let mut seen = None;
    for value in inputs {
        match value {
            SccpValue::Top => return SccpValue::Top,
            SccpValue::Bottom => return SccpValue::Bottom,
            SccpValue::Const(constant) => {
                if let Some(seen_constant) = &seen {
                    if seen_constant != constant {
                        return SccpValue::Bottom;
                    }
                } else {
                    seen = Some(constant.clone());
                }
            }
        }
    }
    seen.map(SccpValue::Const).unwrap_or(SccpValue::Top)
}

fn evaluate_i64_binary(
    kind: &RegionNodeKind,
    inputs: &[NodeId],
    values: &[SccpValue],
) -> SccpValue {
    let Some((left, right)) = const_i64_pair(inputs, values) else {
        return pair_fallback(inputs, values);
    };
    let value = match kind {
        RegionNodeKind::Add => left.checked_add(right),
        RegionNodeKind::Sub => left.checked_sub(right),
        RegionNodeKind::Mul => left.checked_mul(right),
        _ => None,
    };
    value
        .map(|value| SccpValue::Const(RegionConst::I64(value)))
        .unwrap_or(SccpValue::Bottom)
}

fn evaluate_bool_binary(
    kind: &RegionNodeKind,
    inputs: &[NodeId],
    values: &[SccpValue],
) -> SccpValue {
    let Some((left, right)) = const_bool_pair(inputs, values) else {
        return pair_fallback(inputs, values);
    };
    let value = match kind {
        RegionNodeKind::AndBool => left && right,
        RegionNodeKind::OrBool => left || right,
        _ => return SccpValue::Bottom,
    };
    SccpValue::Const(RegionConst::Bool(value))
}

fn evaluate_compare(op: RegionCompareOp, inputs: &[NodeId], values: &[SccpValue]) -> SccpValue {
    let Some((left, right)) = const_i64_pair(inputs, values) else {
        return pair_fallback(inputs, values);
    };
    SccpValue::Const(RegionConst::Bool(match op {
        RegionCompareOp::Eq => left == right,
        RegionCompareOp::NotEq => left != right,
        RegionCompareOp::Lt => left < right,
        RegionCompareOp::Lte => left <= right,
        RegionCompareOp::Gt => left > right,
        RegionCompareOp::Gte => left >= right,
    }))
}

fn const_i64_pair(inputs: &[NodeId], values: &[SccpValue]) -> Option<(i64, i64)> {
    let [left, right] = inputs else {
        return None;
    };
    match (values.get(left.index())?, values.get(right.index())?) {
        (SccpValue::Const(RegionConst::I64(left)), SccpValue::Const(RegionConst::I64(right))) => {
            Some((*left, *right))
        }
        _ => None,
    }
}

fn const_bool_pair(inputs: &[NodeId], values: &[SccpValue]) -> Option<(bool, bool)> {
    let [left, right] = inputs else {
        return None;
    };
    match (values.get(left.index())?, values.get(right.index())?) {
        (SccpValue::Const(RegionConst::Bool(left)), SccpValue::Const(RegionConst::Bool(right))) => {
            Some((*left, *right))
        }
        _ => None,
    }
}

fn pair_fallback(inputs: &[NodeId], values: &[SccpValue]) -> SccpValue {
    if inputs
        .iter()
        .any(|input| matches!(values.get(input.index()), Some(SccpValue::Bottom)))
    {
        SccpValue::Bottom
    } else {
        SccpValue::Top
    }
}

fn executable_edges(
    graph: &OptimizerRegionGraph,
    cfg: &RegionCfg,
    values: &[SccpValue],
) -> (Vec<(NodeId, NodeId)>, Vec<NodeId>) {
    let mut edges = Vec::new();
    let mut unreachable = Vec::new();

    for from in &cfg.control_nodes {
        let branch = graph
            .node(*from)
            .filter(|node| matches!(node.kind, RegionNodeKind::If));
        let branch_value = branch
            .and_then(|node| node.inputs.first())
            .and_then(|input| values.get(input.index()));

        for to in cfg.successors(*from) {
            let keep = match branch_value {
                Some(SccpValue::Const(RegionConst::Bool(true))) => graph
                    .node(*to)
                    .is_none_or(|node| !matches!(node.kind, RegionNodeKind::IfFalse)),
                Some(SccpValue::Const(RegionConst::Bool(false))) => graph
                    .node(*to)
                    .is_none_or(|node| !matches!(node.kind, RegionNodeKind::IfTrue)),
                _ => true,
            };

            if keep {
                edges.push((*from, *to));
            } else {
                unreachable.push(*to);
            }
        }
    }

    (edges, unreachable)
}
