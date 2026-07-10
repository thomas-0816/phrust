use crate::persistent_metadata::PersistentMetadataStats;
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
    pub(crate) cpu_execution_admitted: AtomicU64,
    pub(crate) cpu_execution_queued: AtomicU64,
    pub(crate) cpu_execution_saturated: AtomicU64,
    pub(crate) cpu_execution_rejected: AtomicU64,
    pub(crate) cpu_execution_cancelled: AtomicU64,
    pub(crate) cpu_execution_timeouts: AtomicU64,
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
    pub(crate) persistent_engine_feedback_template_instantiations: AtomicU64,
    pub(crate) persistent_engine_feedback_template_absorptions: AtomicU64,
    pub(crate) response_output_bytes: AtomicU64,
    pub(crate) runtime_diagnostics: AtomicU64,
    pub(crate) session_seed_attempts: AtomicU64,
    pub(crate) session_store_loads: AtomicU64,
    pub(crate) session_lazy_loads: AtomicU64,
    pub(crate) session_finalizations: AtomicU64,
    pub(crate) session_store_writes: AtomicU64,
    pub(crate) session_store_deletes: AtomicU64,
    pub(crate) session_finalize_skipped_inactive: AtomicU64,
    pub(crate) request_headers_seen: AtomicU64,
    pub(crate) request_headers_materialized: AtomicU64,
    pub(crate) request_headers_skipped_direct: AtomicU64,
    pub(crate) phase_admission_wait_count: AtomicU64,
    pub(crate) phase_admission_wait_nanos: AtomicU64,
    pub(crate) phase_cpu_queue_count: AtomicU64,
    pub(crate) phase_cpu_queue_nanos: AtomicU64,
    pub(crate) phase_route_resolution_count: AtomicU64,
    pub(crate) phase_route_resolution_nanos: AtomicU64,
    pub(crate) phase_body_read_count: AtomicU64,
    pub(crate) phase_body_read_nanos: AtomicU64,
    pub(crate) phase_request_context_count: AtomicU64,
    pub(crate) phase_request_context_nanos: AtomicU64,
    pub(crate) phase_script_cache_count: AtomicU64,
    pub(crate) phase_script_cache_nanos: AtomicU64,
    pub(crate) phase_vm_execution_count: AtomicU64,
    pub(crate) phase_vm_execution_nanos: AtomicU64,
    pub(crate) phase_session_seed_count: AtomicU64,
    pub(crate) phase_session_seed_nanos: AtomicU64,
    pub(crate) phase_session_finalize_count: AtomicU64,
    pub(crate) phase_session_finalize_nanos: AtomicU64,
    pub(crate) phase_response_build_count: AtomicU64,
    pub(crate) phase_response_build_nanos: AtomicU64,
}

