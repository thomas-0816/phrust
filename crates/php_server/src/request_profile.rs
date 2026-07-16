use crate::perf_trace::PerfTraceEvent;
use php_vm::api::VmCounters;
use serde_json::{Map, Value};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

const SCHEMA_VERSION: u64 = 5;

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
