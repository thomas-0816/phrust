//! Opt-in metadata-only region profile reports for framework-like traces.

use std::collections::{BTreeMap, BTreeSet};

use php_ir::{IrSourceMapTarget, IrUnit};

use crate::VmCounters;

const MAX_TRACES: usize = 7;
const MAX_CALLSITE_IDS: usize = 64;
const MAX_FUNCTION_IDS: usize = 64;
const MAX_METHOD_IDS: usize = 64;
const MAX_RANGES: usize = 64;
const MAX_MAP_ENTRIES: usize = 64;

const REGION_KINDS: [&str; MAX_TRACES] = [
    "router_dispatch",
    "middleware_service_chain",
    "container_lookup",
    "template_render",
    "json_response",
    "dto_orm_hydration",
    "array_config_traversal",
];

/// Metadata-only report generated after an opt-in VM run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionProfile {
    pub schema_version: u64,
    pub metadata_only: bool,
    pub source_hash: String,
    pub trace_count: u64,
    pub privacy: PrivacyPolicy,
    pub summary: BTreeMap<String, u64>,
    pub traces: Vec<RegionTrace>,
}

/// Privacy constraints applied to every trace record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivacyPolicy {
    pub no_userland_values: bool,
    pub no_raw_source_paths: bool,
    pub no_raw_strings: bool,
    pub stable_ids_only: bool,
}

/// One bounded framework-like region trace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionTrace {
    pub region_id: String,
    pub kind: String,
    pub stable_callsite_ids: Vec<String>,
    pub function_ids: Vec<u32>,
    pub method_ids: Vec<u32>,
    pub bytecode_ranges: Vec<BytecodeRange>,
    pub ic_states: BTreeMap<String, u64>,
    pub branch_bias: BranchBias,
    pub array_shapes: BTreeMap<String, u64>,
    pub object_shapes: BTreeMap<String, u64>,
    pub reference_cow_poison_events: BTreeMap<String, u64>,
    pub include_autoload_events: BTreeMap<String, u64>,
    pub control_flow_rejection_reasons: Vec<String>,
    pub candidate: RegionCandidate,
}

/// Dense bytecode/source-map range captured without source text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BytecodeRange {
    pub function_id: u32,
    pub block_id: u32,
    pub instruction_start: u32,
    pub instruction_end: u32,
    pub source_start: u32,
    pub source_end: u32,
}

/// Branch-bias metadata for a region.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchBias {
    pub state: String,
    pub conditional_branches: u64,
    pub guard_failures: u64,
}

/// Advisory candidate classification for future compilers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionCandidate {
    pub class: String,
    pub reasons: Vec<String>,
}

impl RegionProfile {
    /// Builds a bounded advisory profile from VM counters and IR metadata.
    ///
    /// The report intentionally excludes source text, raw paths, function names,
    /// class names, method names, property names, and userland values.
    #[must_use]
    pub fn from_unit_and_counters(unit: &IrUnit, counters: &VmCounters, source_id: &str) -> Self {
        let metadata = UnitRegionMetadata::from_unit(unit, source_id);
        let mut traces = Vec::with_capacity(MAX_TRACES);
        for kind in REGION_KINDS {
            traces.push(RegionTrace::from_parts(kind, &metadata, counters));
        }

        Self {
            schema_version: 1,
            metadata_only: true,
            source_hash: metadata.source_hash,
            trace_count: traces.len() as u64,
            privacy: PrivacyPolicy {
                no_userland_values: true,
                no_raw_source_paths: true,
                no_raw_strings: true,
                stable_ids_only: true,
            },
            summary: classification_summary(&traces),
            traces,
        }
    }

