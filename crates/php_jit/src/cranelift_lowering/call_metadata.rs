use super::*;

pub(super) fn native_call_target_metadata(target: &RegionCallTarget) -> (u32, u32, u64, u64) {
    match target {
        RegionCallTarget::Function { name, function } => (
            crate::JitNativeCallKind::FUNCTION.0,
            function.map_or(u32::MAX, FunctionId::raw),
            stable_call_symbol_hash(name),
            0,
        ),
        RegionCallTarget::Method { method, .. } => (
            crate::JitNativeCallKind::METHOD.0,
            u32::MAX,
            stable_call_symbol_hash(method),
            0,
        ),
        RegionCallTarget::StaticMethod { class_name, method } => (
            crate::JitNativeCallKind::STATIC_METHOD.0,
            u32::MAX,
            stable_call_symbol_hash(method),
            stable_call_symbol_hash(class_name),
        ),
        RegionCallTarget::Closure { .. } => (crate::JitNativeCallKind::CLOSURE.0, u32::MAX, 0, 0),
        RegionCallTarget::Callable { .. } => (crate::JitNativeCallKind::CALLABLE.0, u32::MAX, 0, 0),
        RegionCallTarget::Pipe { .. } => (crate::JitNativeCallKind::PIPE.0, u32::MAX, 0, 0),
        RegionCallTarget::Constructor { class_name, .. } => (
            crate::JitNativeCallKind::CONSTRUCTOR.0,
            u32::MAX,
            0,
            stable_call_symbol_hash(class_name),
        ),
        RegionCallTarget::DynamicConstructor { .. } => (
            crate::JitNativeCallKind::DYNAMIC_CONSTRUCTOR.0,
            u32::MAX,
            0,
            0,
        ),
        RegionCallTarget::Semantic { operation } => (
            crate::JitNativeCallKind::SEMANTIC_OPERATION.0,
            operation.operation_id().raw(),
            0,
            0,
        ),
    }
}

pub(super) fn stable_call_symbol_hash(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte.to_ascii_lowercase())).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

pub(super) fn stable_builtin_helper_id(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
    if normalized.contains('\\') {
        return None;
    }
    php_runtime::api::BuiltinRegistry::new()
        .get(&normalized)
        .map(php_runtime::api::BuiltinEntry::helper_id)
        .filter(|helper_id| *helper_id != 0)
}

pub(super) fn stable_builtin_dense_id(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
    if normalized.contains('\\') {
        return None;
    }
    php_runtime::api::BuiltinRegistry::new()
        .get(&normalized)
        .map(php_runtime::api::BuiltinEntry::dense_id)
}

pub(super) fn stable_builtin_type_predicate(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
    if normalized.contains('\\') {
        return None;
    }
    match normalized.as_str() {
        "is_null" => Some(0),
        "is_bool" => Some(1),
        "is_int" | "is_integer" | "is_long" => Some(2),
        "is_float" | "is_double" | "is_real" => Some(3),
        "is_string" => Some(4),
        "is_array" => Some(5),
        "is_object" => Some(6),
        "is_resource" => Some(7),
        "is_scalar" => Some(8),
        _ => None,
    }
}

pub(super) fn stable_builtin_is_numeric(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("is_numeric")
}

pub(super) fn stable_builtin_error_reporting(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("error_reporting")
}

/// Exact read-only symbol queries. The selector is part of the dedicated
/// native ABI and never enters the prepared builtin dispatcher.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableSymbolQueryBuiltin {
    Defined,
    FunctionExists,
    ClassExists,
    InterfaceExists,
    TraitExists,
    EnumExists,
    MethodExists,
    PropertyExists,
}

