use crate::perf_trace::PerfTraceEvent;
use php_vm::api::VmCounters;
use serde_json::{Map, Value};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

const SCHEMA_VERSION: u64 = 6;

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
    let stem: String = request_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if stem.is_empty() {
        "request".to_owned()
    } else {
        stem
    }
}

fn request_profile_json(trace: &PerfTraceEvent, counters: Option<&VmCounters>) -> Value {
    let mut root = Map::new();
    root.insert("schema_version".to_owned(), Value::from(SCHEMA_VERSION));
    root.insert(
        "request_id".to_owned(),
        Value::from(trace.request_id.clone()),
    );
    root.insert("method".to_owned(), Value::from(trace.method.clone()));
    root.insert("path".to_owned(), Value::from(trace.path.clone()));
    root.insert("status".to_owned(), Value::from(trace.status));

    let mut phases = Map::new();
    for (name, nanos) in &trace.phases {
        phases.insert(
            (*name).to_owned(),
            Value::from((*nanos).min(u64::MAX as u128) as u64),
        );
    }
    root.insert("phases_nanos".to_owned(), Value::Object(phases));

    let mut native = Map::new();
    if let Some(counters) = counters {
        native.insert(
            "compile_attempts".to_owned(),
            Value::from(counters.native_compile_attempts),
        );
        native.insert(
            "compile_successes".to_owned(),
            Value::from(counters.native_compile_successes),
        );
        native.insert(
            "compile_failures".to_owned(),
            Value::from(counters.native_compile_failures),
        );
        native.insert(
            "compile_time_nanos".to_owned(),
            Value::from(counters.native_compile_time_nanos),
        );
        native.insert(
            "cache_hits".to_owned(),
            Value::from(counters.native_cache_hits),
        );
        native.insert(
            "cache_misses".to_owned(),
            Value::from(counters.native_cache_misses),
        );
        native.insert(
            "cache_compile_waits".to_owned(),
            Value::from(counters.native_cache_compile_waits),
        );
        native.insert(
            "cache_evictions".to_owned(),
            Value::from(counters.native_cache_evictions),
        );
        native.insert(
            "execution_entries".to_owned(),
            Value::from(counters.native_execution_entries),
        );
        native.insert(
            "region_side_exits".to_owned(),
            Value::from(counters.native_region_side_exits),
        );
        native.insert(
            "runtime_helper_calls".to_owned(),
            Value::from(counters.runtime_helper_calls),
        );
        native.insert(
            "runtime_helper_calls_by_id".to_owned(),
            counter_map_json(&counters.runtime_helper_calls_by_id),
        );
        native.insert(
            "runtime_helper_time_nanos".to_owned(),
            Value::from(counters.runtime_helper_time_nanos),
        );
        native.insert(
            "runtime_helper_time_nanos_by_id".to_owned(),
            counter_map_json(&counters.runtime_helper_time_nanos_by_id),
        );
        native.insert(
            "execution_time_nanos".to_owned(),
            Value::from(counters.native_execution_time_nanos),
        );
        native.insert(
            "call_direct".to_owned(),
            Value::from(counters.native_call_direct),
        );
        native.insert(
            "call_dynamic".to_owned(),
            Value::from(counters.native_call_dynamic),
        );
        native.insert(
            "callsite_total".to_owned(),
            Value::from(counters.native_callsite_total),
        );
        native.insert(
            "same_unit_direct_eligible".to_owned(),
            Value::from(counters.native_same_unit_direct_eligible),
        );
        native.insert(
            "same_unit_direct_executed".to_owned(),
            Value::from(counters.native_same_unit_direct_executed),
        );
        native.insert(
            "cross_unit_direct_eligible".to_owned(),
            Value::from(counters.native_cross_unit_direct_eligible),
        );
        native.insert(
            "cross_unit_direct_executed".to_owned(),
            Value::from(counters.native_cross_unit_direct_executed),
        );
        native.insert(
            "method_monomorphic_eligible".to_owned(),
            Value::from(counters.native_method_monomorphic_eligible),
        );
        native.insert(
            "method_monomorphic_executed".to_owned(),
            Value::from(counters.native_method_monomorphic_executed),
        );
        native.insert(
            "builtin_direct_eligible".to_owned(),
            Value::from(counters.native_builtin_direct_eligible),
        );
        native.insert(
            "builtin_direct_executed".to_owned(),
            Value::from(counters.native_builtin_direct_executed),
        );
        native.insert(
            "builtin_calls_by_name".to_owned(),
            counter_map_json(&counters.native_builtin_calls_by_name),
        );
        native.insert(
            "builtin_time_nanos_by_name".to_owned(),
            counter_map_json(&counters.native_builtin_time_nanos_by_name),
        );
        native.insert(
            "call_dynamic_by_reason".to_owned(),
            counter_map_json(&counters.native_call_dynamic_by_reason),
        );
        native.insert(
            "call_dynamic_by_target".to_owned(),
            counter_map_json(&counters.native_call_dynamic_by_target),
        );
        native.insert(
            "callsite_calls_by_id".to_owned(),
            counter_map_json(&counters.native_callsite_calls_by_id),
        );
        native.insert(
            "callsite_inclusive_time_nanos_by_id".to_owned(),
            counter_map_json(&counters.native_callsite_inclusive_time_nanos_by_id),
        );
        native.insert(
            "callsite_exclusive_time_nanos_by_id".to_owned(),
            counter_map_json(&counters.native_callsite_exclusive_time_nanos_by_id),
        );
        native.insert(
            "call_argument_allocation_bytes".to_owned(),
            Value::from(counters.native_call_argument_allocation_bytes),
        );
        native.insert(
            "call_frame_bytes".to_owned(),
            Value::from(counters.native_call_frame_bytes),
        );
        native.insert(
            "transition_count".to_owned(),
            Value::from(counters.native_transition_count),
        );
        native.insert(
            "transition_by_reason".to_owned(),
            counter_map_json(&counters.native_transition_by_reason),
        );
        native.insert(
            "transition_time_nanos".to_owned(),
            Value::from(counters.native_transition_time_nanos),
        );
        native.insert(
            "transition_time_nanos_by_reason".to_owned(),
            counter_map_json(&counters.native_transition_time_nanos_by_reason),
        );
        native.insert(
            "runtime_helper_object_release_fast_paths".to_owned(),
            Value::from(counters.runtime_helper_object_release_fast_paths),
        );
        native.insert(
            "runtime_helper_object_release_root_scans".to_owned(),
            Value::from(counters.runtime_helper_object_release_root_scans),
        );
        native.insert(
            "runtime_helper_calls_by_ir_operation".to_owned(),
            counter_map_json(&counters.runtime_helper_calls_by_ir_operation),
        );
        native.insert(
            "runtime_helper_time_nanos_by_ir_operation".to_owned(),
            counter_map_json(&counters.runtime_helper_time_nanos_by_ir_operation),
        );
        native.insert(
            "runtime_helper_calls_by_function".to_owned(),
            counter_map_json(&counters.runtime_helper_calls_by_function),
        );
        native.insert(
            "runtime_helper_time_nanos_by_function".to_owned(),
            counter_map_json(&counters.runtime_helper_time_nanos_by_function),
        );
        native.insert(
            "runtime_helper_local_read_by_reason".to_owned(),
            counter_map_json(&counters.runtime_helper_local_read_by_reason),
        );
        native.insert(
            "runtime_helper_local_store_by_reason".to_owned(),
            counter_map_json(&counters.runtime_helper_local_store_by_reason),
        );
        native.insert(
            "runtime_helper_reference_read_by_value_class".to_owned(),
            counter_map_json(&counters.runtime_helper_reference_read_by_value_class),
        );
        native.insert(
            "runtime_helper_truthy_by_value_class".to_owned(),
            counter_map_json(&counters.runtime_helper_truthy_by_value_class),
        );
        native.insert(
            "runtime_helper_retain_by_reason".to_owned(),
            counter_map_json(&counters.runtime_helper_retain_by_reason),
        );
        native.insert(
            "runtime_helper_release_by_reason".to_owned(),
            counter_map_json(&counters.runtime_helper_release_by_reason),
        );
        native.insert(
            "runtime_helper_release_to_zero".to_owned(),
            Value::from(counters.runtime_helper_release_to_zero),
        );
        native.insert(
            "runtime_helper_object_release_root_scans_by_reason".to_owned(),
            counter_map_json(&counters.runtime_helper_object_release_root_scans_by_reason),
        );
        native.insert(
            "value_encodes".to_owned(),
            Value::from(counters.native_value_encodes),
        );
        native.insert(
            "value_decodes".to_owned(),
            Value::from(counters.native_value_decodes),
        );
        native.insert(
            "value_table_allocations".to_owned(),
            Value::from(counters.native_value_table_allocations),
        );
        native.insert(
            "value_table_reuses".to_owned(),
            Value::from(counters.native_value_table_reuses),
        );
        native.insert(
            "value_table_high_water".to_owned(),
            Value::from(counters.native_value_table_high_water),
        );
        native.insert(
            "value_table_materializations_by_kind_and_origin".to_owned(),
            counter_map_json(&counters.native_value_table_materializations_by_kind_and_origin),
        );
        native.insert(
            "ssa_promoted_locals".to_owned(),
            Value::from(counters.native_ssa_promoted_locals),
        );
        native.insert(
            "ssa_promoted_registers".to_owned(),
            Value::from(counters.native_ssa_promoted_registers),
        );
        native.insert(
            "ownership_moves".to_owned(),
            Value::from(counters.native_ownership_moves),
        );
        native.insert(
            "ownership_clones".to_owned(),
            Value::from(counters.native_ownership_clones),
        );
        native.insert(
            "ownership_escapes".to_owned(),
            Value::from(counters.native_ownership_escapes),
        );
        native.insert(
            "slow_path_entries_by_reason".to_owned(),
            counter_map_json(&counters.native_slow_path_entries_by_reason),
        );
        native.insert(
            "code_bytes_by_function".to_owned(),
            counter_map_json(&counters.native_code_bytes_by_function),
        );
        native.insert(
            "code_bytes_by_unit".to_owned(),
            counter_map_json(&counters.native_code_bytes_by_unit),
        );
        native.insert(
            "mapped_executable_bytes".to_owned(),
            Value::from(counters.native_mapped_executable_bytes),
        );
        native.insert(
            "frame_arena_capacity_bytes".to_owned(),
            Value::from(counters.native_frame_arena_capacity_bytes),
        );
        native.insert(
            "frame_arena_high_water_bytes".to_owned(),
            Value::from(counters.native_frame_arena_high_water_bytes),
        );
        native.insert(
            "inlined_calls".to_owned(),
            Value::from(counters.native_inlined_calls),
        );
        native.insert(
            "inline_bytes_added".to_owned(),
            Value::from(counters.native_inline_bytes_added),
        );
        native.insert(
            "inline_calls_removed".to_owned(),
            Value::from(counters.native_inline_calls_removed),
        );
        native.insert(
            "tail_calls".to_owned(),
            Value::from(counters.native_tail_calls),
        );
        native.insert(
            "inline_rejected_by_reason".to_owned(),
            counter_map_json(&counters.native_inline_rejected_by_reason),
        );
        native.insert(
            "versions_published".to_owned(),
            Value::from(counters.native_version_published),
        );
    }
    root.insert("native".to_owned(), Value::Object(native));
    Value::Object(root)
}

fn counter_map_json(values: &std::collections::BTreeMap<String, u64>) -> Value {
    Value::Object(
        values
            .iter()
            .map(|(name, value)| (name.clone(), Value::from(*value)))
            .collect(),
    )
}
