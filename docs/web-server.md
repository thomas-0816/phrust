# Run The Web Server

Phrust includes an integrated HTTP server that executes PHP through the
workspace frontend, runtime, and VM. It does not use FPM, FastCGI, CGI, Apache,
`mod_php`, or an external PHP fallback.

## Start The Basic Fixture App

```bash
nix develop -c cargo run -p php_server --bin phrust-server -- --docroot fixtures/server/apps/basic/public --listen 127.0.0.1:8080
```

In another shell:

```bash
curl -i http://127.0.0.1:8080/
```

## Use A Config File

The server supports CLI flags and a simple TOML-style config file:

```bash
nix develop -c cargo run -p php_server --bin phrust-server -- --config path/to/server.toml
```

See [server functionality](server-functionality.md) for config keys,
timeouts, access-log settings, metrics-token handling, cache options, and TLS
options.

## Run Server Checks

```bash
nix develop -c just server-smoke
nix develop -c just server-compat-smoke all
nix develop -c just server-tls-smoke
nix develop -c just server-benchmark-smoke
nix develop -c just verify-server
```

## Inspect Request Performance

Per-request performance tracing is disabled by default. Enable it with
`--perf-trace <path>` or `PHRUST_PERF_TRACE=<path>`. Setting
`PHRUST_PERF_TRACE=1` writes JSONL to
`target/performance/server/perf-trace.jsonl`.

Each JSONL event records route resolution, body read, request-context
construction, entry-script cache lookup, VM execution, session seed/finalize,
response build, response bytes, diagnostics count, and cache/source-read deltas.
Failed PHP requests include the last failure phase that was reached.

`/__phrust/metrics` exposes aggregate phase counts/timing plus source-read and
cache-effectiveness counters for the entry script and include cache. It also
reports session seed/lazy-load/finalize/store counters: requests that never
activate a PHP session should increment seed/finalize counters without
incrementing session-store load/write counters. Header materialization counters
show how many incoming headers were seen, carried into the runtime context, or
skipped because an equivalent direct PHP server value already exists. The server
snapshots process environment variables at startup for normal request contexts;
restart the server to expose changed process environment values to PHP requests.

For deterministic WordPress-shaped request overhead checks, run:

```bash
nix develop -c just wordpress-like-hotpath-smoke
```

The smoke starts `phrust-server`, warms a local front-controller fixture, asserts
structural cache/phase counters instead of wall-clock thresholds, and writes a
local report under `target/performance/wordpress-like/`.

For an optional local real-WordPress diagnostic report, set
`PHRUST_WORDPRESS_DIR` and optionally `PHRUST_MYSQL_TEST_DSN`, then run:

```bash
nix develop -c just wordpress-real-perf-report
```

Missing WordPress or database prerequisites are reported as skips. Reports land
under `target/wordpress-real/` and treat latency numbers as advisory local
measurements.

## Related Docs

- [Server functionality](server-functionality.md)
- [Server architecture](server-architecture.md)
- [Server known gaps](server-known-gaps.md)
- [Cache architecture](cache-architecture.md)
- [API facades](api-facades.md)
