//! VM slot binding metadata for region IR.

use std::collections::BTreeMap;

use super::{NodeId, OptimizerRegionGraph, RegionValueType, VmSlotId};

/// Abstract VM slot kind used by future spill/deopt metadata.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum VmSlotKind {
    Local,
    Register,
    Temporary,
    ReturnValue,
    CallArg,
    ForeachIterator,
    ForeachKey,
    ForeachValue,
    ExceptionState,
    OutputBufferState,
}

impl VmSlotKind {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Register => "register",
            Self::Temporary => "temporary",
            Self::ReturnValue => "return_value",
            Self::CallArg => "call_arg",
            Self::ForeachIterator => "foreach_iterator",
            Self::ForeachKey => "foreach_key",
            Self::ForeachValue => "foreach_value",
            Self::ExceptionState => "exception_state",
            Self::OutputBufferState => "output_buffer_state",
        }
    }
}

/// Semantic flags that make a slot invalid as an optimized spill target.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VmSlotSemanticFlags {
    pub by_ref_alias: bool,
    pub escaped_reference: bool,
    pub shared_cow: bool,
    pub destructor_sensitive: bool,
    pub generator_or_fiber_state: bool,
    pub try_finally_state: bool,
    pub uninitialized_typed_property: bool,
    pub unknown_dynamic_state: bool,
}

impl VmSlotSemanticFlags {
    /// Returns true when the slot can be used as a spill target.
    #[must_use]
    pub const fn is_bindable(self) -> bool {
        !self.by_ref_alias
            && !self.escaped_reference
            && !self.shared_cow
            && !self.destructor_sensitive
            && !self.generator_or_fiber_state
            && !self.try_finally_state
            && !self.uninitialized_typed_property
            && !self.unknown_dynamic_state
    }

    fn rejection_reasons(self) -> Vec<&'static str> {
        let mut reasons = Vec::new();
        if self.by_ref_alias {
            reasons.push("by_ref_alias");
        }
        if self.escaped_reference {
            reasons.push("escaped_reference");
        }
        if self.shared_cow {
            reasons.push("shared_cow");
        }
        if self.destructor_sensitive {
            reasons.push("destructor_sensitive");
        }
        if self.generator_or_fiber_state {
            reasons.push("generator_or_fiber_state");
        }
        if self.try_finally_state {
            reasons.push("try_finally_state");
        }
        if self.uninitialized_typed_property {
            reasons.push("uninitialized_typed_property");
        }
        if self.unknown_dynamic_state {
            reasons.push("unknown_dynamic_state");
        }
        reasons
    }
}

/// One abstract VM slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmSlotDescriptor {
    /// Stable slot id in this map.
    pub id: VmSlotId,
    /// Slot storage kind.
    pub kind: VmSlotKind,
    /// Zero-based index inside the storage kind.
    pub index: u32,
    /// Expected value class, when known.
    pub value_type: Option<RegionValueType>,
    /// Whether the slot is initialized at the binding point, when known.
    pub initialized: Option<bool>,
    /// PHP semantic safety flags.
    pub flags: VmSlotSemanticFlags,
}

/// Node live range in region-node table coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionLiveRange {
    /// Inclusive start node index.
    pub start: NodeId,
    /// Inclusive end node index.
    pub end: NodeId,
}

impl RegionLiveRange {
    /// Creates a live range.
    #[must_use]
    pub const fn new(start: NodeId, end: NodeId) -> Self {
        Self { start, end }
    }

    fn overlaps(self, other: Self) -> bool {
        self.start.raw() <= other.end.raw() && other.start.raw() <= self.end.raw()
    }
}

/// One preferred binding from a region node/value to an abstract VM slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmSlotBinding {
    /// Region node/value being bound.
    pub node: NodeId,
    /// Preferred VM slot.
    pub slot: VmSlotId,
    /// Value class expected for this binding.
    pub value_type: RegionValueType,
    /// Live range occupied by this binding.
    pub live_range: RegionLiveRange,
    /// Allows live-range overlap for explicit alias-aware future cases.
    pub allow_overlap: bool,
}

/// Bind-map metadata for one region graph.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BindMap {
    /// Abstract slot table.
    pub slots: Vec<VmSlotDescriptor>,
    /// Preferred node/value bindings.
    pub bindings: Vec<VmSlotBinding>,
}

impl BindMap {
    /// Adds a slot descriptor.
    pub fn add_slot(
        &mut self,
        kind: VmSlotKind,
        index: u32,
        value_type: Option<RegionValueType>,
        initialized: Option<bool>,
        flags: VmSlotSemanticFlags,
    ) -> VmSlotId {
        let id = VmSlotId::new(self.slots.len() as u32);
        self.slots.push(VmSlotDescriptor {
            id,
            kind,
            index,
            value_type,
            initialized,
            flags,
        });
        id
    }