    /// Serializes the report as deterministic JSON.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut json = String::new();
        json.push_str("{\n");
        json.push_str("  \"schema_version\": ");
        json.push_str(&self.schema_version.to_string());
        json.push_str(",\n  \"metadata_only\": ");
        json.push_str(bool_json(self.metadata_only));
        json.push_str(",\n  \"source_hash\": ");
        json.push_str(&quoted(&self.source_hash));
        json.push_str(",\n  \"trace_count\": ");
        json.push_str(&self.trace_count.to_string());
        json.push_str(",\n  \"privacy\": ");
        json.push_str(&self.privacy.to_json());
        json.push_str(",\n  \"summary\": ");
        json.push_str(&string_u64_map_json(&self.summary));
        json.push_str(",\n  \"traces\": [");
        if !self.traces.is_empty() {
            json.push('\n');
        }
        for (index, trace) in self.traces.iter().enumerate() {
            if index > 0 {
                json.push_str(",\n");
            }
            json.push_str(&trace.to_json());
        }
        if !self.traces.is_empty() {
            json.push('\n');
        }
        json.push_str("  ]\n}\n");
        json
    }
}

impl PrivacyPolicy {
    fn to_json(&self) -> String {
        format!(
            "{{\"no_userland_values\":{},\"no_raw_source_paths\":{},\"no_raw_strings\":{},\"stable_ids_only\":{}}}",
            bool_json(self.no_userland_values),
            bool_json(self.no_raw_source_paths),
            bool_json(self.no_raw_strings),
            bool_json(self.stable_ids_only)
        )
    }
}

impl RegionTrace {
    fn from_parts(kind: &str, metadata: &UnitRegionMetadata, counters: &VmCounters) -> Self {
        let candidate = classify_region(kind, counters);
        Self {
            region_id: format!("region:{}", stable_hash_label(kind)),
            kind: kind.to_owned(),
            stable_callsite_ids: region_callsite_ids(kind, metadata),
            function_ids: metadata.function_ids.clone(),
            method_ids: method_ids(counters),
            bytecode_ranges: metadata.bytecode_ranges.clone(),
            ic_states: ic_states(counters),
            branch_bias: branch_bias(counters),
            array_shapes: array_shapes(counters),
            object_shapes: object_shapes(counters),
            reference_cow_poison_events: reference_cow_poison_events(counters),
            include_autoload_events: include_autoload_events(counters),
            control_flow_rejection_reasons: control_flow_rejection_reasons(unit_has_generator(
                &metadata.function_flags,
            )),
            candidate,
        }
    }

    fn to_json(&self) -> String {
        let mut json = String::new();
        json.push_str("    {\n");
        json.push_str("      \"region_id\": ");
        json.push_str(&quoted(&self.region_id));
        json.push_str(",\n      \"kind\": ");
        json.push_str(&quoted(&self.kind));
        json.push_str(",\n      \"stable_callsite_ids\": ");
        json.push_str(&string_array_json(&self.stable_callsite_ids));
        json.push_str(",\n      \"function_ids\": ");
        json.push_str(&u32_array_json(&self.function_ids));
        json.push_str(",\n      \"method_ids\": ");
        json.push_str(&u32_array_json(&self.method_ids));
        json.push_str(",\n      \"bytecode_ranges\": ");
        json.push_str(&bytecode_ranges_json(&self.bytecode_ranges));
        json.push_str(",\n      \"ic_states\": ");
        json.push_str(&string_u64_map_json(&self.ic_states));
        json.push_str(",\n      \"branch_bias\": ");
        json.push_str(&self.branch_bias.to_json());
        json.push_str(",\n      \"array_shapes\": ");
        json.push_str(&string_u64_map_json(&self.array_shapes));
        json.push_str(",\n      \"object_shapes\": ");
        json.push_str(&string_u64_map_json(&self.object_shapes));
        json.push_str(",\n      \"reference_cow_poison_events\": ");
        json.push_str(&string_u64_map_json(&self.reference_cow_poison_events));
        json.push_str(",\n      \"include_autoload_events\": ");
        json.push_str(&string_u64_map_json(&self.include_autoload_events));
        json.push_str(",\n      \"control_flow_rejection_reasons\": ");
        json.push_str(&string_array_json(&self.control_flow_rejection_reasons));
        json.push_str(",\n      \"candidate\": ");
        json.push_str(&self.candidate.to_json());
        json.push_str("\n    }");
        json
    }
}

