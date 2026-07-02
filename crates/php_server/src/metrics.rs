use hyper::StatusCode;
use php_executor::IncludeCacheStats;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub(crate) struct ServerMetrics {
    pub(crate) requests_total: AtomicU64,
    pub(crate) static_responses: AtomicU64,
    pub(crate) php_responses: AtomicU64,
    pub(crate) four_xx: AtomicU64,
    pub(crate) five_xx: AtomicU64,
    pub(crate) body_too_large: AtomicU64,
    pub(crate) overload: AtomicU64,
    pub(crate) uploads_total: AtomicU64,
    pub(crate) upload_parse_errors: AtomicU64,
    pub(crate) upload_bytes_accepted: AtomicU64,
    pub(crate) upload_files_rejected: AtomicU64,
    pub(crate) execution_timeouts: AtomicU64,
    pub(crate) execution_deadline_disabled: AtomicU64,
    pub(crate) static_streamed_bytes: AtomicU64,
    pub(crate) static_not_modified: AtomicU64,
    pub(crate) static_partial_responses: AtomicU64,
    pub(crate) static_precompressed_hits: AtomicU64,
    pub(crate) script_cache_preload_successes: AtomicU64,
    pub(crate) script_cache_preload_failures: AtomicU64,
    pub(crate) persistent_engine_policy_reuses: AtomicU64,
    pub(crate) persistent_engine_immutable_metadata_reuses: AtomicU64,
    pub(crate) persistent_engine_misses: AtomicU64,
    pub(crate) persistent_engine_request_local_resets: AtomicU64,
    pub(crate) persistent_engine_request_local_rejections: AtomicU64,
}

