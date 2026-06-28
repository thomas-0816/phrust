# Integrated Web Server MVP Design

This note records the initial integration seams for the integrated
`phrust-server` MVP. The server is in-process: request handling must call phrust
crates directly and must not fall back to FPM, FastCGI, CGI, Apache module
behavior, `mod_php`, subprocess PHP execution, or a warmed external worker.

## Current Seams

- `crates/php_server` owns the HTTP transport, routing, static files, request
  limits, metrics endpoint, and server CLI.
- `crates/php_executor` owns the reusable in-process compile/execute API used
  by both the server and the developer VM CLI.
- `crates/php_runtime/src/context.rs` owns deterministic request-local runtime
  state. HTTP execution seeds `_SERVER`, `_GET`, `_POST`, `_COOKIE`,
  `_REQUEST`, `_FILES`, and `_SESSION` without importing host environment
  implicitly.
- `crates/php_vm/src/vm/options.rs` carries the execution options consumed by
  the VM, including request-local runtime context, include loader, IR
  verification, execution format, quickening, inline caches, JIT/tiering, and
  counters.
- `justfile` exposes server-specific smoke commands in addition to the central
  verification surface.

## MVP Shape

- Transport is a direct Tokio + Hyper + Bytes HTTP server, with Hyper utility
  crates as needed for HTTP/1.1 request handling and response bodies.
- The initial transport target is HTTP/1.1 plaintext only.
- PHP execution must go through phrust frontend, IR, runtime, and VM crates
  directly. The server hot path must not call `php`, `php-vm`, `phrust-php`,
  `std::process::Command`, `Command::new`, a FastCGI socket, or any external PHP
  process.
- The transport-independent executor crate keeps CLI and server execution on
  the same in-process compiler/runtime/VM pipeline.
- HTTP request state belongs in `php_runtime` as deterministic owned metadata,
  independent of Hyper types, so `_SERVER`, `_GET`, `_POST`, `_COOKIE`, and
  `_REQUEST` can be seeded per request.
- The first cache is a process-local compiled-script cache. It is not a Zend ABI
  compatibility layer and not an Opcache replacement.
- The MVP applies backpressure with a bounded in-flight request semaphore and
  enforces `--max-body-bytes` while streaming request bodies into memory.
- `--request-timeout-ms` currently bounds request body reads. A full execution
  timeout that interrupts long-running PHP code is later work.
- Default logging is intentionally quiet. Set `RUST_LOG=php_server=debug` to
  inspect startup, route classification, cache lookup, overload, and request
  error decisions.
- `GET /__phrust/metrics` exposes MVP/internal plain-text process-local
  counters. It can be disabled with `--disable-metrics-endpoint`.

## Local Run And Smoke

```bash
nix develop -c cargo run -p php_server --bin phrust-server -- --docroot fixtures/server/apps/basic/public --listen 127.0.0.1:8080
nix develop -c just server-smoke
nix develop -c just server-benchmark-smoke
```

`server-smoke` builds the debug server, starts a temporary fixture docroot, and
checks `/healthz`, a static file, a PHP file, and a query PHP file. The benchmark
smoke is intentionally short and informational; it uses `oha`, `wrk`, `ab`, or a
curl loop when available and does not enforce throughput thresholds.

`fixtures/server/apps/basic/public` and
`fixtures/server/apps/front-controller/public` cover the current simple
unmodified PHP app surface. See `docs/server-known-gaps.md` for unsupported MVP
behavior.

`fixtures/server/apps/compat/` and `just server-compat-smoke` are the Wave 2
compatibility harness. The baseline harness is strict for static serving and
skips future compatibility sections until their implementation prompts land.

## Explicit Later Work

- TLS.
- HTTP/2 and HTTP/3.
- Sendfile or optimized static-file streaming.
- `io_uring` integration.
- Full request execution deadlines for long-running PHP code.
- Thread-per-core runtime layout.
- Advanced cache invalidation.
- Cross-process cache sharing.
- Authenticated or production-grade metrics export.
- Heavy server load tests in default CI.
- Full FPM, FastCGI, CGI, Apache module, or Zend ABI compatibility.
