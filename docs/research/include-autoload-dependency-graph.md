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
- `file -> inode/mtime/content hash/version`: the current safe file version is
  `IncludePathFileFingerprint` with length, modified timestamp in Unix nanos
  when available, and readonly bit. Content hashes and inode/device identifiers
  are deferred until the filesystem abstraction exposes them portably.
- `directory -> version`: directory versioning is a required node for safe
  negative include-path caching. It is not used for include misses yet, so
  missing include paths fall back to normal resolution and diagnostics.
- `autoload rule -> resolver`: SPL/Composer-style callbacks are represented by
  autoload stack epoch, registry epoch, lookup kind, normalized name,
  autoload-enabled flag, include-path configuration, and a reserved Composer map
  fingerprint slot.
- `failed lookup -> negative cache`: negative class-like lookup entries are
  cached only when no visible autoload side effects can be skipped: autoload is
  disabled, or autoload is enabled with an empty autoload callback registry.

## Correctness Blockers

- Symlinks: cached paths use canonical paths, but production graph reuse needs
  inode/device or equivalent identity to detect symlink swaps.
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

The performance report includes the aggregate graph counters plus selected
reason-map entries for file fingerprint invalidation and path semantics fallback.

## Deferred Integrations

Persistent feedback and inline-cache expansion should consume this graph only
after directory versions, Composer map fingerprints, and production-mode
deployment fingerprints exist. Until then, stale or ambiguous graph metadata
must fall back to current include/autoload logic.