impl BranchBias {
    fn to_json(&self) -> String {
        format!(
            "{{\"state\":{},\"conditional_branches\":{},\"guard_failures\":{}}}",
            quoted(&self.state),
            self.conditional_branches,
            self.guard_failures
        )
    }
}

impl RegionCandidate {
    fn to_json(&self) -> String {
        format!(
            "{{\"class\":{},\"reasons\":{}}}",
            quoted(&self.class),
            string_array_json(&self.reasons)
        )
    }
}

#[derive(Clone, Debug)]
struct UnitRegionMetadata {
    source_hash: String,
    function_ids: Vec<u32>,
    function_flags: Vec<FunctionFlagSummary>,
    callsite_ids: Vec<String>,
    bytecode_ranges: Vec<BytecodeRange>,
}

#[derive(Clone, Debug)]
struct FunctionFlagSummary {
    is_generator: bool,
}

impl UnitRegionMetadata {
    fn from_unit(unit: &IrUnit, source_id: &str) -> Self {
        let mut function_ids = Vec::new();
        let mut function_flags = Vec::new();
        let mut bytecode_ranges = Vec::new();

        for (function_index, function) in unit.functions.iter().enumerate() {
            if function_ids.len() < MAX_FUNCTION_IDS {
                function_ids.push(function_index as u32);
            }
            function_flags.push(FunctionFlagSummary {
                is_generator: function.flags.is_generator,
            });
            for (block_index, block) in function.blocks.iter().enumerate() {
                if bytecode_ranges.len() >= MAX_RANGES {
                    break;
                }
                let instruction_start = block
                    .instructions
                    .first()
                    .map_or(0, |instruction| instruction.id.raw());
                let instruction_end = block
                    .instructions
                    .last()
                    .map_or(instruction_start, |instruction| instruction.id.raw());
                let source_start = block
                    .instructions
                    .first()
                    .map_or(function.span.start, |instruction| instruction.span.start);
                let source_end = block
                    .terminator
                    .as_ref()
                    .map_or_else(
                        || {
                            block
                                .instructions
                                .last()
                                .map_or(function.span.end, |instruction| instruction.span.end)
                        },
                        |terminator| terminator.span.end,
                    )
                    .max(source_start);
                bytecode_ranges.push(BytecodeRange {
                    function_id: function_index as u32,
                    block_id: block_index as u32,
                    instruction_start,
                    instruction_end,
                    source_start,
                    source_end,
                });
            }
        }

        let mut callsite_ids = Vec::new();
        for entry in unit.source_map.entries() {
            if callsite_ids.len() >= MAX_CALLSITE_IDS {
                break;
            }
            let target = match &entry.target {
                IrSourceMapTarget::Instruction {
                    function,
                    block,
                    instruction,
                } => format!(
                    "f{}:b{}:i{}",
                    function.raw(),
                    block.raw(),
                    instruction.raw()
                ),
                IrSourceMapTarget::Terminator { function, block } => {
                    format!("f{}:b{}:term", function.raw(), block.raw())
                }
                IrSourceMapTarget::Block { function, block } => {
                    format!("f{}:b{}", function.raw(), block.raw())
                }
                IrSourceMapTarget::Function { function } => format!("f{}", function.raw()),
            };
            callsite_ids.push(stable_hash_label(&target));
        }
        if callsite_ids.is_empty() {
            for range in &bytecode_ranges {
                if callsite_ids.len() >= MAX_CALLSITE_IDS {
                    break;
                }
                callsite_ids.push(stable_hash_label(&format!(
                    "f{}:b{}:{}..{}",
                    range.function_id,
                    range.block_id,
                    range.instruction_start,
                    range.instruction_end
                )));
            }
        }

        Self {
            source_hash: stable_hash_label(source_id),
            function_ids,
            function_flags,
            callsite_ids,
            bytecode_ranges,
        }
    }
}

