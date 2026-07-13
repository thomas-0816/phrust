# Validation Strategy

Validation is split into one broad Rust baseline and focused domain gates.

## Ownership

- `just test` is the only active aggregate recipe that owns
  `cargo test --workspace`.
- `just ci-rust` owns formatting, Clippy, and the workspace test baseline.
- `just ci-domain-gates` runs frontend, runtime, standard-library, server, and
  performance checks after that baseline. Domain gates run only their focused
  fixtures, package tests, differential checks, and smoke tests.
- `just ci-local` composes `quality-fast`, `ci-rust`, the domain gates, and the
  PHPT smoke gate. It must not replay the workspace test from a domain gate.

Standalone domain gates intentionally do not run the full workspace test. Use
the owning aggregate (`just check`, `just verify`, or `just ci-local`) when a
broad repository result is required.

## Regression Guard

`scripts/verify/validation_strategy.py` parses the active `justfile` recipe
graph and fails when a domain gate directly or transitively reintroduces broad
Rust validation. It is part of `architecture-guardrails`, so
`source-integrity`, `quality-fast`, and the aggregate validation paths enforce
the composition rule.
