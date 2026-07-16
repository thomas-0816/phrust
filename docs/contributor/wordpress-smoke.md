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

The `web-install-page` phase accepts both the installation form and the
pre-configuration "Setup Configuration File" page. A WordPress error page is
still a failure even when the server returned HTTP 200.

## Fast native bring-up loop

Do not use a complete WordPress install as the first reproduction step for a
native lowering or verifier failure. Build the incremental, non-LTO cutover
binaries and probe the owning PHP file without executing it:

```bash
nix develop -c just wordpress-cutover-build
nix develop -c just -- wordpress-native-compile \
  /path/to/wordpress/wp-includes/class-wp-user-query.php --json
nix develop -c just -- wordpress-native-compile \
  /path/to/file.php --function 'ClassName::method' --json
```

`native-compile` runs the normal frontend and production Cranelift lowering
with the runtime helper ABI, but does not enter PHP code, start the server, or
connect to the database. A native rejection is a failing exit status, never a
skip. Reduce the failure to a generic fixture or focused Rust test, make the
probe pass, and only then repeat the relevant WordPress phase.

The `cutover` Cargo profile enables incremental compilation, uses optimization
level 2 and parallel code generation, and disables release LTO. The cutover
build wrapper also enables the pinned compiler's parallel frontend; set
`PHRUST_RUSTC_THREADS` to a positive integer to override its default of the
smaller of 20 threads or the number of online CPUs. The Nix development shell
applies the same frontend setting to all Phrust workspace crates and delegates
through `sccache`; registry dependencies keep their ordinary compiler flags.
The incremental cutover recipe bypasses `sccache`, because it rejects Cargo's
incremental mode. Profiling and final performance evidence must use the
canonical `profiling` or release profile.

`just wordpress-real-smoke` and `just wordpress-real-install-smoke` depend on
that same build and execute `target/cutover/php-vm` plus
`target/cutover/phrust-server`. Repeated real-application iterations therefore
reuse the incremental artifacts instead of rebuilding the changed `php_vm`
crate in the non-incremental debug profile. Cargo already schedules independent
crates across the available CPUs; near the end of the graph it is normal to see
one multi-threaded `rustc` process for a dependency-critical workspace crate.
The process count therefore does not measure compiler parallelism. Count its
threads and inspect Cargo timings before introducing a new crate boundary
solely for build speed. Run `nix develop -c just parallel-rustc-wrapper` to
verify the wrapper contract without rebuilding either crate.

PHP native compilation is a separate, request-time cost. Keep one explicit
cache directory per cutover binary and populate it once with read-write access;
then prove restart reuse from a fresh process with read-only access:

```bash
cache=target/wordpress-real/native-cache-cutover
target/cutover/php-vm run --clear-native-cache --native-cache-dir "$cache"
target/cutover/php-vm run \
  --native-cache read-write --native-cache-dir "$cache" \
  --native-cache-stats /path/to/front-controller.php
target/cutover/php-vm run \
  --native-cache read --native-cache-dir "$cache" \
  --native-cache-stats /path/to/front-controller.php
```

The read-only run must report a cache hit and zero native compile attempts.
Always use the exact same binary for both runs: the artifact identity includes
the build ID, Cranelift settings, target CPU, runtime/helper ABI, IR schema, and
semantic configuration. Rebuilding intentionally causes a miss rather than
loading stale machine code. Cache artifacts are bounded and validated; do not
raise the limits to hide unexpectedly large metadata.

Run `db-install` alone until it passes. After the first successful install,
save a deterministic database snapshot and restore it for login, frontpage,
plugin/theme/autoload, and callback iterations. The final combined workflow
still starts with a clean database.

The snapshot helper uses ordinary SQL metadata and row queries rather than
depending on a particular `mysqldump` version:

```bash
PHRUST_MYSQL_TEST_DSN='mysql://user:pass@127.0.0.1:3306/db' \
  nix develop -c just wordpress-db-snapshot dump \
    target/wordpress-real/snapshots/wordpress-installed.sql
PHRUST_MYSQL_TEST_DSN='mysql://user:pass@127.0.0.1:3306/db' \
  nix develop -c just wordpress-db-snapshot restore \
    target/wordpress-real/snapshots/wordpress-installed.sql
```

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
environment (`PHRUST_INCLUDE_REVALIDATE_MS`,
`PHRUST_WORKER_SYMBOL_EPOCH`, and `PHRUST_PERSISTENT_FEEDBACK`) and rejects an
inherited `PHRUST_PERF_ABLATION`. Use `wordpress-root-benchmark-feedback-ab` for
the isolated persistent-feedback arms plus their joint p50/p95/throughput ratio
report. Use `--engine-preset=baseline` or `--engine-preset=default` to compare
native compiler policies; both use the same Cranelift engine.

The benchmark's default HTTP timeout is 120 seconds and the locally launched
Phrust server receives the same value as its execution deadline. This permits
the unmeasured cold warmup to populate frontend and native worker caches. All
headline samples remain warm and instrumentation-free; lowering or compilation
during a measured sample is a benchmark failure to investigate, not part of the
reported application latency.

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

Run `wordpress-root-diagnostics` separately for Phrust native counters, request
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

Request-profile schema 4 separates server phases from native-engine counters.
Use phase timings first to decide whether time is in routing, request setup,
cache lookup, execution, session finalization, or response building. Enable
native counters only when the coarse phase split points at execution overhead,
because counter collection adds measurement cost. Schema 4 reports worker-level
native compile attempts and time, compile-record cache
hits/misses/waits/evictions, entry execution, region side exits, runtime-helper
calls, and published versions. The worker cache is bounded to 4,096 immutable
compiled-unit/function/policy keys. Every published function alias from one
Region graph points to the same immutable record set, so calls do not rebuild
an already available graph under another function key. It sits above Region IR
construction, while the lower Cranelift code manager continues to own and bound
executable generations. Include/eval and native child calls move request-owned
symbol/state maps into the synchronous child and return them afterward,
avoiding repeated whole-request clones without sharing PHP-visible mutable
state across requests. The schema contains no removed interpreter or
source-attribution families. Treat wall-clock numbers as local evidence unless
a dedicated gate records and compares a baseline.

`wordpress-root-profile` leaves warmups unprofiled and sends the explicit
profile trigger only for the measured request. Its server execution deadline is
derived from `--timeout-seconds`, and its diagnostic native-cache directory is
isolated below the selected output directory. These profiles are instrumented
diagnostics and are never eligible as clean benchmark evidence.
