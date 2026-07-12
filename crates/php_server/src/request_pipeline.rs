use crate::{
    multipart::cleanup_uploaded_files, perf_trace::PerfTraceEvent, response::ResponseBody,
    sessions::finalize_session_state, state::AppState,
};
use hyper::{Response, header};
use php_executor::PhpExecutionOutput;
use php_runtime::api::RuntimeUploadedFile;
use std::sync::atomic::Ordering;
use tracing::warn;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RequestStage {
    RouteTargetSelection,
    BodyAndMultipart,
    SessionLoad,
    ExecutorAcquisition,
    Execution,
    SessionAndUploadCleanup,
}

impl RequestStage {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::RouteTargetSelection => "routing",
            Self::BodyAndMultipart => "body_multipart",
            Self::SessionLoad => "session_seed",
            Self::ExecutorAcquisition => "executor_acquisition",
            Self::Execution => "php_vm_execution",
            Self::SessionAndUploadCleanup => "session_finalize",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PhpResponseBytes(pub(crate) u64);

/// Owns request-local cleanup until execution transfers it to the runtime
/// output or exits through an error path.
pub(crate) struct RequestCleanup {
    uploads: Vec<RuntimeUploadedFile>,
    armed: bool,
}

impl RequestCleanup {
    pub(crate) fn new(uploads: Vec<RuntimeUploadedFile>) -> Self {
        Self {
            uploads,
            armed: true,
        }
    }

    pub(crate) fn finalize_output(
        mut self,
        output: &mut PhpExecutionOutput,
        state: &AppState,
    ) -> Result<(), String> {
        output.upload_registry.cleanup_unmoved();
        self.armed = false;
        finalize_session_state(output, state)
    }
}

impl Drop for RequestCleanup {
    fn drop(&mut self) {
        if self.armed {
            cleanup_uploaded_files(&self.uploads);
        }
    }
}

/// Completed request data. Consuming this value is the only path that emits
/// request-final metrics, traces, and profiles.
pub(crate) struct RequestOutcome {
    response: Response<ResponseBody>,
    cache_hit: Option<bool>,
    failure_stage: Option<RequestStage>,
}

impl RequestOutcome {
    pub(crate) fn success(response: Response<ResponseBody>, cache_hit: Option<bool>) -> Self {
        Self {
            response,
            cache_hit,
            failure_stage: None,
        }
    }

    pub(crate) fn failure(
        response: Response<ResponseBody>,
        cache_hit: Option<bool>,
        stage: RequestStage,
    ) -> Self {
        Self {
            response,
            cache_hit,
            failure_stage: Some(stage),
        }
    }

    pub(crate) fn finalize(
        self,
        state: &AppState,
        trace: Option<PerfTraceEvent>,
    ) -> (Response<ResponseBody>, Option<bool>) {
        let response_bytes = self
            .response
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .or_else(|| {
                self.response
                    .extensions()
                    .get::<PhpResponseBytes>()
                    .map(|bytes| bytes.0)
            })
            .unwrap_or(0);
        state
            .services
            .metrics
            .response_output_bytes
            .fetch_add(response_bytes, Ordering::Relaxed);
        if let Some(mut trace) = trace {
            trace.status = self.response.status().as_u16();
            trace.cache_hit = self.cache_hit;
            trace.failure_phase = self.failure_stage.map(RequestStage::name);
            trace.response_bytes = response_bytes;
            if let Some(writer) = &state.observability.perf_trace
                && let Err(error) = writer.write(&trace)
            {
                warn!(%error, path=%writer.path().display(), "perf trace write failed");
            }
            if trace.profile_requested
                && let Some(writer) = &state.observability.request_profile
                && let Err(error) = writer.write(&trace, trace.profile_counters.as_ref())
            {
                warn!(%error, dir=%writer.dir().display(), "request profile write failed");
            }
        }
        (self.response, self.cache_hit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn request_stage_names_are_stable_and_distinct() {
        let stages = [
            RequestStage::RouteTargetSelection,
            RequestStage::BodyAndMultipart,
            RequestStage::SessionLoad,
            RequestStage::ExecutorAcquisition,
            RequestStage::Execution,
            RequestStage::SessionAndUploadCleanup,
        ];
        let mut names = stages.map(RequestStage::name).to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), stages.len());
    }

    #[test]
    fn dropped_request_cleanup_removes_uploaded_temp_files() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "phrust-request-cleanup-{}-{nonce}",
            std::process::id()
        ));
        std::fs::write(&path, b"upload").expect("write upload fixture");
        {
            let _cleanup = RequestCleanup::new(vec![RuntimeUploadedFile {
                field_name: "file".to_string(),
                client_filename: "fixture.txt".to_string(),
                content_type: "text/plain".to_string(),
                temp_path: path.to_string_lossy().into_owned(),
                error: 0,
                size: 6,
            }]);
        }
        assert!(!path.exists());
    }
}
