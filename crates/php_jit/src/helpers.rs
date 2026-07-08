//! Stable runtime helper symbol registry for JIT backends.

use std::mem::size_of;

/// Stable ABI fingerprint for the helper-symbol registry. Bumped whenever the
/// registry's symbol set or any helper ABI changes.
pub const JIT_HELPER_REGISTRY_ABI_HASH: u64 = 0x07c1_481f_0000_0005;

/// Helper completed successfully.
pub const JIT_HELPER_STATUS_OK: i32 = 0;
/// Helper could not produce a PHP-int result and the VM must fall back.
pub const JIT_HELPER_STATUS_FALLBACK: i32 = 1;
/// Native inline arithmetic overflowed and the VM must fall back.
pub const JIT_HELPER_STATUS_OVERFLOW: i32 = 2;
/// A copy-and-patch region requested a native→userland tail call: the region's
/// prefix ran, left each positional `Int` argument in its buffer slot (see
/// `copy_patch::TailCallPlan`), and returned without computing a result. The VM
/// bridge reads the argument slots and performs the userland call through the
/// normal interpreter path. This is a *region* return status alongside the
/// region's `0` (OK, result in `result_slot`) and `1` (guard/overflow side
/// exit); the value `3` is chosen so it never aliases the Cranelift ABI's
/// [`JIT_HELPER_STATUS_OVERFLOW`] (`2`).
pub const JIT_HELPER_STATUS_TAILCALL: i32 = 3;

/// Stable helper id.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct JitHelperId(pub u32);

/// Helper argument kind.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JitHelperArgKind {
    /// Opaque VM context handle.
    VmContext = 1,
    /// Opaque frame handle.
    Frame = 2,
    /// C-compatible ABI value.
    Value = 3,
    /// Raw signed 64-bit integer.
    I64 = 4,
    /// Raw unsigned 64-bit integer.
    U64 = 5,
}

/// Helper return kind.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JitHelperReturnKind {
    /// Helper returns a C-compatible ABI value.
    Value = 1,
    /// Helper returns a C-compatible region exit record.
    Exit = 2,
    /// Helper returns no value.
    Void = 3,
    /// Helper returns a status code and writes the value through an out pointer.
    Status = 4,
}

/// Stable helper symbol metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JitHelperSymbol {
    /// Stable helper id.
    pub id: JitHelperId,
    /// Link symbol name.
    pub name: &'static str,
    /// Argument kinds.
    pub args: &'static [JitHelperArgKind],
    /// Return kind.
    pub returns: JitHelperReturnKind,
    /// True when the helper may return an exception exit.
    pub can_throw: bool,
    /// True when helper can mutate VM-visible state.
    pub has_side_effects: bool,
    /// Short description for reports.
    pub description: &'static str,
}

const I64_I64_OUT_ARGS: &[JitHelperArgKind] = &[
    JitHelperArgKind::I64,
    JitHelperArgKind::I64,
    JitHelperArgKind::U64,
];
const I64_OUT_ARGS: &[JitHelperArgKind] = &[JitHelperArgKind::I64, JitHelperArgKind::U64];
const CONTEXT_VALUE_ARGS: &[JitHelperArgKind] =
    &[JitHelperArgKind::VmContext, JitHelperArgKind::Value];
const CONTEXT_VALUE_VALUE_ARGS: &[JitHelperArgKind] = &[
    JitHelperArgKind::VmContext,
    JitHelperArgKind::Value,
    JitHelperArgKind::Value,
];
const CONTEXT_VALUE_U64_ARGS: &[JitHelperArgKind] = &[
    JitHelperArgKind::VmContext,
    JitHelperArgKind::Value,
    JitHelperArgKind::U64,
];
const VALUE_ARGS: &[JitHelperArgKind] = &[JitHelperArgKind::Value];
const VALUE_OUT_ARGS: &[JitHelperArgKind] = &[JitHelperArgKind::Value, JitHelperArgKind::U64];
const VALUE_U64_OUT_ARGS: &[JitHelperArgKind] = &[
    JitHelperArgKind::Value,
    JitHelperArgKind::U64,
    JitHelperArgKind::U64,
];
const VALUE_U64_U64_ARGS: &[JitHelperArgKind] = &[
    JitHelperArgKind::Value,
    JitHelperArgKind::U64,
    JitHelperArgKind::U64,
];
const CONTEXT_FRAME_ARGS: &[JitHelperArgKind] =
    &[JitHelperArgKind::VmContext, JitHelperArgKind::Frame];

