//! Region IR builder.

use super::{NodeId, RegionValueType::Control};
use super::{
    OptimizerRegionGraph, RegionCompareOp, RegionConst, RegionEffects, RegionId, RegionNode,
    RegionNodeKind, RegionPlacement, RegionValueType, SnapshotEntry, SnapshotId, VmSlotId,
};

/// Region builder options.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionBuilderOptions {
    /// Maximum number of prior floating nodes inspected by one folding CSE lookup.
    pub fold_cse_limit: usize,
}

impl Default for RegionBuilderOptions {
    fn default() -> Self {
        Self { fold_cse_limit: 64 }
    }
}

/// Convenience builder for compact region graphs.
#[derive(Debug)]
pub struct RegionBuilder {
    graph: OptimizerRegionGraph,
    options: RegionBuilderOptions,
}

impl RegionBuilder {
    /// Creates a builder for one region graph.
    #[must_use]
    pub fn new(region_id: RegionId, name: impl Into<String>) -> Self {
        Self::with_options(region_id, name, RegionBuilderOptions::default())
    }

    /// Creates a builder for one region graph with explicit options.
    #[must_use]
    pub fn with_options(
        region_id: RegionId,
        name: impl Into<String>,
        options: RegionBuilderOptions,
    ) -> Self {
        Self {
            graph: OptimizerRegionGraph::new(region_id, name),
            options,
        }
    }

    /// Adds a `Start` control node.
    pub fn start(&mut self) -> NodeId {
        self.emit_node(
            RegionNodeKind::Start,
            Vec::new(),
            None,
            Control,
            RegionPlacement::ControlOnly,
            RegionEffects::PURE,
        )
    }

    /// Adds an `End` control node.
    pub fn end(&mut self, control: NodeId) -> NodeId {
        self.emit_node(
            RegionNodeKind::End,
            Vec::new(),
            Some(control),
            Control,
            RegionPlacement::ControlOnly,
            RegionEffects::PURE,
        )
    }

    /// Adds an i64 parameter bound to an abstract VM slot.
    pub fn param_i64(&mut self, slot: VmSlotId) -> NodeId {
        self.emit_node(
            RegionNodeKind::Param { slot },
            Vec::new(),
            None,
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        )
    }

    /// Emits an i64 constant without CSE.
    pub fn emit_const_i64(&mut self, value: i64) -> NodeId {
        let constant = self.graph.add_constant(RegionConst::I64(value));
        self.emit_node(
            RegionNodeKind::Const(constant),
            Vec::new(),
            None,
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        )
    }

    /// Adds an i64 constant. This compatibility helper always emits.
    pub fn const_i64(&mut self, value: i64) -> NodeId {
        self.emit_const_i64(value)
    }

    /// Folds an i64 constant through the CSE table.
    pub fn fold_const_i64(&mut self, value: i64) -> NodeId {
        self.record_fold_attempt();
        self.cse_or_emit_const_i64(value)
    }

    fn cse_or_emit_const_i64(&mut self, value: i64) -> NodeId {
        if let Some(existing) = self.find_const_i64(value) {
            self.record_cse_hit();
            return existing;
        }

        self.record_cse_miss();
        let constant = self.graph.add_constant(RegionConst::I64(value));
        self.emit_node(
            RegionNodeKind::Const(constant),
            Vec::new(),
            None,
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        )
    }

    /// Emits a bool constant without CSE.
    pub fn emit_const_bool(&mut self, value: bool) -> NodeId {
        let constant = self.graph.add_constant(RegionConst::Bool(value));
        self.emit_node(
            RegionNodeKind::Const(constant),
            Vec::new(),
            None,
            RegionValueType::Bool,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        )
    }

    /// Adds a bool constant. This compatibility helper always emits.
    pub fn const_bool(&mut self, value: bool) -> NodeId {
        self.emit_const_bool(value)
    }

    /// Folds a bool constant through the CSE table.
    pub fn fold_const_bool(&mut self, value: bool) -> NodeId {
        self.record_fold_attempt();
        self.cse_or_emit_const_bool(value)
    }

