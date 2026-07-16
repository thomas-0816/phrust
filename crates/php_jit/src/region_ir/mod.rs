//! Backend-neutral region IR for native compilation and optimizer analysis.
//!
//! [`RegionGraph`] is the structured, multi-block compiler input consumed
//! by Cranelift. [`OptimizerRegionGraph`] remains the index-based optimizer and snapshot
//! substrate used for region-local analysis.

/// Version of the executable Region IR contract consumed by native codegen.
///
/// Increment this whenever serialized cache identity or lowering semantics can
/// no longer be shared with code produced from an earlier Region IR shape.
pub const REGION_IR_SCHEMA_VERSION: u32 = 13;

mod bind;
mod builder;
mod coverage;
mod dump;
mod executable;
mod ids;
mod node;
pub mod opt;
mod osr;
pub mod ownership;
mod rules;
mod semantic_lowering;
mod semantic_ops;
pub mod ssa;
pub mod templates;
pub mod value_flow;
mod verify;

pub use bind::{
    BindMap, BindMapError, BindMapReport, RegionLiveRange, VmSlotBinding, VmSlotDescriptor,
    VmSlotKind, VmSlotSemanticFlags, validate_bind_map,
};
pub use builder::{RegionBuilder, RegionBuilderOptions, build_minimal_scalar_region};
pub use coverage::{
    BASELINE_INSTRUCTION_MANIFEST, BASELINE_TERMINATOR_MANIFEST, BaselineEffectFlags,
    BaselineLoweringClass, BaselineLoweringManifestEntry, baseline_binary_class,
    baseline_call_arg_class, baseline_callable_class, baseline_cast_class, baseline_compare_class,
    baseline_include_class, baseline_instruction_lowering, baseline_terminator_lowering,
    baseline_unary_class,
};
pub use dump::dump_region_graph;
pub use executable::{
    BaselineRegionBuilder, CompileMetadata, NativeCompileError, NativeCompilerTier, RegionBinaryOp,
    RegionBlock, RegionCallResult, RegionCallTarget, RegionCastOp, RegionCompareOpCode,
    RegionDeclarationMetadata, RegionExceptionRegion, RegionGraph, RegionInstruction,
    RegionInstructionKind, RegionMethodIdentity, RegionNativeCall, RegionNativeControl,
    RegionNativeDynamicCode, RegionNativeSuspend, RegionOperand, RegionOsrEntryPoint,
    RegionTerminator, RegionUnaryOp, build_baseline_region,
};
pub use ids::{ConstId, EntryId, ExitId, NodeId, RegionId, SnapshotId, VmSlotId};
pub use node::{
    RegionCompareOp, RegionConst, RegionEffects, RegionNode, RegionNodeKind, RegionPlacement,
    RegionValueType,
};
pub use osr::{
    RegionOsrEntry, RegionOsrEntryMap, RegionOsrMotionPolicy, RegionOsrReport,
    region_osr_motion_policy, select_region_osr_entries,
};
pub use ownership::{
    HelperInputOwnership, HelperOwnershipContract, HelperResultOwnership,
    helper_ownership_contract, value_copy_requires_retain, value_release_required,
};
pub use rules::{dump_region_rule_selection, select_region_rules};
pub use semantic_ops::{
    RegionClassName, RegionPropertyName, RegionSemanticContext, RegionSemanticOp,
    RegionSemanticOperationId,
};
pub use ssa::{
    ExecutableSsaGraph, SsaCertainty, SsaOwnership, SsaValueClass, SsaValueFact,
    build_executable_ssa,
};
pub use value_flow::{ExecutableValueFlow, LocalStorageClass, analyze_executable_value_flow};
pub use verify::{RegionVerifyError, verify_region_graph};

/// Def-use side table for compact node storage.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DefUseLists {
    uses: Vec<Vec<NodeId>>,
}

impl DefUseLists {
    fn push_node(&mut self) {
        self.uses.push(Vec::new());
    }

    fn add_use(&mut self, def: NodeId, user: NodeId) {
        if let Some(uses) = self.uses.get_mut(def.index()) {
            uses.push(user);
        }
    }

    /// Returns users of a node.
    #[must_use]
    pub fn uses(&self, node: NodeId) -> &[NodeId] {
        self.uses
            .get(node.index())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

/// One live VM slot captured by a snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotEntry {
    /// Abstract VM slot.
    pub slot: VmSlotId,
    /// Value class expected in the slot.
    pub value_type: RegionValueType,
}

/// Metadata-only snapshot for future guard/deopt exits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionSnapshot {
    /// Snapshot table identifier.
    pub id: SnapshotId,
    /// Live slots captured by this snapshot.
    pub entries: Vec<SnapshotEntry>,
}

