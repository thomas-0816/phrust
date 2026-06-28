# Executor Architecture

`php_executor` is the transport-independent compile/execute owner for normal
PHP program execution. It is used by the VM CLI compatibility path and by the
integrated HTTP server.

## Ownership

- `crates/php_executor/src/pipeline.rs` owns source analysis, semantic frontend
  invocation, IR lowering, IR verification, and optimizer application.
- `crates/php_executor/src/executor.rs` owns the public `PhpExecutor`,
  `CompiledPhpScript`, compile, execute, and compile-then-execute entrypoints.
- `crates/php_executor/src/request.rs` owns per-request include-loader and
  filesystem capability setup.
- `crates/php_executor/src/diagnostics.rs` owns PHP-shaped diagnostic rendering
  for executor-backed compile and VM failures.
- `crates/php_executor/src/cache.rs` owns the process-local compiled-script
  cache consumed by the server.
- `crates/php_executor/src/engine_compat.rs` keeps the legacy
  `EngineInput`/`execute_php` path stable for compatibility binaries.

## Boundaries

The executor does not own HTTP transport, routing, static files, CLI argument
parsing, disk bytecode artifact caching, or report/debug commands that inspect
frontend or VM internals.

`php_server` supplies HTTP request metadata and response mapping, then calls the
executor. `php_vm_cli` owns user-facing command behavior and delegates normal
execution orchestration to the executor where practical. Debug commands such as
IR dumps, bytecode reports, JIT inspection, or persistent-feedback tools may
continue to call lower layers directly because their output is intentionally
internal.

## API Contract

Public executor inputs and outputs are owned Rust data structures. Transport
layers should not mutate raw `VmOptions` after execution starts. New execution
features should be added as typed executor options first when both server and
CLI paths need the behavior.

PHP-visible stdout, stderr text, exit status, diagnostics, request side effects,
and fixture behavior must stay stable when orchestration moves into the
executor. Any remaining duplicate path must document why it needs direct access
to lower-level metadata.
