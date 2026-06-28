# Performance Include Path Cache

Work item adds a request-local include-/require-path cache behind the
existing inline-cache mode. It caches only path resolution metadata:

- requested include string,
- current `include_path`,
- request current working directory,
- calling file directory,
- canonical resolved path,
- file metadata fingerprint.

The cache never stores PHP source text, compiled units, return values, or include
execution results. Every cache hit still reads the resolved source file through
the include loader before frontend analysis and IR lowering. `include_once` and
`require_once` continue to use the existing canonical-path once table after path
resolution, so a cache hit cannot execute an once file twice.

The resolver keeps the existing security policy: local filesystem paths only,
canonical paths constrained to configured include roots, and no remote
`scheme://` includes. Stale metadata records an include-path IC invalidation and
falls back to normal resolution.

Counters:

- `inline_cache_include_path_slots`
- `include_path_ic_hits`
- `include_path_ic_misses`
- `include_path_ic_invalidations`
- `include_path_ic_guard_failures`
- `include_graph_hits`
- `include_graph_misses`
- `invalidations_by_reason.file_fingerprint_changed`
- `invalidations_by_reason.include_path_epoch_or_guard`
- `fallback_by_path_semantics.missing_path`
- `fallback_by_path_semantics.stream_wrapper`
- `fallback_by_path_semantics.phar_stream`
- `fallback_by_path_semantics.outside_allowed_root`

Validation:

- `nix develop -c just inline-cache-smoke`
- `nix develop -c just cache-roundtrip`
- `nix develop -c just verify-performance`
