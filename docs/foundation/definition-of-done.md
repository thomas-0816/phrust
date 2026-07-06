# Foundation Definition of Done

Foundation is complete when the repository has a reproducible project foundation
for a PHP 8.5 compatible core engine in Rust.

## Required Outcomes

- `flake.nix` and `flake.lock` define the Nix Flake development environment.
- `nix develop` opens a shell with the Rust and PHP reference build tools.
- The PHP reference target is fixed to PHP `8.5.7`, tag `php-8.5.7`, from
  `https://github.com/php/php-src.git`.
- The reference commit is documented in a lockfile once the reference has been
  bootstrapped.
- The authoritative PHP sources are documented:
  - `Zend/zend_language_scanner.l`
  - `Zend/zend_language_parser.y`
  - `Zend/zend_vm_def.h`
  - Relevant Zend, compiler, AST, and `.phpt` test files.
- A minimal Rust workspace skeleton exists for future layers:
  - `Cargo.toml`
  - `crates/php_source`
  - `crates/php_testkit`
- Workspace checks pass:
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
- Scripts exist to fetch, verify, and extract metadata from the PHP reference.
- `references/php-src.metadata.json` is generated when the local PHP reference
  checkout is available.
- Optional scripts exist to build a minimal reference PHP CLI.
- The test-oracle strategy is documented:
  - Token compatibility against `token_get_all()`.
  - Parse compatibility against `php -l`.
  - Runtime compatibility against `.phpt`.
  - Composer and framework smoke tests in later layers.
- `AGENTS.md` documents the Foundation working rules.
- `scripts/verify/foundation.sh` is the central verification script and is
  exposed through `just verify-foundation`.
- Developer quickstart documents `nix develop`, `just help`,
  `just verify-foundation`, `just bootstrap-ref`, `just extract-ref-metadata`, and
  optional `just build-ref-php`.
- `README.md` explains the project goal, quickstart, and boundaries.
- `docs/adr/*` records the Foundation decisions.
  - ADR 0001: target PHP version.
  - ADR 0002: Nix development environment.
  - ADR 0003: reference oracle.
  - ADR 0004: no vendored `php-src`.
  - ADR 0005: Foundation boundaries.
- `docs/foundation/**` records compatibility, syntax, runtime, test, risk, and
  completion criteria.
- `docs/foundation/license-and-copying-policy.md` documents how the project uses
  `php-src` without committing a vendored source copy.
- `docs/foundation/validation-summary.md` records the Foundation audit result.
- `.github/workflows/ci.yml` runs the shared Nix-based repository gates in CI;
  foundation-specific changes should still pass `just verify-foundation`.
- `references/php-src.lock.example.toml` exists.
- Local `php-src` checkouts are ignored and are not committed.

## Explicit Non-Goals

Foundation does not include:

- PHP lexer implementation.
- PHP parser implementation.
- AST or CST implementation beyond future placeholder crates.
- VM implementation.
- Runtime value representation.
- JIT integration.
- Extensions.
- Zend ABI emulation.

## Acceptance Gate

The required acceptance command is:

```bash
nix develop -c just verify-foundation
```

Until Nix and `just` are added, bootstrap validation is limited to static file
checks.
