# 0020: Audited unsafe exception for the runtime-memory module

## Status

Accepted (owner-approved 2026-07-11).

## Context

`php_runtime` and `php_vm` forbid `unsafe` entirely; the hardening gate
enforced this with `-D unsafe-code` across both crates. Two structural
performance items from the engine audit cannot be built under that rule
together with the 16-byte `Value` pin:

- compact single-allocation strings (header with refcount, cached hash,
  interned symbol, and length followed directly by the bytes), and
- Zend-like contiguous array buckets.

Both require raw-layout allocation with interior-mutable header fields
behind a shared pointer, which safe Rust cannot express while the handle
stays pointer-sized. The audit itself prescribes the resolution: unsafe
confined to a small runtime-memory module with extensive invariants and
tests. The precedent is ADR 0018/0019, where `php_jit` unsafe is allowed
only in named, audited modules.

## Decision

- Unsafe enforcement moves from the gate's CLI flag into the crate roots:
  `#![deny(unsafe_code)]` in `php_runtime` and `php_vm`.
- Exactly one module, `php_runtime::runtime_memory`, may override the
  deny with `#[allow(unsafe_code)]`. It contains only low-level memory
  primitives with safe public APIs, documented invariants on every unsafe
  block, exhaustive unit tests, and Miri coverage through the existing
  `safety-audit-smoke` machinery.
- Every unsafe surface added to the module must be recorded in
  `docs/performance/runtime-memory-safety-audit.md` before it lands.
- All other modules in both crates remain forbidden from using unsafe;
  new exceptions require a new ADR.

## Consequences

Compact strings and contiguous arrays become implementable behind safe
APIs. The blast radius of unsafe stays reviewable: one module, one audit
document, deny-by-default everywhere else.