/// Region-level metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionMetadata {
    /// Stable region identifier.
    pub region_id: RegionId,
    /// Human-readable report name.
    pub name: String,
}

/// Construction-time fold/CSE counters for region IR reports.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RegionFoldCounters {
    /// Number of `fold_*` API calls that attempted an optimization.
    pub region_ir_fold_attempts: u64,
    /// Number of transparent fold rules that returned a simplified value.
    pub region_ir_fold_applied: u64,
    /// Number of CSE lookups that reused an existing node.
    pub region_ir_cse_hits: u64,
    /// Number of CSE lookups that emitted a new node.
    pub region_ir_cse_misses: u64,
    /// Number of folds skipped because a PHP-visible semantic guard blocked them.
    pub region_ir_fold_skipped_by_semantics: u64,
}

/// Compact, table-backed region optimizer graph.
#[derive(Clone, Debug, PartialEq)]
pub struct OptimizerRegionGraph {
    nodes: Vec<RegionNode>,
    constants: Vec<RegionConst>,
    snapshots: Vec<RegionSnapshot>,
    bind_map: BindMap,
    def_use: DefUseLists,
    metadata: RegionMetadata,
    fold_counters: RegionFoldCounters,
}

impl OptimizerRegionGraph {
    /// Creates an empty graph.
    #[must_use]
    pub fn new(region_id: RegionId, name: impl Into<String>) -> Self {
        Self {
            nodes: Vec::new(),
            constants: Vec::new(),
            snapshots: Vec::new(),
            bind_map: BindMap::default(),
            def_use: DefUseLists::default(),
            metadata: RegionMetadata {
                region_id,
                name: name.into(),
            },
            fold_counters: RegionFoldCounters::default(),
        }
    }

    /// Returns graph metadata.
    #[must_use]
    pub const fn metadata(&self) -> &RegionMetadata {
        &self.metadata
    }

    /// Returns all nodes in table order.
    #[must_use]
    pub fn nodes(&self) -> &[RegionNode] {
        &self.nodes
    }

    /// Returns all constants in table order.
    #[must_use]
    pub fn constants(&self) -> &[RegionConst] {
        &self.constants
    }

    /// Returns all snapshots in table order.
    #[must_use]
    pub fn snapshots(&self) -> &[RegionSnapshot] {
        &self.snapshots
    }

    /// Returns VM slot binding metadata.
    #[must_use]
    pub const fn bind_map(&self) -> &BindMap {
        &self.bind_map
    }

    /// Returns mutable VM slot binding metadata.
    pub const fn bind_map_mut(&mut self) -> &mut BindMap {
        &mut self.bind_map
    }

    /// Returns the def-use side table.
    #[must_use]
    pub const fn def_use(&self) -> &DefUseLists {
        &self.def_use
    }

    /// Returns construction-time fold/CSE counters.
    #[must_use]
    pub const fn fold_counters(&self) -> RegionFoldCounters {
        self.fold_counters
    }

    /// Returns mutable construction-time fold/CSE counters for builders/tools.
    pub const fn fold_counters_mut(&mut self) -> &mut RegionFoldCounters {
        &mut self.fold_counters
    }

    /// Returns one node by ID.
    #[must_use]
    pub fn node(&self, node: NodeId) -> Option<&RegionNode> {
        self.nodes.get(node.index())
    }

    /// Returns one constant by ID.
    #[must_use]
    pub fn constant(&self, constant: ConstId) -> Option<&RegionConst> {
        self.constants.get(constant.index())
    }

    /// Returns one snapshot by ID.
    #[must_use]
    pub fn snapshot(&self, snapshot: SnapshotId) -> Option<&RegionSnapshot> {
        self.snapshots.get(snapshot.index())
    }

    /// Adds a constant and returns its compact ID.
    pub fn add_constant(&mut self, constant: RegionConst) -> ConstId {
        let id = ConstId::new(self.constants.len() as u32);
        self.constants.push(constant);
        id
    }

    /// Adds a snapshot and returns its compact ID.
    pub fn add_snapshot(&mut self, entries: Vec<SnapshotEntry>) -> SnapshotId {
        let id = SnapshotId::new(self.snapshots.len() as u32);
        self.snapshots.push(RegionSnapshot { id, entries });
        id
    }

