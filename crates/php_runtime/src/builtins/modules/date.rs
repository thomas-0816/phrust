//! Date builtin registry slice.

use super::core;
use crate::builtins::{BuiltinCompatibility, BuiltinEntry};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("date", core::builtin_date, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "date_default_timezone_get",
        core::builtin_date_default_timezone_get,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "date_default_timezone_set",
        core::builtin_date_default_timezone_set,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "strtotime",
        core::builtin_strtotime,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("hrtime", core::builtin_hrtime, BuiltinCompatibility::Php),
    BuiltinEntry::new("time", core::builtin_time, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "timezone_identifiers_list",
        core::builtin_timezone_identifiers_list,
        BuiltinCompatibility::Php,
    ),
];