impl ServerMetrics {
    pub(crate) fn record_response(&self, status: StatusCode) {
        if status.is_client_error() {
            self.four_xx.fetch_add(1, Ordering::Relaxed);
        } else if status.is_server_error() {
            self.five_xx.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn render(
        &self,
        in_flight: u64,
        cache: php_executor::CompiledScriptCacheStats,
        include_cache: IncludeCacheStats,
    ) -> String {
        let shard_entries = cache
            .entries_by_shard
            .iter()
            .enumerate()
            .map(|(shard, entries)| {
                format!("phrust_server_script_cache_shard_entries{{shard=\"{shard}\"}} {entries}\n")
            })
            .collect::<String>();
        format!(
            "# phrust-server MVP internal metrics\n\
phrust_server_requests_total {}\n\
phrust_server_static_responses_total {}\n\
phrust_server_php_responses_total {}\n\
phrust_server_4xx_total {}\n\
phrust_server_5xx_total {}\n\
phrust_server_in_flight {}\n\
phrust_server_body_too_large_total {}\n\
phrust_server_overload_total {}\n\
phrust_server_uploads_total {}\n\
phrust_server_upload_parse_errors_total {}\n\
phrust_server_upload_bytes_accepted_total {}\n\
phrust_server_upload_files_rejected_total {}\n\
phrust_server_execution_timeouts_total {}\n\
phrust_server_execution_deadline_disabled_total {}\n\
phrust_server_static_streamed_bytes_total {}\n\
phrust_server_static_not_modified_total {}\n\
phrust_server_static_partial_responses_total {}\n\
phrust_server_static_precompressed_hits_total {}\n\
phrust_server_script_cache_lookups_total {}\n\
phrust_server_script_cache_hits_total {}\n\
phrust_server_script_cache_misses_total {}\n\
phrust_server_script_cache_source_reads_total {}\n\
phrust_server_script_cache_metadata_stats_total {}\n\
phrust_server_script_cache_stale_invalidations_total {}\n\
phrust_server_script_cache_compile_errors_total {}\n\
phrust_server_script_cache_entries {}\n\
phrust_server_script_cache_evictions_total {}\n\
phrust_server_script_cache_compile_in_progress {}\n\
phrust_server_script_cache_compiles_avoided_total {}\n\
{}\
phrust_server_script_cache_preload_successes_total {}\n\
phrust_server_script_cache_preload_failures_total {}\n\
phrust_server_include_resolution_hits_total {}\n\
phrust_server_include_resolution_misses_total {}\n\
phrust_server_include_compile_hits_total {}\n\
phrust_server_include_compile_misses_total {}\n\
phrust_server_include_source_reads_total {}\n\
phrust_server_include_dependency_metadata_validations_total {}\n\
phrust_server_include_stale_invalidations_total {}\n\
phrust_server_include_stale_dependency_invalidations_total {}\n\
phrust_server_include_compile_errors_total {}\n\
phrust_server_persistent_engine_policy_reuses_total {}\n\
phrust_server_persistent_engine_immutable_metadata_reuses_total {}\n\
phrust_server_persistent_engine_misses_total {}\n\
phrust_server_persistent_engine_request_local_resets_total {}\n\
phrust_server_persistent_engine_rejected_persistence_total{{reason=\"request_local_state\"}} {}\n",
            self.requests_total.load(Ordering::Relaxed),
            self.static_responses.load(Ordering::Relaxed),
            self.php_responses.load(Ordering::Relaxed),
            self.four_xx.load(Ordering::Relaxed),
            self.five_xx.load(Ordering::Relaxed),
            in_flight,
            self.body_too_large.load(Ordering::Relaxed),
            self.overload.load(Ordering::Relaxed),
            self.uploads_total.load(Ordering::Relaxed),
            self.upload_parse_errors.load(Ordering::Relaxed),
            self.upload_bytes_accepted.load(Ordering::Relaxed),
            self.upload_files_rejected.load(Ordering::Relaxed),
            self.execution_timeouts.load(Ordering::Relaxed),
            self.execution_deadline_disabled.load(Ordering::Relaxed),
            self.static_streamed_bytes.load(Ordering::Relaxed),
            self.static_not_modified.load(Ordering::Relaxed),
            self.static_partial_responses.load(Ordering::Relaxed),
            self.static_precompressed_hits.load(Ordering::Relaxed),
            cache.lookups,
            cache.hits,
            cache.misses,
            cache.source_reads,
            cache.metadata_stats,
            cache.stale_invalidations,
            cache.compile_errors,
            cache.entries,
            cache.evictions,
            cache.compile_in_progress,
            cache.compiles_avoided,
            shard_entries,
            self.script_cache_preload_successes.load(Ordering::Relaxed),
            self.script_cache_preload_failures.load(Ordering::Relaxed),
            include_cache.resolution_hits,
            include_cache.resolution_misses,
            include_cache.compile_hits,
            include_cache.compile_misses,
            include_cache.source_reads,
            include_cache.dependency_metadata_validations,
            include_cache.stale_invalidations,
            include_cache.stale_dependency_invalidations,
            include_cache.compile_errors,
            self.persistent_engine_policy_reuses.load(Ordering::Relaxed),
            self.persistent_engine_immutable_metadata_reuses
                .load(Ordering::Relaxed),
            self.persistent_engine_misses.load(Ordering::Relaxed),
            self.persistent_engine_request_local_resets
                .load(Ordering::Relaxed),
            self.persistent_engine_request_local_rejections
                .load(Ordering::Relaxed),
        )
    }
}

use super::state::AppState;
use crate::response::{self, ResponseBody};
use hyper::{
    Response, StatusCode as HttpStatusCode, header,
    http::{HeaderMap, request::Parts},
};

pub(crate) fn metrics_response(state: &AppState, parts: &Parts) -> Response<ResponseBody> {
    if let Some(token) = &state.metrics_token
        && !metrics_token_authorized(&parts.headers, token)
    {
        return response::text(HttpStatusCode::FORBIDDEN, "forbidden\n");
    }
    response::text_dynamic(
        HttpStatusCode::OK,
        state.metrics.render(
            state
                .max_in_flight
                .saturating_sub(state.in_flight.available_permits()) as u64,
            state.engine.script_cache.cache_stats(),
            state.engine.include_cache.cache_stats(),
        ),
        "text/plain; charset=UTF-8",
    )
}

pub(crate) fn metrics_token_authorized(headers: &HeaderMap, token: &str) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == format!("Bearer {token}"))
        || headers
            .get("x-phrust-metrics-token")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::header::HeaderValue;

    #[test]
    fn metrics_token_authorizes_bearer_or_custom_header() {
        let mut headers = HeaderMap::new();
        assert!(!metrics_token_authorized(&headers, "secret"));

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret"),
        );
        assert!(metrics_token_authorized(&headers, "secret"));

        headers.clear();
        headers.insert("x-phrust-metrics-token", HeaderValue::from_static("secret"));
        assert!(metrics_token_authorized(&headers, "secret"));

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer wrong"),
        );
        assert!(metrics_token_authorized(&headers, "secret"));
    }
}
