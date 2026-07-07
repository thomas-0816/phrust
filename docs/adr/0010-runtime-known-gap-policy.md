# ADR 0010: Runtime Known-Gap Policy

## Status

Accepted

## Context

The runtime is intentionally incomplete. Unsupported behavior must be visible
without being misreported as implemented or silently skipped by tests.

## Decision

Every runtime compatibility gap needs a stable ID, a row in
`docs/runtime/known-gaps.md`, and either an executable fixture or an explicit
planned/deferred note with an example. Verifiers and smoke tests should keep
known gaps separate from passing fixtures and actual failures.

## Consequences

- Final status reports can distinguish green behavior, expected failures,
  known gaps, planned work, and skips.
- New runtime work must update fixtures, docs, and diagnostics together.
- A known gap can become implemented only when the status row and fixture proof
  are updated in the same change.
