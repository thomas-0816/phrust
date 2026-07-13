# VM frame-memory safety audit

Ledger of every unsafe surface in `php_vm::frame_memory` (the only
interpreter module exempt from the crate's `#![deny(unsafe_code)]`; see
ADR 0021 — the JIT surfaces carry their own audited allowances). Each
entry names the invariants that make the unsafe sound and the tests that
exercise them. Update this file in the same commit as any change to the
module's unsafe code.

## Unchecked dense operand slot access

`register_slot`, `take_register`, and `local_slot` index the frame's
register/local tables with `get_unchecked(_mut)`.

Invariants:

- Every dense operand index reaching these functions comes from an
  instruction of the currently executing dense function. The dense
  bytecode verifier (`bytecode::verify`, mandatory on every plan build —
  a failed verification rejects the plan) proves each register operand
  `< register_count` and each local operand `< local_count`. The two
  runtime-synthesized operands (the `CallFunctionDiscard` drains in
  `dense_dispatch`) reuse a verified instruction's `dst` register, so the
  bound holds transitively.
- The active frame's tables are sized to exactly the executing function's
  counts before any of its instructions dispatch (`reset_for_function` /
  `RegisterFile::new`/`LocalFile::new` on frame entry), so
  `index < count` implies `index < table.len()`.
- `take_register` takes `&mut RegisterFile`, so no aliasing borrow can
  exist during the slot replace.
- Debug builds restate `index < table.len()` with `debug_assert!`, so the
  full test suite and fixture gates run fully checked.

Behavior note: the accessors do not change diagnostics — the
uninitialized-register/-local handling stays at the call sites in
`vm::operand_read`, byte-identical to the checked path.

Tests: `frame_memory::tests` (in-bounds read/move round-trip; debug
bounds panic), plus the dense fixture and oracle gates which execute the
accessors on every dense operand.