impl StableSymbolQueryBuiltin {
    pub(super) const COUNT: usize = 8;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Defined => 0,
            Self::FunctionExists => 1,
            Self::ClassExists => 2,
            Self::InterfaceExists => 3,
            Self::TraitExists => 4,
            Self::EnumExists => 5,
            Self::MethodExists => 6,
            Self::PropertyExists => 7,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Defined => "phrust_native_defined",
            Self::FunctionExists => "phrust_native_function_exists",
            Self::ClassExists => "phrust_native_class_exists",
            Self::InterfaceExists => "phrust_native_interface_exists",
            Self::TraitExists => "phrust_native_trait_exists",
            Self::EnumExists => "phrust_native_enum_exists",
            Self::MethodExists => "phrust_native_method_exists",
            Self::PropertyExists => "phrust_native_property_exists",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Defined,
            Self::FunctionExists,
            Self::ClassExists,
            Self::InterfaceExists,
            Self::TraitExists,
            Self::EnumExists,
            Self::MethodExists,
            Self::PropertyExists,
        ]
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Defined | Self::FunctionExists => arity == 1,
            Self::ClassExists | Self::InterfaceExists | Self::TraitExists | Self::EnumExists => {
                arity == 1 || arity == 2
            }
            Self::MethodExists | Self::PropertyExists => arity == 2,
        }
    }
}

pub(super) fn stable_builtin_symbol_query(
    target: &RegionCallTarget,
) -> Option<StableSymbolQueryBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "defined" => Some(StableSymbolQueryBuiltin::Defined),
        "function_exists" => Some(StableSymbolQueryBuiltin::FunctionExists),
        "class_exists" => Some(StableSymbolQueryBuiltin::ClassExists),
        "interface_exists" => Some(StableSymbolQueryBuiltin::InterfaceExists),
        "trait_exists" => Some(StableSymbolQueryBuiltin::TraitExists),
        "enum_exists" => Some(StableSymbolQueryBuiltin::EnumExists),
        "method_exists" => Some(StableSymbolQueryBuiltin::MethodExists),
        "property_exists" => Some(StableSymbolQueryBuiltin::PropertyExists),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StablePcreBuiltin {
    Match,
    MatchAll,
    Replace,
    Filter,
    Split,
    Grep,
    Quote,
    LastError,
    LastErrorMessage,
}

impl StablePcreBuiltin {
    pub(super) const COUNT: usize = 9;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Match => 0,
            Self::MatchAll => 1,
            Self::Replace => 2,
            Self::Filter => 3,
            Self::Split => 4,
            Self::Grep => 5,
            Self::Quote => 6,
            Self::LastError => 7,
            Self::LastErrorMessage => 8,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Match => "phrust_native_preg_match",
            Self::MatchAll => "phrust_native_preg_match_all",
            Self::Replace => "phrust_native_preg_replace",
            Self::Filter => "phrust_native_preg_filter",
            Self::Split => "phrust_native_preg_split",
            Self::Grep => "phrust_native_preg_grep",
            Self::Quote => "phrust_native_preg_quote",
            Self::LastError => "phrust_native_preg_last_error",
            Self::LastErrorMessage => "phrust_native_preg_last_error_msg",
        }
    }

    pub(super) const fn argument_is_by_reference(self, index: usize) -> bool {
        matches!(
            (self, index),
            (Self::Match | Self::MatchAll, 2) | (Self::Replace | Self::Filter, 4)
        )
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Match | Self::MatchAll => arity >= 2 && arity <= 5,
            Self::Replace | Self::Filter => arity >= 3 && arity <= 5,
            Self::Split => arity >= 2 && arity <= 4,
            Self::Grep => arity == 2 || arity == 3,
            Self::Quote => arity == 1 || arity == 2,
            Self::LastError | Self::LastErrorMessage => arity == 0,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Match,
            Self::MatchAll,
            Self::Replace,
            Self::Filter,
            Self::Split,
            Self::Grep,
            Self::Quote,
            Self::LastError,
            Self::LastErrorMessage,
        ]
    }
}

/// Non-callback PCRE calls are exact prepared capability handlers. Callback
/// variants stay on the baseline-native callable path because they execute
/// user PHP code.
pub(super) fn stable_builtin_pcre(target: &RegionCallTarget) -> Option<StablePcreBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "preg_match" => Some(StablePcreBuiltin::Match),
        "preg_match_all" => Some(StablePcreBuiltin::MatchAll),
        "preg_replace" => Some(StablePcreBuiltin::Replace),
        "preg_filter" => Some(StablePcreBuiltin::Filter),
        "preg_split" => Some(StablePcreBuiltin::Split),
        "preg_grep" => Some(StablePcreBuiltin::Grep),
        "preg_quote" => Some(StablePcreBuiltin::Quote),
        "preg_last_error" => Some(StablePcreBuiltin::LastError),
        "preg_last_error_msg" => Some(StablePcreBuiltin::LastErrorMessage),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableJsonBuiltin {
    Encode,
    Decode,
    Validate,
    LastError,
    LastErrorMessage,
}

