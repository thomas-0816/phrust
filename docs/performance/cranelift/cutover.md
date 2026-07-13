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
