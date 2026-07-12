//! Audited unchecked frame-slot access (ADR 0021).
//!
//! This is the only interpreter module in `php_vm` allowed to use `unsafe`
//! (the JIT surfaces carry their own audited allowances); every unsafe
//! surface here is recorded with its invariants in
//! `docs/performance/vm-frame-safety-audit.md`. The exported functions are
//! safe, but carry a precondition their callers must discharge through the
//! dense bytecode verifier:
//!
//! - every executed dense operand index is verifier-proven to be smaller
//!   than the owning function's `register_count`/`local_count`
//!   (`bytecode::verify` runs on every plan build and rejects violations),
//! - the active frame's tables are sized to exactly those counts before any
//!   of its instructions dispatch (`reset_for_function` on frame entry),
//!
//! so `index < table.len()` holds for every call that passes an operand of
//! the currently executing dense function. Debug builds restate the bound
//! with `debug_assert!`.
use crate::frame::{LocalFile, RegisterFile};
use php_runtime::api::{Slot, TempValue, Value};

/// Borrows the register slot for a verified dense register operand.
#[must_use]
#[allow(unsafe_code)]
pub(crate) fn register_slot(registers: &RegisterFile, index: u32) -> &TempValue {
    let slots = registers.temp_slots();
    debug_assert!(
        (index as usize) < slots.len(),
        "dense register operand r{index} outside frame of {} slots",
        slots.len()
    );
    // SAFETY: the dense verifier proves the operand index is smaller than
    // the executing function's register count, and the frame was reset to
    // exactly that count on entry (module contract above).
    unsafe { slots.get_unchecked(index as usize) }
}

/// Moves a verified dense register operand out, leaving it uninitialized.
#[must_use]
#[allow(unsafe_code)]
pub(crate) fn take_register(registers: &mut RegisterFile, index: u32) -> Value {
    let slots = registers.temp_slots_mut();
    debug_assert!(
        (index as usize) < slots.len(),
        "dense register operand r{index} outside frame of {} slots",
        slots.len()
    );
    // SAFETY: as in [`register_slot`]; the exclusive borrow additionally
    // guarantees no aliasing access for the duration of the replace.
    let slot = unsafe { slots.get_unchecked_mut(index as usize) };
    std::mem::replace(slot, TempValue::uninitialized()).into_value()
}

/// Borrows the local slot for a verified dense local operand.
#[must_use]
#[allow(unsafe_code)]
pub(crate) fn local_slot(locals: &LocalFile, index: u32) -> &Slot {
    let slots = locals.slot_table();
    debug_assert!(
        (index as usize) < slots.len(),
        "dense local operand local:{index} outside frame of {} slots",
        slots.len()
    );
    // SAFETY: as in [`register_slot`], via the verifier's local-count rule.
    unsafe { slots.get_unchecked(index as usize) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessors_read_and_move_in_bounds_slots() {
        let mut registers = RegisterFile::new(3);
        registers
            .set(php_ir::ids::RegId::new(1), Value::Int(7))
            .expect("in-bounds write");
        assert_eq!(register_slot(&registers, 1).value(), &Value::Int(7));
        assert_eq!(take_register(&mut registers, 1), Value::Int(7));
        assert!(register_slot(&registers, 1).value().is_uninitialized());

        let locals = LocalFile::new(2);
        assert!(local_slot(&locals, 1).read().is_uninitialized());
    }

    #[test]
    #[should_panic(expected = "outside frame")]
    #[cfg(debug_assertions)]
    fn debug_builds_still_check_bounds() {
        let registers = RegisterFile::new(1);
        let _ = register_slot(&registers, 5);
    }
}