    fn cse_or_emit_const_bool(&mut self, value: bool) -> NodeId {
        if let Some(existing) = self.find_const_bool(value) {
            self.record_cse_hit();
            return existing;
        }

        self.record_cse_miss();
        let constant = self.graph.add_constant(RegionConst::Bool(value));
        self.emit_node(
            RegionNodeKind::Const(constant),
            Vec::new(),
            None,
            RegionValueType::Bool,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        )
    }

    /// Emits a copy node without folding.
    pub fn emit_copy(&mut self, value: NodeId) -> NodeId {
        self.emit_node(
            RegionNodeKind::Copy,
            vec![value],
            None,
            self.node_type(value),
            RegionPlacement::Floating,
            RegionEffects::PURE,
        )
    }

    /// Folds `copy(x)` to `x`.
    pub fn fold_copy(&mut self, value: NodeId) -> NodeId {
        self.record_fold_attempt();
        self.record_fold_applied();
        value
    }

    /// Emits a phi node without folding.
    pub fn emit_phi(&mut self, value_type: RegionValueType, inputs: Vec<NodeId>) -> NodeId {
        self.emit_node(
            RegionNodeKind::Phi,
            inputs,
            None,
            value_type,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        )
    }

    /// Folds `phi(x, x, ...)` to `x`; otherwise emits or CSEs the phi.
    pub fn fold_phi(&mut self, value_type: RegionValueType, inputs: Vec<NodeId>) -> NodeId {
        self.record_fold_attempt();
        if let Some(first) = inputs.first().copied()
            && inputs.iter().all(|input| *input == first)
        {
            self.record_fold_applied();
            return first;
        }

        self.fold_or_emit_node(
            RegionNodeKind::Phi,
            inputs,
            None,
            value_type,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        )
    }

    /// Emits an i64 add node without folding.
    pub fn emit_add_i64(&mut self, left: NodeId, right: NodeId) -> NodeId {
        self.emit_node(
            RegionNodeKind::Add,
            vec![left, right],
            None,
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        )
    }

    /// Folds safe i64 add identities/constant pairs and CSEs pure nodes.
    pub fn fold_add_i64(&mut self, left: NodeId, right: NodeId) -> NodeId {
        self.record_fold_attempt();
        if self.const_i64_value(right) == Some(0) {
            self.record_fold_applied();
            return left;
        }
        if self.const_i64_value(left) == Some(0) {
            self.record_fold_applied();
            return right;
        }
        if let (Some(left_value), Some(right_value)) =
            (self.const_i64_value(left), self.const_i64_value(right))
        {
            if let Some(value) = left_value.checked_add(right_value) {
                self.record_fold_applied();
                return self.cse_or_emit_const_i64(value);
            }

            self.record_semantic_skip();
        }

        self.fold_binary_i64(RegionNodeKind::Add, left, right)
    }

    /// Emits an i64 subtract node without folding.
    pub fn emit_sub_i64(&mut self, left: NodeId, right: NodeId) -> NodeId {
        self.emit_node(
            RegionNodeKind::Sub,
            vec![left, right],
            None,
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        )
    }

    /// Folds safe i64 subtract identities/constant pairs and CSEs pure nodes.
    pub fn fold_sub_i64(&mut self, left: NodeId, right: NodeId) -> NodeId {
        self.record_fold_attempt();
        if self.const_i64_value(right) == Some(0) {
            self.record_fold_applied();
            return left;
        }
        if let (Some(left_value), Some(right_value)) =
            (self.const_i64_value(left), self.const_i64_value(right))
        {
            if let Some(value) = left_value.checked_sub(right_value) {
                self.record_fold_applied();
                return self.cse_or_emit_const_i64(value);
            }

            self.record_semantic_skip();
        }

        self.fold_binary_i64(RegionNodeKind::Sub, left, right)
    }

    /// Emits an i64 multiply node without folding.
    pub fn emit_mul_i64(&mut self, left: NodeId, right: NodeId) -> NodeId {
        self.emit_node(
            RegionNodeKind::Mul,
            vec![left, right],
            None,
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        )
    }