/// Stable helper registry.
pub const JIT_HELPER_SYMBOLS: &[JitHelperSymbol] = &[
    JitHelperSymbol {
        id: JitHelperId(1),
        name: "phrust_jit_i64_add_checked",
        args: I64_I64_OUT_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: false,
        has_side_effects: false,
        description: "checked PHP integer addition helper",
    },
    JitHelperSymbol {
        id: JitHelperId(2),
        name: "phrust_jit_i64_mul_checked",
        args: I64_I64_OUT_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: false,
        has_side_effects: false,
        description: "checked PHP integer multiplication helper",
    },
    JitHelperSymbol {
        id: JitHelperId(3),
        name: "phrust_jit_strlen_known",
        args: CONTEXT_VALUE_ARGS,
        returns: JitHelperReturnKind::Exit,
        can_throw: false,
        has_side_effects: false,
        description: "known-shape strlen helper",
    },
    JitHelperSymbol {
        id: JitHelperId(4),
        name: "phrust_jit_count_known",
        args: CONTEXT_VALUE_ARGS,
        returns: JitHelperReturnKind::Exit,
        can_throw: false,
        has_side_effects: false,
        description: "known-shape count helper",
    },
    JitHelperSymbol {
        id: JitHelperId(5),
        name: "phrust_jit_string_concat",
        args: CONTEXT_VALUE_VALUE_ARGS,
        returns: JitHelperReturnKind::Exit,
        can_throw: false,
        has_side_effects: true,
        description: "string concatenation helper with VM-owned allocation",
    },
    JitHelperSymbol {
        id: JitHelperId(6),
        name: "phrust_jit_packed_array_fetch",
        args: CONTEXT_VALUE_U64_ARGS,
        returns: JitHelperReturnKind::Exit,
        can_throw: false,
        has_side_effects: false,
        description: "read-only packed-array integer-index fetch helper",
    },
    JitHelperSymbol {
        id: JitHelperId(7),
        name: "phrust_jit_guard_failed",
        args: CONTEXT_FRAME_ARGS,
        returns: JitHelperReturnKind::Exit,
        can_throw: false,
        has_side_effects: false,
        description: "guard-failure side-exit helper",
    },
    JitHelperSymbol {
        id: JitHelperId(8),
        name: "php_jit_array_is_packed_ints",
        args: VALUE_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: false,
        has_side_effects: false,
        description: "conservative read-only packed-int array layout guard",
    },
    JitHelperSymbol {
        id: JitHelperId(9),
        name: "php_jit_array_len",
        args: VALUE_OUT_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: false,
        has_side_effects: false,
        description: "read-only packed-array length helper",
    },
    JitHelperSymbol {
        id: JitHelperId(10),
        name: "php_jit_array_fetch_int_slow",
        args: VALUE_U64_OUT_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: false,
        has_side_effects: false,
        description: "safe read-only packed-array integer fetch helper",
    },
    JitHelperSymbol {
        id: JitHelperId(11),
        name: "php_jit_property_load_monomorphic_fast",
        args: VALUE_U64_U64_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: false,
        has_side_effects: true,
        description: "guarded monomorphic property-load helper",
    },
    JitHelperSymbol {
        id: JitHelperId(12),
        name: "phrust_jit_record_array_lookup",
        args: CONTEXT_VALUE_VALUE_ARGS,
        returns: JitHelperReturnKind::Exit,
        can_throw: false,
        has_side_effects: true,
        description: "record-shape array lookup helper with symbol-guarded slot read",
    },
    JitHelperSymbol {
        id: JitHelperId(13),
        name: "phrust_jit_abs_i64",
        args: I64_OUT_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: false,
        has_side_effects: false,
        description: "pure PHP integer abs() helper; falls back on i64::MIN overflow",
    },
];

/// Looks up a helper by stable id.
#[must_use]
pub fn lookup_helper_by_id(id: JitHelperId) -> Option<&'static JitHelperSymbol> {
    JIT_HELPER_SYMBOLS.iter().find(|helper| helper.id == id)
}

/// Looks up a helper by symbol name.
#[must_use]
pub fn lookup_helper_by_name(name: &str) -> Option<&'static JitHelperSymbol> {
    JIT_HELPER_SYMBOLS.iter().find(|helper| helper.name == name)
}

