//! Metadata-only runtime operation templates for future region construction.

use std::collections::BTreeMap;

use php_ir::rule_selection::RuleSelectionReport;

use super::{
    OptimizerRegionGraph, RegionBuilder, RegionCompareOp, RegionId, RegionValueType, SnapshotEntry,
    VmSlotId, rules::select_region_rules,
};

pub mod arrays;
pub mod calls;
pub mod int_ops;
pub mod properties;
pub mod strings;

/// Value class accepted by a runtime template parameter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemplateValueClass {
    /// Exact `bool` region scalar.
    Bool,
    /// Exact `i64` region scalar.
    I64,
    /// Exact runtime string with no object conversion.
    ExactString,
    /// Packed array with known layout.
    PackedArray,
    /// Object with known class/layout metadata.
    Object,
    /// Interned array key.
    InternedKey,
    /// Generic PHP value, usually unsupported for lowering.
    Mixed,
}

impl TemplateValueClass {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::I64 => "i64",
            Self::ExactString => "exact_string",
            Self::PackedArray => "packed_array",
            Self::Object => "object",
            Self::InternedKey => "interned_key",
            Self::Mixed => "mixed",
        }
    }
}

/// One runtime template parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateParam {
    /// Stable parameter name.
    pub name: &'static str,
    /// Required input class.
    pub value_class: TemplateValueClass,
    /// VM slot required by this parameter, when the template consumes a slot.
    pub vm_slot: Option<VmSlotId>,
}

impl TemplateParam {
    /// Creates a parameter descriptor.
    #[must_use]
    pub const fn new(
        name: &'static str,
        value_class: TemplateValueClass,
        vm_slot: Option<VmSlotId>,
    ) -> Self {
        Self {
            name,
            value_class,
            vm_slot,
        }
    }
}

/// Guard required before a template lowering can be used.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateGuard {
    /// Stable guard identifier.
    pub id: &'static str,
    /// Human-readable detail.
    pub detail: &'static str,
    /// Whether the guard needs snapshot metadata for fallback.
    pub requires_snapshot: bool,
}

impl TemplateGuard {
    /// Creates a guard descriptor.
    #[must_use]
    pub const fn new(id: &'static str, detail: &'static str, requires_snapshot: bool) -> Self {
        Self {
            id,
            detail,
            requires_snapshot,
        }
    }
}

/// Template implementation kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeTemplateKind {
    /// Checked integer add lowered to scalar region IR.
    IntAddChecked,
    /// Integer compare lowered to scalar region IR.
    IntCompare,
    /// Exact string concat declaration; currently rejected.
    StringConcatExact,
    /// Read-only packed array fetch declaration; currently rejected for COW/ref cases.
    PackedArrayFetchReadonly,
    /// Packed foreach integer sum metadata-only declaration.
    PackedForeachIntSumMetadataOnly,
    /// Guarded property slot fetch declaration.
    PropertySlotFetchGuarded,
    /// Exact `strlen` builtin metadata.
    KnownBuiltinStrlenExact,
    /// Exact packed-array `count` builtin metadata.
    KnownBuiltinCountPackedExact,
    /// Interned-key `isset` array lookup declaration.
    IssetArrayKeyInternedExact,
    /// Record-shape array lookup guarded by the key's interned symbol.
    RecordArrayLookupSymbolGuard,
}

/// Runtime operation template descriptor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeTemplate {
    /// Stable template name.
    pub name: &'static str,
    /// Template implementation kind.
    pub kind: RuntimeTemplateKind,
    /// Required parameters.
    pub params: Vec<TemplateParam>,
    /// Required guards.
    pub guards: Vec<TemplateGuard>,
    /// Required VM slots, independent of parameter slots.
    pub required_vm_slots: Vec<VmSlotId>,
    /// Reference/COW restrictions.
    pub reference_cow_restrictions: Vec<&'static str>,
    /// Possible side exits.
    pub possible_side_exits: Vec<&'static str>,
    /// Snapshot slots required for a native slow path or version transition.
    pub snapshot_requirements: Vec<SnapshotEntry>,
    /// Typed runtime slow-path helper, if any.
    pub slow_path_helper: Option<&'static str>,
    /// Explicit unsupported PHP semantic cases.
    pub unsupported_php_semantic_cases: Vec<&'static str>,
}