    /// Folds safe i64 multiply identities/constant pairs and CSEs pure nodes.
    pub fn fold_mul_i64(&mut self, left: NodeId, right: NodeId) -> NodeId {
        self.record_fold_attempt();
        if self.const_i64_value(right) == Some(1) {
            self.record_fold_applied();
            return left;
        }
        if self.const_i64_value(left) == Some(1) {
            self.record_fold_applied();
            return right;
        }
        if let (Some(left_value), Some(right_value)) =
            (self.const_i64_value(left), self.const_i64_value(right))
        {
            if let Some(value) = left_value.checked_mul(right_value) {
                self.record_fold_applied();
                return self.cse_or_emit_const_i64(value);
            }

            self.record_semantic_skip();
        }

        self.fold_binary_i64(RegionNodeKind::Mul, left, right)
    }

    /// Emits a boolean and node without folding.
    pub fn emit_and_bool(&mut self, left: NodeId, right: NodeId) -> NodeId {
        self.emit_node(
            RegionNodeKind::AndBool,
            vec![left, right],
            None,
            RegionValueType::Bool,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        )
    }

    /// Folds safe boolean and identities/constant pairs and CSEs pure nodes.
    pub fn fold_and_bool(&mut self, left: NodeId, right: NodeId) -> NodeId {
        self.record_fold_attempt();
        if self.const_bool_value(right) == Some(true) {
            self.record_fold_applied();
            return left;
        }
        if self.const_bool_value(left) == Some(true) {
            self.record_fold_applied();
            return right;
        }
        if let (Some(left_value), Some(right_value)) =
            (self.const_bool_value(left), self.const_bool_value(right))
        {
            self.record_fold_applied();
            return self.cse_or_emit_const_bool(left_value && right_value);
        }

        self.fold_binary_bool(RegionNodeKind::AndBool, left, right)
    }

    /// Emits a boolean or node without folding.
    pub fn emit_or_bool(&mut self, left: NodeId, right: NodeId) -> NodeId {
        self.emit_node(
            RegionNodeKind::OrBool,
            vec![left, right],
            None,
            RegionValueType::Bool,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        )
    }

    /// Folds safe boolean or identities/constant pairs and CSEs pure nodes.
    pub fn fold_or_bool(&mut self, left: NodeId, right: NodeId) -> NodeId {
        self.record_fold_attempt();
        if self.const_bool_value(right) == Some(false) {
            self.record_fold_applied();
            return left;
        }
        if self.const_bool_value(left) == Some(false) {
            self.record_fold_applied();
            return right;
        }
        if let (Some(left_value), Some(right_value)) =
            (self.const_bool_value(left), self.const_bool_value(right))
        {
            self.record_fold_applied();
            return self.cse_or_emit_const_bool(left_value || right_value);
        }

        self.fold_binary_bool(RegionNodeKind::OrBool, left, right)
    }

    /// Emits an i64 compare node without folding.
    pub fn emit_compare_i64(&mut self, op: RegionCompareOp, left: NodeId, right: NodeId) -> NodeId {
        self.emit_node(
            RegionNodeKind::Compare(op),
            vec![left, right],
            None,
            RegionValueType::Bool,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        )
    }

    /// Folds constant i64 compares and CSEs pure compare nodes.
    pub fn fold_compare_i64(&mut self, op: RegionCompareOp, left: NodeId, right: NodeId) -> NodeId {
        self.record_fold_attempt();
        if let (Some(left_value), Some(right_value)) =
            (self.const_i64_value(left), self.const_i64_value(right))
        {
            self.record_fold_applied();
            return self.cse_or_emit_const_bool(match op {
                RegionCompareOp::Eq => left_value == right_value,
                RegionCompareOp::NotEq => left_value != right_value,
                RegionCompareOp::Lt => left_value < right_value,
                RegionCompareOp::Lte => left_value <= right_value,
                RegionCompareOp::Gt => left_value > right_value,
                RegionCompareOp::Gte => left_value >= right_value,
            });
        }

        self.fold_or_emit_node(
            RegionNodeKind::Compare(op),
            vec![left, right],
            None,
            RegionValueType::Bool,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        )
    }

