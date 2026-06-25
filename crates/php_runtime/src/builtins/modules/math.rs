//! Math builtin registry slice.

use super::core;
use crate::builtins::{BuiltinCompatibility, BuiltinEntry};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("abs", core::builtin_abs, BuiltinCompatibility::Php),
    BuiltinEntry::new("ceil", core::builtin_ceil, BuiltinCompatibility::Php),
    BuiltinEntry::new("decbin", core::builtin_decbin, BuiltinCompatibility::Php),
    BuiltinEntry::new("floor", core::builtin_floor, BuiltinCompatibility::Php),
    BuiltinEntry::new("fdiv", core::builtin_fdiv, BuiltinCompatibility::Php),
    BuiltinEntry::new("fmod", core::builtin_fmod, BuiltinCompatibility::Php),
    BuiltinEntry::new("intdiv", core::builtin_intdiv, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "is_finite",
        core::builtin_is_finite,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "is_infinite",
        core::builtin_is_infinite,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("is_nan", core::builtin_is_nan, BuiltinCompatibility::Php),
    BuiltinEntry::new("max", core::builtin_max, BuiltinCompatibility::Php),
    BuiltinEntry::new("min", core::builtin_min, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "number_format",
        core::builtin_number_format,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("pow", core::builtin_pow, BuiltinCompatibility::Php),
    BuiltinEntry::new("round", core::builtin_round, BuiltinCompatibility::Php),
    BuiltinEntry::new("sqrt", core::builtin_sqrt, BuiltinCompatibility::Php),
];