impl ServerMetrics {
    pub(crate) fn record_phase(&self, phase: RequestPhase, nanos: u128) {
        let nanos = nanos.min(u64::MAX as u128) as u64;
        let (count, total) = match phase {
            RequestPhase::AdmissionWait => (
                &self.phase_admission_wait_count,
                &self.phase_admission_wait_nanos,
            ),
            RequestPhase::CpuQueue => (&self.phase_cpu_queue_count, &self.phase_cpu_queue_nanos),
            RequestPhase::RouteResolution => (
                &self.phase_route_resolution_count,
                &self.phase_route_resolution_nanos,
            ),
            RequestPhase::BodyRead => (&self.phase_body_read_count, &self.phase_body_read_nanos),
            RequestPhase::RequestContext => (
                &self.phase_request_context_count,
                &self.phase_request_context_nanos,
            ),
            RequestPhase::ScriptCache => (
                &self.phase_script_cache_count,
                &self.phase_script_cache_nanos,
            ),
            RequestPhase::VmExecution => (
                &self.phase_vm_execution_count,
                &self.phase_vm_execution_nanos,
            ),
            RequestPhase::SessionSeed => (
                &self.phase_session_seed_count,
                &self.phase_session_seed_nanos,
            ),
            RequestPhase::SessionFinalize => (
                &self.phase_session_finalize_count,
                &self.phase_session_finalize_nanos,
            ),
            RequestPhase::ResponseBuild => (
                &self.phase_response_build_count,
                &self.phase_response_build_nanos,
            ),
        };
        count.fetch_add(1, Ordering::Relaxed);
        total.fetch_add(nanos, Ordering::Relaxed);
    }

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
        cpu_executing: u64,
        cache: php_executor::CompiledScriptCacheStats,
        include_cache: IncludeCacheStats,
        persistent_metadata: PersistentMetadataStats,
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
phrust_server_cpu_execution_current {}\n\
phrust_server_cpu_execution_admitted_total {}\n\
phrust_server_cpu_execution_queued_total {}\n\
phrust_server_cpu_execution_saturated_total {}\n\
phrust_server_cpu_execution_rejected_total {}\n\
phrust_server_cpu_execution_cancelled_total {}\n\
phrust_server_cpu_execution_timeouts_total {}\n\
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
phrust_server_include_source_bytes_hashed_total {}\n\
phrust_server_include_content_validations_total {}\n\
phrust_server_include_identity_only_hits_total {}\n\
phrust_server_include_content_mismatches_total {}\n\
phrust_server_include_conservative_misses_total {}\n\
phrust_server_include_dependency_metadata_validations_total {}\n\
phrust_server_include_stale_invalidations_total {}\n\
phrust_server_include_stale_dependency_invalidations_total {}\n\
phrust_server_include_compile_errors_total {}\n\
phrust_server_include_directory_version_hits_total {}\n\
phrust_server_include_directory_version_misses_total {}\n\
phrust_server_negative_include_cache_hits_total {}\n\
phrust_server_negative_include_cache_installs_total {}\n\
phrust_server_negative_include_cache_invalidations_total {}\n\
phrust_server_negative_include_cache_blocked_unversioned_total {}\n\
phrust_server_negative_include_cache_blocked_capacity_total {}\n\
phrust_server_composer_fingerprint_stale_total {}\n\
phrust_server_deployment_fingerprint_present_total {}\n\
phrust_server_deployment_fingerprint_missing_total {}\n\
phrust_server_deployment_fingerprint_stale_total {}\n\
phrust_server_immutable_release_cache_hits_total {}\n\
phrust_server_entry_script_source_reads_total {}\n\
phrust_server_response_output_bytes_total {}\n\
phrust_server_runtime_diagnostics_total {}\n\
phrust_server_session_seed_attempts_total {}\n\
phrust_server_session_store_loads_total {}\n\
phrust_server_session_lazy_loads_total {}\n\
phrust_server_session_finalizations_total {}\n\
phrust_server_session_store_writes_total {}\n\
phrust_server_session_store_deletes_total {}\n\
phrust_server_session_finalize_skipped_inactive_total {}\n\
phrust_server_request_headers_seen_total {}\n\
phrust_server_request_headers_materialized_total {}\n\
phrust_server_request_headers_skipped_direct_total {}\n\
phrust_server_request_phase_count{{phase=\"admission_wait\"}} {}\n\
phrust_server_request_phase_nanos_total{{phase=\"admission_wait\"}} {}\n\
phrust_server_request_phase_count{{phase=\"cpu_queue\"}} {}\n\
phrust_server_request_phase_nanos_total{{phase=\"cpu_queue\"}} {}\n\
phrust_server_request_phase_count{{phase=\"route_resolution\"}} {}\n\
phrust_server_request_phase_nanos_total{{phase=\"route_resolution\"}} {}\n\
phrust_server_request_phase_count{{phase=\"body_read\"}} {}\n\
phrust_server_request_phase_nanos_total{{phase=\"body_read\"}} {}\n\
phrust_server_request_phase_count{{phase=\"request_context\"}} {}\n\
phrust_server_request_phase_nanos_total{{phase=\"request_context\"}} {}\n\
phrust_server_request_phase_count{{phase=\"script_cache\"}} {}\n\
phrust_server_request_phase_nanos_total{{phase=\"script_cache\"}} {}\n\
phrust_server_request_phase_count{{phase=\"vm_execution\"}} {}\n\
phrust_server_request_phase_nanos_total{{phase=\"vm_execution\"}} {}\n\
phrust_server_request_phase_count{{phase=\"session_seed\"}} {}\n\
phrust_server_request_phase_nanos_total{{phase=\"session_seed\"}} {}\n\
phrust_server_request_phase_count{{phase=\"session_finalize\"}} {}\n\
phrust_server_request_phase_nanos_total{{phase=\"session_finalize\"}} {}\n\
phrust_server_request_phase_count{{phase=\"response_build\"}} {}\n\
phrust_server_request_phase_nanos_total{{phase=\"response_build\"}} {}\n\
phrust_server_persistent_engine_policy_reuses_total {}\n\
phrust_server_persistent_engine_immutable_metadata_reuses_total {}\n\
phrust_server_persistent_engine_misses_total {}\n\
phrust_server_persistent_engine_request_local_resets_total {}\n\
phrust_server_persistent_engine_rejected_persistence_total{{reason=\"request_local_state\"}} {}\n\
phrust_server_persistent_engine_feedback_templates {}\n\
phrust_server_persistent_engine_feedback_template_instantiations_total {}\n\
phrust_server_persistent_engine_feedback_template_absorptions_total {}\n",
            self.requests_total.load(Ordering::Relaxed),
            self.static_responses.load(Ordering::Relaxed),
            self.php_responses.load(Ordering::Relaxed),
            self.four_xx.load(Ordering::Relaxed),
            self.five_xx.load(Ordering::Relaxed),
            in_flight,
            cpu_executing,
            self.cpu_execution_admitted.load(Ordering::Relaxed),
            self.cpu_execution_queued.load(Ordering::Relaxed),
            self.cpu_execution_saturated.load(Ordering::Relaxed),
            self.cpu_execution_rejected.load(Ordering::Relaxed),
            self.cpu_execution_cancelled.load(Ordering::Relaxed),
            self.cpu_execution_timeouts.load(Ordering::Relaxed),
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
            include_cache.source_bytes_hashed,
            include_cache.content_validations,
            include_cache.identity_only_hits,
            include_cache.content_mismatches,
            include_cache.conservative_misses,
            include_cache.dependency_metadata_validations,
            include_cache.stale_invalidations,
            include_cache.stale_dependency_invalidations,
            include_cache.compile_errors,
            include_cache.directory_version_hits,
            include_cache.directory_version_misses,
            include_cache.negative_cache_hits,
            include_cache.negative_cache_installs,
            include_cache.negative_cache_invalidations,
            include_cache.negative_cache_blocked_unversioned,
            include_cache.negative_cache_blocked_capacity,
            include_cache.composer_fingerprint_stale,
            include_cache.deployment_fingerprint_present,
            include_cache.deployment_fingerprint_missing,
            include_cache.deployment_fingerprint_stale,
            include_cache.immutable_release_hits,
            cache.source_reads,
            self.response_output_bytes.load(Ordering::Relaxed),
            self.runtime_diagnostics.load(Ordering::Relaxed),
            self.session_seed_attempts.load(Ordering::Relaxed),
            self.session_store_loads.load(Ordering::Relaxed),
            self.session_lazy_loads.load(Ordering::Relaxed),
            self.session_finalizations.load(Ordering::Relaxed),
            self.session_store_writes.load(Ordering::Relaxed),
            self.session_store_deletes.load(Ordering::Relaxed),
            self.session_finalize_skipped_inactive
                .load(Ordering::Relaxed),
            self.request_headers_seen.load(Ordering::Relaxed),
            self.request_headers_materialized.load(Ordering::Relaxed),
            self.request_headers_skipped_direct.load(Ordering::Relaxed),
            self.phase_admission_wait_count.load(Ordering::Relaxed),
            self.phase_admission_wait_nanos.load(Ordering::Relaxed),
            self.phase_cpu_queue_count.load(Ordering::Relaxed),
            self.phase_cpu_queue_nanos.load(Ordering::Relaxed),
            self.phase_route_resolution_count.load(Ordering::Relaxed),
            self.phase_route_resolution_nanos.load(Ordering::Relaxed),
            self.phase_body_read_count.load(Ordering::Relaxed),
            self.phase_body_read_nanos.load(Ordering::Relaxed),
            self.phase_request_context_count.load(Ordering::Relaxed),
            self.phase_request_context_nanos.load(Ordering::Relaxed),
            self.phase_script_cache_count.load(Ordering::Relaxed),
            self.phase_script_cache_nanos.load(Ordering::Relaxed),
            self.phase_vm_execution_count.load(Ordering::Relaxed),
            self.phase_vm_execution_nanos.load(Ordering::Relaxed),
            self.phase_session_seed_count.load(Ordering::Relaxed),
            self.phase_session_seed_nanos.load(Ordering::Relaxed),
            self.phase_session_finalize_count.load(Ordering::Relaxed),
            self.phase_session_finalize_nanos.load(Ordering::Relaxed),
            self.phase_response_build_count.load(Ordering::Relaxed),
            self.phase_response_build_nanos.load(Ordering::Relaxed),
            self.persistent_engine_policy_reuses.load(Ordering::Relaxed),
            self.persistent_engine_immutable_metadata_reuses
                .load(Ordering::Relaxed),
            self.persistent_engine_misses.load(Ordering::Relaxed),
            self.persistent_engine_request_local_resets
                .load(Ordering::Relaxed),
            self.persistent_engine_request_local_rejections
                .load(Ordering::Relaxed),
            persistent_metadata.feedback_templates,
            self.persistent_engine_feedback_template_instantiations
                .load(Ordering::Relaxed),
            self.persistent_engine_feedback_template_absorptions
                .load(Ordering::Relaxed),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RequestPhase {
    AdmissionWait,
    CpuQueue,
    RouteResolution,
    BodyRead,
    RequestContext,
    ScriptCache,
    VmExecution,
    SessionSeed,
    SessionFinalize,
    ResponseBuild,
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
    // Re-observe the deployment root's directory version per scrape so
    // `deployment_fingerprint_stale` attributes root mutations. One stat call;
    // metadata only.
    state.engine.include_cache.revalidate_deployment_root();
    response::text_dynamic(
        HttpStatusCode::OK,
        state.metrics.render(
            state
                .max_in_flight
                .saturating_sub(state.in_flight.available_permits()) as u64,
            state
                .cpu_execution_limit
                .saturating_sub(state.cpu_execution.available_permits()) as u64,
            state.engine.script_cache.cache_stats(),
            state.engine.include_cache.cache_stats(),
            state.engine.persistent_metadata_stats(),
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

    #[test]
    fn include_identity_metrics_are_rendered_once() {
        let include_cache = IncludeCacheStats {
            source_reads: 11,
            source_bytes_hashed: 12,
            content_validations: 13,
            identity_only_hits: 14,
            content_mismatches: 15,
            conservative_misses: 16,
            ..IncludeCacheStats::default()
        };
        let rendered = ServerMetrics::default().render(
            0,
            0,
            php_executor::CompiledScriptCacheStats::default(),
            include_cache,
            PersistentMetadataStats::default(),
        );
        for expected in [
            "phrust_server_include_source_reads_total 11\n",
            "phrust_server_include_source_bytes_hashed_total 12\n",
            "phrust_server_include_content_validations_total 13\n",
            "phrust_server_include_identity_only_hits_total 14\n",
            "phrust_server_include_content_mismatches_total 15\n",
            "phrust_server_include_conservative_misses_total 16\n",
        ] {
            assert_eq!(rendered.matches(expected).count(), 1, "{expected}");
        }
    }
}
