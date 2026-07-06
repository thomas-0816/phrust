# Foundation Validation Summary

Target: PHP `8.5.7`, tag `php-8.5.7`.

The foundation layer defines the repository contract for the pinned PHP
reference, Nix development shell, Rust workspace, source-integrity rules, and
validation commands. It does not provide engine execution behavior.

## Current Contract

- `flake.nix` and `flake.lock` provide the Nix development environment.
- `Cargo.toml` defines the Rust workspace.
- `AGENTS.md`, `README.md`, and `docs/contributing.md` describe workflow,
  scope, and validation rules.
- `references/php-src.lock.example.toml`,
  `references/php-src.lock.toml`, and `references/php-src.metadata.json`
  record the pinned PHP reference metadata.
- `third_party/php-src` is a local-only reference checkout and must not be
  committed.
- `docs/adr/0001-target-php-version.md` through
  `docs/adr/0005-layer-boundaries.md` define the initial architectural
  decisions.
- `docs/foundation/` records compatibility target, syntax sources, runtime
  boundaries, test matrix, copying policy, risk register, and definition of
  done.

## Validation

Use the foundation gate when changing reference metadata, Nix setup, workspace
structure, or foundation documentation:

```bash
nix develop -c just verify-foundation
```

The gate checks required files, required content markers, reference bootstrap
scripts, Rust formatting/lint/tests, the optional reference lockfile, and the
optional local `third_party/php-src` checkout when present.

Reference-dependent checks must skip clearly when no PHP reference binary is
available and must be strict when `REFERENCE_PHP` is explicitly set.

## Boundaries

Foundation documentation is a current contract, not a task archive. PHP engine
behavior belongs in the owning lexer, syntax, frontend, runtime, standard
library, server, performance, or PHPT layer documentation.
