# ADR 0005: Layer Boundaries

## Status

Accepted

Historical. This ADR records the original foundation boundary. The current
architecture is broader and is governed by the live layer docs, including
`docs/api-facades.md`, `docs/runtime-module-boundaries.md`, and the
source-integrity ratchets.

## Context

The project goal is broad: a PHP 8.5 compatible core engine in Rust. Without
explicit layer boundaries, foundation work can drift into implementation before
the reference, tooling, and validation contract are stable.

## Decision

Foundation is limited to:

- Nix development environment.
- Rust workspace skeleton.
- Pinned PHP `8.5.7` reference contract.
- Reference bootstrap, verification, metadata, and optional CLI build scripts.
- Documentation for syntax, runtime, tests, risks, license/copying, and ADRs.
- CI preparation.

Foundation does not include:

- Engine implementation.
- Lexer implementation.
- Parser implementation.
- AST or CST implementation beyond placeholder crates.
- VM implementation.
- Runtime value representation.
- Extension implementation.
- Zend ABI emulation.

## Consequences

- Foundation changes are reviewable as infrastructure and documentation.
- Later layers can rely on `nix develop -c just verify-foundation`.
- Implementation work starts only after Foundation is green.

## Alternatives

- Start with a lexer prototype immediately. Rejected because the reference
  contract and test oracle should come first.
- Start with a VM/runtime prototype. Rejected because runtime compatibility
  needs a broader test and documentation map first.

## Date

2026-06-19