    /// Emits a branch node.
    pub fn emit_if(&mut self, control: NodeId, condition: NodeId) -> NodeId {
        self.emit_node(
            RegionNodeKind::If,
            vec![condition],
            Some(control),
            Control,
            RegionPlacement::ControlOnly,
            RegionEffects::PURE,
        )
    }

    /// Emits a return node.
    pub fn emit_return(&mut self, control: NodeId, value: NodeId) -> NodeId {
        self.emit_node(
            RegionNodeKind::Return,
            vec![value],
            Some(control),
            Control,
            RegionPlacement::ControlOnly,
            RegionEffects::PURE,
        )
    }

    /// Adds a metadata-only snapshot.
    pub fn add_snapshot(&mut self, entries: Vec<SnapshotEntry>) -> SnapshotId {
        self.graph.add_snapshot(entries)
    }

    /// Emits a guard tied to a snapshot and a boolean condition.
    pub fn emit_guard(
        &mut self,
        snapshot: SnapshotId,
        control: NodeId,
        condition: NodeId,
    ) -> NodeId {
        self.emit_node(
            RegionNodeKind::Guard { snapshot },
            vec![condition],
            Some(control),
            Control,
            RegionPlacement::Pinned,
            RegionEffects::MAY_DEOPT,
        )
    }

    /// Finishes the graph.
    #[must_use]
    pub fn finish(self) -> OptimizerRegionGraph {
        self.graph
    }

    fn fold_binary_i64(&mut self, kind: RegionNodeKind, left: NodeId, right: NodeId) -> NodeId {
        self.fold_or_emit_node(
            kind,
            vec![left, right],
            None,
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        )
    }

    fn fold_binary_bool(&mut self, kind: RegionNodeKind, left: NodeId, right: NodeId) -> NodeId {
        self.fold_or_emit_node(
            kind,
            vec![left, right],
            None,
            RegionValueType::Bool,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        )
    }

    fn fold_or_emit_node(
        &mut self,
        kind: RegionNodeKind,
        inputs: Vec<NodeId>,
        control: Option<NodeId>,
        value_type: RegionValueType,
        placement: RegionPlacement,
        effects: RegionEffects,
    ) -> NodeId {
        if let Some(existing) =
            self.find_cse(&kind, &inputs, control, value_type, placement, effects)
        {
            self.record_cse_hit();
            return existing;
        }

        if is_cse_eligible(control, value_type, placement, effects) {
            self.record_cse_miss();
        } else {
            self.record_semantic_skip();
        }

        self.emit_node(kind, inputs, control, value_type, placement, effects)
    }

    fn emit_node(
        &mut self,
        kind: RegionNodeKind,
        inputs: Vec<NodeId>,
        control: Option<NodeId>,
        value_type: RegionValueType,
        placement: RegionPlacement,
        effects: RegionEffects,
    ) -> NodeId {
        self.graph.add_node(RegionNode::new(
            kind, inputs, control, value_type, placement, effects,
        ))
    }

    fn find_cse(
        &self,
        kind: &RegionNodeKind,
        inputs: &[NodeId],
        control: Option<NodeId>,
        value_type: RegionValueType,
        placement: RegionPlacement,
        effects: RegionEffects,
    ) -> Option<NodeId> {
        if !is_cse_eligible(control, value_type, placement, effects) {
            return None;
        }

        self.graph
            .nodes()
            .iter()
            .enumerate()
            .rev()
            .take(self.options.fold_cse_limit)
            .find_map(|(index, node)| {
                (node.kind == *kind
                    && node.inputs == inputs
                    && node.control == control
                    && node.value_type == value_type
                    && node.placement == placement
                    && node.effects == effects)
                    .then(|| NodeId::new(index as u32))
            })
    }

    fn node_type(&self, node: NodeId) -> RegionValueType {
        self.graph
            .node(node)
            .map(|node| node.value_type)
            .unwrap_or(RegionValueType::MixedValue)
    }

    fn const_i64_value(&self, node: NodeId) -> Option<i64> {
        let node = self.graph.node(node)?;
        let RegionNodeKind::Const(constant) = node.kind else {
            return None;
        };
        match self.graph.constant(constant)? {
            RegionConst::I64(value) => Some(*value),
            _ => None,
        }
    }

