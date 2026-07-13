# WordPress Smoke Workflow

WordPress is an optional real-application smoke target for exercising Phrust's
server, runtime, standard library, and performance tooling together. It is not a
source of WordPress-specific compatibility behavior: fixes must implement PHP
features or PHP-visible semantics in the owning Phrust layer.

## Setup

Use a local WordPress checkout outside the repository and point the smoke tools
at it:

```bash
PHRUST_WORDPRESS_DIR=/path/to/wordpress \
  nix develop -c just wordpress-root-profile
```

If the smoke needs a database, provide the normal local test DSN:

```bash
PHRUST_WORDPRESS_DIR=/path/to/wordpress \
PHRUST_MYSQL_TEST_DSN='mysql://user:pass@127.0.0.1:3306/db' \
  nix develop -c just wordpress-real-perf-report
```

Missing WordPress or database prerequisites are skips, not passes. Generated
reports land under `target/wordpress-real/` or
`target/performance/wordpress-root/`.

## Useful Commands

```bash
nix develop -c just wordpress-root-profile
nix develop -c just wordpress-reference-image
nix develop -c just wordpress-root-benchmark
nix develop -c just wordpress-root-benchmark-feedback-ab
nix develop -c just wordpress-root-benchmark-cranelift
nix develop -c just wordpress-root-diagnostics
nix develop -c just wordpress-clone-churn-report
```

`wordpress-root-benchmark` is a clean timing and compatibility comparison. It
builds the telemetry-free `release-lean` Phrust server, launches it with an
immutable deployment root, launches stock PHP-FPM 8.5.7 with OPcache behind
nginx, warms both engines, and takes 30 requests at concurrency 1, the available
CPU count, and twice that count. Build the pinned reference image once first.
Both engines use the same WordPress docroot and database configuration. Strict
mode requires `PHRUST_WORDPRESS_DB_IDENTITY` (for example the SHA-256 of the
restored SQL dump) so the database snapshot is recorded rather than assumed.

The clean launcher owns and records the performance-sensitive Phrust
environment (`PHRUST_JIT_COPY_PATCH`, `PHRUST_INCLUDE_REVALIDATE_MS`,
`PHRUST_WORKER_SYMBOL_EPOCH`, and `PHRUST_PERSISTENT_FEEDBACK`) and rejects an
inherited `PHRUST_PERF_ABLATION`. Use `wordpress-root-benchmark-feedback-ab` for
the isolated persistent-feedback arms plus their joint p50/p95/throughput ratio
report, and
`wordpress-root-benchmark-cranelift` for the explicitly featured
`experimental-jit` arm.

The clean report compares HTTP status, normalized headers, and body hashes. Add
application-specific observable endpoints without changing the sampler:

```bash
PHRUST_WORDPRESS_DIR=/path/to/wordpress \
PHRUST_WORDPRESS_DB_IDENTITY=fixture-sha256 \
PHRUST_WORDPRESS_HOST_HEADER=127.0.0.1 \
  nix develop -c just -- wordpress-root-benchmark \
    --observable root=/ --observable state=/benchmark-observable.php
```

Strict clean timing requires the benchmark path to return HTTP 200. Set the
host header to the fixture's configured WordPress site host so a canonical-host
redirect cannot be mistaken for a rendered application request.

The report hashes the WordPress tree deterministically (without exposing file
contents) and records a Git commit only when the docroot itself is a worktree.
Strict baseline comparisons require the same tree hash, database identity,
measurement model, PHP image, and OPcache configuration. Clean mode rejects
inherited Phrust trace or request-profile environment variables instead of
silently timing an instrumented server.

To benchmark already running servers, supply both URLs. Strict remote runs also
require the asserted pinned PHP version:

```bash
PHRUST_WORDPRESS_PHRUST_URL=http://127.0.0.1:18080 \
PHRUST_WORDPRESS_PHP_URL=http://127.0.0.1:18081 \
PHRUST_WORDPRESS_PHP_VERSION=8.5.7 \
  nix develop -c just -- wordpress-root-benchmark --strict
```

Run `wordpress-root-diagnostics` separately for Phrust VM counters, request
profiles, and traces. Its JSON explicitly sets `timing_eligible` to `false`, so
instrumented samples cannot be mistaken for clean latency evidence. Process CPU
and peak RSS are collected from `/proc` for locally launched engines; remote
targets record those fields as unsupported.

The manual CI workflow input `run_wordpress_performance=true` invokes strict
mode. A runner selecting that input must provide the repository variables
`PHRUST_WORDPRESS_DIR`, `PHRUST_WORDPRESS_DB_IDENTITY`,
`PHRUST_WORDPRESS_HOST_HEADER`, and `PHRUST_WORDPRESS_ROOT_BASELINE_JSON`;
missing or incorrect inputs fail rather than being reported as a passing skip.

## Reading Profiles

Request profiles separate server phases from VM work. Use phase timings first
to decide whether time is in routing, request setup, cache lookup, execution,
session finalization, or response building. Enable VM counters or source
attribution only when the coarse phase split points at execution overhead,
because attribution intentionally adds measurement cost.

The useful attribution families are clone/COW churn, dense-vs-rich execution,
include/cache behavior, builtin dispatch, array/object paths, output buffering,
and native-tier counters. Treat wall-clock numbers as local evidence unless a
dedicated gate records and compares a baseline.