impl StableJsonBuiltin {
    pub(super) const COUNT: usize = 5;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Encode => 0,
            Self::Decode => 1,
            Self::Validate => 2,
            Self::LastError => 3,
            Self::LastErrorMessage => 4,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Encode => "phrust_native_json_encode",
            Self::Decode => "phrust_native_json_decode",
            Self::Validate => "phrust_native_json_validate",
            Self::LastError => "phrust_native_json_last_error",
            Self::LastErrorMessage => "phrust_native_json_last_error_msg",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Encode,
            Self::Decode,
            Self::Validate,
            Self::LastError,
            Self::LastErrorMessage,
        ]
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Encode | Self::Validate => arity >= 1 && arity <= 3,
            Self::Decode => arity >= 2 && arity <= 4,
            Self::LastError | Self::LastErrorMessage => arity == 0,
        }
    }
}

pub(super) fn stable_builtin_json(target: &RegionCallTarget) -> Option<StableJsonBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "json_encode" => Some(StableJsonBuiltin::Encode),
        "json_decode" => Some(StableJsonBuiltin::Decode),
        "json_validate" => Some(StableJsonBuiltin::Validate),
        "json_last_error" => Some(StableJsonBuiltin::LastError),
        "json_last_error_msg" => Some(StableJsonBuiltin::LastErrorMessage),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableFormatBuiltin {
    Sprintf,
    Printf,
    Vsprintf,
    Vprintf,
}

impl StableFormatBuiltin {
    pub(super) const COUNT: usize = 4;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Sprintf => 0,
            Self::Printf => 1,
            Self::Vsprintf => 2,
            Self::Vprintf => 3,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Sprintf => "phrust_native_sprintf",
            Self::Printf => "phrust_native_printf",
            Self::Vsprintf => "phrust_native_vsprintf",
            Self::Vprintf => "phrust_native_vprintf",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::Sprintf, Self::Printf, Self::Vsprintf, Self::Vprintf]
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Sprintf | Self::Printf => arity >= 1 && arity <= 6,
            Self::Vsprintf | Self::Vprintf => arity == 2,
        }
    }
}

pub(super) fn stable_builtin_format(target: &RegionCallTarget) -> Option<StableFormatBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "sprintf" => Some(StableFormatBuiltin::Sprintf),
        "printf" => Some(StableFormatBuiltin::Printf),
        "vsprintf" => Some(StableFormatBuiltin::Vsprintf),
        "vprintf" => Some(StableFormatBuiltin::Vprintf),
        _ => None,
    }
}

/// Exact prepared path/filesystem handlers selected at compile time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StablePathBuiltin {
    Basename,
    Dirname,
    Realpath,
    FileExists,
    Fopen,
    Fwrite,
    Fclose,
}

impl StablePathBuiltin {
    pub(super) const COUNT: usize = 7;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Basename => 0,
            Self::Dirname => 1,
            Self::Realpath => 2,
            Self::FileExists => 3,
            Self::Fopen => 4,
            Self::Fwrite => 5,
            Self::Fclose => 6,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Basename => "phrust_native_basename",
            Self::Dirname => "phrust_native_dirname",
            Self::Realpath => "phrust_native_realpath",
            Self::FileExists => "phrust_native_file_exists",
            Self::Fopen => "phrust_native_fopen",
            Self::Fwrite => "phrust_native_fwrite",
            Self::Fclose => "phrust_native_fclose",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Basename | Self::Dirname => arity == 1 || arity == 2,
            Self::Realpath | Self::FileExists => arity == 1,
            // Optional fopen include-path/context shapes retain their one
            // baseline continuation until those capabilities are published.
            Self::Fopen => arity == 2,
            Self::Fwrite => arity == 2 || arity == 3,
            Self::Fclose => arity == 1,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Basename,
            Self::Dirname,
            Self::Realpath,
            Self::FileExists,
            Self::Fopen,
            Self::Fwrite,
            Self::Fclose,
        ]
    }
}