    fn const_bool_value(&self, node: NodeId) -> Option<bool> {
        let node = self.graph.node(node)?;
        let RegionNodeKind::Const(constant) = node.kind else {
            return None;
        };
        match self.graph.constant(constant)? {
            RegionConst::Bool(value) => Some(*value),
            _ => None,
        }
    }

    fn find_const_i64(&self, value: i64) -> Option<NodeId> {
        self.find_const(|constant| matches!(constant, RegionConst::I64(found) if *found == value))
    }

    fn find_const_bool(&self, value: bool) -> Option<NodeId> {
        self.find_const(|constant| matches!(constant, RegionConst::Bool(found) if *found == value))
    }

    fn find_const(&self, matches_constant: impl Fn(&RegionConst) -> bool) -> Option<NodeId> {
        self.graph
            .nodes()
            .iter()
            .enumerate()
            .rev()
            .take(self.options.fold_cse_limit)
            .find_map(|(index, node)| {
                let RegionNodeKind::Const(constant) = node.kind else {
                    return None;
                };
                self.graph
                    .constant(constant)
                    .filter(|constant| matches_constant(constant))
                    .map(|_| NodeId::new(index as u32))
            })
    }

    fn record_fold_attempt(&mut self) {
        self.graph.fold_counters_mut().region_ir_fold_attempts += 1;
    }

    fn record_fold_applied(&mut self) {
        self.graph.fold_counters_mut().region_ir_fold_applied += 1;
    }

    fn record_cse_hit(&mut self) {
        self.graph.fold_counters_mut().region_ir_cse_hits += 1;
    }

    fn record_cse_miss(&mut self) {
        self.graph.fold_counters_mut().region_ir_cse_misses += 1;
    }

    fn record_semantic_skip(&mut self) {
        self.graph
            .fold_counters_mut()
            .region_ir_fold_skipped_by_semantics += 1;
    }
}

fn is_cse_eligible(
    control: Option<NodeId>,
    value_type: RegionValueType,
    placement: RegionPlacement,
    effects: RegionEffects,
) -> bool {
    control.is_none()
        && placement == RegionPlacement::Floating
        && effects.is_pure()
        && matches!(value_type, RegionValueType::Bool | RegionValueType::I64)
}

/// Builds the minimal scalar fixture requested by FPE-31.
#[must_use]
pub fn build_minimal_scalar_region() -> OptimizerRegionGraph {
    let mut builder = RegionBuilder::new(RegionId::new(0), "minimal-scalar");
    let start = builder.start();
    let param = builder.param_i64(VmSlotId::new(0));
    let one = builder.const_i64(1);
    let added = builder.emit_add_i64(param, one);
    let limit = builder.const_i64(1);
    let condition = builder.emit_compare_i64(RegionCompareOp::Lt, added, limit);
    let branch = builder.emit_if(start, condition);
    builder.emit_return(branch, added);
    builder.finish()
}

#[cfg(test)]
mod tests {
    use super::{
        RegionBuilder, RegionBuilderOptions, RegionCompareOp, RegionConst, RegionEffects, RegionId,
        RegionNodeKind, RegionPlacement, RegionValueType, VmSlotId,
    };
    use crate::region_ir::{dump_region_graph, verify_region_graph};

    #[test]
    fn fold_scalar_identities_return_original_values() {
        let mut builder = RegionBuilder::new(RegionId::new(32), "fold-identities");
        let value = builder.param_i64(VmSlotId::new(0));
        let flag = builder.const_bool(false);
        let zero = builder.const_i64(0);
        let one = builder.const_i64(1);
        let true_value = builder.const_bool(true);
        let false_value = builder.const_bool(false);

        assert_eq!(builder.fold_copy(value), value);
        assert_eq!(
            builder.fold_phi(RegionValueType::I64, vec![value, value]),
            value
        );
        assert_eq!(builder.fold_add_i64(value, zero), value);
        assert_eq!(builder.fold_sub_i64(value, zero), value);
        assert_eq!(builder.fold_mul_i64(value, one), value);
        assert_eq!(builder.fold_and_bool(flag, true_value), flag);
        assert_eq!(builder.fold_or_bool(flag, false_value), flag);

        verify_region_graph(&builder.finish()).expect("identity-only graph should verify");
    }

