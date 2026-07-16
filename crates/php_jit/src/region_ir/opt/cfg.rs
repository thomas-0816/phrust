//! Conservative CFG reconstruction for region IR.

use std::collections::{BTreeMap, BTreeSet};

use crate::region_ir::{NodeId, OptimizerRegionGraph, RegionNodeKind, RegionValueType};

/// A node-level CFG view reconstructed from explicit control dependencies.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RegionCfg {
    /// Control nodes in table order.
    pub control_nodes: Vec<NodeId>,
    /// Successor control edges.
    pub successors: BTreeMap<NodeId, Vec<NodeId>>,
    /// Predecessor control edges.
    pub predecessors: BTreeMap<NodeId, Vec<NodeId>>,
    /// Entry control node, when present.
    pub entry: Option<NodeId>,
}

impl RegionCfg {
    /// Returns successors of one node.
    #[must_use]
    pub fn successors(&self, node: NodeId) -> &[NodeId] {
        self.successors.get(&node).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Returns predecessors of one node.
    #[must_use]
    pub fn predecessors(&self, node: NodeId) -> &[NodeId] {
        self.predecessors
            .get(&node)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

/// Reconstructs a conservative CFG over control-typed nodes.
#[must_use]
pub fn build_cfg(graph: &OptimizerRegionGraph) -> RegionCfg {
    let control_nodes: Vec<NodeId> = graph
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| node.value_type == RegionValueType::Control || node.kind.is_control())
        .map(|(index, _)| NodeId::new(index as u32))
        .collect();
    let control_set: BTreeSet<NodeId> = control_nodes.iter().copied().collect();
    let entry = control_nodes.iter().copied().find(|node| {
        graph.node(*node).is_some_and(|node| {
            matches!(node.kind, RegionNodeKind::Start | RegionNodeKind::Entry(_))
        })
    });

    let mut successors: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
    let mut predecessors: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();

    for node in &control_nodes {
        successors.entry(*node).or_default();
        predecessors.entry(*node).or_default();
    }

    for (index, node) in graph.nodes().iter().enumerate() {
        let id = NodeId::new(index as u32);
        if !control_set.contains(&id) {
            continue;
        }
        if let Some(control) = node.control
            && control_set.contains(&control)
        {
            successors.entry(control).or_default().push(id);
            predecessors.entry(id).or_default().push(control);
        }
    }

    for values in successors.values_mut() {
        values.sort_unstable();
        values.dedup();
    }
    for values in predecessors.values_mut() {
        values.sort_unstable();
        values.dedup();
    }

    RegionCfg {
        control_nodes,
        successors,
        predecessors,
        entry,
    }
}
