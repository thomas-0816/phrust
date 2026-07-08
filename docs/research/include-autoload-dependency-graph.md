# Include and Autoload Dependency Graph

FPE-24 treats include resolution and class-like autoload lookup as metadata
observations that can accelerate unchanged Composer-style applications without
changing PHP-visible ordering, diagnostics, or side effects.

The current implementation is a request-local graph projection over existing
inline caches. It stores resolution metadata only. It does not store PHP source
text, compiled units, include return values, autoload callback results, or
Composer files-autoload side effects.

Production-mode assumptions are intentionally opt-in: cross-request reuse needs
engine-owned fingerprints for every file, directory, autoload rule, and runtime
configuration input before it can affect execution.

## Graph Nodes

- `class/function/constant -> file`: class-like lookup observations identify
  positive declarations through the class table and negative results through a
  guarded autoload lookup cache. Function and constant observations should use
  the same model once their lookup ICs exist.
- `include expression -> likely target set`: include-path cache keys record the
  literal requested path, `include_path`, request current working directory, and
  calling file directory.
- `file -> inode/mtime/content hash/version`: the runtime `IncludePathFileFingerprint`
  now records length, modified timestamp in Unix nanos when available, readonly
  bit, and — where the platform exposes it (Unix) — `inode` and `device`. The
  whole struct is compared by equality, so an atomic replace or symlink swap
  that preserves length+mtime still invalidates the cached resolution. On
  platforms without filesystem identity both fields are `None`, which only
  matches another `None` and never widens reuse (fail-closed). Content identity
  exists at the compile-cache layer: `CompiledIncludeKey` carries a stable
  FNV-1a `source_content_hash` computed from the source already in memory at
  compile time (zero extra I/O). Probe-time (stat-based) fingerprints never
  carry a content hash, so any future content-validated reuse treats their
  absence as blocking — conservative by construction.
- `directory -> version`: `IncludeDirectoryVersion` (mtime nanos + Unix
  inode/device, `None` fields only match `None`) is captured for the resolved
  file's parent directory on every resolution — in both the request-local
  include-path IC and the shared process include cache. Revalidation compares
  it and reports `directory_version_hits`/`directory_version_misses`,
  **counters only**: the comparison never affects whether a hit is accepted,
  and missing include paths still fall back to normal resolution and
  diagnostics. Phar entries carry no directory version (always a miss).
- `autoload rule -> resolver`: SPL/Composer-style callbacks are represented by
  autoload stack epoch, registry epoch, lookup kind, normalized name,
  autoload-enabled flag, include-path configuration, and the Composer map
  fingerprint. The fingerprint is computed once per request on first
  autoload-cache use: the engine probes the entry script's directory and up to
  four ancestors for `vendor/composer/` and fingerprints the well-known map
  files (`autoload_classmap/files/psr4/real/static.php`) by file metadata into
  a stable `composer-map-v1:<hash>` string. No map found means `None`
  (unknown), which blocks persistent reuse keyed on it. Presence is counted
  per request (`composer_fingerprint_present/missing`); cross-request changes
  are attributed through the shared include cache
  (`composer_fingerprint_stale`). Because the value is stable within a
  request, wiring it into the (request-local) lookup key changes no hit/miss
  behavior.
- `deployment root -> fingerprint`: production-mode server runs install a
  `DeploymentRootFingerprint` (canonical docroot, directory version at
  startup, operator-declared mode) into the shared include cache. The mode
  comes from `--deployment-mode dev|immutable` (config key
  `deployment_mode`), defaulting to `dev` = mutable, which keeps every
  fingerprint-gated persistent reuse blocked. Metrics scrapes re-observe the
  root and report `deployment_fingerprint_present/missing/stale` via
  `phrust_server_deployment_fingerprint_*`. Metadata and counters only — no
  cache decision consumes it yet.
- `failed lookup -> negative cache`: negative class-like lookup entries are
  cached only when no visible autoload side effects can be skipped: autoload is
  disabled, or autoload is enabled with an empty autoload callback registry.

## Correctness Blockers

- Symlinks: cached paths use canonical paths, and the runtime fingerprint now
  carries inode/device on Unix so a symlink/atomic swap that keeps length+mtime
  is detected. Non-Unix identity and content-hash confirmation remain future
  work.
