//! Stable runtime helper symbol registry for the native compiler.

use std::mem::size_of;

pub use php_runtime::api::JitHelperId;

/// Stable ABI fingerprint for the helper-symbol registry. Bumped whenever the
/// registry's symbol set or any helper ABI changes.
pub const JIT_HELPER_REGISTRY_ABI_HASH: u64 = 0x08c1_4820_0000_0022;

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
    /// Helper returns a two-word value/status record in result registers.
    ValueStatus = 5,
    /// Exact handler returns a two-word typed control result in registers.
    ControlResult = 6,
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

const CONTEXT_VALUE_ARGS: &[JitHelperArgKind] = &[JitHelperArgKind::Value];
const NATIVE_CONTEXT_POINTERS_ARGS: &[JitHelperArgKind] = &[
    JitHelperArgKind::VmContext,
    JitHelperArgKind::U64,
    JitHelperArgKind::U64,
];
const NATIVE_FUNCTION_RESOLVE_ARGS: &[JitHelperArgKind] = &[
    JitHelperArgKind::VmContext,
    JitHelperArgKind::U64,
    JitHelperArgKind::U64,
];
const NATIVE_FRAME_ALLOC_ARGS: &[JitHelperArgKind] = &[
    JitHelperArgKind::VmContext,
    JitHelperArgKind::U64,
    JitHelperArgKind::U64,
];
const NATIVE_FRAME_RELEASE_ARGS: &[JitHelperArgKind] =
    &[JitHelperArgKind::VmContext, JitHelperArgKind::U64];
const NATIVE_OP_0_ARGS: &[JitHelperArgKind] = &[JitHelperArgKind::I64];
const NATIVE_OP_1_ARGS: &[JitHelperArgKind] = &[JitHelperArgKind::I64, JitHelperArgKind::Value];
const NATIVE_OP_2_ARGS: &[JitHelperArgKind] = &[
    JitHelperArgKind::I64,
    JitHelperArgKind::Value,
    JitHelperArgKind::Value,
];
const NATIVE_OP_3_ARGS: &[JitHelperArgKind] = &[
    JitHelperArgKind::I64,
    JitHelperArgKind::Value,
    JitHelperArgKind::Value,
    JitHelperArgKind::Value,
];
const NATIVE_OP_4_ARGS: &[JitHelperArgKind] = &[
    JitHelperArgKind::I64,
    JitHelperArgKind::Value,
    JitHelperArgKind::Value,
    JitHelperArgKind::Value,
    JitHelperArgKind::Value,
];
const NATIVE_OP_5_ARGS: &[JitHelperArgKind] = &[
    JitHelperArgKind::I64,
    JitHelperArgKind::Value,
    JitHelperArgKind::Value,
    JitHelperArgKind::Value,
    JitHelperArgKind::Value,
    JitHelperArgKind::Value,
];
const NATIVE_CONTEXT_VALUE_OUT_ARGS: &[JitHelperArgKind] =
    &[JitHelperArgKind::Value, JitHelperArgKind::U64];
const NATIVE_CONTEXT_VALUE_OUT_3_ARGS: &[JitHelperArgKind] = &[
    JitHelperArgKind::Value,
    JitHelperArgKind::U64,
    JitHelperArgKind::U64,
    JitHelperArgKind::U64,
];
const NATIVE_BUILTIN_DISPATCH_ARGS: &[JitHelperArgKind] = &[
    JitHelperArgKind::I64,
    JitHelperArgKind::I64,
    JitHelperArgKind::I64,
    JitHelperArgKind::I64,
    JitHelperArgKind::I64,
    JitHelperArgKind::U64,
    JitHelperArgKind::I64,
    JitHelperArgKind::U64,
    JitHelperArgKind::I64,
    JitHelperArgKind::U64,
];
const NATIVE_SEMANTIC_DISPATCH_ARGS: &[JitHelperArgKind] = &[
    JitHelperArgKind::I64,
    JitHelperArgKind::I64,
    JitHelperArgKind::I64,
    JitHelperArgKind::I64,
    JitHelperArgKind::U64,
    JitHelperArgKind::I64,
    JitHelperArgKind::U64,
];
const NATIVE_EXACT_BUILTIN_6_ARGS: &[JitHelperArgKind] = &[
    JitHelperArgKind::I64,
    JitHelperArgKind::I64,
    JitHelperArgKind::I64,
    JitHelperArgKind::I64,
    JitHelperArgKind::Value,
    JitHelperArgKind::Value,
    JitHelperArgKind::Value,
    JitHelperArgKind::Value,
    JitHelperArgKind::Value,
    JitHelperArgKind::Value,
];