pub(super) fn stable_builtin_path(target: &RegionCallTarget) -> Option<StablePathBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "basename" => Some(StablePathBuiltin::Basename),
        "dirname" => Some(StablePathBuiltin::Dirname),
        "realpath" => Some(StablePathBuiltin::Realpath),
        "file_exists" => Some(StablePathBuiltin::FileExists),
        "fopen" => Some(StablePathBuiltin::Fopen),
        "fwrite" => Some(StablePathBuiltin::Fwrite),
        "fclose" => Some(StablePathBuiltin::Fclose),
        _ => None,
    }
}

pub(super) fn stable_builtin_length(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
    if normalized.contains('\\') {
        return None;
    }
    match normalized.as_str() {
        "strlen" => Some(0),
        "count" => Some(1),
        _ => None,
    }
}

pub(super) fn stable_builtin_array_key_exists(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\')
        && (normalized.eq_ignore_ascii_case("array_key_exists")
            || normalized.eq_ignore_ascii_case("key_exists"))
}

pub(super) fn stable_builtin_string_predicate(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "str_contains" => Some(0),
        "str_starts_with" => Some(1),
        "str_ends_with" => Some(2),
        _ => None,
    }
}

/// ASCII-only case conversion builtins whose PHP 8 semantics can be emitted
/// directly over the request-owned native string arena.  The numeric value is
/// an internal lowering selector, never a runtime helper operation ID.
pub(super) fn stable_builtin_ascii_case(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "strtolower" => Some(0),
        "strtoupper" => Some(1),
        _ => None,
    }
}

/// Byte-preserving transforms over one native string. The selector chooses
/// reverse, lowercase-first-byte, or uppercase-first-byte behavior.
pub(super) fn stable_builtin_string_transform(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "strrev" => Some(0),
        "lcfirst" => Some(1),
        "ucfirst" => Some(2),
        _ => None,
    }
}

pub(super) fn stable_builtin_str_repeat(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("str_repeat")
}

pub(super) fn stable_builtin_addslashes(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("addslashes")
}

pub(super) fn stable_builtin_substr_count(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("substr_count")
}

/// Native byte comparisons. Bit zero selects ASCII case folding; bit one
/// selects the explicit maximum-length variants.
pub(super) fn stable_builtin_string_compare(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "strcmp" => Some(0),
        "strcasecmp" => Some(1),
        "strncmp" => Some(2),
        "strncasecmp" => Some(3),
        _ => None,
    }
}

/// Byte-position builtins with an exact positional native lowering. The low
/// bit selects ASCII case folding; the high bit selects reverse search.
pub(super) fn stable_builtin_string_position(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "strpos" => Some(0),
        "stripos" => Some(1),
        "strrpos" => Some(2),
        "strripos" => Some(3),
        _ => None,
    }
}

pub(super) fn stable_builtin_ord(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("ord")
}

pub(super) fn stable_builtin_chr(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("chr")
}

/// Native byte-slice transformations. `substr` has its own argument plan;
/// trim selectors encode left/right default-mask trimming.
pub(super) fn stable_builtin_default_trim(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "trim" => Some(0),
        "ltrim" => Some(1),
        "rtrim" => Some(2),
        _ => None,
    }
}

pub(super) fn stable_builtin_substr(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("substr")
}

/// Direct array projections whose result is another authoritative native
/// array. The selector chooses source keys or source values.
pub(super) fn stable_builtin_array_projection(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "array_keys" => Some(0),
        "array_values" => Some(1),
        _ => None,
    }
}

/// Strict native array membership operations. The selector distinguishes a
/// boolean membership result from the matching key result.
pub(super) fn stable_builtin_array_lookup(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "in_array" => Some(0),
        "array_search" => Some(1),
        _ => None,
    }
}

/// Array-key queries that preserve the source key representation.
pub(super) fn stable_builtin_array_edge_key(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "array_key_first" => Some(0),
        "array_key_last" => Some(1),
        _ => None,
    }
}