fn region_callsite_ids(kind: &str, metadata: &UnitRegionMetadata) -> Vec<String> {
    let mut ids = Vec::with_capacity(metadata.callsite_ids.len().min(MAX_CALLSITE_IDS));
    ids.push(stable_hash_label(&format!("region-kind:{kind}")));
    ids.extend(
        metadata
            .callsite_ids
            .iter()
            .take(MAX_CALLSITE_IDS - 1)
            .cloned(),
    );
    ids
}

fn method_ids(counters: &VmCounters) -> Vec<u32> {
    let mut ids = BTreeSet::new();
    for profile in counters.method_call_profiles.values() {
        ids.extend(profile.method_ids.iter().copied());
    }
    ids.into_iter().take(MAX_METHOD_IDS).collect()
}

fn ic_states(counters: &VmCounters) -> BTreeMap<String, u64> {
    let mut values = BTreeMap::new();
    insert_nonzero(&mut values, "inline_cache.hits", counters.inline_cache_hits);
    insert_nonzero(
        &mut values,
        "inline_cache.misses",
        counters.inline_cache_misses,
    );
    insert_nonzero(
        &mut values,
        "inline_cache.monomorphic",
        counters.inline_cache_monomorphic,
    );
    insert_nonzero(
        &mut values,
        "inline_cache.polymorphic",
        counters.inline_cache_polymorphic,
    );
    insert_nonzero(
        &mut values,
        "inline_cache.megamorphic",
        counters.inline_cache_megamorphic,
    );
    insert_nonzero(
        &mut values,
        "function_call_ic.hits",
        counters.function_call_ic_hits,
    );
    insert_nonzero(
        &mut values,
        "function_call_ic.misses",
        counters.function_call_ic_misses,
    );
    insert_nonzero(&mut values, "method_ic.hits", counters.method_ic_hits);
    insert_nonzero(&mut values, "method_ic.misses", counters.method_ic_misses);
    insert_nonzero(&mut values, "property_ic.hits", counters.property_ic_hits);
    insert_nonzero(
        &mut values,
        "property_assign_ic.hits",
        counters.property_assign_ic_hits,
    );
    insert_nonzero(
        &mut values,
        "include_path_ic.hits",
        counters.include_path_ic_hits,
    );
    insert_nonzero(
        &mut values,
        "autoload_class_lookup_ic.hits",
        counters.autoload_class_lookup_ic_hits,
    );
    values
}

fn branch_bias(counters: &VmCounters) -> BranchBias {
    let conditional_branches = counters
        .opcodes
        .iter()
        .filter(|(opcode, _)| opcode.contains("jump") || opcode.contains("branch"))
        .map(|(_, count)| *count)
        .sum();
    let guard_failures = counters.guard_failures
        + counters.inline_cache_guard_failures
        + counters.jit_guard_failures
        + counters.quickening_guard_failures;
    BranchBias {
        state: if conditional_branches == 0 {
            "not_observed".to_owned()
        } else {
            "unknown_bias_metadata_only".to_owned()
        },
        conditional_branches,
        guard_failures,
    }
}

fn array_shapes(counters: &VmCounters) -> BTreeMap<String, u64> {
    let mut shapes = bounded_map(&counters.array_shape_observed_by_kind);
    if shapes.is_empty() && counters.array_dim_fetches > 0 {
        shapes.insert("unknown_array_shape".to_owned(), counters.array_dim_fetches);
    }
    shapes
}

fn object_shapes(counters: &VmCounters) -> BTreeMap<String, u64> {
    let mut shapes = BTreeMap::new();
    for profile in counters.property_fetch_profiles.values() {
        for class_id in &profile.class_ids {
            insert_nonzero(
                &mut shapes,
                &format!("class_id:{class_id}"),
                profile.observations,
            );
        }
    }
    for profile in counters.method_call_profiles.values() {
        for class_id in &profile.class_ids {
            insert_nonzero(
                &mut shapes,
                &format!("class_id:{class_id}"),
                profile.observations,
            );
        }
    }
    bounded_map(&shapes)
}