impl RuntimeTemplate {
    /// Lowers this template into region IR and/or metadata when the context proves safety.
    #[must_use]
    pub fn lower(&self, context: &TemplateLoweringContext) -> TemplateLoweringOutcome {
        match self.kind {
            RuntimeTemplateKind::IntAddChecked => lower_int_add_checked(self),
            RuntimeTemplateKind::IntCompare => lower_int_compare(self),
            RuntimeTemplateKind::KnownBuiltinStrlenExact => lower_strlen_exact(self, context),
            RuntimeTemplateKind::KnownBuiltinCountPackedExact => {
                lower_count_packed_exact(self, context)
            }
            RuntimeTemplateKind::PackedForeachIntSumMetadataOnly => {
                lower_packed_foreach_int_sum(self, context)
            }
            RuntimeTemplateKind::StringConcatExact => {
                if context.exact_strings && context.no_object_or_string_conversion {
                    TemplateLoweringOutcome::rejected("string_region_values_not_available")
                } else {
                    TemplateLoweringOutcome::rejected("object_or_string_conversion")
                }
            }
            RuntimeTemplateKind::PackedArrayFetchReadonly => {
                if context.packed_array_readonly && context.no_references && context.no_cow {
                    TemplateLoweringOutcome::rejected("array_value_region_model_missing")
                } else {
                    TemplateLoweringOutcome::rejected("reference_or_cow_sensitive_array")
                }
            }
            RuntimeTemplateKind::PropertySlotFetchGuarded => {
                if context.no_magic_property_hooks {
                    TemplateLoweringOutcome::rejected("object_value_region_model_missing")
                } else {
                    TemplateLoweringOutcome::rejected("magic_property_or_hook")
                }
            }
            RuntimeTemplateKind::RecordArrayLookupSymbolGuard => {
                // The record lookup executes natively through the helper-backed
                // candidate pipeline (like string concat); region-IR lowering
                // of the raw slot read remains unavailable.
                if context.no_references && context.no_cow {
                    TemplateLoweringOutcome::rejected("record_value_region_model_missing")
                } else {
                    TemplateLoweringOutcome::rejected("reference_or_cow_sensitive_array")
                }
            }
            RuntimeTemplateKind::IssetArrayKeyInternedExact => {
                if context.interned_array_key && context.no_references && context.no_cow {
                    TemplateLoweringOutcome::rejected("array_isset_region_model_missing")
                } else {
                    TemplateLoweringOutcome::rejected("array_key_or_cow_semantics")
                }
            }
        }
    }
}

/// Proof context available to a template lowering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemplateLoweringContext {
    /// Exact string inputs are already proven and no conversion is required.
    pub exact_strings: bool,
    /// Packed-array shape is known and read-only for the operation.
    pub packed_array_readonly: bool,
    /// Inputs are proven not to be references.
    pub no_references: bool,
    /// Inputs are proven not to require copy-on-write separation.
    pub no_cow: bool,
    /// Object/string conversion paths are proven unreachable.
    pub no_object_or_string_conversion: bool,
    /// Magic properties and property hooks are proven unreachable.
    pub no_magic_property_hooks: bool,
    /// Array key is proven interned and exact.
    pub interned_array_key: bool,
    /// Snapshot slots available for side exits.
    pub snapshot_slots: Vec<SnapshotEntry>,
}

impl Default for TemplateLoweringContext {
    fn default() -> Self {
        Self {
            exact_strings: false,
            packed_array_readonly: false,
            no_references: true,
            no_cow: true,
            no_object_or_string_conversion: true,
            no_magic_property_hooks: true,
            interned_array_key: false,
            snapshot_slots: default_snapshot_slots(),
        }
    }
}

impl TemplateLoweringContext {
    /// Conservative default scalar context used by report-only template smokes.
    #[must_use]
    pub fn scalar_safe() -> Self {
        Self::default()
    }

    /// Context proving exact builtin string input.
    #[must_use]
    pub fn exact_string() -> Self {
        Self {
            exact_strings: true,
            ..Self::default()
        }
    }

    /// Context proving exact packed-array input.
    #[must_use]
    pub fn packed_array_readonly() -> Self {
        Self {
            packed_array_readonly: true,
            ..Self::default()
        }
    }
}

