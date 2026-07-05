use crate::perf_trace::PerfTraceEvent;
use php_vm::api::{BoundaryProfile, OperationProfile, VmCounters};
use serde_json::{Map, Value};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

const SCHEMA_VERSION: u64 = 1;

#[derive(Debug)]
pub(crate) struct RequestProfileWriter {
    dir: PathBuf,
}

impl RequestProfileWriter {
    pub(crate) fn open(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn write(
        &self,
        trace: &PerfTraceEvent,
        counters: Option<&VmCounters>,
    ) -> io::Result<PathBuf> {
        let filename = format!("{}.json", profile_file_stem(&trace.request_id));
        let path = self.dir.join(filename);
        let tmp_path = path.with_extension("json.tmp");
        let mut file = File::create(&tmp_path)?;
        serde_json::to_writer_pretty(&mut file, &request_profile_json(trace, counters))?;
        file.write_all(b"\n")?;
        file.flush()?;
        fs::rename(&tmp_path, &path)?;
        Ok(path)
    }
}

fn profile_file_stem(request_id: &str) -> String {
    let mut stem = String::new();
    for ch in request_id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            stem.push(ch);
        } else {
            stem.push('_');
        }
    }
    if stem.is_empty() {
        "request".to_string()
    } else {
        stem
    }
}

fn request_profile_json(trace: &PerfTraceEvent, counters: Option<&VmCounters>) -> Value {
    let mut root = Map::new();
    root.insert("schema_version".to_string(), Value::from(SCHEMA_VERSION));
    root.insert("request".to_string(), request_json(trace));
    root.insert("phases_nanos".to_string(), phases_json(trace));
    root.insert("attribution".to_string(), attribution_json(trace, counters));
    Value::Object(root)
}

fn request_json(trace: &PerfTraceEvent) -> Value {
    let mut request = Map::new();
    request.insert(
        "request_id".to_string(),
        Value::from(trace.request_id.clone()),
    );
    request.insert("method".to_string(), Value::from(trace.method.clone()));
    request.insert("path".to_string(), Value::from(trace.path.clone()));
    request.insert(
        "script_path".to_string(),
        Value::from(trace.script_path.clone()),
    );
    request.insert("status".to_string(), Value::from(trace.status));
    if let Some(cache_hit) = trace.cache_hit {
        request.insert("script_cache_hit".to_string(), Value::from(cache_hit));
    }
    if let Some(failure_phase) = trace.failure_phase {
        request.insert("failure_phase".to_string(), Value::from(failure_phase));
    }
    request.insert("body_bytes".to_string(), Value::from(trace.body_bytes));
    request.insert(
        "response_bytes".to_string(),
        Value::from(trace.response_bytes),
    );
    request.insert(
        "runtime_diagnostics".to_string(),
        Value::from(trace.runtime_diagnostics),
    );
    Value::Object(request)
}

fn phases_json(trace: &PerfTraceEvent) -> Value {
    let mut phases = Map::new();
    for (name, nanos) in &trace.phases {
        phases.insert(
            (*name).to_string(),
            Value::from((*nanos).min(u64::MAX as u128) as u64),
        );
    }
    Value::Object(phases)
}

fn attribution_json(trace: &PerfTraceEvent, counters: Option<&VmCounters>) -> Value {
    let mut attribution = Map::new();
    attribution.insert("summary_counters".to_string(), summary_counters_json(trace));
    let Some(counters) = counters else {
        attribution.insert("vm_counters_collected".to_string(), Value::from(false));
        return Value::Object(attribution);
    };
    attribution.insert("vm_counters_collected".to_string(), Value::from(true));
    attribution.insert("execution".to_string(), execution_json(counters));
    attribution.insert("includes".to_string(), includes_json(counters));
    attribution.insert("calls".to_string(), calls_json(counters));
    attribution.insert("arrays".to_string(), arrays_json(counters));
    attribution.insert("objects".to_string(), objects_json(counters));
    attribution.insert("clones".to_string(), clones_json(counters));
    attribution.insert("output".to_string(), output_json(counters));
    attribution.insert("metadata".to_string(), metadata_json(counters));
    attribution.insert("native".to_string(), native_json(counters));
    Value::Object(attribution)
}

fn summary_counters_json(trace: &PerfTraceEvent) -> Value {
    let mut counters = Map::new();
    for (name, value) in &trace.counters {
        counters.insert((*name).to_string(), Value::from(*value));
    }
    Value::Object(counters)
}