fn reference_cow_poison_events(counters: &VmCounters) -> BTreeMap<String, u64> {
    let mut values = BTreeMap::new();
    insert_nonzero(&mut values, "cow_separations", counters.cow_separations);
    insert_nonzero(
        &mut values,
        "reference_cell_creations",
        counters.reference_cell_creations,
    );
    insert_nonzero(
        &mut values,
        "cow_or_reference_fallbacks",
        counters.cow_or_reference_fallbacks,
    );
    insert_nonzero(
        &mut values,
        "fast_path_disabled_by_reference",
        counters.fast_path_disabled_by_reference,
    );
    insert_nonzero(
        &mut values,
        "dequickened_by_reference",
        counters.dequickened_by_reference,
    );
    insert_nonzero(
        &mut values,
        "ic_invalidated_by_reference",
        counters.ic_invalidated_by_reference,
    );
    insert_nonzero(
        &mut values,
        "dense_bytecode_fallback_by_reference",
        counters.dense_bytecode_fallback_by_reference,
    );
    values
}

fn include_autoload_events(counters: &VmCounters) -> BTreeMap<String, u64> {
    let mut values = BTreeMap::new();
    insert_nonzero(&mut values, "includes", counters.includes);
    insert_nonzero(&mut values, "autoloads", counters.autoloads);
    insert_nonzero(
        &mut values,
        "include_graph_hits",
        counters.include_graph_hits,
    );
    insert_nonzero(
        &mut values,
        "include_graph_misses",
        counters.include_graph_misses,
    );
    insert_nonzero(
        &mut values,
        "autoload_graph_hits",
        counters.autoload_graph_hits,
    );
    insert_nonzero(
        &mut values,
        "autoload_graph_misses",
        counters.autoload_graph_misses,
    );
    insert_nonzero(
        &mut values,
        "negative_lookup_hits",
        counters.negative_lookup_hits,
    );
    for (reason, count) in &counters.fallback_by_path_semantics {
        insert_nonzero(&mut values, &format!("fallback.{reason}"), *count);
    }
    bounded_map(&values)
}

fn control_flow_rejection_reasons(has_generator: bool) -> Vec<String> {
    let mut reasons = vec![
        "exception_resume_state_not_recorded".to_owned(),
        "try_finally_resume_state_not_recorded".to_owned(),
        "fiber_resume_state_not_recorded".to_owned(),
    ];
    if has_generator {
        reasons.push("generator_resume_state_rejected".to_owned());
    } else {
        reasons.push("generator_resume_state_not_observed".to_owned());
    }
    reasons
}