/// Status of one template lowering attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemplateLoweringStatus {
    /// Lowering produced region IR and/or metadata.
    Lowered,
    /// Lowering was rejected with an explicit reason.
    Rejected,
}

impl TemplateLoweringStatus {
    /// Stable report spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lowered => "lowered",
            Self::Rejected => "rejected",
        }
    }
}

/// Lowered template artifact.
#[derive(Clone, Debug, PartialEq)]
pub struct TemplateLoweredArtifact {
    /// Optional scalar region graph.
    pub graph: Option<OptimizerRegionGraph>,
    /// Optional rule-selection metadata for the graph.
    pub rule_selection: Option<RuleSelectionReport>,
    /// Metadata-only facts emitted by the template.
    pub metadata: Vec<&'static str>,
    /// Snapshot slots required by the lowering.
    pub snapshot_slots_required: Vec<SnapshotEntry>,
}

/// Result of one template lowering attempt.
#[derive(Clone, Debug, PartialEq)]
pub struct TemplateLoweringOutcome {
    /// Lowering status.
    pub status: TemplateLoweringStatus,
    /// Lowered artifact when successful.
    pub artifact: Option<TemplateLoweredArtifact>,
    /// Rejection reason when unsuccessful.
    pub rejected_reason: Option<&'static str>,
}

impl TemplateLoweringOutcome {
    fn lowered(artifact: TemplateLoweredArtifact) -> Self {
        Self {
            status: TemplateLoweringStatus::Lowered,
            artifact: Some(artifact),
            rejected_reason: None,
        }
    }

    fn rejected(reason: &'static str) -> Self {
        Self {
            status: TemplateLoweringStatus::Rejected,
            artifact: None,
            rejected_reason: Some(reason),
        }
    }
}

/// One report row for a template lowering attempt.
#[derive(Clone, Debug, PartialEq)]
pub struct TemplateLoweringEntry {
    /// Template name.
    pub template: &'static str,
    /// Lowering status.
    pub status: TemplateLoweringStatus,
    /// Rejection reason.
    pub rejected_reason: Option<&'static str>,
    /// Number of guards declared by the template.
    pub guards_required: usize,
    /// Number of snapshot slots required.
    pub snapshot_slots_required: usize,
    /// Lowered region node count.
    pub region_nodes: usize,
    /// Selected rule count, when region metadata exists.
    pub rule_selection_selected: u64,
}

/// Aggregate template lowering report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TemplateLoweringReport {
    /// Number of templates considered.
    pub templates_considered: u64,
    /// Number of templates lowered.
    pub templates_lowered: u64,
    /// Rejection counters by stable reason.
    pub templates_rejected_by_reason: BTreeMap<&'static str, u64>,
    /// Total guard declarations considered.
    pub guards_required: u64,
    /// Total snapshot slots required by lowered artifacts.
    pub snapshot_slots_required: u64,
    /// Per-template rows.
    pub entries: Vec<TemplateLoweringEntry>,
}

impl TemplateLoweringReport {
    /// Adds one lowering outcome to the report.
    pub fn push(&mut self, template: &RuntimeTemplate, outcome: &TemplateLoweringOutcome) {
        self.templates_considered += 1;
        self.guards_required += template.guards.len() as u64;
        let artifact = outcome.artifact.as_ref();
        let snapshot_slots_required = artifact
            .map(|artifact| artifact.snapshot_slots_required.len())
            .unwrap_or(0);
        self.snapshot_slots_required += snapshot_slots_required as u64;
        let region_nodes = artifact
            .and_then(|artifact| artifact.graph.as_ref())
            .map(|graph| graph.nodes().len())
            .unwrap_or(0);
        let rule_selection_selected = artifact
            .and_then(|artifact| artifact.rule_selection.as_ref())
            .map(|report| report.rule_selection_selected)
            .unwrap_or(0);

        if outcome.status == TemplateLoweringStatus::Lowered {
            self.templates_lowered += 1;
        } else if let Some(reason) = outcome.rejected_reason {
            *self.templates_rejected_by_reason.entry(reason).or_default() += 1;
        }

        self.entries.push(TemplateLoweringEntry {
            template: template.name,
            status: outcome.status,
            rejected_reason: outcome.rejected_reason,
            guards_required: template.guards.len(),
            snapshot_slots_required,
            region_nodes,
            rule_selection_selected,
        });
    }