/// Stable helper registry.
pub const JIT_HELPER_SYMBOLS: &[JitHelperSymbol] = &[
    JitHelperSymbol {
        id: JitHelperId(14),
        name: "phrust_jit_native_call_dispatch",
        args: NATIVE_CONTEXT_POINTERS_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: true,
        has_side_effects: true,
        description: "typed native userland and builtin call dispatcher",
    },
    JitHelperSymbol {
        id: JitHelperId(15),
        name: "phrust_jit_native_dynamic_code",
        args: NATIVE_CONTEXT_POINTERS_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: true,
        has_side_effects: true,
        description: "native include, eval, and declaration compiler boundary",
    },
    JitHelperSymbol {
        id: JitHelperId(16),
        name: "phrust_native_unary",
        args: NATIVE_OP_1_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "typed PHP unary operation",
    },
    JitHelperSymbol {
        id: JitHelperId(17),
        name: "phrust_native_binary",
        args: NATIVE_OP_4_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "typed PHP binary operation",
    },
    JitHelperSymbol {
        id: JitHelperId(18),
        name: "phrust_native_compare",
        args: NATIVE_OP_2_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: false,
        description: "typed PHP comparison",
    },
    JitHelperSymbol {
        id: JitHelperId(19),
        name: "phrust_native_cast",
        args: NATIVE_OP_1_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "typed PHP cast",
    },
    JitHelperSymbol {
        id: JitHelperId(20),
        name: "phrust_native_echo",
        args: CONTEXT_VALUE_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: false,
        has_side_effects: true,
        description: "PHP output operation",
    },
    JitHelperSymbol {
        id: JitHelperId(21),
        name: "phrust_native_local_fetch",
        args: NATIVE_OP_5_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "local and superglobal load",
    },
    JitHelperSymbol {
        id: JitHelperId(22),
        name: "phrust_native_local_store",
        args: NATIVE_OP_4_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "local or reference-cell store",
    },
    JitHelperSymbol {
        id: JitHelperId(23),
        name: "phrust_native_value_release",
        args: CONTEXT_VALUE_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: false,
        has_side_effects: true,
        description: "cold final release of one request-owned value",
    },
    JitHelperSymbol {
        id: JitHelperId(24),
        name: "phrust_native_reference_bind",
        args: NATIVE_OP_3_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "PHP reference binding",
    },
    JitHelperSymbol {
        id: JitHelperId(25),
        name: "phrust_native_return_check",
        args: NATIVE_OP_2_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "declared return-type enforcement",
    },
    JitHelperSymbol {
        id: JitHelperId(26),
        name: "phrust_native_exception_new",
        args: NATIVE_OP_3_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "throwable materialization",
    },
    JitHelperSymbol {
        id: JitHelperId(27),
        name: "phrust_native_array_new",
        args: NATIVE_OP_0_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "PHP array allocation",
    },
    JitHelperSymbol {
        id: JitHelperId(28),
        name: "phrust_native_object_new",
        args: NATIVE_OP_0_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "PHP object allocation",
    },
    JitHelperSymbol {
        id: JitHelperId(29),
        name: "phrust_native_property_fetch",
        args: NATIVE_OP_3_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "object property read",
    },
    JitHelperSymbol {
        id: JitHelperId(30),
        name: "phrust_native_property_assign",
        args: NATIVE_OP_4_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "object property write",
    },
    JitHelperSymbol {
        id: JitHelperId(31),
        name: "phrust_native_object_clone",
        args: NATIVE_OP_1_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "PHP object clone",
    },
    JitHelperSymbol {
        id: JitHelperId(32),
        name: "phrust_native_object_clone_with",
        args: NATIVE_OP_2_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "PHP object clone with replacement properties",
    },
    JitHelperSymbol {
        id: JitHelperId(33),
        name: "phrust_native_array_insert",
        args: NATIVE_OP_3_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "PHP array insert or append",
    },
    JitHelperSymbol {
        id: JitHelperId(34),
        name: "phrust_native_array_fetch",
        args: NATIVE_OP_2_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "PHP array dimension fetch",
    },
    JitHelperSymbol {
        id: JitHelperId(35),
        name: "phrust_native_array_unset",
        args: NATIVE_OP_2_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "PHP array dimension unset",
    },
    JitHelperSymbol {
        id: JitHelperId(36),
        name: "phrust_native_array_spread",
        args: NATIVE_OP_2_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "PHP array spread",
    },
    JitHelperSymbol {
        id: JitHelperId(37),
        name: "phrust_native_foreach_init",
        args: NATIVE_OP_3_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "foreach iterator initialization",
    },
    JitHelperSymbol {
        id: JitHelperId(38),
        name: "phrust_native_foreach_next",
        args: NATIVE_CONTEXT_VALUE_OUT_3_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: true,
        has_side_effects: true,
        description: "foreach iterator advance",
    },
    JitHelperSymbol {
        id: JitHelperId(39),
        name: "phrust_native_foreach_cleanup",
        args: CONTEXT_VALUE_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: false,
        has_side_effects: true,
        description: "foreach iterator release",
    },
    JitHelperSymbol {
        id: JitHelperId(40),
        name: "phrust_native_constant_fetch",
        args: NATIVE_OP_2_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "runtime constant lookup",
    },
    JitHelperSymbol {
        id: JitHelperId(41),
        name: "phrust_native_truthy",
        args: NATIVE_CONTEXT_VALUE_OUT_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: true,
        has_side_effects: false,
        description: "PHP truthiness conversion",
    },
    JitHelperSymbol {
        id: JitHelperId(42),
        name: "phrust_native_runtime_fatal",
        args: &[JitHelperArgKind::I64, JitHelperArgKind::I64],
        returns: JitHelperReturnKind::Status,
        can_throw: false,
        has_side_effects: true,
        description: "deterministic PHP runtime fatal publication",
    },
    JitHelperSymbol {
        id: JitHelperId(43),
        name: "phrust_native_execution_poll",
        args: &[],
        returns: JitHelperReturnKind::Status,
        can_throw: true,
        has_side_effects: true,
        description: "cooperative execution deadline poll",
    },
    JitHelperSymbol {
        id: JitHelperId(44),
        name: "phrust_native_frame_alloc",
        args: NATIVE_FRAME_ALLOC_ARGS,
        returns: JitHelperReturnKind::Value,
        can_throw: false,
        has_side_effects: true,
        description: "bounded request-local native frame allocation",
    },
    JitHelperSymbol {
        id: JitHelperId(45),
        name: "phrust_native_frame_release",
        args: NATIVE_FRAME_RELEASE_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: false,
        has_side_effects: true,
        description: "LIFO request-local native frame release",
    },
    JitHelperSymbol {
        id: JitHelperId(46),
        name: "phrust_native_argument_check",
        args: NATIVE_OP_5_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "direct-call declared parameter type enforcement",
    },
    JitHelperSymbol {
        id: JitHelperId(47),
        name: "phrust_jit_native_function_resolve",
        args: NATIVE_FUNCTION_RESOLVE_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: true,
        has_side_effects: true,
        description: "compile-on-demand resolver for one statically known PHP callee",
    },
    JitHelperSymbol {
        id: JitHelperId(48),
        name: "phrust_native_type_predicate",
        args: NATIVE_OP_1_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: false,
        has_side_effects: false,
        description: "direct PHP type predicate",
    },
    JitHelperSymbol {
        id: JitHelperId(49),
        name: "phrust_native_stable_length",
        args: NATIVE_OP_3_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: false,
        description: "typed strlen/count fallback for stable value views",
    },
    JitHelperSymbol {
        id: JitHelperId(50),
        name: "phrust_native_array_insert_local",
        args: NATIVE_OP_3_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: true,
        has_side_effects: true,
        description: "PHP array insert consuming and replacing one local owner",
    },
    JitHelperSymbol {
        id: JitHelperId(51),
        name: "phrust_native_string_predicate",
        args: NATIVE_OP_2_ARGS,
        returns: JitHelperReturnKind::ValueStatus,
        can_throw: false,
        has_side_effects: false,
        description: "direct PHP string contains/starts-with/ends-with predicate",
    },
    JitHelperSymbol {
        id: JitHelperId(52),
        name: "phrust_baseline_native_builtin_dispatch",
        args: NATIVE_BUILTIN_DISPATCH_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: true,
        has_side_effects: true,
        description: "direct stable-ID builtin dispatch without a generic call frame",
    },
    JitHelperSymbol {
        id: JitHelperId(53),
        name: "phrust_jit_native_semantic_dispatch",
        args: NATIVE_SEMANTIC_DISPATCH_ARGS,
        returns: JitHelperReturnKind::Status,
        can_throw: true,
        has_side_effects: true,
        description: "direct typed semantic dispatch without a generic call frame",
    },
    JitHelperSymbol {
        id: JitHelperId(55),
        name: "phrust_native_preg_match",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared preg_match handler",
    },
    JitHelperSymbol {
        id: JitHelperId(56),
        name: "phrust_native_preg_match_all",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared preg_match_all handler",
    },
    JitHelperSymbol {
        id: JitHelperId(57),
        name: "phrust_native_preg_replace",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared preg_replace handler",
    },
    JitHelperSymbol {
        id: JitHelperId(58),
        name: "phrust_native_preg_filter",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared preg_filter handler",
    },
    JitHelperSymbol {
        id: JitHelperId(59),
        name: "phrust_native_preg_split",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared preg_split handler",
    },
    JitHelperSymbol {
        id: JitHelperId(60),
        name: "phrust_native_preg_grep",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared preg_grep handler",
    },
    JitHelperSymbol {
        id: JitHelperId(61),
        name: "phrust_native_preg_quote",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared preg_quote handler",
    },
    JitHelperSymbol {
        id: JitHelperId(62),
        name: "phrust_native_preg_last_error",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: false,
        has_side_effects: false,
        description: "exact prepared preg_last_error handler",
    },
    JitHelperSymbol {
        id: JitHelperId(63),
        name: "phrust_native_preg_last_error_msg",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: false,
        has_side_effects: false,
        description: "exact prepared preg_last_error_msg handler",
    },
    JitHelperSymbol {
        id: JitHelperId(64),
        name: "phrust_native_json_encode",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared json_encode handler",
    },
    JitHelperSymbol {
        id: JitHelperId(65),
        name: "phrust_native_json_decode",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared json_decode handler",
    },
    JitHelperSymbol {
        id: JitHelperId(66),
        name: "phrust_native_json_validate",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared json_validate handler",
    },
    JitHelperSymbol {
        id: JitHelperId(67),
        name: "phrust_native_json_last_error",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: false,
        has_side_effects: false,
        description: "exact prepared json_last_error handler",
    },
    JitHelperSymbol {
        id: JitHelperId(68),
        name: "phrust_native_json_last_error_msg",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: false,
        has_side_effects: false,
        description: "exact prepared json_last_error_msg handler",
    },
    JitHelperSymbol {
        id: JitHelperId(69),
        name: "phrust_native_sprintf",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared sprintf handler",
    },
    JitHelperSymbol {
        id: JitHelperId(70),
        name: "phrust_native_printf",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared printf handler",
    },
    JitHelperSymbol {
        id: JitHelperId(71),
        name: "phrust_native_vsprintf",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared vsprintf handler",
    },
    JitHelperSymbol {
        id: JitHelperId(72),
        name: "phrust_native_vprintf",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared vprintf handler",
    },
    JitHelperSymbol {
        id: JitHelperId(73),
        name: "phrust_native_defined",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: false,
        description: "exact prepared defined handler",
    },
    JitHelperSymbol {
        id: JitHelperId(74),
        name: "phrust_native_function_exists",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: false,
        description: "exact prepared function_exists handler",
    },
    JitHelperSymbol {
        id: JitHelperId(75),
        name: "phrust_native_class_exists",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared class_exists handler",
    },
    JitHelperSymbol {
        id: JitHelperId(76),
        name: "phrust_native_interface_exists",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared interface_exists handler",
    },
    JitHelperSymbol {
        id: JitHelperId(77),
        name: "phrust_native_trait_exists",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared trait_exists handler",
    },
    JitHelperSymbol {
        id: JitHelperId(78),
        name: "phrust_native_enum_exists",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: true,
        description: "exact prepared enum_exists handler",
    },
    JitHelperSymbol {
        id: JitHelperId(79),
        name: "phrust_native_method_exists",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: false,
        description: "exact prepared method_exists handler",
    },
    JitHelperSymbol {
        id: JitHelperId(80),
        name: "phrust_native_property_exists",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: false,
        description: "exact prepared property_exists handler",
    },
    JitHelperSymbol {
        id: JitHelperId(81),
        name: "phrust_native_basename",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: false,
        description: "exact prepared basename handler",
    },
    JitHelperSymbol {
        id: JitHelperId(82),
        name: "phrust_native_dirname",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: false,
        description: "exact prepared dirname handler",
    },
    JitHelperSymbol {
        id: JitHelperId(83),
        name: "phrust_native_realpath",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: true,
        has_side_effects: false,
        description: "exact prepared realpath capability handler",
    },
    JitHelperSymbol {
        id: JitHelperId(84),
        name: "phrust_native_file_exists",
        args: NATIVE_EXACT_BUILTIN_6_ARGS,
        returns: JitHelperReturnKind::ControlResult,
        can_throw: false,
        has_side_effects: false,
        description: "exact prepared file_exists capability handler",
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

/// Resolves one stable helper ID to the current process address.
///
/// Persistent artifacts call this after identity validation. Keeping the
/// mapping beside the registry makes it impossible for the VM loader to grow a
/// second, differently versioned helper-name table.
#[must_use]
pub fn resolve_helper_address(
    id: JitHelperId,
    runtime: crate::JitRuntimeHelperAddresses,
) -> Option<usize> {
    let helper = lookup_helper_by_id(id)?;
    match helper.name {
        "phrust_jit_native_call_dispatch" => Some(runtime.native_call_dispatch),
        "phrust_baseline_native_builtin_dispatch" => Some(runtime.native_builtin_dispatch),
        "phrust_native_defined" => Some(runtime.native_defined),
        "phrust_native_function_exists" => Some(runtime.native_function_exists),
        "phrust_native_class_exists" => Some(runtime.native_class_exists),
        "phrust_native_interface_exists" => Some(runtime.native_interface_exists),
        "phrust_native_trait_exists" => Some(runtime.native_trait_exists),
        "phrust_native_enum_exists" => Some(runtime.native_enum_exists),
        "phrust_native_method_exists" => Some(runtime.native_method_exists),
        "phrust_native_property_exists" => Some(runtime.native_property_exists),
        "phrust_native_preg_match" => Some(runtime.native_preg_match),
        "phrust_native_preg_match_all" => Some(runtime.native_preg_match_all),
        "phrust_native_preg_replace" => Some(runtime.native_preg_replace),
        "phrust_native_preg_filter" => Some(runtime.native_preg_filter),
        "phrust_native_preg_split" => Some(runtime.native_preg_split),
        "phrust_native_preg_grep" => Some(runtime.native_preg_grep),
        "phrust_native_preg_quote" => Some(runtime.native_preg_quote),
        "phrust_native_preg_last_error" => Some(runtime.native_preg_last_error),
        "phrust_native_preg_last_error_msg" => Some(runtime.native_preg_last_error_msg),
        "phrust_native_json_encode" => Some(runtime.native_json_encode),
        "phrust_native_json_decode" => Some(runtime.native_json_decode),
        "phrust_native_json_validate" => Some(runtime.native_json_validate),
        "phrust_native_json_last_error" => Some(runtime.native_json_last_error),
        "phrust_native_json_last_error_msg" => Some(runtime.native_json_last_error_msg),
        "phrust_native_sprintf" => Some(runtime.native_sprintf),
        "phrust_native_printf" => Some(runtime.native_printf),
        "phrust_native_vsprintf" => Some(runtime.native_vsprintf),
        "phrust_native_vprintf" => Some(runtime.native_vprintf),
        "phrust_native_basename" => Some(runtime.native_basename),
        "phrust_native_dirname" => Some(runtime.native_dirname),
        "phrust_native_realpath" => Some(runtime.native_realpath),
        "phrust_native_file_exists" => Some(runtime.native_file_exists),
        "phrust_jit_native_semantic_dispatch" => Some(runtime.native_semantic_dispatch),
        "phrust_jit_native_function_resolve" => Some(runtime.native_function_resolve),
        "phrust_native_frame_alloc" => Some(runtime.native_frame_alloc),
        "phrust_native_frame_release" => Some(runtime.native_frame_release),
        "phrust_jit_native_dynamic_code" => Some(runtime.native_dynamic_code),
        "phrust_native_unary" => Some(runtime.native_unary),
        "phrust_native_binary" => Some(runtime.native_binary),
        "phrust_native_compare" => Some(runtime.native_compare),
        "phrust_native_cast" => Some(runtime.native_cast),
        "phrust_native_echo" => Some(runtime.native_echo),
        "phrust_native_local_fetch" => Some(runtime.native_local_fetch),
        "phrust_native_local_store" => Some(runtime.native_local_store),
        "phrust_native_value_release" => Some(runtime.native_value_release),
        "phrust_native_reference_bind" => Some(runtime.native_reference_bind),
        "phrust_native_argument_check" => Some(runtime.native_argument_check),
        "phrust_native_return_check" => Some(runtime.native_return_check),
        "phrust_native_exception_new" => Some(runtime.native_exception_new),
        "phrust_native_array_new" => Some(runtime.native_array_new),
        "phrust_native_object_new" => Some(runtime.native_object_new),
        "phrust_native_property_fetch" => Some(runtime.native_property_fetch),
        "phrust_native_property_assign" => Some(runtime.native_property_assign),
        "phrust_native_object_clone" => Some(runtime.native_object_clone),
        "phrust_native_object_clone_with" => Some(runtime.native_object_clone_with),
        "phrust_native_array_insert" => Some(runtime.native_array_insert),
        "phrust_native_array_insert_local" => Some(runtime.native_array_insert_local),
        "phrust_native_array_fetch" => Some(runtime.native_array_fetch),
        "phrust_native_array_unset" => Some(runtime.native_array_unset),
        "phrust_native_array_spread" => Some(runtime.native_array_spread),
        "phrust_native_foreach_init" => Some(runtime.native_foreach_init),
        "phrust_native_foreach_next" => Some(runtime.native_foreach_next),
        "phrust_native_foreach_cleanup" => Some(runtime.native_foreach_cleanup),
        "phrust_native_constant_fetch" => Some(runtime.native_constant_fetch),
        "phrust_native_truthy" => Some(runtime.native_truthy),
        "phrust_native_type_predicate" => Some(runtime.native_type_predicate),
        "phrust_native_stable_length" => Some(runtime.native_stable_length),
        "phrust_native_string_predicate" => Some(runtime.native_string_predicate),
        "phrust_native_runtime_fatal" => Some(runtime.native_runtime_fatal),
        "phrust_native_execution_poll" => Some(runtime.native_execution_poll),
        _ => None,
    }
    .filter(|address| *address != 0)
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

#[cfg(test)]
mod tests {
    use super::{
        JIT_HELPER_REGISTRY_ABI_HASH, JIT_HELPER_SYMBOLS, JitHelperArgKind, JitHelperId,
        JitHelperReturnKind, helper_registry_is_stable, helper_registry_layout_summary,
        lookup_helper_by_id, lookup_helper_by_name,
    };

    #[test]
    fn helper_registry_ids_names_and_layout_are_stable() {
        assert_ne!(JIT_HELPER_REGISTRY_ABI_HASH, 0);
        assert!(helper_registry_is_stable());
        assert_eq!(helper_registry_layout_summary(), (4, 4, 4));
        assert_eq!(
            JIT_HELPER_SYMBOLS.first().expect("first").id,
            JitHelperId(14)
        );
        assert_eq!(JIT_HELPER_SYMBOLS.last().expect("last").id, JitHelperId(84));
    }

    #[test]
    fn helper_registry_lookups_return_signatures() {
        let call = lookup_helper_by_name("phrust_jit_native_call_dispatch").expect("call helper");
        assert_eq!(call.id, JitHelperId(14));
        assert_eq!(
            call.args,
            &[
                JitHelperArgKind::VmContext,
                JitHelperArgKind::U64,
                JitHelperArgKind::U64
            ]
        );
        assert_eq!(call.returns, JitHelperReturnKind::Status);
        assert!(call.can_throw);
        assert!(call.has_side_effects);

        let truthy = lookup_helper_by_id(JitHelperId(41)).expect("truthy helper");
        assert_eq!(truthy.name, "phrust_native_truthy");
        assert!(!truthy.has_side_effects);
    }
}
