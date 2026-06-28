# Cache Architecture

This repository has two intentionally separate cache classes. They share the
same safety rule: a cache hit must never change PHP-visible stdout, stderr, exit
status, diagnostics, request side effects, or fixture behavior.

## CLI Bytecode Artifact Cache

The CLI bytecode cache is a disk artifact cache owned by `php_vm_cli` and
`php_bytecode_cache`. It exists for local CLI execution and performance
experiments, and is enabled with `php-vm run --bytecode-cache=...`.

Cache artifacts are stored under the configured cache directory as `.phbc`
files named by a hex digest. The cache format is a project-owned envelope with a
verified IR payload; corrupt, stale, unreadable, or missing artifacts fall back
to compile-from-source behavior.

The bytecode cache fingerprint records these dimensions:

- source bytes hash;
- canonical source identity when available;
- engine crate version;
- PHP compatibility target;
- frontend, cache, and IR format versions;
- optimization level;
- Rust target label;
- feature flags such as bytecode-cache mode;
- runtime and configuration values that influence the compiled artifact.

The CLI reports bytecode cache state through `--bytecode-cache-stats` JSON:
`hit`, `miss`, `wrote`, `cleared`, `compile_error`, `load_error`,
`store_error`, and the cache file path when one was selected.

## Server Compiled Script Cache

The server cache is a process-local, in-memory compiled-script cache owned by
`php_executor::CompiledScriptCache` and consumed by `php_server`. It exists only
to reuse immutable compiled entry scripts across HTTP requests in the current
server process. It does not persist artifacts across process restarts and does
not use the `.phbc` disk format.

The server cache key records the safe MVP invalidation dimensions available at
entry-script compile time:

- canonical script path;
- source length;
- source mtime in nanoseconds when the filesystem provides it;
- source text hash;
- optimization level;
- executor crate version;
- debug assertion mode.

When the same canonical path is requested with changed source metadata, changed
source hash, or a changed optimization level, old entries for that path are
removed and counted as stale invalidations. Compile failures are counted but are
not inserted, so a later successful edit can compile and cache normally.

The integrated server exposes these counters on `/__phrust/metrics`:
`phrust_server_script_cache_hits_total`,
`phrust_server_script_cache_misses_total`,
`phrust_server_script_cache_stale_invalidations_total`,
`phrust_server_script_cache_compile_errors_total`, and
`phrust_server_script_cache_entries`.

## Why The Key Logic Is Not Shared

The two caches have different lifetimes and trust boundaries. The bytecode cache
loads untrusted local disk data and must validate a portable artifact against a
format version, target label, PHP target, feature set, and runtime config. The
server cache stores `Arc<CompiledPhpScript>` values created by the current
process and never deserializes them from disk, so its key can be smaller and
focused on entry-script staleness.

Moving both implementations behind one shared key builder would currently add
indirection without removing meaningful duplication. The shared invariant is
documented here instead: include every input that can change the compiled
artifact within that cache's lifetime and boundary, and prefer misses over
unsafe reuse.

## Known Boundaries

The server cache invalidates the requested entry script. Dynamic include,
autoload, and dependency graph invalidation remain runtime and future dependency
metadata work; they are not treated as an OPcache replacement.

The CLI bytecode cache remains an optional local optimization. Programs must run
correctly when the cache is disabled, empty, corrupt, or stale.
