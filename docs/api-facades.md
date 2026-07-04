# API Facades

`php_runtime` and `php_vm` expose explicit facade modules for workspace and
downstream consumers. New imports should use these modules instead of relying on
crate-root compatibility re-exports.

## Runtime

- `php_runtime::api` is the stable surface for runtime values, request/response
  context, diagnostics, output buffers, resources, sessions, database state, and
  builtin registry types.
- `php_runtime::debug` is for tests and GC/debug tooling that intentionally
  inspects weak handles or internal reference/object state.
- `php_runtime::experimental` is for instrumentation and native/JIT integration
  helpers such as layout stats, numeric-string cache telemetry, and JIT array
  ABI helpers.

## VM

- `php_vm::api` is the stable surface for VM execution consumers: `Vm`,
  `VmOptions`, `VmResult`, execution modes, include loading, counters, and tiering
  options.
- `php_vm::experimental` is for performance tooling, bytecode layout tooling,
  JIT planning, region profiling, persistent feedback, deoptimization, and
  low-level VM counters.

## Compatibility Re-exports

Crate-root re-exports remain for compatibility during the migration, but they are
not the intended import style. `nix develop -c just source-integrity` runs a
facade import check that rejects new root `php_vm::...` or `php_runtime::...`
usage unless it is documented in `scripts/verify/api_facade_allowlist.txt`.

The current allowlist is limited to legacy `php_vm` implementation files that
still consume runtime internals directly. Downstream crates should not add new
entries; move imports to `api`, `debug`, or `experimental` instead.

## Dependency Boundary Exceptions

Workspace dependency edges are checked separately by
`nix develop -c just dependency-boundaries`. The machine-readable policy lives
in `scripts/verify/dependency_boundary_allowlist.json` and records only edges
that cross an ownership boundary for a concrete reason, such as VM execution
consuming runtime values or the runtime tokenizer service reusing the lexer.

New facade or dependency exceptions need:

- the exact source and target crate;
- a stable category;
- a reason tied to an existing layer boundary;
- a follow-up plan when the edge is temporary.

Generated reports are written to `target/architecture/` and must not be
committed. Remove allowlist entries when the underlying edge is removed.