    /// Adds a node and updates def-use metadata.
    pub fn add_node(&mut self, node: RegionNode) -> NodeId {
        let id = NodeId::new(self.nodes.len() as u32);
        self.def_use.push_node();
        for input in &node.inputs {
            self.def_use.add_use(*input, id);
        }
        if let Some(control) = node.control {
            self.def_use.add_use(control, id);
        }
        self.nodes.push(node);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::{
        OptimizerRegionGraph, RegionBuilder, RegionConst, RegionEffects, RegionId, RegionNode,
        RegionNodeKind, RegionPlacement, RegionValueType, SnapshotEntry, VmSlotId,
        build_minimal_scalar_region, dump_region_graph, verify_region_graph,
    };

    #[test]
    fn region_ir_constructs_minimal_graph() {
        let graph = build_minimal_scalar_region();

        verify_region_graph(&graph).expect("minimal scalar graph should verify");
        assert_eq!(graph.nodes().len(), 8);
        assert_eq!(
            graph.constants(),
            &[RegionConst::I64(1), RegionConst::I64(1)]
        );
        assert_eq!(graph.def_use().uses(super::NodeId::new(1)).len(), 1);
    }

    #[test]
    fn region_ir_rejects_invalid_node_reference() {
        let mut graph = OptimizerRegionGraph::new(RegionId::new(7), "bad-ref");
        graph.add_node(RegionNode::new(
            RegionNodeKind::Add,
            vec![super::NodeId::new(99), super::NodeId::new(0)],
            None,
            RegionValueType::I64,
            RegionPlacement::Floating,
            RegionEffects::PURE,
        ));

        let errors = verify_region_graph(&graph).expect_err("bad reference should fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "invalid_node_input")
        );
    }

    #[test]
    fn region_ir_rejects_type_mismatch() {
        let mut builder = RegionBuilder::new(RegionId::new(2), "type-mismatch");
        builder.start();
        let left = builder.const_bool(true);
        let right = builder.const_i64(1);
        builder.emit_add_i64(left, right);
        let graph = builder.finish();

        let errors = verify_region_graph(&graph).expect_err("i64 add with bool should fail");
        assert!(
            errors
                .iter()
                .any(|error| error.code == "typed_input_mismatch")
        );
    }

    #[test]
    fn region_ir_rejects_missing_snapshot() {
        let mut graph = OptimizerRegionGraph::new(RegionId::new(3), "missing-snapshot");
        let start = graph.add_node(RegionNode::new(
            RegionNodeKind::Start,
            Vec::new(),
            None,
            RegionValueType::Control,
            RegionPlacement::ControlOnly,
            RegionEffects::PURE,
        ));
        graph.add_node(RegionNode::new(
            RegionNodeKind::Guard {
                snapshot: super::SnapshotId::new(77),
            },
            Vec::new(),
            Some(start),
            RegionValueType::Control,
            RegionPlacement::Pinned,
            RegionEffects::MAY_DEOPT,
        ));

        let errors = verify_region_graph(&graph).expect_err("missing snapshot should fail");
        assert!(errors.iter().any(|error| error.code == "invalid_snapshot"));
    }

    #[test]
    fn region_ir_stable_dump_output() {
        let graph = build_minimal_scalar_region();

        assert_eq!(
            dump_region_graph(&graph),
            concat!(
                "region r0 minimal-scalar\n",
                "constants:\n",
                "  c0 = i64 1\n",
                "  c1 = i64 1\n",
                "snapshots:\n",
                "nodes:\n",
                "  n0 = Start : control [placement=control-only effects=pure]\n",
                "  n1 = Param slot=v0 : i64 [placement=floating effects=pure]\n",
                "  n2 = Const c0 : i64 [placement=floating effects=pure]\n",
                "  n3 = Add inputs=[n1,n2] : i64 [placement=floating effects=pure]\n",
                "  n4 = Const c1 : i64 [placement=floating effects=pure]\n",
                "  n5 = Compare.lt inputs=[n3,n4] : bool [placement=floating effects=pure]\n",
                "  n6 = If control=n0 inputs=[n5] : control [placement=control-only effects=pure]\n",
                "  n7 = Return control=n6 inputs=[n3] : control [placement=control-only effects=pure]\n",
            )
        );
    }

    #[test]
    fn region_ir_snapshot_entries_verify() {
        let mut builder = RegionBuilder::new(RegionId::new(4), "snapshot");
        let start = builder.start();
        let value = builder.param_i64(VmSlotId::new(0));
        let snapshot = builder.add_snapshot(vec![SnapshotEntry {
            slot: VmSlotId::new(0),
            value_type: RegionValueType::I64,
        }]);
        builder.emit_guard(snapshot, start, value);

        verify_region_graph(&builder.finish()).expect("valid snapshot should verify");
    }
}