fn classify_region(kind: &str, counters: &VmCounters) -> RegionCandidate {
    match kind {
        "router_dispatch" | "middleware_service_chain" | "container_lookup" => {
            if counters.inline_cache_hits
                + counters.function_call_ic_hits
                + counters.method_ic_hits
                + counters.property_ic_hits
                > 0
            {
                candidate("inline-cache-only", ["stable_ic_feedback_present"])
            } else {
                candidate("unsupported", ["callsite_ic_feedback_absent"])
            }
        }
        "template_render" => {
            if counters.superinstructions_executed.values().sum::<u64>()
                + counters.output_fast_appends
                + counters.string_concat_fast_path_hits
                + counters.concat_prealloc_hits
                > 0
            {
                candidate(
                    "superinstruction-candidate",
                    ["string_output_concat_metadata_present"],
                )
            } else {
                candidate("unsupported", ["template_output_metadata_absent"])
            }
        }
        "json_response" => {
            if counters.internal_function_dispatches
                + counters.builtin_call_ic_hits
                + counters.builtin_intrinsic_candidates
                + counters.output_bytes
                > 0
            {
                candidate(
                    "baseline-native-candidate",
                    ["builtin_output_metadata_present"],
                )
            } else {
                candidate("unsupported", ["json_builtin_metadata_absent"])
            }
        }
        "dto_orm_hydration" => {
            if counters.property_ic_hits
                + counters.property_assign_ic_hits
                + counters.method_ic_hits
                + counters.object_allocations
                > 0
            {
                candidate("inline-cache-only", ["object_shape_ic_metadata_present"])
            } else {
                candidate("unsupported", ["object_shape_metadata_absent"])
            }
        }
        "array_config_traversal" => {
            if counters
                .array_fast_path_hits_by_family
                .values()
                .sum::<u64>()
                > 0
                && counters.cow_or_reference_fallbacks == 0
            {
                candidate(
                    "Cranelift-packed-numeric-candidate",
                    ["array_fast_path_metadata_without_reference_poison"],
                )
            } else if counters.array_dim_fetches > 0 {
                candidate(
                    "inline-cache-only",
                    ["array_metadata_present_with_generic_guards"],
                )
            } else {
                candidate("unsupported", ["array_shape_metadata_absent"])
            }
        }
        _ => candidate("unsupported", ["unknown_region_kind"]),
    }
}

fn candidate<const N: usize>(class: &str, reasons: [&str; N]) -> RegionCandidate {
    RegionCandidate {
        class: class.to_owned(),
        reasons: reasons.into_iter().map(str::to_owned).collect(),
    }
}

fn classification_summary(traces: &[RegionTrace]) -> BTreeMap<String, u64> {
    let mut summary = BTreeMap::new();
    for trace in traces {
        *summary.entry(trace.candidate.class.clone()).or_default() += 1;
    }
    summary
}

fn unit_has_generator(flags: &[FunctionFlagSummary]) -> bool {
    flags.iter().any(|flag| flag.is_generator)
}

fn insert_nonzero(values: &mut BTreeMap<String, u64>, key: &str, value: u64) {
    if value > 0 {
        values.insert(key.to_owned(), value);
    }
}

fn bounded_map(values: &BTreeMap<String, u64>) -> BTreeMap<String, u64> {
    values
        .iter()
        .take(MAX_MAP_ENTRIES)
        .map(|(key, value)| (key.clone(), *value))
        .collect()
}

fn bytecode_ranges_json(ranges: &[BytecodeRange]) -> String {
    let mut json = String::from("[");
    for (index, range) in ranges.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push('{');
        json.push_str("\"function_id\":");
        json.push_str(&range.function_id.to_string());
        json.push_str(",\"block_id\":");
        json.push_str(&range.block_id.to_string());
        json.push_str(",\"instruction_start\":");
        json.push_str(&range.instruction_start.to_string());
        json.push_str(",\"instruction_end\":");
        json.push_str(&range.instruction_end.to_string());
        json.push_str(",\"source_start\":");
        json.push_str(&range.source_start.to_string());
        json.push_str(",\"source_end\":");
        json.push_str(&range.source_end.to_string());
        json.push('}');
    }
    json.push(']');
    json
}

fn string_u64_map_json(values: &BTreeMap<String, u64>) -> String {
    let mut json = String::from("{");
    for (index, (key, value)) in values.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&quoted(key));
        json.push(':');
        json.push_str(&value.to_string());
    }
    json.push('}');
    json
}

fn string_array_json(values: &[String]) -> String {
    let mut json = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&quoted(value));
    }
    json.push(']');
    json
}

fn u32_array_json(values: &[u32]) -> String {
    let mut json = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&value.to_string());
    }
    json.push(']');
    json
}

fn quoted(value: &str) -> String {
    let mut json = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            '\n' => json.push_str("\\n"),
            '\r' => json.push_str("\\r"),
            '\t' => json.push_str("\\t"),
            c if c.is_control() => {
                json.push_str("\\u");
                json.push_str(&format!("{:04x}", c as u32));
            }
            c => json.push(c),
        }
    }
    json.push('"');
    json
}

