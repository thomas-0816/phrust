# Performance Shared Cache Research Spike

This note investigates future shared-cache options for SAPI or daemon layers.
Performance does not add production shared memory code.

## Options

| Option | Scope | Benefits | Risks |
| --- | --- | --- | --- |
| PNA2 native unit-bundle cache | Existing process-restart boundary | Reuses validated deduplicated native code, simple inspection, no shared mutable memory | Filesystem latency, target/ABI coupling, dependency invalidation still conservative |
| Memory-mapped file | Future cache reader over immutable cache files | Kernel page cache sharing, lower copy overhead, simple read-only mapping model | File truncation races, platform-specific mmap semantics, needs strict header/version checks |
| Process-local cache | In-memory cache inside one long-lived worker | Fastest simple lookup, no cross-process synchronization | Duplication across workers, request isolation concerns, daemon lifecycle invalidation required |
| Shared memory cache | Future SAPI/FPM/daemon shared arena | Cross-worker reuse, potential preload support | Hardest security model, stale dependency risk, permissions, crash recovery, ABI/version compatibility |

## Security Risks

Shared cache entries are untrusted inputs unless produced and consumed inside a
single trusted process lifetime. Any future design must keep:

- content-addressed keys with PHP target, engine version, feature flags, source
  digest, and dependency metadata;
- bounds-checked parsing with corrupt-entry fallback;
- read-only mappings for runtime consumers;
- per-user or per-project permissions so unrelated projects cannot inject cache
  entries;
- W^X publication with no writable-and-executable mapping.

## Invalidation Risks

The current native cache keys source/IR content and compile configuration. A shared
cache needs stronger dependency tracking before it can safely cache includes,
autoload maps, preload state, or framework-generated files. Cache invalidation
must include:

- root source digest;
- included file fingerprints;
- include path and working-directory-sensitive resolution inputs;
- Composer map fingerprints;
- PHP target and engine ABI/schema versions;
- compiler policy and runtime/helper ABI identities.

## Recommendation

Future runtime research should prototype immutable mmap reads over the PNA2
format before attempting mutable shared memory. PHPT runtime or a dedicated SAPI
layer can evaluate a shared arena after daemon lifecycle, preload semantics,
and dependency invalidation are specified.