    #[test]
    fn fold_const_pairs_use_checked_scalar_rules() {
        let mut builder = RegionBuilder::new(RegionId::new(33), "fold-const-pairs");
        let two = builder.const_i64(2);
        let three = builder.const_i64(3);
        let folded_add = builder.fold_add_i64(two, three);
        let folded_sub = builder.fold_sub_i64(three, two);
        let folded_mul = builder.fold_mul_i64(two, three);
        let folded_cmp = builder.fold_compare_i64(RegionCompareOp::Lt, two, three);
        let min = builder.const_i64(i64::MIN);
        let minus_one = builder.const_i64(-1);
        let checked_sub = builder.fold_sub_i64(min, minus_one);

        let graph = builder.finish();
        verify_region_graph(&graph).expect("checked constant fold graph should verify");

        assert_eq!(const_i64_at(&graph, folded_add), Some(5));
        assert_eq!(const_i64_at(&graph, folded_sub), Some(1));
        assert_eq!(const_i64_at(&graph, folded_mul), Some(6));
        assert_eq!(const_bool_at(&graph, folded_cmp), Some(true));
        assert_eq!(const_i64_at(&graph, checked_sub), Some(i64::MIN + 1));
    }

    #[test]
    fn fold_i64_overflow_keeps_original_operation() {
        let mut builder = RegionBuilder::new(RegionId::new(36), "fold-overflow");
        let min = builder.const_i64(i64::MIN);
        let one = builder.const_i64(1);
        let overflow = builder.fold_sub_i64(min, one);

        let graph = builder.finish();
        verify_region_graph(&graph).expect("overflow fallback graph should verify");

        assert!(matches!(
            graph.node(overflow).unwrap().kind,
            RegionNodeKind::Sub
        ));
    }

    #[test]
    fn fold_bool_const_pairs_and_cse_budget() {
        let mut builder = RegionBuilder::with_options(
            RegionId::new(34),
            "fold-bool-cse",
            RegionBuilderOptions { fold_cse_limit: 8 },
        );
        let left = builder.param_i64(VmSlotId::new(0));
        let right = builder.param_i64(VmSlotId::new(1));
        let true_value = builder.const_bool(true);
        let false_value = builder.const_bool(false);

        let and_const = builder.fold_and_bool(true_value, false_value);
        let or_const = builder.fold_or_bool(false_value, true_value);
        let add = builder.fold_add_i64(left, right);
        let add_again = builder.fold_add_i64(left, right);

        let graph = builder.finish();
        verify_region_graph(&graph).expect("bool/CSE graph should verify");

        assert_eq!(const_bool_at(&graph, and_const), Some(false));
        assert_eq!(const_bool_at(&graph, or_const), Some(true));
        assert_eq!(add_again, add);
    }

    #[test]
    fn fold_cse_limit_bounds_reverse_search() {
        let mut builder = RegionBuilder::with_options(
            RegionId::new(35),
            "fold-cse-limit",
            RegionBuilderOptions { fold_cse_limit: 1 },
        );
        let left = builder.param_i64(VmSlotId::new(0));
        let right = builder.param_i64(VmSlotId::new(1));
        let first = builder.fold_add_i64(left, right);
        builder.const_i64(99);
        let second = builder.fold_add_i64(left, right);

        let graph = builder.finish();
        verify_region_graph(&graph).expect("bounded CSE graph should verify");
        assert_ne!(first, second);
    }

    #[test]
    fn fold_counters_track_required_metrics() {
        let mut builder = RegionBuilder::new(RegionId::new(37), "fold-counters");
        let left = builder.param_i64(VmSlotId::new(0));
        let right = builder.param_i64(VmSlotId::new(1));
        let zero = builder.const_i64(0);
        let min = builder.const_i64(i64::MIN);
        let one = builder.const_i64(1);

        assert_eq!(builder.fold_add_i64(left, zero), left);
        let first_add = builder.fold_add_i64(left, right);
        assert_eq!(builder.fold_add_i64(left, right), first_add);
        let overflow = builder.fold_sub_i64(min, one);

        let graph = builder.finish();
        verify_region_graph(&graph).expect("counter graph should verify");
        assert!(matches!(
            graph.node(overflow).unwrap().kind,
            RegionNodeKind::Sub
        ));

        let counters = graph.fold_counters();
        assert_eq!(counters.region_ir_fold_attempts, 4);
        assert_eq!(counters.region_ir_fold_applied, 1);
        assert_eq!(counters.region_ir_cse_hits, 1);
        assert_eq!(counters.region_ir_cse_misses, 2);
        assert_eq!(counters.region_ir_fold_skipped_by_semantics, 1);
    }

