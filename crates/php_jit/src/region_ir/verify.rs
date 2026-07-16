//! Region IR verifier.

use super::{
    NodeId, OptimizerRegionGraph, RegionNode, RegionNodeKind, RegionPlacement, RegionValueType,
    SnapshotId,
};

/// Stable verifier error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionVerifyError {
    /// Machine-readable error code.
    pub code: &'static str,
    /// Human-readable detail.
    pub detail: String,
    /// Node where the error was found, when node-local.
    pub node: Option<NodeId>,
}

impl RegionVerifyError {
    fn new(code: &'static str, detail: impl Into<String>, node: Option<NodeId>) -> Self {
        Self {
            code,
            detail: detail.into(),
            node,
        }
    }
}

/// Verifies a region graph.
pub fn verify_region_graph(graph: &OptimizerRegionGraph) -> Result<(), Vec<RegionVerifyError>> {
    let mut errors = Vec::new();

    for (index, node) in graph.nodes().iter().enumerate() {
        let id = NodeId::new(index as u32);
        verify_references(graph, id, node, &mut errors);
        verify_node_shape(graph, id, node, &mut errors);
    }

    for snapshot in graph.snapshots() {
        if snapshot.id.index() >= graph.snapshots().len() {
            errors.push(RegionVerifyError::new(
                "invalid_snapshot_id",
                format!(
                    "snapshot id s{} is outside snapshot table",
                    snapshot.id.raw()
                ),
                None,
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn verify_references(
    graph: &OptimizerRegionGraph,
    id: NodeId,
    node: &RegionNode,
    errors: &mut Vec<RegionVerifyError>,
) {
    for input in &node.inputs {
        if graph.node(*input).is_none() {
            errors.push(RegionVerifyError::new(
                "invalid_node_input",
                format!(
                    "node n{} references missing input n{}",
                    id.raw(),
                    input.raw()
                ),
                Some(id),
            ));
        }
    }

    if let Some(control) = node.control {
        match graph.node(control) {
            Some(control_node) if control_node.value_type == RegionValueType::Control => {}
            Some(_) => errors.push(RegionVerifyError::new(
                "invalid_control_input",
                format!(
                    "node n{} control n{} is not control-typed",
                    id.raw(),
                    control.raw()
                ),
                Some(id),
            )),
            None => errors.push(RegionVerifyError::new(
                "invalid_control_input",
                format!(
                    "node n{} references missing control n{}",
                    id.raw(),
                    control.raw()
                ),
                Some(id),
            )),
        }
    }
}

fn verify_node_shape(
    graph: &OptimizerRegionGraph,
    id: NodeId,
    node: &RegionNode,
    errors: &mut Vec<RegionVerifyError>,
) {
    if node.placement == RegionPlacement::Floating && !node.effects.is_pure() {
        errors.push(RegionVerifyError::new(
            "floating_node_has_effects",
            format!("node n{} is floating but has effects", id.raw()),
            Some(id),
        ));
    }

    if (node.placement == RegionPlacement::Pinned || !node.effects.is_pure())
        && !matches!(
            node.kind,
            RegionNodeKind::Start | RegionNodeKind::Param { .. }
        )
        && node.control.is_none()
    {
        errors.push(RegionVerifyError::new(
            "pinned_node_missing_control",
            format!("node n{} is pinned/effectful without control", id.raw()),
            Some(id),
        ));
    }

    match &node.kind {
        RegionNodeKind::Start => {
            expect_no_inputs(id, node, errors);
            if node.control.is_some() {
                errors.push(RegionVerifyError::new(
                    "start_has_control",
                    "Start node must not have a control dependency",
                    Some(id),
                ));
            }
            expect_type(id, node, RegionValueType::Control, errors);
        }
        RegionNodeKind::Param { .. } => {
            expect_no_inputs(id, node, errors);
            if node.control.is_some() {
                errors.push(RegionVerifyError::new(
                    "param_has_control",
                    "Param node must not have a control dependency",
                    Some(id),
                ));
            }
        }
        RegionNodeKind::Const(constant) => match graph.constant(*constant) {
            Some(value) => {
                if value.value_type() != node.value_type {
                    errors.push(RegionVerifyError::new(
                        "const_type_mismatch",
                        format!(
                            "node n{} has type {} but constant c{} has type {}",
                            id.raw(),
                            node.value_type.as_str(),
                            constant.raw(),
                            value.value_type().as_str()
                        ),
                        Some(id),
                    ));
                }
            }
            None => errors.push(RegionVerifyError::new(
                "invalid_const",
                format!(
                    "node n{} references missing constant c{}",
                    id.raw(),
                    constant.raw()
                ),
                Some(id),
            )),
        },
        RegionNodeKind::Copy => {
            expect_input_count(id, node, 1, errors);
            if let Some(input) = node.inputs.first()
                && let Some(input_node) = graph.node(*input)
                && input_node.value_type != node.value_type
            {
                errors.push(RegionVerifyError::new(
                    "typed_input_mismatch",
                    format!(
                        "node n{} expected input n{} to be {}, found {}",
                        id.raw(),
                        input.raw(),
                        node.value_type.as_str(),
                        input_node.value_type.as_str()
                    ),
                    Some(id),
                ));
            }
        }
        RegionNodeKind::Phi => {
            if node.inputs.is_empty() {
                errors.push(RegionVerifyError::new(
                    "input_count_mismatch",
                    format!("node n{} expected at least one input", id.raw()),
                    Some(id),
                ));
            }
            expect_inputs_type(graph, id, node, node.value_type, errors);
        }
        RegionNodeKind::Add | RegionNodeKind::Sub | RegionNodeKind::Mul => {
            expect_input_count(id, node, 2, errors);
            expect_inputs_type(graph, id, node, RegionValueType::I64, errors);
            expect_type(id, node, RegionValueType::I64, errors);
        }
        RegionNodeKind::AndBool | RegionNodeKind::OrBool => {
            expect_input_count(id, node, 2, errors);
            expect_inputs_type(graph, id, node, RegionValueType::Bool, errors);
            expect_type(id, node, RegionValueType::Bool, errors);
        }
        RegionNodeKind::Compare(_) => {
            expect_input_count(id, node, 2, errors);
            expect_inputs_type(graph, id, node, RegionValueType::I64, errors);
            expect_type(id, node, RegionValueType::Bool, errors);
        }
        RegionNodeKind::If => {
            expect_input_count(id, node, 1, errors);
            expect_inputs_type(graph, id, node, RegionValueType::Bool, errors);
            expect_type(id, node, RegionValueType::Control, errors);
            if node.control.is_none() {
                errors.push(RegionVerifyError::new(
                    "control_node_missing_control",
                    "If node requires a control dependency",
                    Some(id),
                ));
            }
        }
        RegionNodeKind::Return => {
            expect_input_count(id, node, 1, errors);
            expect_type(id, node, RegionValueType::Control, errors);
            if node.control.is_none() {
                errors.push(RegionVerifyError::new(
                    "control_node_missing_control",
                    "Return node requires a control dependency",
                    Some(id),
                ));
            }
        }
        RegionNodeKind::Guard { snapshot } | RegionNodeKind::DeoptPoint { snapshot } => {
            expect_snapshot(graph, id, *snapshot, errors);
            if node.control.is_none() {
                errors.push(RegionVerifyError::new(
                    "guard_missing_control",
                    "Guard/DeoptPoint node requires a control dependency",
                    Some(id),
                ));
            }
        }
        RegionNodeKind::Snapshot(snapshot) => {
            expect_snapshot(graph, id, *snapshot, errors);
        }
        _ => {}
    }
}

fn expect_snapshot(
    graph: &OptimizerRegionGraph,
    id: NodeId,
    snapshot: SnapshotId,
    errors: &mut Vec<RegionVerifyError>,
) {
    if graph.snapshot(snapshot).is_none() {
        errors.push(RegionVerifyError::new(
            "invalid_snapshot",
            format!(
                "node n{} references missing snapshot s{}",
                id.raw(),
                snapshot.raw()
            ),
            Some(id),
        ));
    }
}

fn expect_no_inputs(id: NodeId, node: &RegionNode, errors: &mut Vec<RegionVerifyError>) {
    if !node.inputs.is_empty() {
        errors.push(RegionVerifyError::new(
            "unexpected_inputs",
            format!("node n{} must not have data inputs", id.raw()),
            Some(id),
        ));
    }
}

fn expect_input_count(
    id: NodeId,
    node: &RegionNode,
    expected: usize,
    errors: &mut Vec<RegionVerifyError>,
) {
    if node.inputs.len() != expected {
        errors.push(RegionVerifyError::new(
            "input_count_mismatch",
            format!(
                "node n{} expected {} input(s), found {}",
                id.raw(),
                expected,
                node.inputs.len()
            ),
            Some(id),
        ));
    }
}

fn expect_inputs_type(
    graph: &OptimizerRegionGraph,
    id: NodeId,
    node: &RegionNode,
    expected: RegionValueType,
    errors: &mut Vec<RegionVerifyError>,
) {
    for input in &node.inputs {
        if let Some(input_node) = graph.node(*input)
            && input_node.value_type != expected
        {
            errors.push(RegionVerifyError::new(
                "typed_input_mismatch",
                format!(
                    "node n{} expected input n{} to be {}, found {}",
                    id.raw(),
                    input.raw(),
                    expected.as_str(),
                    input_node.value_type.as_str()
                ),
                Some(id),
            ));
        }
    }
}

fn expect_type(
    id: NodeId,
    node: &RegionNode,
    expected: RegionValueType,
    errors: &mut Vec<RegionVerifyError>,
) {
    if node.value_type != expected {
        errors.push(RegionVerifyError::new(
            "node_type_mismatch",
            format!(
                "node n{} expected type {}, found {}",
                id.raw(),
                expected.as_str(),
                node.value_type.as_str()
            ),
            Some(id),
        ));
    }
}