    /// Adds a node-to-slot binding.
    pub fn add_binding(
        &mut self,
        node: NodeId,
        slot: VmSlotId,
        value_type: RegionValueType,
        live_range: RegionLiveRange,
    ) {
        self.bindings.push(VmSlotBinding {
            node,
            slot,
            value_type,
            live_range,
            allow_overlap: false,
        });
    }

    /// Adds a node-to-slot binding that explicitly allows overlap.
    pub fn add_binding_allow_overlap(
        &mut self,
        node: NodeId,
        slot: VmSlotId,
        value_type: RegionValueType,
        live_range: RegionLiveRange,
    ) {
        self.bindings.push(VmSlotBinding {
            node,
            slot,
            value_type,
            live_range,
            allow_overlap: true,
        });
    }
}

/// Bind-map validation error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BindMapError {
    /// Machine-readable reason.
    pub code: &'static str,
    /// Human-readable detail.
    pub detail: String,
}

impl BindMapError {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

/// Summary report for VM slot binding metadata.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BindMapReport {
    /// Number of abstract VM slots.
    pub vm_slot_count: u64,
    /// Number of bindings accepted as bindable.
    pub bindable_values: u64,
    /// Rejection counts keyed by stable reason.
    pub rejected_bindings_by_reason: BTreeMap<&'static str, u64>,
    /// Number of non-overlapping bindings that could reuse a slot.
    pub slot_reuse_candidates: u64,
    /// Validation errors.
    pub errors: Vec<BindMapError>,
}

impl BindMapReport {
    fn reject(&mut self, code: &'static str, detail: impl Into<String>) {
        *self.rejected_bindings_by_reason.entry(code).or_default() += 1;
        self.errors.push(BindMapError::new(code, detail));
    }
}

/// Validates VM slot binding metadata against a region graph.
#[must_use]
pub fn validate_bind_map(graph: &OptimizerRegionGraph) -> BindMapReport {
    let mut report = BindMapReport {
        vm_slot_count: graph.bind_map().slots.len() as u64,
        ..BindMapReport::default()
    };

    for slot in &graph.bind_map().slots {
        if slot.id.index() >= graph.bind_map().slots.len() {
            report.reject(
                "invalid_slot_id",
                format!("slot id v{} is outside slot table", slot.id.raw()),
            );
        }
    }

    for binding in &graph.bind_map().bindings {
        let Some(node) = graph.node(binding.node) else {
            report.reject(
                "invalid_binding_node",
                format!("binding references missing node n{}", binding.node.raw()),
            );
            continue;
        };
        let Some(slot) = graph.bind_map().slots.get(binding.slot.index()) else {
            report.reject(
                "invalid_binding_slot",
                format!("binding references missing slot v{}", binding.slot.raw()),
            );
            continue;
        };

        if node.value_type != binding.value_type {
            report.reject(
                "binding_node_type_mismatch",
                format!(
                    "binding for n{} says {} but node has {}",
                    binding.node.raw(),
                    binding.value_type.as_str(),
                    node.value_type.as_str()
                ),
            );
        }

        if let Some(slot_type) = slot.value_type
            && slot_type != binding.value_type
        {
            report.reject(
                "binding_slot_type_mismatch",
                format!(
                    "binding for n{} says {} but slot v{} expects {}",
                    binding.node.raw(),
                    binding.value_type.as_str(),
                    binding.slot.raw(),
                    slot_type.as_str()
                ),
            );
        }

        for reason in slot.flags.rejection_reasons() {
            report.reject(
                reason,
                format!("slot v{} is not bindable: {}", binding.slot.raw(), reason),
            );
        }

        if binding.live_range.start.raw() > binding.live_range.end.raw() {
            report.reject(
                "invalid_live_range",
                format!(
                    "binding for n{} has inverted live range n{}..n{}",
                    binding.node.raw(),
                    binding.live_range.start.raw(),
                    binding.live_range.end.raw()
                ),
            );
        }
    }

    for (left_index, left) in graph.bind_map().bindings.iter().enumerate() {
        for right in graph.bind_map().bindings.iter().skip(left_index + 1) {
            if left.slot == right.slot
                && left.live_range.overlaps(right.live_range)
                && !left.allow_overlap
                && !right.allow_overlap
            {
                report.reject(
                    "overlapping_live_range",
                    format!(
                        "slot v{} has overlapping bindings n{} and n{}",
                        left.slot.raw(),
                        left.node.raw(),
                        right.node.raw()
                    ),
                );
            } else if left.slot == right.slot && !left.live_range.overlaps(right.live_range) {
                report.slot_reuse_candidates += 1;
            }
        }
    }

    let rejected_count = report.errors.len() as u64;
    report.bindable_values = graph
        .bind_map()
        .bindings
        .len()
        .saturating_sub(rejected_count as usize) as u64;
    report
}

#[cfg(test)]
mod tests {
    use super::{BindMap, RegionLiveRange, VmSlotKind, VmSlotSemanticFlags, validate_bind_map};
    use crate::region_ir::{
        RegionBuilder, RegionId, RegionValueType, VmSlotId, build_minimal_scalar_region,
    };