- Case sensitivity: class names are normalized, but filesystem path case rules
  differ by platform and mounted volume.
- `include_path`: included in the include cache key and autoload lookup key; any
  configuration change must invalidate cached observations.
- Working directory: request CWD is part of include cache keys. `chdir` support
  must bump include configuration epochs before broader caching can use it.
- `phar://`: PHAR includes are handled by the PHAR loader and counted as path
  semantics fallback for this graph layer.
- Generated files: file mutation is guarded by file fingerprints; missing-file
  negative include cache remains disabled until directory versions are available.
- Deployment swaps: cross-request reuse must treat deployment roots, symlink
  targets, and content versions as immutable engine-owned inputs.
- Dev mode: Composer or framework dev mode can create files, regenerate maps, or
  change class resolution. Persistent reuse must be disabled unless the runtime
  has explicit production-mode fingerprints.
- Realpath cache: PHP realpath behavior can affect include resolution. The graph
  cannot consume stale host realpath state without an explicit epoch.
- Stream wrappers: non-PHAR `scheme://` includes are not cached; they fall back
  to the existing unsupported-stream behavior.

## Current Safe Layer

Include-path hits are request-local metadata hits. A hit is accepted only after
the current file fingerprint matches the cached fingerprint; stale metadata
records `invalidations_by_reason.file_fingerprint_changed`, invalidates the IC,
and falls back to normal include-path resolution.

Autoload lookup hits are request-local class-table observations. Positive hits
are rechecked against the current direct class table before returning. Negative
hits increment `negative_lookup_hits` only when side-effect-free negative caching
was installed.

Ambiguous path semantics are not cached. They are reported through
`fallback_by_path_semantics`, currently including `missing_path`,
`stream_wrapper`, `phar_stream`, `outside_allowed_root`, `loader_disabled`, and
`loader_error`.

## Counters

- `include_graph_hits`
- `include_graph_misses`
- `autoload_graph_hits`
- `autoload_graph_misses`
- `negative_lookup_hits`
- `invalidations_by_reason`
- `fallback_by_path_semantics`
- `directory_version_hits` / `directory_version_misses` — revalidations whose
  stored parent-directory version matched / did not match (or was
  unobservable); reported in VM counters (IC path + shared-cache delta) and
  server metrics (`phrust_server_include_directory_version_*`).
- `composer_fingerprint_present` / `composer_fingerprint_missing` — per-request
  map detection; `composer_fingerprint_stale` — cross-request map change seen
  by the shared include cache.
- `deployment_fingerprint_present` / `deployment_fingerprint_missing` /
  `deployment_fingerprint_stale` — server-level, exported as
  `phrust_server_deployment_fingerprint_*`.
- `negative_include_cache_blocked_by_reason` — why a missing include path was
  *not* negatively cached (currently always
  `directory_versions_unvalidated`).

The performance report includes the aggregate graph counters plus selected
reason-map entries for file fingerprint invalidation and path semantics fallback.

## Exact Fallback Conditions

Every P2 fingerprint is fail-closed and none of them changes an accept/reject
decision yet:

- A directory that cannot be inspected (missing, replaced by a file, phar
  entry) yields no `IncludeDirectoryVersion`; comparisons against `None`
  always count as misses and can never validate a future negative-cache
  entry.
- No detected `vendor/composer` directory yields fingerprint `None` =
  unknown; unknown blocks any persistent reuse keyed on the Composer map. A
  map file that appears, disappears, or changes across requests counts
  `composer_fingerprint_stale`.
- A docroot that cannot be canonicalized yields no deployment fingerprint
  (`deployment_fingerprint_missing`); `--deployment-mode dev` (the default)
  declares the root mutable, which by itself blocks fingerprint-gated
  persistent reuse even when the fingerprint is present.
- Missing include-path negative caching stays disabled unconditionally;
  every candidate records `negative_include_cache_blocked_by_reason` instead
  of installing an entry.

## Deferred Integrations

Persistent feedback and inline-cache expansion should consume this graph only
after the directory-version, Composer-map, and deployment-root fingerprints
prove stable under real workloads. The fingerprints now exist as metadata and
counters; consuming them (directory-version-validated negative caching,
cross-request cache keys) is the next, separately gated step. Until then,
stale or ambiguous graph metadata must fall back to current include/autoload
logic.