    /// Stable Markdown report.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Runtime IR Templates\n\n");
        out.push_str(&format!(
            "- templates_considered: {}\n",
            self.templates_considered
        ));
        out.push_str(&format!(
            "- templates_lowered: {}\n",
            self.templates_lowered
        ));
        out.push_str(&format!("- guards_required: {}\n", self.guards_required));
        out.push_str(&format!(
            "- snapshot_slots_required: {}\n\n",
            self.snapshot_slots_required
        ));
        out.push_str("## Rejections\n\n");
        for (reason, count) in &self.templates_rejected_by_reason {
            out.push_str(&format!("- {reason}: {count}\n"));
        }
        out.push_str("\n## Templates\n\n");
        out.push_str("| Template | Status | Reason | Guards | Snapshot Slots | Region Nodes | Rules Selected |\n");
        out.push_str("| --- | --- | --- | ---: | ---: | ---: | ---: |\n");
        for entry in &self.entries {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} |\n",
                entry.template,
                entry.status.as_str(),
                entry.rejected_reason.unwrap_or(""),
                entry.guards_required,
                entry.snapshot_slots_required,
                entry.region_nodes,
                entry.rule_selection_selected
            ));
        }
        out
    }

    /// Stable compact JSON report.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\n");
        out.push_str(&format!(
            "  \"templates_considered\": {},\n",
            self.templates_considered
        ));
        out.push_str(&format!(
            "  \"templates_lowered\": {},\n",
            self.templates_lowered
        ));
        out.push_str("  \"templates_rejected_by_reason\": {");
        for (index, (reason, count)) in self.templates_rejected_by_reason.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!("\"{}\": {}", json_escape(reason), count));
        }
        out.push_str("},\n");
        out.push_str(&format!(
            "  \"guards_required\": {},\n",
            self.guards_required
        ));
        out.push_str(&format!(
            "  \"snapshot_slots_required\": {},\n",
            self.snapshot_slots_required
        ));
        out.push_str("  \"templates\": [\n");
        for (index, entry) in self.entries.iter().enumerate() {
            if index > 0 {
                out.push_str(",\n");
            }
            out.push_str("    {");
            out.push_str(&format!(
                "\"template\": \"{}\", \"status\": \"{}\", ",
                json_escape(entry.template),
                entry.status.as_str()
            ));
            match entry.rejected_reason {
                Some(reason) => {
                    out.push_str(&format!(
                        "\"rejected_reason\": \"{}\", ",
                        json_escape(reason)
                    ));
                }
                None => out.push_str("\"rejected_reason\": null, "),
            }
            out.push_str(&format!(
                "\"guards_required\": {}, \"snapshot_slots_required\": {}, \"region_nodes\": {}, \"rule_selection_selected\": {}",
                entry.guards_required,
                entry.snapshot_slots_required,
                entry.region_nodes,
                entry.rule_selection_selected
            ));
            out.push('}');
        }
        out.push_str("\n  ]\n");
        out.push_str("}\n");
        out
    }
}

/// Returns every initial runtime template descriptor.
#[must_use]
pub fn runtime_templates() -> Vec<RuntimeTemplate> {
    let mut templates = Vec::new();
    templates.extend(int_ops::templates());
    templates.extend(strings::templates());
    templates.extend(arrays::templates());
    templates.extend(properties::templates());
    templates.extend(calls::templates());
    templates
}

/// Lowers the default report catalog used by performance smoke gates.
#[must_use]
pub fn lower_default_template_catalog() -> TemplateLoweringReport {
    let mut report = TemplateLoweringReport::default();
    for template in runtime_templates() {
        let context = default_context_for(&template);
        let outcome = template.lower(&context);
        report.push(&template, &outcome);
    }
    report
}

fn lower_int_add_checked(template: &RuntimeTemplate) -> TemplateLoweringOutcome {
    let mut builder = RegionBuilder::new(RegionId::new(430), template.name);
    let start = builder.start();
    let left = builder.param_i64(VmSlotId::new(0));
    let right = builder.param_i64(VmSlotId::new(1));
    let added = builder.emit_add_i64(left, right);
    builder.emit_return(start, added);
    lowered_graph(builder.finish(), vec!["checked_i64_add"])
}

