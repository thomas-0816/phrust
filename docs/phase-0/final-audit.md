# Phase 0 Final Audit

Date: 2026-06-19

Target: PHP `8.5.7`, tag `php-8.5.7`

Result: PASS

## Summary

Phase 0 is complete for the repository foundation. The project has a Nix Flake
development shell, a Rust workspace skeleton, a pinned PHP reference contract,
reference bootstrap/metadata/build scripts, documentation, fixture structure,
CI preparation, and a central verification command.

No PHP engine, lexer, parser, AST/CST, VM, runtime value model, JIT, extension,
or Zend ABI implementation was added.

## Fulfilled Points

- `flake.nix` and `flake.lock` exist.
- `nix develop` provides the Phase 0 development shell.
- `Cargo.toml` defines a minimal Rust workspace with placeholder crates only.
- `AGENTS.md` documents the Phase 0 working rules.
- `README.md` documents quickstart, target, scope, and CI commands.
- PHP reference target is consistently `8.5.7` / `php-8.5.7`.
- `references/php-src.lock.example.toml` exists.
- `references/php-src.lock.toml` was generated from the network reference.
- `references/php-src.metadata.json` was generated.
- `third_party/php-src` is ignored and is not intended for commit.
- Optional reference PHP CLI was built and reports PHP `8.5.7`.
- `token_get_all` is available in the built reference CLI.
- `docs/adr/*` contains the Phase 0 decisions.
- `docs/phase-0/*` contains compatibility, syntax, runtime, test, risk,
  license/copying, completion, and audit documentation.
- `tests/fixtures/*` contains only placeholder structure.
- `.github/workflows/phase0.yml` runs the required Nix-based Phase 0 gate.

## Commands Run

```bash
nix --extra-experimental-features 'nix-command flakes' flake show
nix --extra-experimental-features 'nix-command flakes' develop -c just help
nix --extra-experimental-features 'nix-command flakes' develop -c cargo fmt --all --check
nix --extra-experimental-features 'nix-command flakes' develop -c cargo clippy --workspace --all-targets -- -D warnings
nix --extra-experimental-features 'nix-command flakes' develop -c cargo test --workspace
nix --extra-experimental-features 'nix-command flakes' develop -c just bootstrap-ref
nix --extra-experimental-features 'nix-command flakes' develop -c just extract-ref-metadata
nix --extra-experimental-features 'nix-command flakes' develop -c just build-ref-php
nix --extra-experimental-features 'nix-command flakes' develop -c just verify-ref
nix --extra-experimental-features 'nix-command flakes' develop -c just verify-phase0
```

## Command Results

- `nix flake show`: passed.
- `just help`: passed.
- Rust formatting, linting, and tests: passed.
- `just bootstrap-ref`: passed; resolved commit
  `35eab8c08bc590758d05813b0ff7a3d8c3e67b79`.
- `just extract-ref-metadata`: passed.
- `just build-ref-php`: passed; CLI reports PHP `8.5.7`.
- `just verify-ref`: passed; `token_get_all` is available.
- `just verify-phase0`: passed.

## Open Optional Points

No Phase 0 blockers remain.

Future phases still need to implement actual compatibility work:

- Token fixtures against `token_get_all()`.
- Source/span primitives beyond placeholders.
- Lexer, parser, runtime, and PHPT test harness work.
- Composer and framework smoke tests.

These are explicitly outside Phase 0.
