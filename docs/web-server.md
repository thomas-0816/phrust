# Run The Web Server

Phrust includes a PHP-compatible built-in server front door:
`phrust-php -S <addr> [-t <docroot>] [router]`. It executes PHP through the
workspace frontend, runtime, and VM. It does not use FPM, FastCGI, CGI, Apache,
`mod_php`, or an external PHP fallback.

## Start The Basic Fixture App

```bash
nix develop -c cargo run -p php_vm_cli --bin phrust-php -- -S 127.0.0.1:8080 -t fixtures/server/apps/basic/public
```

In another shell:

```bash
curl -i http://127.0.0.1:8080/
```

## Use A Config File

The advanced `phrust-server` binary supports server-specific CLI flags and a
simple TOML-style config file:

```bash
nix develop -c cargo run -p php_server --bin phrust-server -- --config path/to/server.toml
```

See [server functionality](server-functionality.md) for config keys,
timeouts, access-log settings, metrics-token handling, cache options, and TLS
options.

HTTP admission and PHP CPU execution are bounded independently.
`max_in_flight` limits accepted requests, while `cpu_execution_limit` (or
`--cpu-execution-limit`) defaults to the host's available parallelism and
limits CPU-bound PHP work. A saturated CPU gate queues for at most
`request_timeout_ms`; that queue budget is separate from the cooperative
`max_execution_ms` deadline, which starts when execution begins. Queue and
execution state is request-local and permits are released on cancellation.
The metrics endpoint exposes admitted, queued, current, saturated, rejected,
cancelled, and queue-timeout totals plus the cumulative `cpu_queue` phase.

Prefix request rewrites are a webserver-only routing feature. Configure them
with `--rewrite-prefix-query /api=route` or
`rewrite_prefix_query = "/api=route"` for `phrust-server`, or set
`PHRUST_SERVER_REWRITE_PREFIX_QUERY=/api=route` for the PHP-compatible
`phrust-php -S` entrypoint. Matching requests execute through `/` while
prepending the matched suffix as a query parameter. The PHP engine only sees the
resulting ordinary request URI and query string; it does not know which rewrite
rule, if any, was applied.

## Run Server Checks

```bash
nix develop -c just server-smoke
nix develop -c just cli-server-smoke
nix develop -c just verify-user-interfaces
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

Each JSONL event records route resolution, body read, CPU queue wait, request-context
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
Persistent-engine metrics distinguish immutable metadata reuse from request
state. Script/include cache hits and quickening feedback templates may persist
across requests; PHP globals, request context, output buffers, sessions, and
runtime values are reset per request. A request-local reset is therefore counted
as a reset, not as rejected persistence.

For deterministic front-controller request overhead checks, run:

```bash
nix develop -c just front-controller-hotpath-smoke
```

The smoke starts `phrust-server`, warms a local front-controller fixture, asserts
structural cache/phase counters instead of wall-clock thresholds, and writes a
local report under `target/performance/front-controller-hotpath/`.

For an optional local real-WordPress diagnostic report, set
`PHRUST_WORDPRESS_DIR` and optionally `PHRUST_MYSQL_TEST_DSN`, then run:

```bash
nix develop -c just wordpress-real-perf-report
```

Missing WordPress or database prerequisites are reported as skips. Reports land
under `target/wordpress-real/` and treat latency numbers as advisory local
measurements.

For a real WordPress root request-profile JSON plus markdown summary, set
`PHRUST_WORDPRESS_DIR` and run:

```bash
nix develop -c just wordpress-root-profile
```

For the clean root-page benchmark, first build the pinned PHP-FPM 8.5.7 image,
then point the tool at a WordPress tree:

```bash
PHRUST_WORDPRESS_DIR=/path/to/wordpress \
  nix develop -c just wordpress-reference-image
PHRUST_WORDPRESS_DIR=/path/to/wordpress \
  nix develop -c just wordpress-root-benchmark
```

The helper starts release Phrust and stock PHP-FPM with OPcache behind nginx.
Reports land under `target/performance/wordpress-root/` and include p50/p95,
throughput, CPU/RSS where supported, response equivalence, identities, and
Phrust/PHP ratios. Use `just wordpress-root-diagnostics` for a separate
instrumented Phrust pass; diagnostic samples are never mixed into clean timing.

See [WordPress smoke workflow](contributor/wordpress-smoke.md) for the profile
schema and how to interpret clone, fallback, dense/rich, array, object, builtin,
include, output, and native attribution families.

To focus specifically on value churn after a profile exists, run:

```bash
nix develop -c just wordpress-clone-churn-report
```

The report lands under `target/performance/clone-churn/` and ranks value clone,
array-handle clone, COW separation, reference-cell creation, and by-reference
fallback source families. Set `PHRUST_CLONE_CHURN_BASELINE` to an earlier
request-profile JSON to include before/after clone counter deltas.

## Related Docs

- [Server functionality](server-functionality.md)
- [WordPress smoke workflow](contributor/wordpress-smoke.md)
- [PHP user interface matrix](user/php-user-interface-matrix.md)
- [Switching from PHP](user/switching-from-php.md)
- [Server architecture](server-architecture.md)
- [Server known gaps](server-known-gaps.md)
- [Cache architecture](runtime/cache-architecture.md)
- [API facades](api-facades.md)
