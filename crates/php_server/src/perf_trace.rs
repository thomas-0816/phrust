use serde_json::{Map, Value};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Debug)]
pub(crate) struct PerfTraceWriter {
    path: PathBuf,
    file: Mutex<File>,
}

impl PerfTraceWriter {
    pub(crate) fn open(path: impl Into<PathBuf>) -> io::Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            path,
            file: Mutex::new(file),
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn write(&self, event: &PerfTraceEvent) -> io::Result<()> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| io::Error::other("perf trace lock poisoned"))?;
        serde_json::to_writer(&mut *file, &event.to_json())?;
        file.write_all(b"\n")?;
        file.flush()
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PerfTraceEvent {
    pub(crate) request_id: String,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) script_path: String,
    pub(crate) status: u16,
    pub(crate) cache_hit: Option<bool>,
    pub(crate) failure_phase: Option<&'static str>,
    pub(crate) body_bytes: u64,
    pub(crate) response_bytes: u64,
    pub(crate) runtime_diagnostics: u64,
    pub(crate) phases: Vec<(&'static str, u128)>,
    pub(crate) counters: Vec<(&'static str, u64)>,
}

impl PerfTraceEvent {
    fn to_json(&self) -> Value {
        let mut phases = Map::new();
        for (name, nanos) in &self.phases {
            phases.insert(
                (*name).to_string(),
                Value::from((*nanos).min(u64::MAX as u128) as u64),
            );
        }
        let mut counters = Map::new();
        for (name, value) in &self.counters {
            counters.insert((*name).to_string(), Value::from(*value));
        }
        let mut object = Map::new();
        object.insert(
            "request_id".to_string(),
            Value::from(self.request_id.clone()),
        );
        object.insert("method".to_string(), Value::from(self.method.clone()));
        object.insert("path".to_string(), Value::from(self.path.clone()));
        object.insert(
            "script_path".to_string(),
            Value::from(self.script_path.clone()),
        );
        object.insert("status".to_string(), Value::from(self.status));
        if let Some(cache_hit) = self.cache_hit {
            object.insert("script_cache_hit".to_string(), Value::from(cache_hit));
        }
        if let Some(failure_phase) = self.failure_phase {
            object.insert("failure_phase".to_string(), Value::from(failure_phase));
        }
        object.insert("body_bytes".to_string(), Value::from(self.body_bytes));
        object.insert(
            "response_bytes".to_string(),
            Value::from(self.response_bytes),
        );
        object.insert(
            "runtime_diagnostics".to_string(),
            Value::from(self.runtime_diagnostics),
        );
        object.insert("phases_nanos".to_string(), Value::Object(phases));
        object.insert("counters".to_string(), Value::Object(counters));
        Value::Object(object)
    }
}
