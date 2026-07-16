# Performance Include Path Cache

The performance layer provides two include-cache layers:

- a request-local include-/require-path inline cache behind the existing
  inline-cache mode, and
- an optional process-local `IncludeCache` that shares path-resolution metadata
  and compiled include units across VM requests.

The request-local inline cache stores only path resolution metadata:

- requested include string,
- current `include_path`,
- request current working directory,
- calling file directory,
- selected candidate path (re-canonicalized on hits),
- canonical resolved path,
- file metadata fingerprint.

The process-local cache stores the same resolution metadata plus compiled
include units keyed by canonical path, stable opened-file generation, content
hash, compiler version, debug-assertion mode, optimization level, and local
dependency identities. It never stores return values, include execution
results, request locals, globals, symbol tables, autoload registries,
include-once tables, or call-site strictness state. `include_once` and
`require_once` continue to use the request-local canonical-path once table after
path resolution, so a process-local cache hit cannot execute a once file twice
inside one request and cannot suppress execution in a later request.

Mutable deployments validate and hash primary and dependency bytes before every
compiled hit. An explicitly immutable deployment uses metadata-only hits only
while deployment-root, parent-directory, and reliable file-generation guards
remain current. This makes the default path content-safe and keeps the faster
operator-selected path fail-closed when any guard is missing.

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
- `phrust_server_include_source_bytes_hashed_total`
- `phrust_server_include_content_validations_total`
- `phrust_server_include_identity_only_hits_total`
- `phrust_server_include_content_mismatches_total`
- `phrust_server_include_conservative_misses_total`

## Compiled-Hit Benchmark

Run the focused Criterion benchmark with:

```bash
nix develop -c cargo bench --manifest-path crates/php_bench/Cargo.toml \
  --bench perf_hotpaths -- include_cache
```

The 2026-07-10 arm64 macOS sample used a 71-byte include and produced:

| Mode | Median hit latency | Approx. throughput | Content work |
| --- | ---: | ---: | --- |
| Mutable | 9.20 us | 109k hits/s | one validation/read/hash per hit |
| Immutable, guards valid | 2.44 us | 410k hits/s | one warm-up read; identity-only hits afterward |

The immutable fast path was about 3.8x faster in this focused sample. That is
the explicit cost of content-safe mutable reuse, not a reason to weaken its
validation. The eight-thread compile-stampede regression additionally requires
one compile miss, seven compile hits, and the same installed compiled-unit
pointer for all callers; waiting callers may revalidate content before return.

Validation:

- `nix develop -c just inline-cache-model-tests`
- `nix develop -c just cache-roundtrip`
- `nix develop -c cargo test -p php_vm include_cache_ --lib`
- `nix develop -c just verify-performance`