/// Returns true when helper ids and names are sorted and unique.
#[must_use]
pub fn helper_registry_is_stable() -> bool {
    for (index, helper) in JIT_HELPER_SYMBOLS.iter().enumerate() {
        if helper.name.is_empty() {
            return false;
        }
        if index > 0 && JIT_HELPER_SYMBOLS[index - 1].id >= helper.id {
            return false;
        }
        for other in &JIT_HELPER_SYMBOLS[index + 1..] {
            if helper.id == other.id || helper.name == other.name {
                return false;
            }
        }
    }
    true
}

/// Returns a compact ABI layout summary.
#[must_use]
pub const fn helper_registry_layout_summary() -> (usize, usize, usize) {
    (
        size_of::<JitHelperId>(),
        size_of::<JitHelperArgKind>(),
        size_of::<JitHelperReturnKind>(),
    )
}

fn write_checked_result(out: *mut i64, value: Option<i64>) -> i32 {
    let Some(value) = value else {
        return JIT_HELPER_STATUS_FALLBACK;
    };
    let Some(out) = std::ptr::NonNull::new(out) else {
        return JIT_HELPER_STATUS_FALLBACK;
    };
    // SAFETY: The pointer is supplied by JIT-generated code for the duration
    // of the helper call. A null pointer is rejected above.
    unsafe {
        out.as_ptr().write(value);
    }
    JIT_HELPER_STATUS_OK
}

/// Checked PHP integer addition helper for Cranelift helper-call lowering.
///
/// SAFETY: The unmangled symbol is part of the stable performance helper registry.
/// Its C ABI, argument order, and status/out-pointer contract are documented in
/// `JIT_HELPER_SYMBOLS` and validated by registry layout tests.
#[unsafe(no_mangle)]
pub extern "C" fn phrust_jit_i64_add_checked(lhs: i64, rhs: i64, out: *mut i64) -> i32 {
    write_checked_result(out, lhs.checked_add(rhs))
}

/// Checked PHP integer multiplication helper for Cranelift helper-call lowering.
///
/// SAFETY: The unmangled symbol is part of the stable performance helper registry.
/// Its C ABI, argument order, and status/out-pointer contract are documented in
/// `JIT_HELPER_SYMBOLS` and validated by registry layout tests.
#[unsafe(no_mangle)]
pub extern "C" fn phrust_jit_i64_mul_checked(lhs: i64, rhs: i64, out: *mut i64) -> i32 {
    write_checked_result(out, lhs.checked_mul(rhs))
}

/// Pure PHP integer `abs()` helper for the copy-and-patch native tier.
///
/// Writes `*out = x.abs()` and returns [`JIT_HELPER_STATUS_OK`] for every `x`
/// except `i64::MIN`. For `i64::MIN` the magnitude does not fit in an `i64`, so
/// PHP returns a *float* (`9.2233720368547758E+18`); this helper reports
/// [`JIT_HELPER_STATUS_FALLBACK`] and leaves `out` unwritten so the emitted code
/// side-exits and the interpreter produces the float result. `checked_abs`
/// yields `None` exactly for `i64::MIN`, matching that boundary.
///
/// SAFETY: The unmangled symbol is part of the stable performance helper
/// registry. Its C ABI, argument order, and status/out-pointer contract are
/// documented in `JIT_HELPER_SYMBOLS` and validated by registry layout tests.
#[unsafe(no_mangle)]
pub extern "C" fn phrust_jit_abs_i64(x: i64, out: *mut i64) -> i32 {
    write_checked_result(out, x.checked_abs())
}

#[cfg(test)]
mod tests {
    use super::{
        JIT_HELPER_REGISTRY_ABI_HASH, JIT_HELPER_STATUS_FALLBACK, JIT_HELPER_STATUS_OK,
        JIT_HELPER_STATUS_OVERFLOW, JIT_HELPER_SYMBOLS, JitHelperArgKind, JitHelperId,
        JitHelperReturnKind, helper_registry_is_stable, helper_registry_layout_summary,
        lookup_helper_by_id, lookup_helper_by_name, phrust_jit_abs_i64, phrust_jit_i64_add_checked,
        phrust_jit_i64_mul_checked,
    };

    #[test]
    fn helper_registry_ids_names_and_layout_are_stable() {
        assert_ne!(JIT_HELPER_REGISTRY_ABI_HASH, 0);
        assert!(helper_registry_is_stable());
        assert_eq!(helper_registry_layout_summary(), (4, 4, 4));
        assert_eq!(
            JIT_HELPER_SYMBOLS.first().expect("first").id,
            JitHelperId(1)
        );
        assert_eq!(JIT_HELPER_SYMBOLS.last().expect("last").id, JitHelperId(13));
    }