/// PHP array internal-pointer operations. Read-only selectors consume the
/// authoritative native slot; mutating selectors require an exact caller
/// local and update that slot after COW separation.
pub(super) fn stable_builtin_array_pointer(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "current" => Some(0),
        "key" => Some(1),
        "next" => Some(2),
        "reset" => Some(3),
        "prev" => Some(4),
        "end" => Some(5),
        _ => None,
    }
}

/// Exact local-mutating array stack operations. Zero pops one owner from the
/// tail; one appends one or more prepared positional values.
pub(super) fn stable_builtin_array_stack(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "array_pop" => Some(0),
        "array_push" => Some(1),
        _ => None,
    }
}

pub(super) fn stable_builtin_array_is_list(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("array_is_list")
}

pub(super) fn stable_builtin_implode(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\')
        && (normalized.eq_ignore_ascii_case("implode") || normalized.eq_ignore_ascii_case("join"))
}

pub(super) fn stable_builtin_explode(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("explode")
}

pub(super) fn stable_builtin_array_slice(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("array_slice")
}

pub(super) fn stable_builtin_array_reverse(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("array_reverse")
}

pub(super) fn stable_builtin_array_merge(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("array_merge")
}

pub(super) fn stable_builtin_str_replace(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("str_replace")
}

pub(super) fn stable_builtin_string_span(target: &RegionCallTarget) -> Option<u32> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "strspn" => Some(0),
        "strcspn" => Some(1),
        _ => None,
    }
}

pub(super) fn native_argument_flags(argument: &php_ir::instruction::IrCallArg) -> u32 {
    let mut flags = crate::JitNativeArgFlags::default();
    if argument.name.is_some() {
        flags = flags.union(crate::JitNativeArgFlags::NAMED);
    }
    if argument.unpack {
        flags = flags.union(crate::JitNativeArgFlags::UNPACK);
    }
    if argument.by_ref_local.is_some()
        || argument.by_ref_dim.is_some()
        || argument.by_ref_property.is_some()
        || argument.by_ref_property_dim.is_some()
    {
        flags = flags.union(crate::JitNativeArgFlags::BY_REFERENCE);
    }
    if argument.value_kind == php_ir::instruction::IrCallArgValueKind::IndirectTemporary {
        flags = flags.union(crate::JitNativeArgFlags::INDIRECT_TEMPORARY);
    }
    flags.0
}

pub(super) fn native_argument_has_location(argument: &php_ir::instruction::IrCallArg) -> bool {
    argument.by_ref_local.is_some()
        || argument.by_ref_dim.is_some()
        || argument.by_ref_property.is_some()
        || argument.by_ref_property_dim.is_some()
}

