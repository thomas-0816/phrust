# Cranelift-only cutover

ADR 0017 makes executable Region IR lowered by Cranelift the sole production
execution contract. The migration is sequential: each prompt must pass its own
gate and shrink the temporary source allowlist before the next prompt begins.

## Prompt 0 baseline

The pre-cutover revision is
`c300e22a5f389c1e6b022f40184e79c9980e8cd7`. It is checked out detached at
`../phrust-interpreter-oracle` and built with a separate Cargo target directory.
The cutover harness invokes that `php-vm` binary as an external process; the
production workspace has no dependency on an oracle crate or library.

Run the prerequisite gate from the cutover worktree:

```sh
nix develop -c just cranelift-only-precondition
```

The gate writes `target/cranelift-only/precondition.json` and
`target/cranelift-only/precondition.md`. These generated reports record the
branch and oracle revisions, Cranelift version, Region IR schema, native ABI
hashes, target triple, and exact Cranelift CPU-feature identity. Reports under
`target/` are local evidence and must not be committed.

`scripts/verify/cranelift_only_allowlist.json` is the temporary legacy-source
inventory. New alternate-executor references are rejected immediately. A path
must be removed from the allowlist in the same prompt that removes its last
legacy reference. Prompt 14 requires both allowlist categories to be empty.

## Prompt 1 alternate-emitter removal

The former handwritten native emitter and its generated stencil artifacts are
deleted. `nix develop -c just cranelift-only-no-alternate-emitter` rejects any
new reference to that implementation and builds both product binaries through
the Cranelift dependency graph.

## Prompt 2 mandatory compiler boundary

Cranelift is a non-optional dependency and there is no product backend or
native-off selector. The two supported profiles are:

- `baseline`: eager, non-speculative Cranelift compilation with adaptive
  optimization disabled;
- `default` (and its `fast` alias): optimizing Cranelift compilation.

`Vm::execute` is the product native entry boundary. Until later prompts make
lowering exhaustive, an unsupported entry fails setup with
`E_NATIVE_UNSUPPORTED_LOWERING`, the precise IR `InstructionKind`, and its
byte span. It never resumes through the retained in-crate test oracle.

Run the mandatory graph and release-symbol gate with:

```sh
nix develop -c just cranelift-only-mandatory
```

The gate checks every product binary's Cargo graph, builds the release server,
requires Cranelift compiler symbols, and rejects retired emitter or interpreter
entry symbols in that binary.