    #[test]
    fn fold_cse_rejects_effectful_nodes() {
        let mut builder = RegionBuilder::new(RegionId::new(38), "effectful-cse");
        let start = builder.start();
        let condition = builder.const_bool(true);
        let snapshot = builder.add_snapshot(Vec::new());
        let first = builder.fold_or_emit_node(
            RegionNodeKind::Guard { snapshot },
            vec![condition],
            Some(start),
            RegionValueType::Control,
            RegionPlacement::Pinned,
            RegionEffects::MAY_DEOPT,
        );
        let second = builder.fold_or_emit_node(
            RegionNodeKind::Guard { snapshot },
            vec![condition],
            Some(start),
            RegionValueType::Control,
            RegionPlacement::Pinned,
            RegionEffects::MAY_DEOPT,
        );

        let graph = builder.finish();
        verify_region_graph(&graph).expect("effectful CSE graph should verify");
        assert_ne!(first, second);
        assert_eq!(graph.fold_counters().region_ir_cse_hits, 0);
        assert_eq!(graph.fold_counters().region_ir_fold_skipped_by_semantics, 2);
    }

    #[test]
    fn fold_dump_shows_stable_before_after_graphs() {
        let mut raw = RegionBuilder::new(RegionId::new(39), "raw-add-zero");
        let raw_value = raw.param_i64(VmSlotId::new(0));
        let raw_zero = raw.const_i64(0);
        raw.emit_add_i64(raw_value, raw_zero);
        let raw_graph = raw.finish();

        let mut folded = RegionBuilder::new(RegionId::new(40), "fold-add-zero");
        let folded_value = folded.param_i64(VmSlotId::new(0));
        let folded_zero = folded.const_i64(0);
        assert_eq!(folded.fold_add_i64(folded_value, folded_zero), folded_value);
        let folded_graph = folded.finish();

        assert_eq!(
            dump_region_graph(&raw_graph),
            concat!(
                "region r39 raw-add-zero\n",
                "constants:\n",
                "  c0 = i64 0\n",
                "snapshots:\n",
                "nodes:\n",
                "  n0 = Param slot=v0 : i64 [placement=floating effects=pure]\n",
                "  n1 = Const c0 : i64 [placement=floating effects=pure]\n",
                "  n2 = Add inputs=[n0,n1] : i64 [placement=floating effects=pure]\n",
            )
        );
        assert_eq!(
            dump_region_graph(&folded_graph),
            concat!(
                "region r40 fold-add-zero\n",
                "constants:\n",
                "  c0 = i64 0\n",
                "snapshots:\n",
                "nodes:\n",
                "  n0 = Param slot=v0 : i64 [placement=floating effects=pure]\n",
                "  n1 = Const c0 : i64 [placement=floating effects=pure]\n",
            )
        );
    }

    fn const_i64_at(
        graph: &crate::region_ir::OptimizerRegionGraph,
        node: crate::region_ir::NodeId,
    ) -> Option<i64> {
        let RegionNodeKind::Const(constant) = graph.node(node)?.kind else {
            return None;
        };
        match graph.constant(constant)? {
            RegionConst::I64(value) => Some(*value),
            _ => None,
        }
    }

    fn const_bool_at(
        graph: &crate::region_ir::OptimizerRegionGraph,
        node: crate::region_ir::NodeId,
    ) -> Option<bool> {
        let RegionNodeKind::Const(constant) = graph.node(node)?.kind else {
            return None;
        };
        match graph.constant(constant)? {
            RegionConst::Bool(value) => Some(*value),
            _ => None,
        }
    }
}
