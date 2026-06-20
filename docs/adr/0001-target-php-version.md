# ADR 0001: Target PHP Version

## Status

Accepted

## Context

The project needs a fixed PHP behavior target before any compatibility work can
start. PHP patch releases may change syntax details, diagnostics, tests, or
runtime behavior. Phase 0 must therefore freeze a concrete reference version.

## Decision

Phase 0 targets PHP `8.5.7` from `https://github.com/php/php-src.git`, using
Git tag `php-8.5.7`.

The resolved commit will be written to `references/php-src.lock.toml` once the
reference checkout is bootstrapped.

The bootstrap process is:

```bash
nix develop -c just bootstrap-ref
```

That command creates or updates `third_party/php-src`, resolves the checked-out
commit, verifies the critical files, and writes the lockfile.

## Consequences

- All later compatibility work uses PHP `8.5.7` as the primary target.
- The target version must not be automatically advanced.
- Updating to a later PHP patch release requires a new ADR or an explicit
  update to this ADR.
- Local reference checkouts belong under `third_party/php-src` and are ignored
  by Git.
- The lockfile records the exact commit used for compatibility work.

## Alternatives

- Track the moving `PHP-8.5` branch. Rejected because it is not reproducible.
- Target an older stable PHP version. Rejected because the stated project goal
  is complete PHP 8.5 syntax support.

## Date

2026-06-19
