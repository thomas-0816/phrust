# Server Architecture

`php_server` is an integrated in-process HTTP/HTTPS server for development,
compatibility smoke coverage, and simple standalone PHP application runs. It is
not a Zend SAPI implementation and does not provide production FPM, FastCGI,
CGI, Apache module, `mod_php`, or external PHP process compatibility.

## Ownership

- `crates/php_server/src/config.rs` owns server CLI/configuration parsing.
- `crates/php_server/src/routing.rs` owns docroot routing, front-controller
  routing, path normalization, and static-vs-PHP classification.
- `crates/php_server/src/response.rs` owns HTTP response construction from
  static files and executor output.
- `crates/php_server/src/server.rs` owns Hyper/Tokio request handling, optional
  Rustls termination, limits, metrics, graceful shutdown, and executor/cache
  wiring.
- `crates/php_server/src/multipart.rs` owns bounded multipart parsing and
  upload temp-file cleanup.
- `crates/php_server/src/session_store.rs` owns process-local web session
  persistence on disk.
- `php_executor` owns PHP compilation, diagnostics rendering, VM invocation,
  request include-loader construction, the server compiled-script cache, and
  the shared include cache.

## No External PHP Boundary

The server hot path must not call `php`, `php-vm`, `phrust-php`,
`std::process::Command`, a FastCGI socket, CGI, FPM, Apache module hooks, or a
warmed external worker. PHP requests execute through the workspace frontend,
runtime, and VM in the server process.

Request body reads obey `--max-body-bytes` and the `--request-timeout-ms`
body-read timeout. PHP execution is bounded by the cooperative
`--max-execution-ms` VM deadline and by the server in-flight request limit.
The default limit is 200 concurrent requests; excess requests wait up to 500 ms
for capacity before receiving `503 Service Unavailable`.
Deadline checks happen in VM dispatch; native blocking builtins are observed
when control returns to dispatch rather than by preemptive Tokio cancellation.

Static files stream through Tokio file I/O with `HEAD`, validators, byte
ranges, and precompressed sidecar selection. PHP scripts execute in a blocking
region inside the server process because the request-local PHP runtime state is
not `Send`.

## Transport And Configuration

Plain HTTP is the default. When `--tls-cert` and `--tls-key` are provided, the
same Hyper service is wrapped in Rustls and the startup handshake prints
`listening https://<addr>`. TLS currently advertises `http/1.1` through ALPN;
HTTP/2 and HTTP/3 are not enabled.

Server configuration can come from CLI flags or a simple TOML-style
`--config <path>` file, with CLI flags taking precedence. Production-oriented
options include upload and session paths, request limits, cooperative execution
deadlines, metrics endpoint controls, access logs, TLS files, script-cache
limits, preload, and the loopback-only cache-clear endpoint.

## Cache And Metrics

The server consumes `php_executor::CompiledScriptCache`, an in-memory,
process-local compiled entry-script cache. It is intentionally separate from
the CLI disk bytecode artifact cache and is not an Opcache replacement. See
`docs/runtime/cache-architecture.md` for key and invalidation rules.

The optional `/__phrust/metrics` endpoint exposes process-local counters for
requests, overloads, response classes, upload parsing, execution timeouts,
static streaming, script-cache hits/misses/stale invalidations, script-cache
preload, include-cache hits/misses, compile errors, and current cache entries.
It also exposes per-phase request timing as
`phrust_server_request_phase_count`/`_nanos_total` labelled by `phase`,
including an `admission_wait` phase that measures time spent waiting for an
in-flight permit at the concurrency-limiter (blocking-region) admission gate —
the queue-wait/worker-saturation signal, complementing the `in_flight` gauge and
the `overload` rejection counter. It is an internal plain-text endpoint. It can
be disabled with `--disable-metrics-endpoint` or protected with
`--metrics-token`.

The include cache additionally reports production-fingerprint counters:
`phrust_server_include_directory_version_hits/misses_total`,
`phrust_server_include_source_bytes_hashed_total`,
`phrust_server_include_content_validations_total`,
`phrust_server_include_identity_only_hits_total`,
`phrust_server_include_content_mismatches_total`,
`phrust_server_include_conservative_misses_total`,
`phrust_server_composer_fingerprint_stale_total`,
`phrust_server_deployment_fingerprint_present/missing/stale_total`, and the
default-on directory-version-guarded negative include cache
(`phrust_server_negative_include_cache_hits/installs/invalidations/`
`blocked_unversioned/blocked_capacity_total`; disable with
`PHRUST_NEGATIVE_INCLUDE_CACHE=off`). The
deployment-root fingerprint is installed at startup from the docroot and the
`--deployment-mode dev|immutable` declaration (config key `deployment_mode`,
default `dev` = mutable, which keeps fingerprint-gated persistent reuse
blocked); each metrics scrape re-observes the root's directory version to
attribute staleness. See
`docs/research/include-autoload-dependency-graph.md` for the fingerprint
model.

## Validation

Use these gates for server work:

```bash
nix develop -c cargo test -p php_server -p php_executor
nix develop -c just server-smoke
nix develop -c just server-compat-smoke all
nix develop -c just server-tls-smoke
nix develop -c just server-benchmark-smoke
nix develop -c just verify-server
```

Run `nix develop -c just quality-docs` when public docs, rustdoc, or architecture
links change.
