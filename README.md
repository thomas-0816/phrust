# phrust

`phrust` is a Rust project intended to become a PHP 8.5 compatible core
engine.

Phase 0 is foundation only. Phase 1 adds the lexer/tokenization layer. The
project still does not implement a parser, AST/CST, VM, runtime values, JIT,
extensions, or Zend ABI emulation.

## Reference Target

- PHP series: `8.5`
- PHP version: `8.5.7`
- Git tag: `php-8.5.7`
- Repository: `https://github.com/php/php-src.git`

The reference version is fixed for Phase 0. It must not be automatically
advanced to a newer patch release without a new ADR.

## Development Environment

### Quickstart

1. Install Nix with Flake support.

2. Enter the development shell:

```bash
nix develop
```

3. List available commands:

```bash
just help
```

4. Run the required Phase 1 checks:

```bash
just verify-phase1
```

5. Fetch and pin the PHP reference:

```bash
just bootstrap-ref
```

6. Extract reference metadata:

```bash
just extract-ref-metadata
```

7. Optionally build the reference PHP CLI:

```bash
just build-ref-php
```

The central Phase 1 validation command can also be run without entering the
shell first:

```bash
nix develop -c just verify-phase1
```

## CI

Phase 0 and Phase 1 CI use Nix. Phase 1 runs:

```bash
nix develop -c just verify-phase1
```

Required CI does not clone or build `php-src`. Reference-dependent fixture
checks skip clearly if a PHP reference binary is unavailable.

## Phase 0 Scope

Phase 0 establishes:

- A pinned PHP 8.5.7 reference contract.
- Documentation for the authoritative PHP syntax and runtime sources.
- A minimal Rust workspace skeleton with placeholder crates:
  - `crates/php_source`
  - `crates/php_testkit`
- Scripts to fetch and verify the PHP reference.
- A test-oracle plan for lexer, parser, runtime, and framework compatibility.
- CI preparation around `nix develop -c just verify-phase0`.

Phase 0 explicitly does not build the engine.

It also does not implement a lexer, parser, AST/CST, VM, runtime value model,
JIT, extensions, or Zend ABI emulation.

## Phase 1 Scope

Phase 1 adds the PHP lexer/tokenization layer. It targets curated fixture
compatibility with:

```php
token_get_all($code, 0)
```

The central Phase 1 validation command is:

```bash
nix develop -c just verify-phase1
```

Phase 1 still does not implement a parser, AST/CST lowering, VM, runtime, JIT,
extensions, or Zend ABI emulation.

Useful Phase 1 commands:

```bash
export REFERENCE_PHP="$PWD/third_party/php-src/sapi/cli/php"
nix develop -c just lex tests/fixtures/lexer/010-tags.php
nix develop -c just lexer-fixtures
nix develop -c just lexer-diff
nix develop -c just lexer-diff-report
nix develop -c just fuzz-lexer-smoke
nix develop -c just bench-lexer
nix develop -c just lexer-corpus-smoke
```

`just lexer-fixtures` and `just lexer-diff` both run strict comparison for the
curated fixtures. `docs/phase-1/known-lexer-differences.md` records that no
curated fixture differences are currently accepted.

## Rust Workspace

The workspace uses Cargo resolver `3` and Rust edition `2024`. The current
crates are:

- `php_source`: byte-oriented source maps and spans.
- `php_lexer`: Phase 1 PHP lexer/tokenization library.
- `php_lexer_cli`: JSON output CLI for differential testing.
- `php_testkit`: reference testing helpers.

## Reference Source Policy

The `php-src` checkout is local only and will live under `third_party/php-src`.
It must not be committed. Reference metadata and lockfiles belong under
`references/`.

## Documentation

- [Phase 0 Definition of Done](docs/phase-0/phase-0-definition-of-done.md)
- [Phase 0 Risk Register](docs/phase-0/risk-register.md)
- [ADR 0001: Target PHP Version](docs/adr/0001-target-php-version.md)
- [ADR 0002: Nix Development Environment](docs/adr/0002-nix-dev-environment.md)
- [ADR 0003: Reference Oracle](docs/adr/0003-reference-oracle.md)
- [ADR 0004: No Vendored php-src](docs/adr/0004-no-vendored-php-src.md)
- [ADR 0005: Phase Boundaries](docs/adr/0005-phase-boundaries.md)
- [ADR 0006: Byte-Oriented Lossless Lexer](docs/adr/0006-byte-oriented-lossless-lexer.md)
- [ADR 0007: Token Oracle Normalization](docs/adr/0007-token-oracle-normalization.md)
- [ADR 0008: Lexer Parser Boundary](docs/adr/0008-lexer-parser-boundary.md)
- [Phase 1 Definition of Done](docs/phase-1/phase-1-definition-of-done.md)
- [Phase 1 Final Audit](docs/phase-1/final-audit.md)
- [Known Lexer Differences](docs/phase-1/known-lexer-differences.md)
- [Lexer to Parser Handoff](docs/phase-2/lexer-to-parser-handoff.md)