pub(super) fn known_user_argument_requires_reference(
    call: &RegionNativeCall,
    index: usize,
    function_params: &BTreeMap<FunctionId, NativeFunctionMetadata>,
    external_function_signatures: &[crate::JitExternalFunctionSignature],
    caller: FunctionId,
) -> Option<bool> {
    let argument = call.args.get(index)?;
    if let Some(requirement) = call.declared_argument_reference_requirement(index) {
        return Some(requirement);
    }
    if let RegionCallTarget::Method { method, .. } = &call.target {
        // Region IR records lvalue provenance for ordinary by-value arguments
        // as well as true by-reference parameters. For internal instance
        // methods the receiver class is dynamic, but when every published
        // method with this name agrees on the parameter mode the arginfo is
        // still authoritative. This prevents speculative ReferenceCell
        // creation for families such as Closure::bindTo without specializing
        // the callsite or receiver identity.
        let mut requirements = php_std::generated::arginfo::GENERATED_METHODS
            .iter()
            .filter(|candidate| candidate.name.eq_ignore_ascii_case(method))
            .map(|candidate| {
                argument
                    .name
                    .as_deref()
                    .map_or_else(
                        || {
                            candidate.params.get(index).or_else(|| {
                                candidate
                                    .params
                                    .last()
                                    .filter(|parameter| parameter.variadic)
                            })
                        },
                        |name| {
                            candidate
                                .params
                                .iter()
                                .find(|parameter| parameter.name.eq_ignore_ascii_case(name))
                                .or_else(|| {
                                    candidate
                                        .params
                                        .last()
                                        .filter(|parameter| parameter.variadic)
                                })
                        },
                    )
                    .is_some_and(|parameter| parameter.by_ref)
            });
        if let Some(requirement) = requirements.next()
            && requirements.all(|candidate| candidate == requirement)
        {
            return Some(requirement);
        }
    }
    if let RegionCallTarget::Function {
        name,
        function: None,
    } = &call.target
    {
        let normalized = name.trim_start_matches('\\');
        let has_local_metadata = function_params.values().any(|(candidate, ..)| {
            candidate
                .trim_start_matches('\\')
                .eq_ignore_ascii_case(normalized)
        });
        if !has_local_metadata {
            let signature = external_function_signatures.iter().find(|signature| {
                signature
                    .name
                    .trim_start_matches('\\')
                    .eq_ignore_ascii_case(normalized)
            })?;
            let parameter = argument.name.as_deref().map_or_else(
                || {
                    signature.params.get(index).or_else(|| {
                        signature
                            .params
                            .last()
                            .filter(|parameter| parameter.variadic)
                    })
                },
                |name| {
                    signature
                        .params
                        .iter()
                        .find(|parameter| parameter.name.eq_ignore_ascii_case(name))
                        .or_else(|| {
                            signature
                                .params
                                .last()
                                .filter(|parameter| parameter.variadic)
                        })
                },
            );
            return Some(parameter.is_some_and(|parameter| parameter.by_ref));
        }
    }
    let method_matches = |candidate: &str, method: &str| {
        candidate
            .rsplit_once("::")
            .is_some_and(|(_, candidate_method)| candidate_method.eq_ignore_ascii_case(method))
    };
    let metadata = match &call.target {
        RegionCallTarget::Function {
            name,
            function: None,
        } => {
            let normalized = name.trim_start_matches('\\');
            vec![function_params.values().find(|(candidate, ..)| {
                candidate
                    .trim_start_matches('\\')
                    .eq_ignore_ascii_case(normalized)
            })?]
        }
        RegionCallTarget::Function {
            function: Some(function),
            ..
        } => vec![function_params.get(function)?],
        RegionCallTarget::StaticMethod { class_name, method } => {
            let resolved_class = if matches!(class_name.as_str(), "self" | "static") {
                function_params
                    .get(&caller)
                    .and_then(|(name, ..)| name.rsplit_once("::").map(|(class, _)| class))
            } else {
                Some(class_name.trim_start_matches('\\'))
            };
            let exact = resolved_class.and_then(|class| {
                function_params.values().find(|(candidate, ..)| {
                    candidate.rsplit_once("::").is_some_and(
                        |(candidate_class, candidate_method)| {
                            candidate_class
                                .trim_start_matches('\\')
                                .eq_ignore_ascii_case(class)
                                && candidate_method.eq_ignore_ascii_case(method)
                        },
                    )
                })
            });
            exact.map_or_else(
                || {
                    function_params
                        .values()
                        .filter(|(candidate, ..)| method_matches(candidate, method))
                        .collect()
                },
                |metadata| vec![metadata],
            )
        }
        RegionCallTarget::Method { method, .. } => function_params
            .values()
            .filter(|(candidate, ..)| method_matches(candidate, method))
            .collect(),
        RegionCallTarget::Constructor { class_name, .. } => function_params
            .values()
            .filter(|(candidate, ..)| {
                candidate.rsplit_once("::").is_some_and(|(class, method)| {
                    class
                        .trim_start_matches('\\')
                        .eq_ignore_ascii_case(class_name.trim_start_matches('\\'))
                        && method.eq_ignore_ascii_case("__construct")
                })
            })
            .collect(),
        _ => return None,
    };
    let mut requirements = metadata.into_iter().map(|metadata| {
        let parameters = &metadata.1;
        argument
            .name
            .as_deref()
            .map_or_else(
                || {
                    parameters
                        .get(index)
                        .or_else(|| parameters.last().filter(|parameter| parameter.variadic))
                },
                |name| {
                    parameters
                        .iter()
                        .find(|parameter| parameter.name.eq_ignore_ascii_case(name))
                        .or_else(|| parameters.last().filter(|parameter| parameter.variadic))
                },
            )
            .is_some_and(|parameter| parameter.by_ref)
    });
    let requirement = requirements.next()?;
    requirements
        .all(|candidate| candidate == requirement)
        .then_some(requirement)
}
