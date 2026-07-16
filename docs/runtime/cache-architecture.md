# Cache Architecture

This repository has several intentionally separate cache classes. They share the
same safety rule: a cache hit must never change PHP-visible stdout, stderr, exit
status, diagnostics, request side effects, or fixture behavior.

## Persistent Native Artifact Cache

The PNA2 disk cache is owned by `php_jit` and configured by `php_vm_cli` with
`php-vm run --native-cache=...`. It stores validated Cranelift machine code and
symbolic relocations so another process can publish executable entries without
recompiling unchanged IR.

Artifacts are content addressed. Their identity covers source/IR content,
compiler and target identity, CPU features, compile policy, and the versioned
runtime/helper ABIs. Helper addresses are never serialized: the loader resolves
stable helper IDs through the current registry, applies bounded relocations in
writable memory, then changes the mapping to executable. Corrupt, stale,
unreadable, oversized, or incompatible artifacts are rejected and rebuilt.

The native cache validates at least these dimensions:

- PNA2 unit-bundle schema and checksum;
- source/IR and native compile-policy identities;
- compiler version, target triple, and CPU feature identity;
- runtime and helper ABI versions and hashes;
- code, function, relocation, transition, and metadata bounds;
- cache-directory ownership, symlink, lock, and total-size constraints.

The CLI exposes `--native-cache off|read|write|read-write`,
`--native-cache-dir`, `--clear-native-cache`, and `--native-cache-stats`.
Detailed format, W^X, and restart validation live in
[`native-compile-cache.md`](../performance/native-compile-cache.md).

## Server Compiled Script Cache

The server cache is a process-local, in-memory compiled-script cache owned by
`php_executor::CompiledScriptCache` and consumed by `php_server`. It exists only
to reuse immutable compiled entry scripts across HTTP requests in the current
server process. It does not persist these object graphs across process restarts
and is independent of the PNA2 machine-code cache.

The server cache key records the invalidation dimensions available at
entry-script compile time:

- canonical script path;
- source length;
- source mtime in nanoseconds when the filesystem provides it;
- source text hash;
- optimization level;
- executor crate version;
- debug assertion mode.

Entry-script validation is metadata-first. A warm request canonicalizes the
path, stats the file, and reuses a cached `CompiledPhpScript` when the path,
metadata, optimization level, executor version, and debug assertion mode still
match. It does not reread source or recompute the source hash on that hit. A
miss or stale metadata path reads source, computes the source hash, and either
compiles or finds an exact entry populated by another request. This policy
prefers stale misses over unsafe reuse, but it intentionally trusts filesystem
metadata for the hot request path rather than claiming OPcache-equivalent
dependency safety.

`CompiledPhpScript` also stores a VM-facing `CompiledUnit` handle created at
compile time. Request execution clones only that cheap handle; it does not
clone the lowered `IrUnit` to enter the VM.

When the same canonical path is requested with changed source metadata, changed
source hash on the compile path, or a changed optimization level, old entries
for that path are removed and counted as stale invalidations. Compile failures
are counted but are not inserted, so a later successful edit can compile and
cache normally.

The integrated server exposes these counters on `/__phrust/metrics`:
`phrust_server_script_cache_lookups_total`,
`phrust_server_script_cache_hits_total`,
`phrust_server_script_cache_misses_total`,
`phrust_server_script_cache_source_reads_total`,
`phrust_server_script_cache_metadata_stats_total`,
`phrust_server_script_cache_stale_invalidations_total`,
`phrust_server_script_cache_compile_errors_total`,
`phrust_server_script_cache_compiles_avoided_total`, and
`phrust_server_script_cache_entries`.

## Server Include Cache

The server also owns a process-local include cache used by request VMs. Include
path resolution entries are keyed by the including directory, requested path,
include path entries, cwd, and allowed-root fingerprint. They retain the
selected candidate path so symlink target swaps invalidate the cached canonical
target. Compiled include entries are keyed by canonical path, an opened-source
identity (filesystem generation plus content hash), optimization level,
compiler/runtime fingerprint, and opened-source identities for local
dependencies discovered during compile. The compiler fingerprint includes the
explicit declaration-to-path map supplied by the include/executor layer, so a
mapping change cannot reuse an artifact linked against a previous dependency.
Compilation does not recursively search allowed roots for matching filenames.

In the default mutable mode, a warm compiled-include hit reads through an opened
file handle, verifies that the file generation stayed stable across the read,
hashes the exact bytes, and compares both generation and content before
returning the cached `CompiledUnit`. Recorded local dependencies follow the same
policy. An operator-declared immutable deployment may use a metadata-only hit
only while the deployment-root, parent-directory, and reliable file-generation
guards all remain current. Missing platform identity or an unobservable guard
fails closed to content validation or a conservative miss. A compile miss
reuses the already validated bytes instead of reading the primary source twice.

`include_once` and `require_once` remain request-local VM state. The shared
include cache only reuses resolution metadata and compiled units; it never
decides whether an include should execute for a request.

Include cache metrics include resolution hits/misses, compiled include
hits/misses, source reads and bytes hashed, content validations, identity-only
hits, content mismatches, conservative misses, dependency metadata validations,
stale invalidations, stale dependency invalidations, and compile errors.

## Why The Key Logic Is Not Shared

The caches have different lifetimes and trust boundaries. The native cache
loads untrusted local machine code and must validate the artifact, target,
compile policy, and ABI identities before W^X publication. The
server cache stores `Arc<CompiledPhpScript>` and `Arc<CompiledUnit>` values
created by the current process and never deserializes them from disk, so its key
can be smaller and focused on process-local staleness.

Moving both implementations behind one shared key builder would currently add
indirection without removing meaningful duplication. The shared invariant is
documented here instead: include every input that can change the compiled
artifact within that cache's lifetime and boundary, and prefer misses over
unsafe reuse.

## Known Boundaries

The server entry cache invalidates the requested entry script. The compiled
include cache records local source dependencies discovered during compile, but
dynamic include graphs, autoload registration order, and cross-process
invalidation remain known boundaries. These caches are not treated as an
OPcache replacement.

The persistent native cache is optional; native compilation is not. Programs
must run correctly when the cache is disabled, empty, corrupt, or stale.
