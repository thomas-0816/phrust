# ADR 0008: Parser Error Recovery

## Status

Accepted

## Context

The parser must handle invalid PHP source from editors, fixtures, fuzz tests,
and partial files. It cannot panic or loop forever on malformed input.

## Decision

Parser functions must either consume input, complete successfully, or recover at
well-defined synchronization points. Invalid regions are represented with
diagnostics and error nodes.

Diagnostics use stable IDs, byte spans, expected syntax sets, and recovery
metadata. Exact PHP error text matching is not required for the parser layer.

## Consequences

- Error output is stable enough for snapshots.
- Recovery behavior is testable independently from semantic validation.
- Grammar code must be careful to guarantee progress in every loop.