    #[test]
    fn helper_registry_lookups_return_signatures() {
        let add = lookup_helper_by_name("phrust_jit_i64_add_checked").expect("add helper");
        assert_eq!(add.id, JitHelperId(1));
        assert_eq!(
            add.args,
            &[
                JitHelperArgKind::I64,
                JitHelperArgKind::I64,
                JitHelperArgKind::U64
            ]
        );
        assert_eq!(add.returns, JitHelperReturnKind::Status);
        assert!(!add.can_throw);
        assert!(!add.has_side_effects);

        let concat = lookup_helper_by_id(JitHelperId(5)).expect("concat helper");
        assert_eq!(concat.name, "phrust_jit_string_concat");
        assert!(concat.has_side_effects);

        let array_len = lookup_helper_by_name("php_jit_array_len").expect("array len helper");
        assert_eq!(array_len.id, JitHelperId(9));
        assert_eq!(
            array_len.args,
            &[JitHelperArgKind::Value, JitHelperArgKind::U64]
        );
        assert_eq!(array_len.returns, JitHelperReturnKind::Status);
        assert!(!array_len.can_throw);
        assert!(!array_len.has_side_effects);

        let abs = lookup_helper_by_name("phrust_jit_abs_i64").expect("abs helper");
        assert_eq!(abs.id, JitHelperId(13));
        assert_eq!(abs.args, &[JitHelperArgKind::I64, JitHelperArgKind::U64]);
        assert_eq!(abs.returns, JitHelperReturnKind::Status);
        assert!(!abs.can_throw);
        assert!(!abs.has_side_effects);

        let property_load = lookup_helper_by_name("php_jit_property_load_monomorphic_fast")
            .expect("property helper");
        assert_eq!(property_load.id, JitHelperId(11));
        assert_eq!(
            property_load.args,
            &[
                JitHelperArgKind::Value,
                JitHelperArgKind::U64,
                JitHelperArgKind::U64
            ]
        );
        assert_eq!(property_load.returns, JitHelperReturnKind::Status);
        assert!(!property_load.can_throw);
        assert!(property_load.has_side_effects);
    }

    #[test]
    fn int_helpers_write_results_and_report_overflow() {
        let mut out = 0;
        assert_eq!(
            phrust_jit_i64_add_checked(20, 22, &mut out),
            JIT_HELPER_STATUS_OK
        );
        assert_eq!(out, 42);
        assert_eq!(
            phrust_jit_i64_mul_checked(6, 7, &mut out),
            JIT_HELPER_STATUS_OK
        );
        assert_eq!(out, 42);
        assert_eq!(
            phrust_jit_i64_add_checked(i64::MAX, 1, &mut out),
            JIT_HELPER_STATUS_FALLBACK
        );
        assert_eq!(
            phrust_jit_i64_mul_checked(i64::MAX, 2, std::ptr::null_mut()),
            JIT_HELPER_STATUS_FALLBACK
        );
        assert_eq!(JIT_HELPER_STATUS_OVERFLOW, 2);
    }

    #[test]
    fn abs_helper_returns_magnitude_and_falls_back_on_int_min() {
        let mut out = 0;
        assert_eq!(phrust_jit_abs_i64(5, &mut out), JIT_HELPER_STATUS_OK);
        assert_eq!(out, 5);
        assert_eq!(phrust_jit_abs_i64(-7, &mut out), JIT_HELPER_STATUS_OK);
        assert_eq!(out, 7);
        assert_eq!(phrust_jit_abs_i64(0, &mut out), JIT_HELPER_STATUS_OK);
        assert_eq!(out, 0);

        // i64::MIN's magnitude overflows i64 (PHP returns a float), so the
        // helper falls back and leaves `out` untouched.
        out = 42;
        assert_eq!(
            phrust_jit_abs_i64(i64::MIN, &mut out),
            JIT_HELPER_STATUS_FALLBACK
        );
        assert_eq!(out, 42, "the fallback must not write a result");
        // A null out pointer is rejected rather than dereferenced.
        assert_eq!(
            phrust_jit_abs_i64(3, std::ptr::null_mut()),
            JIT_HELPER_STATUS_FALLBACK
        );
    }
}
