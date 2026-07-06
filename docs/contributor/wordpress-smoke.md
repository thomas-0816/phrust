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
nix develop -c just wordpress-root-benchmark
nix develop -c just wordpress-clone-churn-report
```

To benchmark an already running server:

```bash
PHRUST_WORDPRESS_URL=http://127.0.0.1:18080 \
  nix develop -c just wordpress-root-benchmark
```

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
