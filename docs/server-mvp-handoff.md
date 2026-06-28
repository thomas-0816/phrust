# Integrated Web Server MVP Handoff

## Implemented

- Added `php_executor` as the reusable in-process compile/execute API for CLI
  and server consumers.
- Added `php_server` with HTTP/1.1 plaintext serving, `/healthz`, static files,
  direct PHP scripts, optional front controller routing, request body limits,
  bounded in-flight requests, graceful shutdown, quiet-by-default tracing, and
  an optional internal metrics endpoint.
- Seeded basic HTTP superglobals for server requests: `$_SERVER`, `$_GET`,
  `$_POST`, `$_COOKIE`, `$_REQUEST`, `$_FILES`, and `$_SESSION`.
- Added request-local PHP response state and builtins for `header()`,
  `headers_list()`, `headers_sent()`, and `http_response_code()`.
- Added a process-local compiled script cache with hit/miss metrics.
- Added basic app and front-controller fixtures under `fixtures/server/apps/`.
- Added smoke scripts and `just` recipes for server smoke and optional benchmark
  smoke.

## How To Run

```bash
nix develop -c cargo run -p php_server --bin phrust-server -- --docroot fixtures/server/apps/basic/public --listen 127.0.0.1:8080
```

Useful checks:

```bash
nix develop -c just server-smoke
nix develop -c just server-benchmark-smoke
nix develop -c cargo run -p php_server --bin phrust-server -- --help
```

The server executes PHP in-process through phrust crates. It does not use FPM,
FastCGI, CGI, Apache, `mod_php`, or an external PHP process fallback.

## Intentionally Not Implemented

- Multipart form uploads and populated `$_FILES`.
- TLS termination.
- HTTP/2 or HTTP/3.
- Sendfile or optimized static-file streaming.
- Full PHP output flushing and advanced output buffering semantics.
- Complete PHP header edge-case compatibility.
- Cross-process compiled script cache sharing.
- A production process manager or full SAPI compatibility layer.

See `docs/server-known-gaps.md` for the current known-gap list.

## Performance-Critical Next Steps

- Follow the serial Wave 2 plan in `docs/server-wave2-functionality.md` for
  unmodified PHP app compatibility first, then speed and hardening work.
- Add execution deadlines for long-running PHP code, not only request body read
  timeouts.
- Replace simple static-file reads with streaming or platform sendfile support.
- Add cache invalidation policy and optional shared cache storage.
- Add targeted load tests for cache hit paths, front controllers, body limits,
  and overload behavior.
- Profile hot server requests with warm compiled scripts before widening the
  benchmark suite.

## Commands Run And Results

- `nix develop -c cargo run -p php_server --bin phrust-server -- --help`: passed;
  help lists listen, docroot, index, front controller, body/in-flight/timeouts,
  metrics, and script-cache options.
- `rg "FastCGI|php-fpm|mod_php|CGI|std::process::Command|Command::new" crates/php_server crates/php_executor docs README.md`:
  passed review; no production server/executor process fallback hits. Remaining
  matches are documentation policy or PHPT capability references.
- `nix develop -c just fmt`: passed.
- `nix develop -c cargo clippy --workspace --all-targets -- -D warnings`:
  passed.
- `nix develop -c cargo test --workspace`: passed.
- `nix develop -c just verify-runtime`: passed.
- `nix develop -c just verify-stdlib`: passed. The gate warned that
  `REFERENCE_PHP` is not configured, so reference-dependent diff rows skipped
  or used known gaps as designed.
- `nix develop -c just server-smoke`: passed.
- `nix develop -c just server-benchmark-smoke`: passed with ApacheBench,
  30 complete requests and 0 failed requests against `/hello.php`.

Prompt-specific checks also passed while building the MVP:

- `nix develop -c cargo fmt --all --check`
- `nix develop -c cargo clippy -p php_server -p php_executor -p php_runtime --all-targets -- -D warnings`
- `nix develop -c cargo test -p php_server`
- `nix develop -c cargo test -p php_executor`
- `nix develop -c cargo test -p php_runtime`
- `nix develop -c bash scripts/server/smoke.sh`
- `nix develop -c bash scripts/server/benchmark_smoke.sh`
- `nix develop -c just server-smoke`