fn lower_int_compare(template: &RuntimeTemplate) -> TemplateLoweringOutcome {
    let mut builder = RegionBuilder::new(RegionId::new(431), template.name);
    let start = builder.start();
    let left = builder.param_i64(VmSlotId::new(0));
    let right = builder.param_i64(VmSlotId::new(1));
    let compared = builder.emit_compare_i64(RegionCompareOp::Lt, left, right);
    builder.emit_return(start, compared);
    lowered_graph(builder.finish(), vec!["exact_i64_compare_lt"])
}

fn lower_strlen_exact(
    template: &RuntimeTemplate,
    context: &TemplateLoweringContext,
) -> TemplateLoweringOutcome {
    if context.exact_strings && context.no_object_or_string_conversion {
        return lowered_metadata(template, vec!["known_builtin_strlen_exact"]);
    }
    TemplateLoweringOutcome::rejected("object_or_string_conversion")
}

fn lower_count_packed_exact(
    template: &RuntimeTemplate,
    context: &TemplateLoweringContext,
) -> TemplateLoweringOutcome {
    if context.packed_array_readonly && context.no_references && context.no_cow {
        return lowered_metadata(template, vec!["known_builtin_count_packed_exact"]);
    }
    TemplateLoweringOutcome::rejected("reference_or_cow_sensitive_array")
}

fn lower_packed_foreach_int_sum(
    template: &RuntimeTemplate,
    context: &TemplateLoweringContext,
) -> TemplateLoweringOutcome {
    if context.packed_array_readonly && context.no_references && context.no_cow {
        return lowered_metadata(template, vec!["packed_foreach_int_sum_metadata_only"]);
    }
    TemplateLoweringOutcome::rejected("reference_or_cow_sensitive_array")
}

fn lowered_graph(
    graph: OptimizerRegionGraph,
    metadata: Vec<&'static str>,
) -> TemplateLoweringOutcome {
    let rule_selection = select_region_rules(&graph);
    TemplateLoweringOutcome::lowered(TemplateLoweredArtifact {
        graph: Some(graph),
        rule_selection: Some(rule_selection),
        metadata,
        snapshot_slots_required: default_snapshot_slots(),
    })
}

fn lowered_metadata(
    template: &RuntimeTemplate,
    metadata: Vec<&'static str>,
) -> TemplateLoweringOutcome {
    TemplateLoweringOutcome::lowered(TemplateLoweredArtifact {
        graph: None,
        rule_selection: None,
        metadata,
        snapshot_slots_required: template.snapshot_requirements.clone(),
    })
}

fn default_context_for(template: &RuntimeTemplate) -> TemplateLoweringContext {
    match template.kind {
        RuntimeTemplateKind::KnownBuiltinStrlenExact => TemplateLoweringContext::exact_string(),
        RuntimeTemplateKind::KnownBuiltinCountPackedExact
        | RuntimeTemplateKind::PackedForeachIntSumMetadataOnly => {
            TemplateLoweringContext::packed_array_readonly()
        }
        RuntimeTemplateKind::StringConcatExact => TemplateLoweringContext {
            exact_strings: false,
            no_object_or_string_conversion: false,
            ..TemplateLoweringContext::default()
        },
        RuntimeTemplateKind::PackedArrayFetchReadonly => TemplateLoweringContext {
            packed_array_readonly: true,
            no_references: false,
            no_cow: false,
            ..TemplateLoweringContext::default()
        },
        RuntimeTemplateKind::PropertySlotFetchGuarded => TemplateLoweringContext {
            no_magic_property_hooks: false,
            ..TemplateLoweringContext::default()
        },
        RuntimeTemplateKind::IssetArrayKeyInternedExact => TemplateLoweringContext {
            interned_array_key: true,
            no_cow: false,
            ..TemplateLoweringContext::default()
        },
        _ => TemplateLoweringContext::default(),
    }
}

