use crate::perf_trace::PerfTraceEvent;
use php_vm::api::VmCounters;
use serde_json::{Map, Value};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

const SCHEMA_VERSION: u64 = 3;

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
            "compiled_regions".to_owned(),
            Value::from(counters.native_compiled_regions),
        );
        native.insert(
            "executions".to_owned(),
            Value::from(counters.native_executions),
        );
        native.insert(
            "side_exits".to_owned(),
            Value::from(counters.jit_side_exits),
        );
        native.insert(
            "helper_calls".to_owned(),
            Value::from(counters.jit_helper_calls),
        );
        native.insert(
            "code_generations".to_owned(),
            Value::from(counters.jit_code_generations),
        );
    }
    root.insert("native".to_owned(), Value::Object(native));
    Value::Object(root)
}
