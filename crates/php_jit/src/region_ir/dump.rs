//! Deterministic region IR dump.

use super::{OptimizerRegionGraph, RegionConst, RegionNodeKind};

/// Dumps a region graph in a stable textual format.
#[must_use]
pub fn dump_region_graph(graph: &OptimizerRegionGraph) -> String {
    let mut out = String::new();
    out.push_str("region r");
    out.push_str(&graph.metadata().region_id.raw().to_string());
    out.push(' ');
    out.push_str(&graph.metadata().name);
    out.push('\n');

    out.push_str("constants:\n");
    for (index, constant) in graph.constants().iter().enumerate() {
        out.push_str("  c");
        out.push_str(&index.to_string());
        out.push_str(" = ");
        dump_constant(&mut out, constant);
        out.push('\n');
    }

    out.push_str("snapshots:\n");
    for snapshot in graph.snapshots() {
        out.push_str("  s");
        out.push_str(&snapshot.id.raw().to_string());
        out.push_str(" = [");
        for (index, entry) in snapshot.entries.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push('v');
            out.push_str(&entry.slot.raw().to_string());
            out.push(':');
            out.push_str(entry.value_type.as_str());
        }
        out.push_str("]\n");
    }

    out.push_str("nodes:\n");
    for (index, node) in graph.nodes().iter().enumerate() {
        out.push_str("  n");
        out.push_str(&index.to_string());
        out.push_str(" = ");
        out.push_str(node.kind.name());
        dump_kind_suffix(&mut out, &node.kind);
        if let Some(control) = node.control {
            out.push_str(" control=n");
            out.push_str(&control.raw().to_string());
        }
        if !node.inputs.is_empty() {
            out.push_str(" inputs=[");
            for (input_index, input) in node.inputs.iter().enumerate() {
                if input_index > 0 {
                    out.push(',');
                }
                out.push('n');
                out.push_str(&input.raw().to_string());
            }
            out.push(']');
        }
        out.push_str(" : ");
        out.push_str(node.value_type.as_str());
        out.push_str(" [placement=");
        out.push_str(node.placement.as_str());
        out.push_str(" effects=");
        out.push_str(&node.effects.dump_label());
        out.push_str("]\n");
    }

    out
}

fn dump_constant(out: &mut String, constant: &RegionConst) {
    match constant {
        RegionConst::Bool(value) => {
            out.push_str("bool ");
            out.push_str(if *value { "true" } else { "false" });
        }
        RegionConst::I64(value) => {
            out.push_str("i64 ");
            out.push_str(&value.to_string());
        }
        RegionConst::F64(value) => {
            out.push_str("f64 ");
            out.push_str(&value.to_string());
        }
        RegionConst::StringHandle(value) => {
            out.push_str("string-handle ");
            out.push_str(value);
        }
    }
}

fn dump_kind_suffix(out: &mut String, kind: &RegionNodeKind) {
    match kind {
        RegionNodeKind::Const(constant) => {
            out.push_str(" c");
            out.push_str(&constant.raw().to_string());
        }
        RegionNodeKind::Param { slot } => {
            out.push_str(" slot=v");
            out.push_str(&slot.raw().to_string());
        }
        RegionNodeKind::Compare(op) => {
            out.push('.');
            out.push_str(op.as_str());
        }
        RegionNodeKind::Entry(entry) => {
            out.push_str(" e");
            out.push_str(&entry.raw().to_string());
        }
        RegionNodeKind::Exit(exit) => {
            out.push_str(" x");
            out.push_str(&exit.raw().to_string());
        }
        RegionNodeKind::Guard { snapshot } | RegionNodeKind::DeoptPoint { snapshot } => {
            out.push_str(" snapshot=s");
            out.push_str(&snapshot.raw().to_string());
        }
        RegionNodeKind::Snapshot(snapshot) => {
            out.push_str(" s");
            out.push_str(&snapshot.raw().to_string());
        }
        _ => {}
    }
}