    #[test]
    fn bind_map_accepts_simple_local_and_register_binding() {
        let mut graph = build_minimal_scalar_region();
        let local = graph.bind_map_mut().add_slot(
            VmSlotKind::Local,
            0,
            Some(RegionValueType::I64),
            Some(true),
            VmSlotSemanticFlags::default(),
        );
        let register = graph.bind_map_mut().add_slot(
            VmSlotKind::Register,
            0,
            Some(RegionValueType::I64),
            Some(true),
            VmSlotSemanticFlags::default(),
        );
        graph.bind_map_mut().add_binding(
            crate::region_ir::NodeId::new(1),
            local,
            RegionValueType::I64,
            RegionLiveRange::new(
                crate::region_ir::NodeId::new(1),
                crate::region_ir::NodeId::new(3),
            ),
        );
        graph.bind_map_mut().add_binding(
            crate::region_ir::NodeId::new(3),
            register,
            RegionValueType::I64,
            RegionLiveRange::new(
                crate::region_ir::NodeId::new(3),
                crate::region_ir::NodeId::new(7),
            ),
        );

        let report = validate_bind_map(&graph);
        assert!(report.errors.is_empty());
        assert_eq!(report.vm_slot_count, 2);
        assert_eq!(report.bindable_values, 2);
    }

    #[test]
    fn bind_map_accepts_call_arg_binding() {
        let mut builder = RegionBuilder::new(RegionId::new(36), "call-arg-bind");
        let node = builder.param_i64(VmSlotId::new(0));
        let mut graph = builder.finish();
        let call_arg = graph.bind_map_mut().add_slot(
            VmSlotKind::CallArg,
            0,
            Some(RegionValueType::I64),
            Some(true),
            VmSlotSemanticFlags::default(),
        );
        graph.bind_map_mut().add_binding(
            node,
            call_arg,
            RegionValueType::I64,
            RegionLiveRange::new(node, node),
        );

        assert!(validate_bind_map(&graph).errors.is_empty());
    }

    #[test]
    fn bind_map_rejects_reference_binding() {
        let report = report_for_single_flag(VmSlotSemanticFlags {
            by_ref_alias: true,
            ..VmSlotSemanticFlags::default()
        });

        assert!(
            report
                .rejected_bindings_by_reason
                .contains_key("by_ref_alias")
        );
    }

    #[test]
    fn bind_map_rejects_cow_sensitive_binding() {
        let report = report_for_single_flag(VmSlotSemanticFlags {
            shared_cow: true,
            ..VmSlotSemanticFlags::default()
        });

        assert!(
            report
                .rejected_bindings_by_reason
                .contains_key("shared_cow")
        );
    }

    #[test]
    fn bind_map_rejects_generator_or_fiber_state_binding() {
        let report = report_for_single_flag(VmSlotSemanticFlags {
            generator_or_fiber_state: true,
            ..VmSlotSemanticFlags::default()
        });

        assert!(
            report
                .rejected_bindings_by_reason
                .contains_key("generator_or_fiber_state")
        );
    }

    #[test]
    fn bind_map_rejects_overlapping_live_ranges() {
        let mut graph = build_minimal_scalar_region();
        let slot = graph.bind_map_mut().add_slot(
            VmSlotKind::Temporary,
            0,
            Some(RegionValueType::I64),
            Some(true),
            VmSlotSemanticFlags::default(),
        );
        graph.bind_map_mut().add_binding(
            crate::region_ir::NodeId::new(1),
            slot,
            RegionValueType::I64,
            RegionLiveRange::new(
                crate::region_ir::NodeId::new(1),
                crate::region_ir::NodeId::new(4),
            ),
        );
        graph.bind_map_mut().add_binding(
            crate::region_ir::NodeId::new(3),
            slot,
            RegionValueType::I64,
            RegionLiveRange::new(
                crate::region_ir::NodeId::new(3),
                crate::region_ir::NodeId::new(7),
            ),
        );

        let report = validate_bind_map(&graph);
        assert!(
            report
                .rejected_bindings_by_reason
                .contains_key("overlapping_live_range")
        );
    }

    fn report_for_single_flag(flags: VmSlotSemanticFlags) -> super::BindMapReport {
        let mut graph = build_minimal_scalar_region();
        let slot = graph.bind_map_mut().add_slot(
            VmSlotKind::Local,
            0,
            Some(RegionValueType::I64),
            Some(true),
            flags,
        );
        graph.bind_map_mut().add_binding(
            crate::region_ir::NodeId::new(1),
            slot,
            RegionValueType::I64,
            RegionLiveRange::new(
                crate::region_ir::NodeId::new(1),
                crate::region_ir::NodeId::new(2),
            ),
        );
        validate_bind_map(&graph)
    }

    #[test]
    fn bind_map_default_is_empty() {
        assert_eq!(BindMap::default().slots.len(), 0);
    }
}
