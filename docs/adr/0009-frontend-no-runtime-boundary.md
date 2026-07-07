# ADR 0009: Semantic Frontend Runtime Boundary

## Status

Accepted

## Context

Semantic frontend work can easily expand into execution semantics, autoloading,
include/eval behavior, attributes, and runtime value modeling.

## Decision

Semantic frontend stops at semantic frontend output. It does not execute PHP files,
instantiate attributes, run include/require/eval, invoke autoloaders, create
runtime values, generate bytecode, dispatch VM opcodes, emulate Zend ABI, or
load extensions.

## Consequences

- Semantic frontend remains auditable against `php -l` and deterministic fixtures.
- Runtime-dependent behavior is marked as deferred metadata or known gaps.
- Runtime receives structured HIR and semantic metadata as its input contract.