fn execution_json(counters: &VmCounters) -> Value {
    object_from_pairs([
        ("rich_instructions", counters.instructions_executed),
        (
            "dense_bytecode_instructions",
            counters.bytecode_instructions_executed,
        ),
        (
            "entry_rich_instructions",
            counters.entry_rich_instructions_executed,
        ),
        (
            "include_rich_instructions",
            counters.include_rich_instructions_executed,
        ),
        (
            "entry_bytecode_instructions",
            counters.entry_bytecode_instructions_executed,
        ),
        (
            "include_bytecode_instructions",
            counters.include_bytecode_instructions_executed,
        ),
        (
            "dense_functions_executed",
            counters.dense_functions_executed,
        ),
        (
            "rich_fallback_functions_executed",
            counters.rich_fallback_functions_executed,
        ),
    ])
    .with_map(
        "opcodes",
        map_to_json(&counters.opcodes, SortDirection::Descending),
    )
    .with_map(
        "bytecode_executed_by_family",
        map_to_json(
            &counters.bytecode_executed_by_family,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "dense_instruction_families",
        map_to_json(
            &counters.dense_instruction_families_executed,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "rich_fallback_functions_by_name",
        map_to_json(
            &counters.rich_fallback_functions_by_name,
            SortDirection::Descending,
        ),
    )
    .into()
}

fn includes_json(counters: &VmCounters) -> Value {
    object_from_pairs([
        ("includes", counters.includes),
        ("autoloads", counters.autoloads),
        ("include_once_skips", counters.include_once_skips),
        ("include_resolution_hits", counters.include_resolution_hits),
        (
            "include_resolution_misses",
            counters.include_resolution_misses,
        ),
        ("include_compile_hits", counters.include_compile_hits),
        ("include_compile_misses", counters.include_compile_misses),
        (
            "dense_include_entry_attempts",
            counters.dense_include_entry_attempts,
        ),
        (
            "dense_include_entry_successes",
            counters.dense_include_entry_successes,
        ),
        (
            "dense_include_entry_fallbacks",
            counters.dense_include_entry_fallbacks,
        ),
    ])
    .with_map(
        "dense_include_entry_fallback_by_reason",
        map_to_json(
            &counters.dense_include_entry_fallback_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "dense_include_entry_fallback_by_path",
        map_to_json(
            &counters.dense_include_entry_fallback_by_path,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "include_fallback_by_reason",
        map_to_json(
            &counters.include_fallback_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "include_stale_invalidation_by_reason",
        map_to_json(
            &counters.include_stale_invalidation_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "include_profiles_by_path",
        boundary_profiles_to_json(&counters.include_profiles_by_path),
    )
    .into()
}

fn calls_json(counters: &VmCounters) -> Value {
    object_from_pairs([
        (
            "function_calls",
            counters.function_calls
                + counters.dense_direct_call_hits
                + counters.dense_callable_call_hits,
        ),
        (
            "method_calls",
            counters.method_calls
                + counters.dense_method_call_hits
                + counters.dense_static_call_hits,
        ),
        (
            "internal_function_dispatches",
            counters.internal_function_dispatches,
        ),
        (
            "internal_function_dispatch_cache_hits",
            counters.internal_function_dispatch_cache_hits,
        ),
        (
            "internal_function_dispatch_cache_misses",
            counters.internal_function_dispatch_cache_misses,
        ),
        ("frame_allocations", counters.frame_allocations),
        ("frame_reuses", counters.frame_reuses),
        ("frames_allocated", counters.frames_allocated),
        ("frames_reused", counters.frames_reused),
        (
            "register_files_allocated",
            counters.register_files_allocated,
        ),
        ("register_files_reused", counters.register_files_reused),
        ("tiny_frame_candidates", counters.tiny_frame_candidates),
        ("specialized_frame_hits", counters.specialized_frame_hits),
        ("arg_array_avoided", counters.arg_array_avoided),
        ("heap_frame_avoided", counters.heap_frame_avoided),
        ("direct_arg_frame_hits", counters.direct_arg_frame_hits),
        (
            "direct_method_frame_hits",
            counters.direct_method_frame_hits,
        ),
        (
            "direct_closure_frame_hits",
            counters.direct_closure_frame_hits,
        ),
        (
            "direct_constructor_frame_hits",
            counters.direct_constructor_frame_hits,
        ),
        (
            "argument_vector_allocations_avoided",
            counters.argument_vector_allocations_avoided,
        ),
        ("function_call_ic_hits", counters.function_call_ic_hits),
        ("function_call_ic_misses", counters.function_call_ic_misses),
        ("method_ic_hits", counters.method_ic_hits),
        ("method_ic_misses", counters.method_ic_misses),
        ("builtin_call_ic_hits", counters.builtin_call_ic_hits),
        ("builtin_call_ic_misses", counters.builtin_call_ic_misses),
        ("dense_direct_call_hits", counters.dense_direct_call_hits),
        ("dense_method_call_hits", counters.dense_method_call_hits),
        ("dense_static_call_hits", counters.dense_static_call_hits),
        ("dense_call_ic_hits", counters.dense_call_ic_hits),
        ("dense_call_ic_misses", counters.dense_call_ic_misses),
    ])
    .with_map(
        "dense_call_fallback_by_reason",
        map_to_json(
            &counters.dense_call_fallback_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "dense_function_fallback_by_reason",
        map_to_json(
            &counters.dense_function_fallback_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "dense_method_dispatch_fallback_by_reason",
        map_to_json(
            &counters.dense_method_dispatch_fallback_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "frame_reuse_blocked_by_reason",
        map_to_json(
            &counters.frame_reuse_blocked_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "call_frame_layout_observed",
        map_to_json(
            &counters.call_frame_layout_observed,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "generic_frame_fallback_by_reason",
        map_to_json(
            &counters.generic_frame_fallback_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "direct_frame_fallback_by_reason",
        map_to_json(
            &counters.direct_frame_fallback_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "builtin_fast_stub_hits",
        map_to_json(&counters.builtin_fast_stub_hits, SortDirection::Descending),
    )
    .with_map(
        "builtin_fast_stub_misses",
        map_to_json(
            &counters.builtin_fast_stub_misses,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "builtin_fast_stub_fallback_by_reason",
        map_to_json(
            &counters.builtin_fast_stub_fallback_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "intrinsic_hits",
        map_to_json(&counters.intrinsic_hits, SortDirection::Descending),
    )
    .with_map(
        "intrinsic_misses",
        map_to_json(&counters.intrinsic_misses, SortDirection::Descending),
    )
    .with_map(
        "function_profiles_by_name",
        boundary_profiles_to_json(&counters.function_profiles_by_name),
    )
    .with_map(
        "method_profiles_by_name",
        boundary_profiles_to_json(&counters.method_profiles_by_name),
    )
    .with_map(
        "builtin_profiles_by_name",
        boundary_profiles_to_json(&counters.builtin_profiles_by_name),
    )
    .into()
}

fn arrays_json(counters: &VmCounters) -> Value {
    object_from_pairs([
        ("array_dim_fetches", counters.array_dim_fetches),
        (
            "packed_dim_fast_path_hits",
            counters.packed_dim_fast_path_hits,
        ),
        (
            "packed_dim_fast_path_misses",
            counters.packed_dim_fast_path_misses,
        ),
        (
            "array_packed_append_fast_path_hits",
            counters.array_packed_append_fast_path_hits,
        ),
        (
            "array_packed_read_fast_path_hits",
            counters.array_packed_read_fast_path_hits,
        ),
        (
            "array_sequential_foreach_fast_path_hits",
            counters.array_sequential_foreach_fast_path_hits,
        ),
        ("record_shape_hits", counters.record_shape_hits),
        ("record_shape_misses", counters.record_shape_misses),
        ("small_map_hits", counters.small_map_hits),
        ("small_map_misses", counters.small_map_misses),
        ("packed_append_fast_hits", counters.packed_append_fast_hits),
        (
            "packed_foreach_fast_hits",
            counters.packed_foreach_fast_hits,
        ),
        (
            "array_count_fast_path_hits",
            counters.array_count_fast_path_hits,
        ),
        (
            "array_packed_direct_gets",
            counters.array_packed_direct_gets,
        ),
        (
            "array_mixed_indexed_gets",
            counters.array_mixed_indexed_gets,
        ),
        (
            "array_linear_scan_fallbacks",
            counters.array_linear_scan_fallbacks,
        ),
        (
            "array_metadata_recomputes",
            counters.array_metadata_recomputes,
        ),
        (
            "array_packed_to_mixed_transitions",
            counters.array_packed_to_mixed_transitions,
        ),
        (
            "numeric_string_classify_calls",
            counters.numeric_string_classify_calls,
        ),
        (
            "numeric_string_cache_hits",
            counters.numeric_string_cache_hits,
        ),
        (
            "numeric_string_cache_misses",
            counters.numeric_string_cache_misses,
        ),
    ])
    .with_map(
        "array_fast_path_hits_by_family",
        map_to_json(
            &counters.array_fast_path_hits_by_family,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "array_fast_path_fallback_by_reason",
        map_to_json(
            &counters.array_fast_path_fallback_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "array_shape_observed_by_kind",
        map_to_json(
            &counters.array_shape_observed_by_kind,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "packed_to_mixed_by_reason",
        map_to_json(
            &counters.packed_to_mixed_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "record_to_mixed_by_reason",
        map_to_json(
            &counters.record_to_mixed_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "foreach_clone_required_by_reason",
        map_to_json(
            &counters.foreach_clone_required_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "operation_profiles_by_family",
        operation_profiles_to_json(&counters.array_operation_profiles_by_family),
    )
    .into()
}

fn objects_json(counters: &VmCounters) -> Value {
    object_from_pairs([
        ("object_allocations", counters.object_allocations),
        ("property_fetches", counters.property_fetches),
        ("property_accesses", counters.property_accesses),
        ("property_ic_hits", counters.property_ic_hits),
        ("property_ic_misses", counters.property_ic_misses),
        (
            "property_ic_guard_failures",
            counters.property_ic_guard_failures,
        ),
        ("property_assign_ic_hits", counters.property_assign_ic_hits),
        (
            "property_assign_ic_misses",
            counters.property_assign_ic_misses,
        ),
        (
            "property_assign_ic_guard_failures",
            counters.property_assign_ic_guard_failures,
        ),
        (
            "object_declared_slot_reads",
            counters.object_declared_slot_reads,
        ),
        (
            "object_declared_slot_writes",
            counters.object_declared_slot_writes,
        ),
        (
            "object_dynamic_property_map_reads",
            counters.object_dynamic_property_map_reads,
        ),
        (
            "object_dynamic_property_map_writes",
            counters.object_dynamic_property_map_writes,
        ),
    ])
    .with_map(
        "property_ic_fallback_reasons",
        map_to_json(
            &counters.property_ic_fallback_reasons,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "property_assign_ic_fallback_reasons",
        map_to_json(
            &counters.property_assign_ic_fallback_reasons,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "property_fetch_profiles",
        Value::from(counters.property_fetch_profiles.len() as u64),
    )
    .with_map(
        "method_call_profiles",
        Value::from(counters.method_call_profiles.len() as u64),
    )
    .with_map(
        "operation_profiles_by_family",
        operation_profiles_to_json(&counters.object_operation_profiles_by_family),
    )
    .into()
}

fn clones_json(counters: &VmCounters) -> Value {
    object_from_pairs([
        ("value_clones", counters.value_clones),
        ("array_handle_clones", counters.array_handle_clones),
        ("cow_separations", counters.cow_separations),
        (
            "reference_cell_creations",
            counters.reference_cell_creations,
        ),
        (
            "by_ref_arg_value_materializations",
            counters.by_ref_arg_value_materializations,
        ),
        (
            "by_ref_arg_cow_separations",
            counters.by_ref_arg_cow_separations,
        ),
        (
            "by_ref_arg_cow_separations_avoided",
            counters.by_ref_arg_cow_separations_avoided,
        ),
    ])
    .with_map(
        "value_clone_by_reason",
        map_to_json(&counters.value_clone_by_reason, SortDirection::Descending),
    )
    .with_map(
        "value_clone_by_source_family",
        map_to_json(
            &counters.value_clone_by_source_family,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "array_handle_clone_by_source_family",
        map_to_json(
            &counters.array_handle_clone_by_source_family,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "cow_separation_by_source_family",
        map_to_json(
            &counters.cow_separation_by_source_family,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "reference_cell_creation_by_source_family",
        map_to_json(
            &counters.reference_cell_creation_by_source_family,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "by_ref_arg_fallback_by_reason",
        map_to_json(
            &counters.by_ref_arg_fallback_by_reason,
            SortDirection::Descending,
        ),
    )
    .into()
}

fn output_json(counters: &VmCounters) -> Value {
    object_from_pairs([
        ("output_bytes", counters.output_bytes),
        ("output_buffer_appends", counters.output_buffer_appends),
        (
            "output_buffer_batch_writes",
            counters.output_buffer_batch_writes,
        ),
        ("output_batched_appends", counters.output_batched_appends),
        ("output_batch_bytes", counters.output_batch_bytes),
        ("output_buffer_flushes", counters.output_buffer_flushes),
        ("output_fast_appends", counters.output_fast_appends),
    ])
    .with_map(
        "output_slow_appends_by_reason",
        map_to_json(
            &counters.output_slow_appends_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "operation_profiles_by_family",
        operation_profiles_to_json(&counters.output_operation_profiles_by_family),
    )
    .into()
}

fn metadata_json(counters: &VmCounters) -> Value {
    object_from_pairs([
        (
            "request_arena_allocations",
            counters.request_arena_allocations,
        ),
        ("request_arena_bytes", counters.request_arena_bytes),
        ("request_pool_resets", counters.request_pool_resets),
        (
            "persistent_engine_allocations",
            counters.persistent_engine_allocations,
        ),
        ("persistent_engine_bytes", counters.persistent_engine_bytes),
        (
            "destructor_sensitive_arena_blocks",
            counters.destructor_sensitive_arena_blocks,
        ),
        ("include_resolution_hits", counters.include_resolution_hits),
        (
            "include_resolution_misses",
            counters.include_resolution_misses,
        ),
        ("include_compile_hits", counters.include_compile_hits),
        ("include_compile_misses", counters.include_compile_misses),
        ("quickening_attempts", counters.quickening_attempts),
        ("quickening_specialized", counters.quickening_specialized),
        ("quickening_guard_hits", counters.quickening_guard_hits),
        ("quickening_guard_misses", counters.quickening_guard_misses),
        (
            "quickening_guard_failures",
            counters.quickening_guard_failures,
        ),
        (
            "quickening_fallback_calls",
            counters.quickening_fallback_calls,
        ),
        ("quickening_dequickens", counters.quickening_dequickens),
        ("quickening_megamorphic", counters.quickening_megamorphic),
        ("quickening_disabled", counters.quickening_disabled),
    ])
    .with_map(
        "arena_fallback_allocations_by_reason",
        map_to_json(
            &counters.arena_fallback_allocations_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "quickening_candidates_by_family",
        map_to_json(
            &counters.quickening_candidates_by_family,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "quickening_applied_by_family",
        map_to_json(
            &counters.quickening_applied_by_family,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "quickened_executions_by_family",
        map_to_json(
            &counters.quickened_executions_by_family,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "quickening_guard_failures_by_family",
        map_to_json(
            &counters.quickening_guard_failures_by_family,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "quickening_dequickened_by_reason",
        map_to_json(
            &counters.quickening_dequickened_by_reason,
            SortDirection::Descending,
        ),
    )
    .into()
}

fn native_json(counters: &VmCounters) -> Value {
    object_from_pairs([
        ("native_candidates", counters.native_candidates),
        ("native_compiled_regions", counters.native_compiled_regions),
        ("native_executions", counters.native_executions),
        (
            "native_compile_budget_rejections",
            counters.native_compile_budget_rejections,
        ),
        ("jit_compile_attempts", counters.jit_compile_attempts),
        ("jit_compiled", counters.jit_compiled),
        ("jit_executed", counters.jit_executed),
        ("jit_bailouts", counters.jit_bailouts),
        ("jit_side_exits", counters.jit_side_exits),
        ("jit_guard_failures", counters.jit_guard_failures),
        (
            "jit_tiering_budget_rejections",
            counters.jit_tiering_budget_rejections,
        ),
    ])
    .with_map(
        "native_eligibility_rejections_by_reason",
        map_to_json(
            &counters.native_eligibility_rejections_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "native_side_exits_by_reason",
        map_to_json(
            &counters.native_side_exits_by_reason,
            SortDirection::Descending,
        ),
    )
    .with_map(
        "jit_side_exit_reasons",
        map_to_json(&counters.jit_side_exit_reasons, SortDirection::Descending),
    )
    .with_map(
        "jit_blacklist_reasons",
        map_to_json(&counters.jit_blacklist_reasons, SortDirection::Descending),
    )
    .into()
}

struct JsonObject(Map<String, Value>);

impl JsonObject {
    fn with_map(mut self, name: &str, value: Value) -> Self {
        self.0.insert(name.to_string(), value);
        self
    }
}

impl From<JsonObject> for Value {
    fn from(value: JsonObject) -> Self {
        Value::Object(value.0)
    }
}

fn object_from_pairs<const N: usize>(pairs: [(&str, u64); N]) -> JsonObject {
    let mut object = Map::new();
    for (key, value) in pairs {
        object.insert(key.to_string(), Value::from(value));
    }
    JsonObject(object)
}

#[derive(Clone, Copy)]
enum SortDirection {
    Descending,
}

fn map_to_json(map: &std::collections::BTreeMap<String, u64>, _sort: SortDirection) -> Value {
    let mut entries = map.iter().collect::<Vec<_>>();
    entries.sort_by(|(left_key, left_value), (right_key, right_value)| {
        right_value
            .cmp(left_value)
            .then_with(|| left_key.cmp(right_key))
    });
    let values = entries
        .into_iter()
        .map(|(key, value)| {
            let mut entry = Map::new();
            entry.insert("name".to_string(), Value::from(key.clone()));
            entry.insert("count".to_string(), Value::from(*value));
            Value::Object(entry)
        })
        .collect::<Vec<_>>();
    Value::Array(values)
}

fn boundary_profiles_to_json(
    profiles: &std::collections::BTreeMap<String, BoundaryProfile>,
) -> Value {
    let mut entries = profiles.iter().collect::<Vec<_>>();
    entries.sort_by(|(left_key, left), (right_key, right)| {
        right
            .inclusive_nanos
            .cmp(&left.inclusive_nanos)
            .then_with(|| right.count.cmp(&left.count))
            .then_with(|| left_key.cmp(right_key))
    });
    let values = entries
        .into_iter()
        .map(|(key, profile)| {
            let mut entry = Map::new();
            entry.insert("name".to_string(), Value::from(key.clone()));
            entry.insert("count".to_string(), Value::from(profile.count));
            entry.insert(
                "inclusive_nanos".to_string(),
                Value::from(profile.inclusive_nanos),
            );
            entry.insert(
                "exclusive_nanos".to_string(),
                Value::from(profile.exclusive_nanos),
            );
            entry.insert(
                "rich_instructions".to_string(),
                Value::from(profile.rich_instructions),
            );
            entry.insert(
                "dense_instructions".to_string(),
                Value::from(profile.dense_instructions),
            );
            let average_inclusive = if profile.count == 0 {
                0
            } else {
                profile.inclusive_nanos / profile.count
            };
            let average_exclusive = if profile.count == 0 {
                0
            } else {
                profile.exclusive_nanos / profile.count
            };
            entry.insert("average_nanos".to_string(), Value::from(average_inclusive));
            entry.insert(
                "average_exclusive_nanos".to_string(),
                Value::from(average_exclusive),
            );
            Value::Object(entry)
        })
        .collect::<Vec<_>>();
    Value::Array(values)
}

fn operation_profiles_to_json(
    profiles: &std::collections::BTreeMap<String, OperationProfile>,
) -> Value {
    let mut entries = profiles.iter().collect::<Vec<_>>();
    entries.sort_by(|(left_key, left), (right_key, right)| {
        right
            .inclusive_nanos
            .cmp(&left.inclusive_nanos)
            .then_with(|| right.count.cmp(&left.count))
            .then_with(|| left_key.cmp(right_key))
    });
    let values = entries
        .into_iter()
        .map(|(key, profile)| {
            let mut entry = Map::new();
            entry.insert("name".to_string(), Value::from(key.clone()));
            entry.insert("count".to_string(), Value::from(profile.count));
            entry.insert(
                "inclusive_nanos".to_string(),
                Value::from(profile.inclusive_nanos),
            );
            let average_inclusive = if profile.count == 0 {
                0
            } else {
                profile.inclusive_nanos / profile.count
            };
            entry.insert("average_nanos".to_string(), Value::from(average_inclusive));
            Value::Object(entry)
        })
        .collect::<Vec<_>>();
    Value::Array(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_profile_file_stem_is_filesystem_safe() {
        assert_eq!(profile_file_stem("req-00000001"), "req-00000001");
        assert_eq!(profile_file_stem("../bad id"), "___bad_id");
        assert_eq!(profile_file_stem(""), "request");
    }

    #[test]
    fn request_profile_json_groups_existing_counter_families() {
        let trace = PerfTraceEvent {
            request_id: "req-00000001".to_string(),
            method: "GET".to_string(),
            path: "/".to_string(),
            script_path: "/var/www/html/index.php".to_string(),
            status: 200,
            cache_hit: Some(true),
            body_bytes: 0,
            response_bytes: 12,
            runtime_diagnostics: 0,
            phases: vec![("php_vm_execution", 42)],
            counters: vec![("vm_value_clones", 7)],
            ..PerfTraceEvent::default()
        };
        let mut counters = VmCounters::default();
        counters.value_clones = 7;
        counters.array_handle_clones = 3;
        counters.array_packed_direct_gets = 9;
        counters.array_mixed_indexed_gets = 5;
        counters.array_linear_scan_fallbacks = 2;
        counters.array_metadata_recomputes = 1;
        counters.frame_allocations = 10;
        counters.frame_reuses = 8;
        counters.frames_allocated = 10;
        counters.frames_reused = 8;
        counters.register_files_allocated = 10;
        counters.register_files_reused = 8;
        counters.tiny_frame_candidates = 6;
        counters.specialized_frame_hits = 4;
        counters.arg_array_avoided = 3;
        counters.heap_frame_avoided = 2;
        counters.direct_arg_frame_hits = 5;
        counters.direct_method_frame_hits = 4;
        counters.direct_closure_frame_hits = 3;
        counters.direct_constructor_frame_hits = 2;
        counters.argument_vector_allocations_avoided = 14;
        counters.include_compile_hits = 2;
        counters.request_arena_allocations = 3;
        counters.request_arena_bytes = 4096;
        counters.request_pool_resets = 1;
        counters.persistent_engine_allocations = 2;
        counters.persistent_engine_bytes = 2048;
        counters
            .arena_fallback_allocations_by_reason
            .insert("destructor_sensitive".to_string(), 1);
        counters.destructor_sensitive_arena_blocks = 1;
        counters.quickening_attempts = 9;
        counters
            .quickening_candidates_by_family
            .insert("string_concat".to_string(), 4);
        counters.quickening_specialized = 6;
        counters
            .quickening_applied_by_family
            .insert("dim_fetch".to_string(), 3);
        counters
            .quickened_executions_by_family
            .insert("dim_fetch".to_string(), 2);
        counters.quickening_guard_hits = 5;
        counters.quickening_guard_misses = 4;
        counters.quickening_guard_failures = 3;
        counters
            .quickening_guard_failures_by_family
            .insert("dim_fetch".to_string(), 2);
        counters.quickening_fallback_calls = 2;
        counters.quickening_dequickens = 1;
        counters
            .quickening_dequickened_by_reason
            .insert("megamorphic".to_string(), 1);
        counters.quickening_megamorphic = 1;
        counters.quickening_disabled = 1;
        counters.jit_compile_attempts = 1;
        counters
            .value_clone_by_reason
            .insert("return_value".to_string(), 1);
        counters
            .value_clone_by_source_family
            .insert("return_value".to_string(), 4);
        counters
            .array_handle_clone_by_source_family
            .insert("array_element_read".to_string(), 2);
        counters
            .cow_separation_by_source_family
            .insert("by_ref_argument_binding".to_string(), 1);
        counters
            .reference_cell_creation_by_source_family
            .insert("by_ref_argument_binding".to_string(), 1);
        counters
            .rich_fallback_functions_by_name
            .insert("fallback_helper".to_string(), 2);
        counters
            .dense_function_fallback_by_reason
            .insert("unsupported_terminator".to_string(), 3);
        counters
            .dense_method_dispatch_fallback_by_reason
            .insert("magic_method".to_string(), 2);
        counters
            .dense_call_fallback_by_reason
            .insert("unknown_function".to_string(), 1);
        counters
            .frame_reuse_blocked_by_reason
            .insert("by_ref_param".to_string(), 2);
        counters
            .call_frame_layout_observed
            .insert("known_function_frame".to_string(), 6);
        counters
            .generic_frame_fallback_by_reason
            .insert("variadic_named_argument_frame".to_string(), 1);
        counters
            .direct_frame_fallback_by_reason
            .insert("dynamic_callable".to_string(), 1);
        counters
            .dense_include_entry_fallback_by_path
            .insert("/srv/app/fallback.php".to_string(), 1);
        counters.include_profiles_by_path.insert(
            "/srv/app/lib.php".to_string(),
            BoundaryProfile {
                count: 2,
                inclusive_nanos: 200,
                exclusive_nanos: 120,
                rich_instructions: 20,
                dense_instructions: 6,
            },
        );
        counters.function_profiles_by_name.insert(
            "profile_helper".to_string(),
            BoundaryProfile {
                count: 4,
                inclusive_nanos: 400,
                exclusive_nanos: 300,
                rich_instructions: 40,
                dense_instructions: 8,
            },
        );
        counters.method_profiles_by_name.insert(
            "ProfileThing::name".to_string(),
            BoundaryProfile {
                count: 1,
                inclusive_nanos: 300,
                exclusive_nanos: 250,
                rich_instructions: 30,
                dense_instructions: 3,
            },
        );
        counters.builtin_profiles_by_name.insert(
            "count".to_string(),
            BoundaryProfile {
                count: 8,
                inclusive_nanos: 80,
                exclusive_nanos: 80,
                rich_instructions: 8,
                dense_instructions: 0,
            },
        );
        counters.array_operation_profiles_by_family.insert(
            "dim_fetch".to_string(),
            OperationProfile {
                count: 3,
                inclusive_nanos: 90,
            },
        );
        counters.object_operation_profiles_by_family.insert(
            "property_fetch".to_string(),
            OperationProfile {
                count: 2,
                inclusive_nanos: 70,
            },
        );
        counters.output_operation_profiles_by_family.insert(
            "echo".to_string(),
            OperationProfile {
                count: 1,
                inclusive_nanos: 50,
            },
        );

        let profile = request_profile_json(&trace, Some(&counters));
        let attribution = profile
            .get("attribution")
            .and_then(Value::as_object)
            .expect("attribution object");
        assert_eq!(
            attribution.get("vm_counters_collected"),
            Some(&Value::from(true))
        );
        assert!(attribution.get("clones").is_some());
        assert!(attribution.get("includes").is_some());
        assert!(attribution.get("metadata").is_some());
        assert!(attribution.get("native").is_some());
        assert_eq!(
            profile["attribution"]["includes"]["include_profiles_by_path"][0]["name"],
            Value::from("/srv/app/lib.php")
        );
        assert_eq!(
            profile["attribution"]["calls"]["function_profiles_by_name"][0]["name"],
            Value::from("profile_helper")
        );
        assert_eq!(
            profile["attribution"]["calls"]["method_profiles_by_name"][0]["name"],
            Value::from("ProfileThing::name")
        );
        assert_eq!(
            profile["attribution"]["calls"]["builtin_profiles_by_name"][0]["name"],
            Value::from("count")
        );
        assert_eq!(
            profile["attribution"]["calls"]["function_profiles_by_name"][0]["exclusive_nanos"],
            Value::from(300)
        );
        assert_eq!(
            profile["attribution"]["includes"]["include_profiles_by_path"][0]["rich_instructions"],
            Value::from(20)
        );
        assert_eq!(
            profile["attribution"]["includes"]["include_profiles_by_path"][0]["dense_instructions"],
            Value::from(6)
        );
        assert_eq!(
            profile["attribution"]["execution"]["rich_fallback_functions_by_name"][0]["name"],
            Value::from("fallback_helper")
        );
        assert_eq!(
            profile["attribution"]["includes"]["dense_include_entry_fallback_by_path"][0]["name"],
            Value::from("/srv/app/fallback.php")
        );
        assert_eq!(
            profile["attribution"]["calls"]["dense_function_fallback_by_reason"][0]["name"],
            Value::from("unsupported_terminator")
        );
        assert_eq!(
            profile["attribution"]["calls"]["dense_method_dispatch_fallback_by_reason"][0]["name"],
            Value::from("magic_method")
        );
        assert_eq!(
            profile["attribution"]["calls"]["dense_call_fallback_by_reason"][0]["name"],
            Value::from("unknown_function")
        );
        assert_eq!(
            profile["attribution"]["calls"]["frame_allocations"],
            Value::from(10)
        );
        assert_eq!(
            profile["attribution"]["calls"]["frame_reuses"],
            Value::from(8)
        );
        assert_eq!(
            profile["attribution"]["calls"]["register_files_reused"],
            Value::from(8)
        );
        assert_eq!(
            profile["attribution"]["calls"]["specialized_frame_hits"],
            Value::from(4)
        );
        assert_eq!(
            profile["attribution"]["calls"]["argument_vector_allocations_avoided"],
            Value::from(14)
        );
        assert_eq!(
            profile["attribution"]["calls"]["frame_reuse_blocked_by_reason"][0]["name"],
            Value::from("by_ref_param")
        );
        assert_eq!(
            profile["attribution"]["calls"]["call_frame_layout_observed"][0]["name"],
            Value::from("known_function_frame")
        );
        assert_eq!(
            profile["attribution"]["calls"]["generic_frame_fallback_by_reason"][0]["name"],
            Value::from("variadic_named_argument_frame")
        );
        assert_eq!(
            profile["attribution"]["calls"]["direct_frame_fallback_by_reason"][0]["name"],
            Value::from("dynamic_callable")
        );
        assert_eq!(
            profile["attribution"]["clones"]["value_clone_by_source_family"][0]["name"],
            Value::from("return_value")
        );
        assert_eq!(
            profile["attribution"]["clones"]["array_handle_clone_by_source_family"][0]["name"],
            Value::from("array_element_read")
        );
        assert_eq!(
            profile["attribution"]["clones"]["cow_separation_by_source_family"][0]["name"],
            Value::from("by_ref_argument_binding")
        );
        assert_eq!(
            profile["attribution"]["clones"]["reference_cell_creation_by_source_family"][0]["name"],
            Value::from("by_ref_argument_binding")
        );
        assert_eq!(
            profile["attribution"]["arrays"]["operation_profiles_by_family"][0]["name"],
            Value::from("dim_fetch")
        );
        assert_eq!(
            profile["attribution"]["arrays"]["array_packed_direct_gets"],
            Value::from(9)
        );
        assert_eq!(
            profile["attribution"]["arrays"]["array_mixed_indexed_gets"],
            Value::from(5)
        );
        assert_eq!(
            profile["attribution"]["arrays"]["array_linear_scan_fallbacks"],
            Value::from(2)
        );
        assert_eq!(
            profile["attribution"]["arrays"]["array_metadata_recomputes"],
            Value::from(1)
        );
        assert_eq!(
            profile["attribution"]["metadata"]["request_arena_allocations"],
            Value::from(3)
        );
        assert_eq!(
            profile["attribution"]["metadata"]["persistent_engine_bytes"],
            Value::from(2048)
        );
        assert_eq!(
            profile["attribution"]["metadata"]["quickening_candidates_by_family"][0]["name"],
            Value::from("string_concat")
        );
        assert_eq!(
            profile["attribution"]["metadata"]["quickening_dequickened_by_reason"][0]["name"],
            Value::from("megamorphic")
        );
        assert_eq!(
            profile["attribution"]["objects"]["operation_profiles_by_family"][0]["name"],
            Value::from("property_fetch")
        );
        assert_eq!(
            profile["attribution"]["output"]["operation_profiles_by_family"][0]["name"],
            Value::from("echo")
        );
    }
}
