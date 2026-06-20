# ADR 0002: Nix Development Environment

## Status

Accepted

## Context

The project needs one reproducible development environment for Rust tooling,
PHP reference build tools, scripts, and CI. The PHP reference build requires a
large set of platform packages, so ad hoc setup would be fragile.

## Decision

Use Nix Flakes and `nix develop` as the primary development environment.

`flake.lock` will be committed so developers and CI use the same Nix inputs.
Docker is not the primary development environment for Phase 0.

The dev shell provides:

- Rust tooling: compiler, Cargo, rustfmt, Clippy, and rust-analyzer.
- Project tools: Git, curl, wget, `just`, jq, ripgrep, fd, and tree.
- PHP reference build tools: autoconf, automake, libtool, bison, re2c, make,
  pkg-config, ccache, CMake, Ninja, and Clang.
- PHP-adjacent libraries: libxml2, SQLite, OpenSSL, zlib, bzip2, xz, and
  libzip.
- Python 3 for deterministic metadata scripts.

## Consequences

- Validation commands should run through `nix develop -c ...` once the flake
  exists.
- CI should install Nix and run the same Phase 0 verification command.
- Platform-specific tool differences should be expressed in `flake.nix`.

## Alternatives

- Docker Compose as the primary development environment. Rejected for Phase 0
  because Nix Flakes are the required workflow.
- Host-installed toolchains. Rejected because they are not reproducible enough.

## Date

2026-06-19
