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
  helpers such as layout stats, numeric-string cache telemetry, JIT array ABI
  helpers, and the VM-coupled PCRE compiler/cache backend.

## VM

- `php_vm::api` is the stable surface for VM execution consumers: `Vm`,
  `VmOptions`, `VmResult`, execution modes, include loading, counters, and tiering
  options.
- `php_vm::experimental` is for performance tooling, bytecode layout tooling,
  JIT planning, region profiling, persistent feedback, deoptimization, and
  low-level VM counters.

## Root Surface

Crate-root compatibility re-exports have been removed. `php_runtime` exposes
only `api`, `debug`, and `experimental`; `php_vm` exposes only `api` and
`experimental`. `nix develop -c just source-integrity` rejects direct internal
imports and any accidental new public root module, item, or re-export.

The facade allowlist is intentionally empty. New code must classify imports as
stable, debug-only, or experimental instead of adding a compatibility alias.

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
