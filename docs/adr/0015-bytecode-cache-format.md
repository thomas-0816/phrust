# ADR 0015: Bytecode Cache Format

## Status

Accepted.

## Context

Performance introduces a bytecode/IR cache after the measurement and verifier
foundation. The cache is intended to make repeated frontend-to-IR loading
faster for deterministic local runs and later CLI workflows. It must not change
runtime semantics, skip diagnostics, or become a production Opcache clone.

The engine target remains PHP `8.5.7` as defined by the project reference
target. Cache artifacts must be treated as untrusted input even when they were
created by a previous local run of this engine.

## Decision

Use a small, versioned binary envelope owned by this project, with a length
prefixed metadata header and a payload section. The first Performance implementation
serializes and validates header metadata only. A later work item may add verified
IR or VM bytecode payloads after the verifier and invalidation rules are in
place.

The cache format is not shared memory and is not a PHP Opcache-compatible file.
It is a local artifact format for this engine. It may later be stored on disk or
behind a CLI flag, but cache use must remain optional and safely ignorable.

### Goals

- Speed up repeat frontend, semantic, and IR load paths for unchanged inputs.
- Preserve the Foundation through Standard library observable behavior baseline.
- Make cache hits reproducible and explainable through explicit fingerprints.
- Reject stale, corrupt, incompatible, or future cache artifacts cleanly.
- Keep the format small enough for deterministic tests and focused audits.

### Non-Goals

- No full production PHP Opcache clone.
- No shared-memory cache manager.
- No cross-process invalidation daemon.
- No Zend extension ABI compatibility.
- No execution of serialized PHP runtime state.
- No serialized native pointers, closures, resources, or host object handles.
- No cache hit that bypasses required diagnostics for the current source.

## Format

All multi-byte integer fields are little-endian. The header records the writer
endianness and target triple so incompatible artifacts can be rejected before a
payload is decoded. Future cross-endian support requires a new ADR or explicit
format-version migration.

The binary envelope starts with:

| Field | Type | Rule |
| --- | --- | --- |
| Magic | 8 bytes | Fixed project magic for bytecode-cache artifacts. |
| Cache format version | `u16` | Must match a supported reader version. |
| Header length | `u32` | Length of the canonical metadata block. |
| Payload length | `u64` | Length of the payload after metadata. |
| Header checksum | `u32` | Checksum over the metadata block. |
| Payload checksum | `u32` | Checksum over the payload bytes. |

The metadata block is encoded as deterministic JSON in Performance because it keeps
tests and diagnostics inspectable while the payload shape is still evolving. The
metadata object must be serialized with stable field ordering by the cache crate.
Payload bytes may later use a compact project-owned binary encoding, but the
metadata header remains the compatibility gate.

Required metadata fields:

| Field | Meaning |
| --- | --- |
| `engine_version` | Engine crate or workspace version that wrote the cache. |
| `php_target_version` | Must be exactly `8.5.7` for Performance. |
| `source_fingerprint` | Hash of the source bytes plus canonical source path identity when available. |
| `compiler_fingerprint` | Hash of parser, semantic, IR, feature, and opt-level inputs. |
| `abi_version` | Engine runtime/IR ABI compatibility version. |
| `cache_format_version` | Repeated in metadata for signed or text-level inspection. |
| `endianness` | Writer endianness. |
| `target_triple` | Writer target triple. |
| `ini_fingerprint` | Hash of INI/config values that can alter compile or runtime behavior. |
| `dependencies` | Include/require dependencies known at compile time. |
| `created_with` | Tool or crate version that wrote the artifact. |

The `compiler_fingerprint` must include at least:

- PHP target version.
- Parser and semantic frontend version markers.
- IR format and verifier version markers.
- Enabled engine feature flags.
- Optimization level and optimization-pass configuration.
- Runtime configuration that can affect lowering or diagnostics.

The `ini_fingerprint` must include any INI or engine config value known to
affect parsing, lowering, name lookup, include behavior, diagnostics, or runtime
selection. Unknown or unsupported config influences require a cache miss.

The `dependencies` list records include or require inputs when statically known.
Each dependency entry should include the resolved path when available, a source
fingerprint, and a resolution mode. Dynamic includes, eval, autoload-sensitive
resolution, or unresolved include paths must be represented as cache-blocking
metadata or a known partial-dependency state that prevents unsafe reuse.

## Validation

Cache loading is a defensive parse:

1. Check magic bytes before reading versioned fields.
2. Reject unsupported, unknown future, or explicitly deprecated format versions.
3. Reject target PHP versions other than `8.5.7`.
4. Reject incompatible engine, ABI, endianness, or target-triple metadata.
5. Bound-check header and payload lengths before allocation.
6. Verify metadata and payload checksums before decoding nested data.
7. Decode metadata with strict unknown-critical-field handling.
8. Recompute the source, compiler, config, and dependency fingerprints.
9. Verify any payload with the IR or bytecode verifier before execution.
10. On any failure, return a cache miss or structured load error without panic.

The cache reader must not trust paths, dependency lists, length fields,
checksums, serialized strings, enum tags, local slots, registers, jump targets,
or diagnostic metadata from the artifact. A valid envelope is only permission to
continue validation; it is not proof that the payload is executable.

## Invalidation

An artifact is invalid and must be ignored when any of these values change:

- Source bytes or canonical source identity.
- PHP target version.
- Engine version or ABI version.
- Cache format version.
- Parser, semantic, IR, verifier, VM, runtime, or standard-library version
  markers included in the compiler fingerprint.
- Feature flags or optimization level.
- INI/config values included in the config fingerprint.
- Include dependencies, dependency fingerprints, or dependency resolution mode.
- Target triple or unsupported platform compatibility assumptions.

When dependency completeness is unknown, the cache must miss. Dynamic includes,
autoload-sensitive behavior, and eval do not become cache hits unless a later
implementation can prove a precise invalidation rule.

## Migration

Readers may support older format versions only through explicit migration code
covered by tests. Unknown future versions must be rejected, not best-effort
decoded. Writers should emit only the current format version. If a metadata field
becomes security-critical, the cache format version must be bumped.

## Security

Bytecode-cache artifacts are untrusted input. Loading a cache must not panic,
read arbitrary paths, allocate unbounded memory, deserialize host object state,
or execute stale or malformed bytecode. Corruption, version mismatch, target
mismatch, checksum mismatch, verifier failure, or incomplete dependency
information must be handled as a structured load error or cache miss.

## Consequences

The first implementation can be small: a crate with public metadata types,
header serialization, and strict load/store errors. The VM and frontend can
remain unchanged until the cache crate has tests for versioning, target
mismatch, corrupt input, and future-format rejection.