fn bool_json(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn stable_hash_label(value: &str) -> String {
    format!("fnv1a64:{:016x}", stable_hash(value.as_bytes()))
}

fn stable_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use php_ir::{FunctionFlags, IrBuilder, IrConstant, IrSpan, Operand, UnitId};

    use super::RegionProfile;
    use crate::VmCounters;

    fn sample_unit() -> php_ir::IrUnit {
        let mut builder = IrBuilder::new(UnitId::new(0));
        let file = builder.add_file("tests/fixtures/private-source.php");
        let function = builder.start_function(
            "private_function_name",
            FunctionFlags::default(),
            IrSpan::new(file, 0, 10),
        );
        let block = builder.append_block(function);
        let constant = builder.add_constant(IrConstant::Int(1));
        let register = builder.alloc_register(function);
        let instruction =
            builder.emit_load_const(function, block, register, constant, IrSpan::new(file, 1, 2));
        builder.add_source_map(
            php_ir::IrSourceMapTarget::Instruction {
                function,
                block,
                instruction,
            },
            "hir:expr:0",
            IrSpan::new(file, 1, 2),
        );
        builder.terminate_return(
            function,
            block,
            Some(Operand::Register(register)),
            IrSpan::new(file, 2, 3),
        );
        builder.set_entry(function);
        builder.finish()
    }

    #[test]
    fn region_profile_json_is_metadata_only_and_bounded() {
        let unit = sample_unit();
        let mut counters = VmCounters {
            inline_cache_hits: 4,
            function_call_ic_hits: 2,
            method_ic_hits: 1,
            property_ic_hits: 3,
            output_fast_appends: 5,
            string_concat_fast_path_hits: 6,
            concat_prealloc_hits: 7,
            internal_function_dispatches: 8,
            builtin_intrinsic_candidates: 1,
            output_bytes: 12,
            array_dim_fetches: 9,
            object_allocations: 2,
            ..VmCounters::default()
        };
        counters
            .array_fast_path_hits_by_family
            .insert("packed_int_fetch".to_owned(), 3);
        counters
            .array_shape_observed_by_kind
            .insert("packed".to_owned(), 2);

        let profile = RegionProfile::from_unit_and_counters(
            &unit,
            &counters,
            "/private/path/request.php?secret=token",
        );
        let json = profile.to_json();

        assert!(profile.metadata_only);
        assert_eq!(profile.traces.len(), 7);
        assert!(json.contains("\"router_dispatch\""));
        assert!(json.contains("\"array_config_traversal\""));
        assert!(json.contains("\"stable_callsite_ids\""));
        assert!(json.contains("\"bytecode_ranges\""));
        assert!(json.contains("\"Cranelift-packed-numeric-candidate\""));
        assert!(!json.contains("private_function_name"));
        assert!(!json.contains("private-source.php"));
        assert!(!json.contains("secret"));
        assert!(!json.contains("token"));
    }

    #[test]
    fn region_profile_classifies_candidate_regions() {
        let unit = sample_unit();
        let counters = VmCounters {
            inline_cache_hits: 1,
            output_fast_appends: 1,
            internal_function_dispatches: 1,
            property_ic_hits: 1,
            array_dim_fetches: 1,
            ..VmCounters::default()
        };
        let profile = RegionProfile::from_unit_and_counters(&unit, &counters, "fixture.php");

        assert_eq!(
            profile
                .summary
                .get("inline-cache-only")
                .copied()
                .unwrap_or(0),
            5
        );
        assert_eq!(
            profile
                .summary
                .get("superinstruction-candidate")
                .copied()
                .unwrap_or(0),
            1
        );
        assert_eq!(
            profile
                .summary
                .get("baseline-native-candidate")
                .copied()
                .unwrap_or(0),
            1
        );
        assert_eq!(profile.summary.get("unsupported").copied().unwrap_or(0), 0);
    }
}
