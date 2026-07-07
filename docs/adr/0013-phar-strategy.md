# ADR 0013: PHAR Strategy

## Status

Accepted for the standard-library layer.

## Context

Composer PHAR compatibility requires more than local PHP source execution. A
useful `composer.phar --version` path depends on PHAR archive parsing, manifest
metadata, stream-wrapper integration for `phar://`, stub execution,
signature/hash validation decisions, and interaction with process, filesystem,
network, and plugin/script capabilities.

Standard library already requires Composer source-mode smokes because they exercise the
same userland bootstrap and standard-library surface without making PHAR a
mandatory runtime feature. Source mode also keeps network, plugin, script, and
host shell behavior outside required gates.

## Decision

PHAR support is not implemented as a required Standard library feature. Composer source
mode remains the mandatory Composer gate. A later optional work item may add a
read-only PHAR MVP, but only after it defines the archive format subset,
wrapper behavior, diagnostics, and security boundaries.

The optional read-only MVP, if accepted later, should be limited to:

- opening a local `.phar` file under existing filesystem allowed roots
- reading the PHAR manifest and file table without writes or alias mutation
- executing the archive stub only through the existing parser, semantic, IR,
  and VM pipeline
- exposing `phar://` reads through the existing stream wrapper capability model
- rejecting unsupported compression, signatures, metadata mutation, and write
  modes with deterministic diagnostics

## Consequences

- `composer.phar` is rejected by Standard library source-mode tooling instead of being
  treated as a half-supported input.
- PHAR-related Composer requirements are tracked as
  `STDLIB-GAP-PHAR-REQUIRED`.
- No PHAR parser, stream wrapper, or archive execution path is added silently in
  Standard library.
- The required Composer proof path remains `composer-smoke`,
  `composer-smoke-platform`, `composer-smoke-autoload`, and
  `composer-smoke-source`.
