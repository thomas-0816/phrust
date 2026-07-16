//! Metadata-only OSR entry handling for region IR.

use std::collections::BTreeMap;

use super::{
    EntryId, NodeId, OptimizerRegionGraph, RegionNode, RegionNodeKind, RegionPlacement,
    RegionValueType, VmSlotId,
};

/// Region OSR entry metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionOsrEntry {
    /// Region `Entry` table ID.
    pub entry: EntryId,
    /// Node carrying the `Entry` operation.
    pub node: NodeId,
    /// Optional fake/control predecessor that models baseline-native loop entry.
    pub fake_control_predecessor: Option<NodeId>,
    /// VM slots that must be live at entry.
    pub live_slots: Vec<VmSlotId>,
    /// Unsupported reasons attached to this entry.
    pub unsupported_reasons: Vec<String>,
}

/// Region OSR counters.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RegionOsrReport {
    pub osr_entry_candidates: u64,
    pub osr_entry_representable: u64,
    pub osr_entry_rejected_by_reason: BTreeMap<String, u64>,
    pub osr_live_slots: u64,
}

/// Region OSR entry map.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RegionOsrEntryMap {
    pub entries: Vec<RegionOsrEntry>,
    pub report: RegionOsrReport,
}

/// Scheduling policy for motion across OSR entries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionOsrMotionPolicy {
    pub movable_across_entry: bool,
    pub reason: &'static str,
}

/// Builds metadata for region `Entry` nodes.
#[must_use]
pub fn select_region_osr_entries(graph: &OptimizerRegionGraph) -> RegionOsrEntryMap {
    let mut map = RegionOsrEntryMap::default();
    for (index, node) in graph.nodes().iter().enumerate() {
        let RegionNodeKind::Entry(entry) = node.kind else {
            continue;
        };
        let live_slots = collect_entry_live_slots(graph, node);
        let unsupported_reasons = entry_unsupported_reasons(graph, node);
        let entry = RegionOsrEntry {
            entry,
            node: NodeId::new(index as u32),
            fake_control_predecessor: node.control,
            live_slots,
            unsupported_reasons,
        };
        map.report.osr_entry_candidates += 1;
        map.report.osr_live_slots += entry.live_slots.len() as u64;
        if entry.unsupported_reasons.is_empty() {
            map.report.osr_entry_representable += 1;
        } else {
            for reason in &entry.unsupported_reasons {
                *map.report
                    .osr_entry_rejected_by_reason
                    .entry(reason.clone())
                    .or_default() += 1;
            }
        }
        map.entries.push(entry);
    }
    map
}

/// Returns whether a region node may move across an OSR entry.
#[must_use]
pub fn region_osr_motion_policy(node: &RegionNode) -> RegionOsrMotionPolicy {
    if !node.effects.is_pure() {
        return RegionOsrMotionPolicy {
            movable_across_entry: false,
            reason: "effectful_or_deopt_node_pinned",
        };
    }
    if node.placement != RegionPlacement::Floating {
        return RegionOsrMotionPolicy {
            movable_across_entry: false,
            reason: "control_or_pinned_node",
        };
    }
    match node.kind {
        RegionNodeKind::Add
        | RegionNodeKind::Sub
        | RegionNodeKind::Mul
        | RegionNodeKind::AndBool
        | RegionNodeKind::OrBool
        | RegionNodeKind::Compare(_)
        | RegionNodeKind::Copy
        | RegionNodeKind::Const(_)
        | RegionNodeKind::Param { .. } => RegionOsrMotionPolicy {
            movable_across_entry: true,
            reason: "pure_scalar_dependencies_required_at_entry",
        },
        _ => RegionOsrMotionPolicy {
            movable_across_entry: false,
            reason: "unsupported_region_shape",
        },
    }
}

fn collect_entry_live_slots(graph: &OptimizerRegionGraph, entry: &RegionNode) -> Vec<VmSlotId> {
    let mut slots = Vec::new();
    for input in &entry.inputs {
        if let Some(RegionNode {
            kind: RegionNodeKind::Param { slot },
            ..
        }) = graph.node(*input)
        {
            slots.push(*slot);
        }
    }
    slots.sort();
    slots.dedup();
    slots
}

fn entry_unsupported_reasons(graph: &OptimizerRegionGraph, entry: &RegionNode) -> Vec<String> {
    let mut reasons = Vec::new();
    for input in &entry.inputs {
        let Some(input_node) = graph.node(*input) else {
            reasons.push("missing_entry_dependency".to_string());
            continue;
        };
        let policy = region_osr_motion_policy(input_node);
        if !policy.movable_across_entry {
            reasons.push(policy.reason.to_string());
        }
        if matches!(
            input_node.value_type,
            RegionValueType::ArrayHandle
                | RegionValueType::ObjectHandle
                | RegionValueType::MixedValue
        ) {
            reasons.push("reference_or_cow_state".to_string());
        }
    }
    reasons.sort();
    reasons.dedup();
    reasons
}

#[cfg(test)]
mod tests {
    use super::{region_osr_motion_policy, select_region_osr_entries};
    use crate::region_ir::{
        EntryId, OptimizerRegionGraph, RegionEffects, RegionId, RegionNode, RegionNodeKind,
        RegionPlacement, RegionValueType, VmSlotId,
    };

    #[test]
    fn region_osr_entry_records_fake_control_and_live_slots() {
        let mut graph = OptimizerRegionGraph::new(RegionId::new(380), "region-osr");
        let loop_header = graph.add_node(RegionNode::new(
            RegionNodeKind::LoopBegin,
            Vec::new(),
            None,
            RegionValueType::Control,
            RegionPlacement::ControlOnly,
            RegionEffects::PURE,
        ));
        let param = graph.add_node(RegionNode::new(
            RegionNodeKind::Param {
                slot: VmSlotId::new(0),
            },
            Vec::new(),
            Some(loop_header),
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        ));
        graph.add_node(RegionNode::new(
            RegionNodeKind::Entry(EntryId::new(0)),
            vec![param],
            Some(loop_header),
            RegionValueType::Control,
            RegionPlacement::ControlOnly,
            RegionEffects::PURE,
        ));

        let map = select_region_osr_entries(&graph);

        assert_eq!(map.report.osr_entry_candidates, 1);
        assert_eq!(map.report.osr_entry_representable, 1);
        assert_eq!(map.report.osr_live_slots, 1);
        assert_eq!(map.entries[0].fake_control_predecessor, Some(loop_header));
        assert_eq!(map.entries[0].live_slots, vec![VmSlotId::new(0)]);
    }

    #[test]
    fn region_osr_motion_policy_pins_effectful_nodes() {
        let call = RegionNode::new(
            RegionNodeKind::Call,
            Vec::new(),
            None,
            RegionValueType::MixedValue,
            RegionPlacement::Pinned,
            RegionEffects {
                may_call: true,
                ..RegionEffects::PURE
            },
        );

        let policy = region_osr_motion_policy(&call);

        assert!(!policy.movable_across_entry);
        assert_eq!(policy.reason, "effectful_or_deopt_node_pinned");
    }

    #[test]
    fn region_osr_motion_policy_allows_pure_scalar_nodes() {
        let add = RegionNode::new(
            RegionNodeKind::Add,
            Vec::new(),
            None,
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        );

        let policy = region_osr_motion_policy(&add);

        assert!(policy.movable_across_entry);
        assert_eq!(policy.reason, "pure_scalar_dependencies_required_at_entry");
    }
}
