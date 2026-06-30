# Performance Include Path Cache

Work item adds two include-cache layers:

- a request-local include-/require-path inline cache behind the existing
  inline-cache mode, and
- an optional process-local `IncludeCache` that shares path-resolution metadata
  and compiled include units across VM requests.

The request-local inline cache stores only path resolution metadata:

- requested include string,
- current `include_path`,
- request current working directory,
- calling file directory,
- canonical resolved path,
- file metadata fingerprint.

The process-local cache stores the same resolution metadata plus compiled
include units keyed by canonical path, file fingerprint, compiler version,
debug-assertion mode, and optimization level. It never stores return values,
include execution results, request locals, globals, symbol tables, autoload
registries, include-once tables, or call-site strictness state. `include_once`
and `require_once` continue to use the request-local canonical-path once table
after path resolution, so a process-local cache hit cannot execute an once file
twice inside one request and cannot suppress execution in a later request.

The resolver keeps the existing security policy: absolute paths are
canonicalized directly, relative paths search configured `include_path` entries,
the including file directory, the request current working directory, and the raw
relative path, and all local canonical paths must remain below configured
include roots. Remote `scheme://` includes are rejected; `phar://` is supported
only for local archives allowed by the configured filesystem roots. Stale
metadata records an include-path IC invalidation or shared-cache stale
invalidation and falls back to normal resolution/compilation. Poisoned shared
cache locks return `E_PHP_VM_INCLUDE_CACHE_POISONED` instead of panicking.

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
