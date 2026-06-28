# Diagnostics Architecture

Diagnostics are layered so parser acceptance, semantic checks, VM compile
failures, runtime failures, and transport mapping remain auditable.

## Owners

- `php_syntax` owns parser diagnostics over byte-based source spans.
- `php_semantics` owns semantic diagnostics over the CST/HIR boundary.
- `php_ir` owns lowering and IR verification diagnostics.
- `php_runtime` owns runtime diagnostic payloads, stack frames, PHP error
  rendering helpers, and `ExecutionStatus`/`ExitStatus`.
- `php_vm` owns VM execution results and attaches runtime diagnostics to
  compile/runtime/fatal/unsupported exits.
- `php_executor::diagnostics` owns the normal PHP-shaped stderr text for
  executor-backed CLI and server execution.
- `php_vm_cli` owns command-specific report/debug formatting when the command is
  not normal PHP execution.

## Formatting Rule

Normal program execution should have one PHP-shaped diagnostic formatting path:
`php_executor::diagnostics`. CLI compatibility binaries and server execution
should consume executor output instead of reimplementing fatal-line or runtime
status text.

Specialized reports may format structured diagnostics directly only when their
purpose is inspection rather than PHP-compatible execution. Examples include IR
dumps, bytecode-pattern reports, frontend snapshots, and JIT/performance debug
commands.

## Span And Status Rules

Byte spans are the source of truth. Line numbers are derived for display when a
PHP-shaped message needs `on line N` wording.

Parser and semantic diagnostics must stay separate from runtime diagnostics so
parser acceptance remains comparable with the PHP lint oracle. Runtime and VM
diagnostics should use stable diagnostic IDs and attach source spans when the VM
has compile-time metadata. Exit status mapping flows through
`php_runtime::api::ExitStatus` into executor-owned `PhpExecutionStatus`.

## Validation

Use focused tests around the layer that changed, then an executor-backed smoke
or domain gate:

```bash
nix develop -c cargo test -p php_executor
nix develop -c just vm-smoke
nix develop -c just verify-runtime
```

Docs and rustdoc changes are covered by:

```bash
nix develop -c just quality-docs
```
