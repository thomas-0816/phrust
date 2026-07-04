# Panic and Unwrap Policy

Public lexer, parser, runtime, VM, executor, and server paths must not panic on
invalid PHP input or request data. `nix develop -c just panic-unwrap-policy`
scans production Rust sources for `unwrap`, `expect`, `panic`, `todo`, and
`unimplemented`.

Allowed uses must be narrow and documented in
`scripts/verify/panic_unwrap_allowlist.jsonl` with:

- `path`: source file path relative to the repository root.
- `pattern`: stable source substring that identifies the use.
- `category`: one of `internal-invariant`, `startup`, `test-helper`,
  `response-builder`, or `generated`.
- `reason`: why the use cannot be replaced with a recoverable error today.

Test modules are ignored by the production scan. New production panics should
prefer explicit diagnostics, `Result`, or deterministic unsupported-feature
returns before an allowlist entry is considered.
