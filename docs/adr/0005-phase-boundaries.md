# ADR 0005: Phase Boundaries

## Status

Accepted

## Context

The project goal is broad: a PHP 8.5 compatible core engine in Rust. Without
explicit phase boundaries, foundation work can drift into implementation before
the reference, tooling, and validation contract are stable.

## Decision

Phase 0 is limited to:

- Nix development environment.
- Rust workspace skeleton.
- Pinned PHP `8.5.7` reference contract.
- Reference bootstrap, verification, metadata, and optional CLI build scripts.
- Documentation for syntax, runtime, tests, risks, license/copying, and ADRs.
- CI preparation.

Phase 0 does not include:

- Engine implementation.
- Lexer implementation.
- Parser implementation.
- AST or CST implementation beyond placeholder crates.
- VM implementation.
- Runtime value representation.
- Extension implementation.
- Zend ABI emulation.

## Consequences

- Phase 0 changes are reviewable as infrastructure and documentation.
- Later phases can rely on `nix develop -c just verify-phase0`.
- Implementation work starts only after Phase 0 is green.

## Alternatives

- Start with a lexer prototype immediately. Rejected because the reference
  contract and test oracle should come first.
- Start with a VM/runtime prototype. Rejected because runtime compatibility
  needs a broader test and documentation map first.

## Date

2026-06-19
