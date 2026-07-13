# ADR 0021: Audited unchecked frame-slot access in `php_vm`

## Status

Accepted (owner-approved 2026-07-11), extends the ADR 0020 model to one
additional module.

## Context

`php_vm` enforces `#![deny(unsafe_code)]` at the crate root (ADR 0020),
with documented allowances only on the audited JIT surfaces. The
interpreter's register and local accesses go through checked accessors:
every dense operand read/write pays a slice bounds check whose outcome is
already decided at plan-build time — the dense bytecode verifier rejects
any unit containing a register operand `>= register_count` or a local
operand `>= local_count` (`bytecode::verify`, mandatory on every plan
build), and frames are sized to exactly those counts on entry
(`reset_for_function`). The feedback audit names a "verified unchecked
interpreter core" as a required P2 correction: validation that the
verifier already performed must not be re-paid per access.

## Decision

One additional module, `php_vm::frame_memory`, may use `unsafe` for
unchecked slot access, under the same regime as
`php_runtime::runtime_memory`:

- Every unsafe surface is recorded with its invariants in
  `docs/performance/vm-frame-safety-audit.md`, updated in the same commit
  as any change to the module.
- The soundness argument must tie each unchecked index to the dense
  verifier guarantee plus the frame-sizing site, and carry a
  `debug_assert!` restating the bound so debug builds still check.
- Callers outside the module never see `unsafe`; the exported functions
  are safe-to-misuse only insofar as their documented preconditions are
  discharged by the verifier — call sites must reference that argument.
- No other module in `php_vm` gains `unsafe` beyond the existing JIT
  allowances; extending the exception again requires a new ADR.

## Consequences

Dense-path operand reads and writes drop their per-access bounds checks
while behavior (including the uninitialized-register diagnostics) stays
identical; debug builds retain full checking. The module's audit ledger
and Miri-facing tests gate changes the same way `runtime_memory`'s do.
