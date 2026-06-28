# Server Architecture

`php_server` is an integrated in-process HTTP server for development and
compatibility smoke coverage. It is not a Zend SAPI implementation and does not
provide production FPM, FastCGI, CGI, Apache module, `mod_php`, or external PHP
process compatibility.

## Ownership

- `crates/php_server/src/config.rs` owns server CLI/configuration parsing.
- `crates/php_server/src/routing.rs` owns docroot routing, front-controller
  routing, path normalization, and static-vs-PHP classification.
- `crates/php_server/src/response.rs` owns HTTP response construction from
  static files and executor output.
- `crates/php_server/src/server.rs` owns Hyper/Tokio request handling, blocking
  boundaries, limits, metrics, graceful shutdown, and executor/cache wiring.
- `php_executor` owns PHP compilation, diagnostics rendering, VM invocation,
  request include-loader construction, and the server compiled-script cache.

## No External PHP Boundary

The server hot path must not call `php`, `php-vm`, `phrust-php`,
`std::process::Command`, a FastCGI socket, CGI, FPM, Apache module hooks, or a
warmed external worker. PHP requests execute through the workspace frontend,
runtime, and VM in the server process.

Static file reads, route metadata checks, and PHP compile/execute work run
behind Tokio blocking tasks. Request body reads obey `--max-body-bytes` and the
current `--request-timeout-ms` body-read timeout. Once PHP execution starts,
there is no safe preemptive VM cancellation hook yet; long-running execution is
bounded by the in-flight request limit, not a per-script timeout.

## Cache And Metrics

The server consumes `php_executor::CompiledScriptCache`, an in-memory,
process-local compiled entry-script cache. It is intentionally separate from
the CLI disk bytecode artifact cache and is not an Opcache replacement. See
`docs/cache-architecture.md` for key and invalidation rules.

The optional `/__phrust/metrics` endpoint exposes process-local counters for
requests, overloads, response classes, script-cache hits/misses/stale
invalidations, compile errors, and current cache entries. It is an internal
plain-text endpoint and can be disabled with `--disable-metrics-endpoint`.

## Validation

Use these gates for server work:

```bash
nix develop -c cargo test -p php_server -p php_executor
nix develop -c just server-smoke
nix develop -c just verify-server
```

Run `nix develop -c just quality-docs` when public docs, rustdoc, or architecture
links change.
