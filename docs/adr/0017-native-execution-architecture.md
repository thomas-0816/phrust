# ADR 0017: Production native execution architecture

- Status: accepted
- Date: 2026-07-13

## Context

Phrust previously exposed native compilation as an optional experiment beside
multiple execution backends. That product model no longer matched the runtime:
the cutover established exhaustive Region IR lowering, typed runtime helpers,
compiled calls and control flow, native suspension/resume, dynamic-code
compilation, native version transitions, and restart-persistent machine code.

## Decision

Cranelift is the only production compiler and is a regular, non-optional
dependency of both product binaries. The sole input pipeline is:

```text
php_lexer -> php_syntax -> php_ast -> php_semantics/HIR -> php_ir
          -> Region IR -> Cranelift -> native code -> typed runtime helpers
```

Every executable PHP function, include, eval unit, callback, generator, and
fiber continuation receives a published native entry before execution. Guard
failures reconstruct precise state and transfer to baseline or less-specialized
native code. They never enter a second executor.

`baseline` and `default` are policies for the same compiler. `baseline` keeps
optimization and speculation minimal; `default` enables adaptive optimization,
compiled calls, OSR, and the native cache.

The persistent cache stores validated Cranelift machine code keyed by source,
compiler version, target and CPU features, runtime ABI, helper ABI, and compile
policy. Loading preserves W^X and validates all metadata before publication.
Its audited `unsafe` boundary is confined to executable mappings, validated
entry invocation, and Unix ownership/process probes in `native_cache.rs`.
Cranelift runtime-operation ABI writes are confined to
`cranelift_lowering/fallback_helpers.rs`; other lowering modules remain subject
to the default no-`unsafe` performance audit.

Process startup reports compiler version, target and CPU feature identity,
runtime/helper ABI hashes, preset, cache mode/path, and loaded/compiled artifact
counts. Product telemetry uses only the canonical native families documented in
`docs/performance/counter-families.md`.

The cutover foundation is reproducible with:

```bash
nix develop -c just cranelift-only-precondition
```

That gate verifies the detached SHA-pinned migration oracle, regenerates the
current ABI/target/source identity under `target/cranelift-only/`, and runs the
zero-exception final architecture ratchet. The external oracle binary is a test
artifact only and is absent from the candidate Cargo graph.

`just cranelift-exhaustive-lowering` also checks the authoritative Region IR
builder itself: every source instruction variant must enter an explicit native
lowering path and none may map directly to `MissingLowering`. This complements
the generated coverage manifest so declared coverage cannot hide a missing
implementation.

## Consequences

- Users never enable or select a compiler backend.
- Unsupported lowering is a deterministic compilation diagnostic, not an
  execution fallback.
- Correctness comparison uses the `baseline` preset and the external PHP 8.5.7
  oracle; no in-product interpreter is authoritative.
- Packaging, CI, CLI help, server configuration, and examples ship one native
  engine.