fn default_snapshot_slots() -> Vec<SnapshotEntry> {
    vec![
        SnapshotEntry {
            slot: VmSlotId::new(0),
            value_type: RegionValueType::I64,
        },
        SnapshotEntry {
            slot: VmSlotId::new(1),
            value_type: RegionValueType::I64,
        },
    ]
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::{
        RuntimeTemplateKind, TemplateLoweringContext, TemplateLoweringStatus, TemplateValueClass,
        lower_default_template_catalog, runtime_templates,
    };
    use crate::region_ir::verify_region_graph;

    #[test]
    fn template_int_add_lowers_to_native_region_ir() {
        let template = find_template(RuntimeTemplateKind::IntAddChecked);
        assert_eq!(template.params[0].value_class, TemplateValueClass::I64);
        let outcome = template.lower(&TemplateLoweringContext::scalar_safe());
        assert_eq!(outcome.status, TemplateLoweringStatus::Lowered);

        let artifact = outcome.artifact.expect("lowered artifact");
        let graph = artifact.graph.expect("region graph");
        assert!(!graph.nodes().is_empty());
        assert!(verify_region_graph(&graph).is_ok());
        assert!(artifact.rule_selection.is_some());
    }

    #[test]
    fn exact_strlen_and_count_lower_to_metadata() {
        let strlen = find_template(RuntimeTemplateKind::KnownBuiltinStrlenExact);
        let strlen_outcome = strlen.lower(&TemplateLoweringContext::exact_string());
        assert_eq!(strlen_outcome.status, TemplateLoweringStatus::Lowered);
        assert_eq!(
            strlen_outcome
                .artifact
                .as_ref()
                .expect("strlen artifact")
                .metadata,
            vec!["known_builtin_strlen_exact"]
        );

        let count = find_template(RuntimeTemplateKind::KnownBuiltinCountPackedExact);
        let count_outcome = count.lower(&TemplateLoweringContext::packed_array_readonly());
        assert_eq!(count_outcome.status, TemplateLoweringStatus::Lowered);
        assert_eq!(
            count_outcome
                .artifact
                .as_ref()
                .expect("count artifact")
                .metadata,
            vec!["known_builtin_count_packed_exact"]
        );
    }

    #[test]
    fn object_string_conversion_case_is_rejected() {
        let template = find_template(RuntimeTemplateKind::StringConcatExact);
        let outcome = template.lower(&TemplateLoweringContext {
            exact_strings: false,
            no_object_or_string_conversion: false,
            ..TemplateLoweringContext::default()
        });

        assert_eq!(outcome.status, TemplateLoweringStatus::Rejected);
        assert_eq!(outcome.rejected_reason, Some("object_or_string_conversion"));
    }

    #[test]
    fn reference_or_cow_sensitive_array_case_is_rejected() {
        let template = find_template(RuntimeTemplateKind::PackedArrayFetchReadonly);
        let outcome = template.lower(&TemplateLoweringContext {
            packed_array_readonly: true,
            no_references: false,
            no_cow: false,
            ..TemplateLoweringContext::default()
        });

        assert_eq!(outcome.status, TemplateLoweringStatus::Rejected);
        assert_eq!(
            outcome.rejected_reason,
            Some("reference_or_cow_sensitive_array")
        );
    }

    #[test]
    fn magic_property_or_hook_case_is_rejected() {
        let template = find_template(RuntimeTemplateKind::PropertySlotFetchGuarded);
        let outcome = template.lower(&TemplateLoweringContext {
            no_magic_property_hooks: false,
            ..TemplateLoweringContext::default()
        });

        assert_eq!(outcome.status, TemplateLoweringStatus::Rejected);
        assert_eq!(outcome.rejected_reason, Some("magic_property_or_hook"));
    }

    #[test]
    fn guard_and_snapshot_metadata_are_reported() {
        let template = find_template(RuntimeTemplateKind::IntAddChecked);
        let outcome = template.lower(&TemplateLoweringContext::scalar_safe());
        let artifact = outcome.artifact.expect("artifact");

        assert!(template.guards.iter().any(|guard| guard.requires_snapshot));
        assert_eq!(artifact.snapshot_slots_required.len(), 2);
    }

    #[test]
    fn default_template_report_has_required_counters() {
        let report = lower_default_template_catalog();
        let json = report.to_json();
        let markdown = report.to_markdown();

        assert_eq!(report.templates_considered, 10);
        assert!(report.templates_lowered >= 4);
        assert!(report.guards_required > 0);
        assert!(report.snapshot_slots_required > 0);
        assert!(json.contains("\"templates_considered\""));
        assert!(markdown.starts_with("# Runtime IR Templates\n"));
    }

    fn find_template(kind: RuntimeTemplateKind) -> super::RuntimeTemplate {
        runtime_templates()
            .into_iter()
            .find(|template| template.kind == kind)
            .expect("template exists")
    }
}
