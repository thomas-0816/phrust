use super::*;

pub(super) fn stable_call_symbol_hash(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte.to_ascii_lowercase())).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Baseline-only compatibility identity for the generic builtin binder.
///
/// Optimizing lowering selects one exact family handler and must never use
/// this registry identity.
pub(super) fn baseline_builtin_helper_id(target: &RegionCallTarget) -> Option<u32> {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableTypePredicateBuiltin {
    Null,
    Bool,
    Int,
    Float,
    String,
    Array,
    Object,
    Resource,
    Scalar,
    Countable,
    Iterable,
}

pub(super) fn stable_builtin_type_predicate(
    target: &RegionCallTarget,
) -> Option<StableTypePredicateBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
    if normalized.contains('\\') {
        return None;
    }
    match normalized.as_str() {
        "is_null" => Some(StableTypePredicateBuiltin::Null),
        "is_bool" => Some(StableTypePredicateBuiltin::Bool),
        "is_int" | "is_integer" | "is_long" => Some(StableTypePredicateBuiltin::Int),
        "is_float" | "is_double" | "is_real" => Some(StableTypePredicateBuiltin::Float),
        "is_string" => Some(StableTypePredicateBuiltin::String),
        "is_array" => Some(StableTypePredicateBuiltin::Array),
        "is_object" => Some(StableTypePredicateBuiltin::Object),
        "is_resource" => Some(StableTypePredicateBuiltin::Resource),
        "is_scalar" => Some(StableTypePredicateBuiltin::Scalar),
        "is_countable" => Some(StableTypePredicateBuiltin::Countable),
        "is_iterable" => Some(StableTypePredicateBuiltin::Iterable),
        _ => None,
    }
}

/// Complete ASCII C-locale character-classification family. The enum is
/// consumed only while emitting CLIF; generated code never receives a
/// predicate ID or enters generic builtin dispatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableCtypeBuiltin {
    Alnum,
    Alpha,
    Cntrl,
    Digit,
    Graph,
    Lower,
    Print,
    Punct,
    Space,
    Upper,
    Xdigit,
}

#[cfg(test)]
impl StableCtypeBuiltin {
    pub(super) const fn all() -> [Self; 11] {
        [
            Self::Alnum,
            Self::Alpha,
            Self::Cntrl,
            Self::Digit,
            Self::Graph,
            Self::Lower,
            Self::Print,
            Self::Punct,
            Self::Space,
            Self::Upper,
            Self::Xdigit,
        ]
    }

    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Alnum => "ctype_alnum",
            Self::Alpha => "ctype_alpha",
            Self::Cntrl => "ctype_cntrl",
            Self::Digit => "ctype_digit",
            Self::Graph => "ctype_graph",
            Self::Lower => "ctype_lower",
            Self::Print => "ctype_print",
            Self::Punct => "ctype_punct",
            Self::Space => "ctype_space",
            Self::Upper => "ctype_upper",
            Self::Xdigit => "ctype_xdigit",
        }
    }
}

pub(super) fn stable_builtin_ctype(target: &RegionCallTarget) -> Option<StableCtypeBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "ctype_alnum" => Some(StableCtypeBuiltin::Alnum),
        "ctype_alpha" => Some(StableCtypeBuiltin::Alpha),
        "ctype_cntrl" => Some(StableCtypeBuiltin::Cntrl),
        "ctype_digit" => Some(StableCtypeBuiltin::Digit),
        "ctype_graph" => Some(StableCtypeBuiltin::Graph),
        "ctype_lower" => Some(StableCtypeBuiltin::Lower),
        "ctype_print" => Some(StableCtypeBuiltin::Print),
        "ctype_punct" => Some(StableCtypeBuiltin::Punct),
        "ctype_space" => Some(StableCtypeBuiltin::Space),
        "ctype_upper" => Some(StableCtypeBuiltin::Upper),
        "ctype_xdigit" => Some(StableCtypeBuiltin::Xdigit),
        _ => None,
    }
}

/// Tokenizer builtins whose successful native shapes are published directly
/// from lexer records. Every variant names one fixed ABI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableTokenizerBuiltin {
    GetAll,
    Name,
}

impl StableTokenizerBuiltin {
    pub(super) const COUNT: usize = 2;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::GetAll => 0,
            Self::Name => 1,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::GetAll => "phrust_native_token_get_all",
            Self::Name => "phrust_native_token_name",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::GetAll, Self::Name]
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::GetAll => arity == 1 || arity == 2,
            Self::Name => arity == 1,
        }
    }
}

pub(super) fn stable_builtin_tokenizer(
    target: &RegionCallTarget,
) -> Option<StableTokenizerBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "token_get_all" => Some(StableTokenizerBuiltin::GetAll),
        "token_name" => Some(StableTokenizerBuiltin::Name),
        _ => None,
    }
}

/// Exact native handlers for the complete mbstring function family.
///
/// Every fixed name has a distinct symbol; no operation selector crosses the
/// optimizing ABI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableMbstringBuiltin {
    DetectEncoding,
    CheckEncoding,
    ConvertEncoding,
    InternalEncoding,
    ListEncodings,
    EncodingAliases,
    SubstituteCharacter,
    Strlen,
    Strtolower,
    Strtoupper,
    Stripos,
    Strpos,
    Strripos,
    Strrpos,
    SubstrCount,
    Substr,
    Strcut,
    Strwidth,
    Strimwidth,
    ConvertCase,
    Ucfirst,
    Lcfirst,
    Ord,
    Chr,
    ParseStr,
    Iconv,
    NormalizerNormalize,
    NormalizerIsNormalized,
}

impl StableMbstringBuiltin {
    pub(super) const COUNT: usize = 28;

    pub(super) const fn index(self) -> usize {
        self as usize
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::DetectEncoding => "phrust_native_mb_detect_encoding",
            Self::CheckEncoding => "phrust_native_mb_check_encoding",
            Self::ConvertEncoding => "phrust_native_mb_convert_encoding",
            Self::InternalEncoding => "phrust_native_mb_internal_encoding",
            Self::ListEncodings => "phrust_native_mb_list_encodings",
            Self::EncodingAliases => "phrust_native_mb_encoding_aliases",
            Self::SubstituteCharacter => "phrust_native_mb_substitute_character",
            Self::Strlen => "phrust_native_mb_strlen",
            Self::Strtolower => "phrust_native_mb_strtolower",
            Self::Strtoupper => "phrust_native_mb_strtoupper",
            Self::Stripos => "phrust_native_mb_stripos",
            Self::Strpos => "phrust_native_mb_strpos",
            Self::Strripos => "phrust_native_mb_strripos",
            Self::Strrpos => "phrust_native_mb_strrpos",
            Self::SubstrCount => "phrust_native_mb_substr_count",
            Self::Substr => "phrust_native_mb_substr",
            Self::Strcut => "phrust_native_mb_strcut",
            Self::Strwidth => "phrust_native_mb_strwidth",
            Self::Strimwidth => "phrust_native_mb_strimwidth",
            Self::ConvertCase => "phrust_native_mb_convert_case",
            Self::Ucfirst => "phrust_native_mb_ucfirst",
            Self::Lcfirst => "phrust_native_mb_lcfirst",
            Self::Ord => "phrust_native_mb_ord",
            Self::Chr => "phrust_native_mb_chr",
            Self::ParseStr => "phrust_native_mb_parse_str",
            Self::Iconv => "phrust_native_iconv",
            Self::NormalizerNormalize => "phrust_native_normalizer_normalize",
            Self::NormalizerIsNormalized => "phrust_native_normalizer_is_normalized",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::DetectEncoding => arity >= 1 && arity <= 3,
            Self::CheckEncoding => arity <= 2,
            Self::ConvertEncoding => arity >= 2 && arity <= 3,
            Self::InternalEncoding | Self::SubstituteCharacter => arity <= 1,
            Self::ListEncodings => arity == 0,
            Self::EncodingAliases => arity == 1,
            Self::Strlen
            | Self::Strtolower
            | Self::Strtoupper
            | Self::Strwidth
            | Self::Ucfirst
            | Self::Lcfirst
            | Self::Ord
            | Self::Chr => arity >= 1 && arity <= 2,
            Self::Stripos | Self::Strpos | Self::Strripos | Self::Strrpos => {
                arity >= 2 && arity <= 4
            }
            Self::SubstrCount | Self::ConvertCase => arity >= 2 && arity <= 3,
            Self::Substr | Self::Strcut => arity >= 2 && arity <= 4,
            Self::Strimwidth => arity >= 3 && arity <= 5,
            Self::ParseStr => arity == 2,
            Self::Iconv => arity == 3,
            Self::NormalizerNormalize | Self::NormalizerIsNormalized => arity >= 1 && arity <= 2,
        }
    }

    pub(super) const fn argument_is_by_reference(self, index: usize) -> bool {
        matches!(self, Self::ParseStr) && index == 1
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::DetectEncoding,
            Self::CheckEncoding,
            Self::ConvertEncoding,
            Self::InternalEncoding,
            Self::ListEncodings,
            Self::EncodingAliases,
            Self::SubstituteCharacter,
            Self::Strlen,
            Self::Strtolower,
            Self::Strtoupper,
            Self::Stripos,
            Self::Strpos,
            Self::Strripos,
            Self::Strrpos,
            Self::SubstrCount,
            Self::Substr,
            Self::Strcut,
            Self::Strwidth,
            Self::Strimwidth,
            Self::ConvertCase,
            Self::Ucfirst,
            Self::Lcfirst,
            Self::Ord,
            Self::Chr,
            Self::ParseStr,
            Self::Iconv,
            Self::NormalizerNormalize,
            Self::NormalizerIsNormalized,
        ]
    }
}

pub(super) fn stable_builtin_mbstring(target: &RegionCallTarget) -> Option<StableMbstringBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "mb_detect_encoding" => Some(StableMbstringBuiltin::DetectEncoding),
        "mb_check_encoding" => Some(StableMbstringBuiltin::CheckEncoding),
        "mb_convert_encoding" => Some(StableMbstringBuiltin::ConvertEncoding),
        "mb_internal_encoding" => Some(StableMbstringBuiltin::InternalEncoding),
        "mb_list_encodings" => Some(StableMbstringBuiltin::ListEncodings),
        "mb_encoding_aliases" => Some(StableMbstringBuiltin::EncodingAliases),
        "mb_substitute_character" => Some(StableMbstringBuiltin::SubstituteCharacter),
        "mb_strlen" => Some(StableMbstringBuiltin::Strlen),
        "mb_strtolower" => Some(StableMbstringBuiltin::Strtolower),
        "mb_strtoupper" => Some(StableMbstringBuiltin::Strtoupper),
        "mb_stripos" => Some(StableMbstringBuiltin::Stripos),
        "mb_strpos" => Some(StableMbstringBuiltin::Strpos),
        "mb_strripos" => Some(StableMbstringBuiltin::Strripos),
        "mb_strrpos" => Some(StableMbstringBuiltin::Strrpos),
        "mb_substr_count" => Some(StableMbstringBuiltin::SubstrCount),
        "mb_substr" => Some(StableMbstringBuiltin::Substr),
        "mb_strcut" => Some(StableMbstringBuiltin::Strcut),
        "mb_strwidth" => Some(StableMbstringBuiltin::Strwidth),
        "mb_strimwidth" => Some(StableMbstringBuiltin::Strimwidth),
        "mb_convert_case" => Some(StableMbstringBuiltin::ConvertCase),
        "mb_ucfirst" => Some(StableMbstringBuiltin::Ucfirst),
        "mb_lcfirst" => Some(StableMbstringBuiltin::Lcfirst),
        "mb_ord" => Some(StableMbstringBuiltin::Ord),
        "mb_chr" => Some(StableMbstringBuiltin::Chr),
        "mb_parse_str" => Some(StableMbstringBuiltin::ParseStr),
        "iconv" => Some(StableMbstringBuiltin::Iconv),
        "normalizer_normalize" => Some(StableMbstringBuiltin::NormalizerNormalize),
        "normalizer_is_normalized" => Some(StableMbstringBuiltin::NormalizerIsNormalized),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableBcmathBuiltin {
    Add,
    Comp,
    Div,
    Mod,
    Mul,
    Pow,
    PowMod,
    Scale,
    Sqrt,
    Sub,
}

impl StableBcmathBuiltin {
    pub(super) const COUNT: usize = 10;

    pub(super) const fn index(self) -> usize {
        self as usize
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Add => "phrust_native_bcadd",
            Self::Comp => "phrust_native_bccomp",
            Self::Div => "phrust_native_bcdiv",
            Self::Mod => "phrust_native_bcmod",
            Self::Mul => "phrust_native_bcmul",
            Self::Pow => "phrust_native_bcpow",
            Self::PowMod => "phrust_native_bcpowmod",
            Self::Scale => "phrust_native_bcscale",
            Self::Sqrt => "phrust_native_bcsqrt",
            Self::Sub => "phrust_native_bcsub",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Add | Self::Comp | Self::Div | Self::Mod | Self::Mul | Self::Pow | Self::Sub => {
                arity >= 2 && arity <= 3
            }
            Self::PowMod => arity >= 3 && arity <= 4,
            Self::Scale => arity <= 1,
            Self::Sqrt => arity >= 1 && arity <= 2,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Add,
            Self::Comp,
            Self::Div,
            Self::Mod,
            Self::Mul,
            Self::Pow,
            Self::PowMod,
            Self::Scale,
            Self::Sqrt,
            Self::Sub,
        ]
    }
}

pub(super) fn stable_builtin_bcmath(target: &RegionCallTarget) -> Option<StableBcmathBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "bcadd" => Some(StableBcmathBuiltin::Add),
        "bccomp" => Some(StableBcmathBuiltin::Comp),
        "bcdiv" => Some(StableBcmathBuiltin::Div),
        "bcmod" => Some(StableBcmathBuiltin::Mod),
        "bcmul" => Some(StableBcmathBuiltin::Mul),
        "bcpow" => Some(StableBcmathBuiltin::Pow),
        "bcpowmod" => Some(StableBcmathBuiltin::PowMod),
        "bcscale" => Some(StableBcmathBuiltin::Scale),
        "bcsqrt" => Some(StableBcmathBuiltin::Sqrt),
        "bcsub" => Some(StableBcmathBuiltin::Sub),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableFilterBuiltin {
    Input,
    HasVar,
    InputArray,
    VarArray,
    List,
    Id,
    Var,
}

impl StableFilterBuiltin {
    pub(super) const COUNT: usize = 7;

    pub(super) const fn index(self) -> usize {
        self as usize
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Input => "phrust_native_filter_input",
            Self::HasVar => "phrust_native_filter_has_var",
            Self::InputArray => "phrust_native_filter_input_array",
            Self::VarArray => "phrust_native_filter_var_array",
            Self::List => "phrust_native_filter_list",
            Self::Id => "phrust_native_filter_id",
            Self::Var => "phrust_native_filter_var",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Input => arity >= 2 && arity <= 4,
            Self::HasVar => arity == 2,
            Self::InputArray | Self::VarArray => arity >= 1 && arity <= 3,
            Self::List => arity == 0,
            Self::Id => arity == 1,
            Self::Var => arity >= 1 && arity <= 3,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Input,
            Self::HasVar,
            Self::InputArray,
            Self::VarArray,
            Self::List,
            Self::Id,
            Self::Var,
        ]
    }
}

pub(super) fn stable_builtin_filter(target: &RegionCallTarget) -> Option<StableFilterBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "filter_input" => Some(StableFilterBuiltin::Input),
        "filter_has_var" => Some(StableFilterBuiltin::HasVar),
        "filter_input_array" => Some(StableFilterBuiltin::InputArray),
        "filter_var_array" => Some(StableFilterBuiltin::VarArray),
        "filter_list" => Some(StableFilterBuiltin::List),
        "filter_id" => Some(StableFilterBuiltin::Id),
        "filter_var" => Some(StableFilterBuiltin::Var),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableSessionBuiltin {
    Abort,
    CacheExpire,
    CacheLimiter,
    Commit,
    Destroy,
    Gc,
    Decode,
    Encode,
    CreateId,
    GetCookieParams,
    Id,
    ModuleName,
    Name,
    RegenerateId,
    RegisterShutdown,
    Reset,
    SavePath,
    SetCookieParams,
    SetSaveHandler,
    Start,
    Status,
    Unset,
    WriteClose,
}

impl StableSessionBuiltin {
    pub(super) const COUNT: usize = 23;

    pub(super) const fn index(self) -> usize {
        self as usize
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Abort => "phrust_native_session_abort",
            Self::CacheExpire => "phrust_native_session_cache_expire",
            Self::CacheLimiter => "phrust_native_session_cache_limiter",
            Self::Commit => "phrust_native_session_commit",
            Self::Destroy => "phrust_native_session_destroy",
            Self::Gc => "phrust_native_session_gc",
            Self::Decode => "phrust_native_session_decode",
            Self::Encode => "phrust_native_session_encode",
            Self::CreateId => "phrust_native_session_create_id",
            Self::GetCookieParams => "phrust_native_session_get_cookie_params",
            Self::Id => "phrust_native_session_id",
            Self::ModuleName => "phrust_native_session_module_name",
            Self::Name => "phrust_native_session_name",
            Self::RegenerateId => "phrust_native_session_regenerate_id",
            Self::RegisterShutdown => "phrust_native_session_register_shutdown",
            Self::Reset => "phrust_native_session_reset",
            Self::SavePath => "phrust_native_session_save_path",
            Self::SetCookieParams => "phrust_native_session_set_cookie_params",
            Self::SetSaveHandler => "phrust_native_session_set_save_handler",
            Self::Start => "phrust_native_session_start",
            Self::Status => "phrust_native_session_status",
            Self::Unset => "phrust_native_session_unset",
            Self::WriteClose => "phrust_native_session_write_close",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::CacheExpire
            | Self::CacheLimiter
            | Self::CreateId
            | Self::Id
            | Self::ModuleName
            | Self::Name
            | Self::RegenerateId
            | Self::SavePath
            | Self::Start => arity <= 1,
            Self::Decode => arity == 1,
            Self::SetCookieParams => arity >= 1 && arity <= 5,
            Self::SetSaveHandler => arity >= 1 && arity <= 9,
            Self::Abort
            | Self::Commit
            | Self::Destroy
            | Self::Gc
            | Self::Encode
            | Self::GetCookieParams
            | Self::RegisterShutdown
            | Self::Reset
            | Self::Status
            | Self::Unset
            | Self::WriteClose => arity == 0,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Abort,
            Self::CacheExpire,
            Self::CacheLimiter,
            Self::Commit,
            Self::Destroy,
            Self::Gc,
            Self::Decode,
            Self::Encode,
            Self::CreateId,
            Self::GetCookieParams,
            Self::Id,
            Self::ModuleName,
            Self::Name,
            Self::RegenerateId,
            Self::RegisterShutdown,
            Self::Reset,
            Self::SavePath,
            Self::SetCookieParams,
            Self::SetSaveHandler,
            Self::Start,
            Self::Status,
            Self::Unset,
            Self::WriteClose,
        ]
    }
}

pub(super) fn stable_builtin_session(target: &RegionCallTarget) -> Option<StableSessionBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "session_abort" => Some(StableSessionBuiltin::Abort),
        "session_cache_expire" => Some(StableSessionBuiltin::CacheExpire),
        "session_cache_limiter" => Some(StableSessionBuiltin::CacheLimiter),
        "session_commit" => Some(StableSessionBuiltin::Commit),
        "session_destroy" => Some(StableSessionBuiltin::Destroy),
        "session_gc" => Some(StableSessionBuiltin::Gc),
        "session_decode" => Some(StableSessionBuiltin::Decode),
        "session_encode" => Some(StableSessionBuiltin::Encode),
        "session_create_id" => Some(StableSessionBuiltin::CreateId),
        "session_get_cookie_params" => Some(StableSessionBuiltin::GetCookieParams),
        "session_id" => Some(StableSessionBuiltin::Id),
        "session_module_name" => Some(StableSessionBuiltin::ModuleName),
        "session_name" => Some(StableSessionBuiltin::Name),
        "session_regenerate_id" => Some(StableSessionBuiltin::RegenerateId),
        "session_register_shutdown" => Some(StableSessionBuiltin::RegisterShutdown),
        "session_reset" => Some(StableSessionBuiltin::Reset),
        "session_save_path" => Some(StableSessionBuiltin::SavePath),
        "session_set_cookie_params" => Some(StableSessionBuiltin::SetCookieParams),
        "session_set_save_handler" => Some(StableSessionBuiltin::SetSaveHandler),
        "session_start" => Some(StableSessionBuiltin::Start),
        "session_status" => Some(StableSessionBuiltin::Status),
        "session_unset" => Some(StableSessionBuiltin::Unset),
        "session_write_close" => Some(StableSessionBuiltin::WriteClose),
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

pub(super) fn stable_builtin_error_log(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("error_log")
}

pub(super) fn stable_builtin_sleep(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("sleep")
}

/// Scalar math primitives whose ordinary int/float forms are emitted over
/// native numeric slots. Each discriminant is compile-time lowering metadata,
/// never a runtime operation ID.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableScalarMathBuiltin {
    Abs,
    Ceil,
    Floor,
    Sqrt,
    Fdiv,
    Fmod,
    IsFinite,
    IsInfinite,
    IsNan,
    Pi,
}

impl StableScalarMathBuiltin {
    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Fdiv | Self::Fmod => arity == 2,
            Self::Pi => arity == 0,
            Self::Abs
            | Self::Ceil
            | Self::Floor
            | Self::Sqrt
            | Self::IsFinite
            | Self::IsInfinite
            | Self::IsNan => arity == 1,
        }
    }
}

pub(super) fn stable_builtin_scalar_math(
    target: &RegionCallTarget,
) -> Option<StableScalarMathBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "abs" => Some(StableScalarMathBuiltin::Abs),
        "ceil" => Some(StableScalarMathBuiltin::Ceil),
        "floor" => Some(StableScalarMathBuiltin::Floor),
        "sqrt" => Some(StableScalarMathBuiltin::Sqrt),
        "fdiv" => Some(StableScalarMathBuiltin::Fdiv),
        "fmod" => Some(StableScalarMathBuiltin::Fmod),
        "is_finite" => Some(StableScalarMathBuiltin::IsFinite),
        "is_infinite" => Some(StableScalarMathBuiltin::IsInfinite),
        "is_nan" => Some(StableScalarMathBuiltin::IsNan),
        "pi" => Some(StableScalarMathBuiltin::Pi),
        _ => None,
    }
}

/// Stateless transcendental math builtins whose ordinary numeric forms call
/// one exact, compile-time-selected pure symbol. The index is publication
/// metadata only: generated code never passes it to a runtime dispatcher.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StablePureMathBuiltin {
    Acos,
    Acosh,
    Asin,
    Asinh,
    Atan,
    Atan2,
    Atanh,
    Cos,
    Cosh,
    Deg2Rad,
    Exp,
    Expm1,
    Fpow,
    Hypot,
    Log,
    Log10,
    Log1p,
    Rad2Deg,
    Sin,
    Sinh,
    Tan,
    Tanh,
}

impl StablePureMathBuiltin {
    pub(super) const COUNT: usize = 22;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Acos => 0,
            Self::Acosh => 1,
            Self::Asin => 2,
            Self::Asinh => 3,
            Self::Atan => 4,
            Self::Atan2 => 5,
            Self::Atanh => 6,
            Self::Cos => 7,
            Self::Cosh => 8,
            Self::Deg2Rad => 9,
            Self::Exp => 10,
            Self::Expm1 => 11,
            Self::Fpow => 12,
            Self::Hypot => 13,
            Self::Log => 14,
            Self::Log10 => 15,
            Self::Log1p => 16,
            Self::Rad2Deg => 17,
            Self::Sin => 18,
            Self::Sinh => 19,
            Self::Tan => 20,
            Self::Tanh => 21,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Acos => "phrust_native_acos_f64",
            Self::Acosh => "phrust_native_acosh_f64",
            Self::Asin => "phrust_native_asin_f64",
            Self::Asinh => "phrust_native_asinh_f64",
            Self::Atan => "phrust_native_atan_f64",
            Self::Atan2 => "phrust_native_atan2_f64",
            Self::Atanh => "phrust_native_atanh_f64",
            Self::Cos => "phrust_native_cos_f64",
            Self::Cosh => "phrust_native_cosh_f64",
            Self::Deg2Rad => "phrust_native_deg2rad_f64",
            Self::Exp => "phrust_native_exp_f64",
            Self::Expm1 => "phrust_native_expm1_f64",
            Self::Fpow => "phrust_native_fpow_f64",
            Self::Hypot => "phrust_native_hypot_f64",
            Self::Log => "phrust_native_log_f64",
            Self::Log10 => "phrust_native_log10_f64",
            Self::Log1p => "phrust_native_log1p_f64",
            Self::Rad2Deg => "phrust_native_rad2deg_f64",
            Self::Sin => "phrust_native_sin_f64",
            Self::Sinh => "phrust_native_sinh_f64",
            Self::Tan => "phrust_native_tan_f64",
            Self::Tanh => "phrust_native_tanh_f64",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Atan2 | Self::Fpow | Self::Hypot => arity == 2,
            Self::Acos
            | Self::Acosh
            | Self::Asin
            | Self::Asinh
            | Self::Atan
            | Self::Atanh
            | Self::Cos
            | Self::Cosh
            | Self::Deg2Rad
            | Self::Exp
            | Self::Expm1
            | Self::Log
            | Self::Log10
            | Self::Log1p
            | Self::Rad2Deg
            | Self::Sin
            | Self::Sinh
            | Self::Tan
            | Self::Tanh => arity == 1,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Acos,
            Self::Acosh,
            Self::Asin,
            Self::Asinh,
            Self::Atan,
            Self::Atan2,
            Self::Atanh,
            Self::Cos,
            Self::Cosh,
            Self::Deg2Rad,
            Self::Exp,
            Self::Expm1,
            Self::Fpow,
            Self::Hypot,
            Self::Log,
            Self::Log10,
            Self::Log1p,
            Self::Rad2Deg,
            Self::Sin,
            Self::Sinh,
            Self::Tan,
            Self::Tanh,
        ]
    }
}

pub(super) fn stable_builtin_pure_math(target: &RegionCallTarget) -> Option<StablePureMathBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "acos" => Some(StablePureMathBuiltin::Acos),
        "acosh" => Some(StablePureMathBuiltin::Acosh),
        "asin" => Some(StablePureMathBuiltin::Asin),
        "asinh" => Some(StablePureMathBuiltin::Asinh),
        "atan" => Some(StablePureMathBuiltin::Atan),
        "atan2" => Some(StablePureMathBuiltin::Atan2),
        "atanh" => Some(StablePureMathBuiltin::Atanh),
        "cos" => Some(StablePureMathBuiltin::Cos),
        "cosh" => Some(StablePureMathBuiltin::Cosh),
        "deg2rad" => Some(StablePureMathBuiltin::Deg2Rad),
        "exp" => Some(StablePureMathBuiltin::Exp),
        "expm1" => Some(StablePureMathBuiltin::Expm1),
        "fpow" => Some(StablePureMathBuiltin::Fpow),
        "hypot" => Some(StablePureMathBuiltin::Hypot),
        "log" => Some(StablePureMathBuiltin::Log),
        "log10" => Some(StablePureMathBuiltin::Log10),
        "log1p" => Some(StablePureMathBuiltin::Log1p),
        "rad2deg" => Some(StablePureMathBuiltin::Rad2Deg),
        "sin" => Some(StablePureMathBuiltin::Sin),
        "sinh" => Some(StablePureMathBuiltin::Sinh),
        "tan" => Some(StablePureMathBuiltin::Tan),
        "tanh" => Some(StablePureMathBuiltin::Tanh),
        _ => None,
    }
}

/// Native reduction over PHP's ordinary comparison ordering.
///
/// The operation identity is compilation metadata only. Optimizing code
/// reduces fixed arguments or one direct array through the existing exact
/// scalar/array/object comparison lanes and never enters builtin dispatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableExtremaBuiltin {
    Max,
    Min,
}

pub(super) fn stable_builtin_extrema(target: &RegionCallTarget) -> Option<StableExtremaBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "max" => Some(StableExtremaBuiltin::Max),
        "min" => Some(StableExtremaBuiltin::Min),
        _ => None,
    }
}

/// Scalar conversion and type-name consumers that can stay on the same native
/// value representation as casts and tag tests. Publication rejects optional
/// or reference-mutating forms before an optimizing region is entered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableScalarConsumerBuiltin {
    BoolVal,
    FloatVal,
    IntVal,
    StrVal,
    GetType,
    GetDebugType,
}

pub(super) fn stable_builtin_scalar_consumer(
    target: &RegionCallTarget,
) -> Option<StableScalarConsumerBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "boolval" => Some(StableScalarConsumerBuiltin::BoolVal),
        "floatval" => Some(StableScalarConsumerBuiltin::FloatVal),
        "intval" => Some(StableScalarConsumerBuiltin::IntVal),
        "strval" => Some(StableScalarConsumerBuiltin::StrVal),
        "gettype" => Some(StableScalarConsumerBuiltin::GetType),
        "get_debug_type" => Some(StableScalarConsumerBuiltin::GetDebugType),
        _ => None,
    }
}

/// Numeric builtins that are the function-form counterparts of native
/// arithmetic or one exact pure numeric call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableNumericOperatorBuiltin {
    Pow,
    IntDiv,
    Round,
}

impl StableNumericOperatorBuiltin {
    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Pow | Self::IntDiv => arity == 2,
            Self::Round => arity >= 1 && arity <= 3,
        }
    }
}

pub(super) fn stable_builtin_numeric_operator(
    target: &RegionCallTarget,
) -> Option<StableNumericOperatorBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "pow" => Some(StableNumericOperatorBuiltin::Pow),
        "intdiv" => Some(StableNumericOperatorBuiltin::IntDiv),
        "round" => Some(StableNumericOperatorBuiltin::Round),
        _ => None,
    }
}

/// Exact native handlers for PHP's complete integer/base conversion family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableBaseConversionBuiltin {
    BaseConvert,
    BinDec,
    DecBin,
    DecHex,
    DecOct,
    HexDec,
    OctDec,
}

impl StableBaseConversionBuiltin {
    pub(super) const COUNT: usize = 7;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::BaseConvert => 0,
            Self::BinDec => 1,
            Self::DecBin => 2,
            Self::DecHex => 3,
            Self::DecOct => 4,
            Self::HexDec => 5,
            Self::OctDec => 6,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::BaseConvert => "phrust_native_base_convert",
            Self::BinDec => "phrust_native_bindec",
            Self::DecBin => "phrust_native_decbin",
            Self::DecHex => "phrust_native_dechex",
            Self::DecOct => "phrust_native_decoct",
            Self::HexDec => "phrust_native_hexdec",
            Self::OctDec => "phrust_native_octdec",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::BaseConvert => arity == 3,
            _ => arity == 1,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::BaseConvert,
            Self::BinDec,
            Self::DecBin,
            Self::DecHex,
            Self::DecOct,
            Self::HexDec,
            Self::OctDec,
        ]
    }
}

pub(super) fn stable_builtin_base_conversion(
    target: &RegionCallTarget,
) -> Option<StableBaseConversionBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "base_convert" => Some(StableBaseConversionBuiltin::BaseConvert),
        "bindec" => Some(StableBaseConversionBuiltin::BinDec),
        "decbin" => Some(StableBaseConversionBuiltin::DecBin),
        "dechex" => Some(StableBaseConversionBuiltin::DecHex),
        "decoct" => Some(StableBaseConversionBuiltin::DecOct),
        "hexdec" => Some(StableBaseConversionBuiltin::HexDec),
        "octdec" => Some(StableBaseConversionBuiltin::OctDec),
        _ => None,
    }
}

/// Exact stateless conversions between textual and packed network addresses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableNetworkAddressBuiltin {
    Ip2Long,
    Long2Ip,
    InetPton,
    InetNtop,
}

impl StableNetworkAddressBuiltin {
    pub(super) const COUNT: usize = 4;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Ip2Long => 0,
            Self::Long2Ip => 1,
            Self::InetPton => 2,
            Self::InetNtop => 3,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Ip2Long => "phrust_native_ip2long",
            Self::Long2Ip => "phrust_native_long2ip",
            Self::InetPton => "phrust_native_inet_pton",
            Self::InetNtop => "phrust_native_inet_ntop",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::Ip2Long, Self::Long2Ip, Self::InetPton, Self::InetNtop]
    }
}

pub(super) fn stable_builtin_network_address(
    target: &RegionCallTarget,
) -> Option<StableNetworkAddressBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "ip2long" => Some(StableNetworkAddressBuiltin::Ip2Long),
        "long2ip" => Some(StableNetworkAddressBuiltin::Long2Ip),
        "inet_pton" => Some(StableNetworkAddressBuiltin::InetPton),
        "inet_ntop" => Some(StableNetworkAddressBuiltin::InetNtop),
        _ => None,
    }
}

/// Complete stateless zlib/gzip encode-decode family over native byte strings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableCompressionCodecBuiltin {
    GzEncode,
    GzCompress,
    GzDeflate,
    GzDecode,
    GzUncompress,
    GzInflate,
    ZlibDecode,
    ZlibEncode,
}

impl StableCompressionCodecBuiltin {
    pub(super) const COUNT: usize = 8;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::GzEncode => 0,
            Self::GzCompress => 1,
            Self::GzDeflate => 2,
            Self::GzDecode => 3,
            Self::GzUncompress => 4,
            Self::GzInflate => 5,
            Self::ZlibDecode => 6,
            Self::ZlibEncode => 7,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::GzEncode => "phrust_native_gzencode",
            Self::GzCompress => "phrust_native_gzcompress",
            Self::GzDeflate => "phrust_native_gzdeflate",
            Self::GzDecode => "phrust_native_gzdecode",
            Self::GzUncompress => "phrust_native_gzuncompress",
            Self::GzInflate => "phrust_native_gzinflate",
            Self::ZlibDecode => "phrust_native_zlib_decode",
            Self::ZlibEncode => "phrust_native_zlib_encode",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::GzEncode | Self::GzCompress | Self::GzDeflate => arity >= 1 && arity <= 3,
            Self::GzDecode | Self::GzUncompress | Self::GzInflate | Self::ZlibDecode => {
                arity >= 1 && arity <= 2
            }
            Self::ZlibEncode => arity >= 2 && arity <= 3,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::GzEncode,
            Self::GzCompress,
            Self::GzDeflate,
            Self::GzDecode,
            Self::GzUncompress,
            Self::GzInflate,
            Self::ZlibDecode,
            Self::ZlibEncode,
        ]
    }
}

pub(super) fn stable_builtin_compression_codec(
    target: &RegionCallTarget,
) -> Option<StableCompressionCodecBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "gzencode" => Some(StableCompressionCodecBuiltin::GzEncode),
        "gzcompress" => Some(StableCompressionCodecBuiltin::GzCompress),
        "gzdeflate" => Some(StableCompressionCodecBuiltin::GzDeflate),
        "gzdecode" => Some(StableCompressionCodecBuiltin::GzDecode),
        "gzuncompress" => Some(StableCompressionCodecBuiltin::GzUncompress),
        "gzinflate" => Some(StableCompressionCodecBuiltin::GzInflate),
        "zlib_decode" => Some(StableCompressionCodecBuiltin::ZlibDecode),
        "zlib_encode" => Some(StableCompressionCodecBuiltin::ZlibEncode),
        _ => None,
    }
}

/// Exact symbol operations. The selector is part of the dedicated native ABI
/// and never enters the prepared builtin dispatcher.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableSymbolQueryBuiltin {
    Define,
    Defined,
    Constant,
    FunctionExists,
    ClassExists,
    InterfaceExists,
    TraitExists,
    EnumExists,
    MethodExists,
    PropertyExists,
}

impl StableSymbolQueryBuiltin {
    pub(super) const COUNT: usize = 10;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Define => 0,
            Self::Defined => 1,
            Self::Constant => 2,
            Self::FunctionExists => 3,
            Self::ClassExists => 4,
            Self::InterfaceExists => 5,
            Self::TraitExists => 6,
            Self::EnumExists => 7,
            Self::MethodExists => 8,
            Self::PropertyExists => 9,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Define => "phrust_native_define",
            Self::Defined => "phrust_native_defined",
            Self::Constant => "phrust_native_constant",
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
            Self::Define,
            Self::Defined,
            Self::Constant,
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
            Self::Define => arity == 2,
            Self::Defined | Self::Constant | Self::FunctionExists => arity == 1,
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
        "define" => Some(StableSymbolQueryBuiltin::Define),
        "defined" => Some(StableSymbolQueryBuiltin::Defined),
        "constant" => Some(StableSymbolQueryBuiltin::Constant),
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
            Self::Decode => arity >= 1 && arity <= 4,
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
    NumberFormat,
}

impl StableFormatBuiltin {
    pub(super) const COUNT: usize = 5;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Sprintf => 0,
            Self::Printf => 1,
            Self::Vsprintf => 2,
            Self::Vprintf => 3,
            Self::NumberFormat => 4,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Sprintf => "phrust_native_sprintf",
            Self::Printf => "phrust_native_printf",
            Self::Vsprintf => "phrust_native_vsprintf",
            Self::Vprintf => "phrust_native_vprintf",
            Self::NumberFormat => "phrust_native_number_format",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Sprintf,
            Self::Printf,
            Self::Vsprintf,
            Self::Vprintf,
            Self::NumberFormat,
        ]
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Sprintf | Self::Printf => arity >= 1,
            Self::Vsprintf | Self::Vprintf => arity == 2,
            Self::NumberFormat => arity >= 1 && arity <= 4,
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
        "number_format" => Some(StableFormatBuiltin::NumberFormat),
        _ => None,
    }
}

/// Exact stateless digest/checksum operations. Each selector resolves to one
/// fixed native symbol; it is never passed to a runtime dispatcher.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableHashBuiltin {
    Md5,
    Sha1,
    Crc32,
    Hash,
    HashHmac,
    HashEquals,
    SodiumGenericHash,
}

impl StableHashBuiltin {
    pub(super) const COUNT: usize = 7;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Md5 => 0,
            Self::Sha1 => 1,
            Self::Crc32 => 2,
            Self::Hash => 3,
            Self::HashHmac => 4,
            Self::HashEquals => 5,
            Self::SodiumGenericHash => 6,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Md5 => "phrust_native_md5",
            Self::Sha1 => "phrust_native_sha1",
            Self::Crc32 => "phrust_native_crc32",
            Self::Hash => "phrust_native_hash",
            Self::HashHmac => "phrust_native_hash_hmac",
            Self::HashEquals => "phrust_native_hash_equals",
            Self::SodiumGenericHash => "phrust_native_sodium_crypto_generichash",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Md5 | Self::Sha1 => arity == 1 || arity == 2,
            Self::Crc32 => arity == 1,
            Self::Hash => arity >= 2 && arity <= 4,
            Self::HashHmac => arity == 3 || arity == 4,
            Self::HashEquals => arity == 2,
            Self::SodiumGenericHash => arity >= 1 && arity <= 3,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Md5,
            Self::Sha1,
            Self::Crc32,
            Self::Hash,
            Self::HashHmac,
            Self::HashEquals,
            Self::SodiumGenericHash,
        ]
    }
}

pub(super) fn stable_builtin_hash(target: &RegionCallTarget) -> Option<StableHashBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "md5" => Some(StableHashBuiltin::Md5),
        "sha1" => Some(StableHashBuiltin::Sha1),
        "crc32" => Some(StableHashBuiltin::Crc32),
        "hash" => Some(StableHashBuiltin::Hash),
        "hash_hmac" => Some(StableHashBuiltin::HashHmac),
        "hash_equals" => Some(StableHashBuiltin::HashEquals),
        "sodium_crypto_generichash" => Some(StableHashBuiltin::SodiumGenericHash),
        _ => None,
    }
}

/// Exact byte-to-byte codec operations over authoritative native strings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableByteCodecBuiltin {
    Base64Encode,
    Base64Decode,
    Bin2Hex,
    Hex2Bin,
    QuotedPrintableDecode,
    UrlEncode,
    RawUrlEncode,
    UrlDecode,
    RawUrlDecode,
    UuEncode,
    UuDecode,
    AddCSlashes,
    StripCSlashes,
    StripSlashes,
    QuoteMeta,
    Pack,
    Unpack,
    SodiumBin2Base64,
}

impl StableByteCodecBuiltin {
    pub(super) const COUNT: usize = 18;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Base64Encode => 0,
            Self::Base64Decode => 1,
            Self::Bin2Hex => 2,
            Self::Hex2Bin => 3,
            Self::QuotedPrintableDecode => 4,
            Self::UrlEncode => 5,
            Self::RawUrlEncode => 6,
            Self::UrlDecode => 7,
            Self::RawUrlDecode => 8,
            Self::UuEncode => 9,
            Self::UuDecode => 10,
            Self::AddCSlashes => 11,
            Self::StripCSlashes => 12,
            Self::StripSlashes => 13,
            Self::QuoteMeta => 14,
            Self::Pack => 15,
            Self::Unpack => 16,
            Self::SodiumBin2Base64 => 17,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Base64Encode => "phrust_native_base64_encode",
            Self::Base64Decode => "phrust_native_base64_decode",
            Self::Bin2Hex => "phrust_native_bin2hex",
            Self::Hex2Bin => "phrust_native_hex2bin",
            Self::QuotedPrintableDecode => "phrust_native_quoted_printable_decode",
            Self::UrlEncode => "phrust_native_urlencode",
            Self::RawUrlEncode => "phrust_native_rawurlencode",
            Self::UrlDecode => "phrust_native_urldecode",
            Self::RawUrlDecode => "phrust_native_rawurldecode",
            Self::UuEncode => "phrust_native_convert_uuencode",
            Self::UuDecode => "phrust_native_convert_uudecode",
            Self::AddCSlashes => "phrust_native_addcslashes",
            Self::StripCSlashes => "phrust_native_stripcslashes",
            Self::StripSlashes => "phrust_native_stripslashes",
            Self::QuoteMeta => "phrust_native_quotemeta",
            Self::Pack => "phrust_native_pack",
            Self::Unpack => "phrust_native_unpack",
            Self::SodiumBin2Base64 => "phrust_native_sodium_bin2base64",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Base64Decode => arity == 1 || arity == 2,
            Self::AddCSlashes | Self::SodiumBin2Base64 => arity == 2,
            Self::Pack => arity >= 1,
            Self::Unpack => arity == 2 || arity == 3,
            Self::Base64Encode
            | Self::Bin2Hex
            | Self::Hex2Bin
            | Self::QuotedPrintableDecode
            | Self::UrlEncode
            | Self::RawUrlEncode
            | Self::UrlDecode
            | Self::RawUrlDecode
            | Self::UuEncode
            | Self::UuDecode
            | Self::StripCSlashes
            | Self::StripSlashes
            | Self::QuoteMeta => arity == 1,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Base64Encode,
            Self::Base64Decode,
            Self::Bin2Hex,
            Self::Hex2Bin,
            Self::QuotedPrintableDecode,
            Self::UrlEncode,
            Self::RawUrlEncode,
            Self::UrlDecode,
            Self::RawUrlDecode,
            Self::UuEncode,
            Self::UuDecode,
            Self::AddCSlashes,
            Self::StripCSlashes,
            Self::StripSlashes,
            Self::QuoteMeta,
            Self::Pack,
            Self::Unpack,
            Self::SodiumBin2Base64,
        ]
    }
}

pub(super) fn stable_builtin_byte_codec(
    target: &RegionCallTarget,
) -> Option<StableByteCodecBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "base64_encode" => Some(StableByteCodecBuiltin::Base64Encode),
        "base64_decode" => Some(StableByteCodecBuiltin::Base64Decode),
        "bin2hex" => Some(StableByteCodecBuiltin::Bin2Hex),
        "hex2bin" => Some(StableByteCodecBuiltin::Hex2Bin),
        "quoted_printable_decode" => Some(StableByteCodecBuiltin::QuotedPrintableDecode),
        "urlencode" => Some(StableByteCodecBuiltin::UrlEncode),
        "rawurlencode" => Some(StableByteCodecBuiltin::RawUrlEncode),
        "urldecode" => Some(StableByteCodecBuiltin::UrlDecode),
        "rawurldecode" => Some(StableByteCodecBuiltin::RawUrlDecode),
        "convert_uuencode" => Some(StableByteCodecBuiltin::UuEncode),
        "convert_uudecode" => Some(StableByteCodecBuiltin::UuDecode),
        "addcslashes" => Some(StableByteCodecBuiltin::AddCSlashes),
        "stripcslashes" => Some(StableByteCodecBuiltin::StripCSlashes),
        "stripslashes" => Some(StableByteCodecBuiltin::StripSlashes),
        "quotemeta" => Some(StableByteCodecBuiltin::QuoteMeta),
        "pack" => Some(StableByteCodecBuiltin::Pack),
        "unpack" => Some(StableByteCodecBuiltin::Unpack),
        "sodium_bin2base64" => Some(StableByteCodecBuiltin::SodiumBin2Base64),
        _ => None,
    }
}

/// Exact native searches and comparisons over authoritative byte strings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableStringSearchCompareBuiltin {
    StrStr,
    StrIStr,
    StrRChr,
    StrPBrk,
    SubstrCompare,
    StrNatCmp,
    StrNatCaseCmp,
}

impl StableStringSearchCompareBuiltin {
    pub(super) const COUNT: usize = 7;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::StrStr => 0,
            Self::StrIStr => 1,
            Self::StrRChr => 2,
            Self::StrPBrk => 3,
            Self::SubstrCompare => 4,
            Self::StrNatCmp => 5,
            Self::StrNatCaseCmp => 6,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::StrStr => "phrust_native_strstr",
            Self::StrIStr => "phrust_native_stristr",
            Self::StrRChr => "phrust_native_strrchr",
            Self::StrPBrk => "phrust_native_strpbrk",
            Self::SubstrCompare => "phrust_native_substr_compare",
            Self::StrNatCmp => "phrust_native_strnatcmp",
            Self::StrNatCaseCmp => "phrust_native_strnatcasecmp",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::StrStr | Self::StrIStr | Self::StrRChr => arity == 2 || arity == 3,
            Self::StrPBrk | Self::StrNatCmp | Self::StrNatCaseCmp => arity == 2,
            Self::SubstrCompare => arity >= 3 && arity <= 5,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::StrStr,
            Self::StrIStr,
            Self::StrRChr,
            Self::StrPBrk,
            Self::SubstrCompare,
            Self::StrNatCmp,
            Self::StrNatCaseCmp,
        ]
    }
}

pub(super) fn stable_builtin_string_search_compare(
    target: &RegionCallTarget,
) -> Option<StableStringSearchCompareBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "strstr" => Some(StableStringSearchCompareBuiltin::StrStr),
        "stristr" => Some(StableStringSearchCompareBuiltin::StrIStr),
        "strrchr" => Some(StableStringSearchCompareBuiltin::StrRChr),
        "strpbrk" => Some(StableStringSearchCompareBuiltin::StrPBrk),
        "substr_compare" => Some(StableStringSearchCompareBuiltin::SubstrCompare),
        "strnatcmp" => Some(StableStringSearchCompareBuiltin::StrNatCmp),
        "strnatcasecmp" => Some(StableStringSearchCompareBuiltin::StrNatCaseCmp),
        _ => None,
    }
}

/// Exact native byte-rewrite handlers selected at compilation time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableStringRewriteBuiltin {
    UcWords,
    StrPad,
    StrTr,
    StripTags,
    SubstrReplace,
    StrSplit,
    VersionCompare,
}

impl StableStringRewriteBuiltin {
    pub(super) const COUNT: usize = 7;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::UcWords => 0,
            Self::StrPad => 1,
            Self::StrTr => 2,
            Self::StripTags => 3,
            Self::SubstrReplace => 4,
            Self::StrSplit => 5,
            Self::VersionCompare => 6,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::UcWords => "phrust_native_ucwords",
            Self::StrPad => "phrust_native_str_pad",
            Self::StrTr => "phrust_native_strtr",
            Self::StripTags => "phrust_native_strip_tags",
            Self::SubstrReplace => "phrust_native_substr_replace",
            Self::StrSplit => "phrust_native_str_split",
            Self::VersionCompare => "phrust_native_version_compare",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::UcWords | Self::StripTags => arity == 1 || arity == 2,
            Self::StrPad => arity >= 2 && arity <= 4,
            Self::SubstrReplace => arity == 3 || arity == 4,
            Self::StrTr => arity == 2 || arity == 3,
            Self::StrSplit => arity == 1 || arity == 2,
            Self::VersionCompare => arity == 2 || arity == 3,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::UcWords,
            Self::StrPad,
            Self::StrTr,
            Self::StripTags,
            Self::SubstrReplace,
            Self::StrSplit,
            Self::VersionCompare,
        ]
    }
}

pub(super) fn stable_builtin_string_rewrite(
    target: &RegionCallTarget,
) -> Option<StableStringRewriteBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "ucwords" => Some(StableStringRewriteBuiltin::UcWords),
        "str_pad" => Some(StableStringRewriteBuiltin::StrPad),
        "strtr" => Some(StableStringRewriteBuiltin::StrTr),
        "strip_tags" => Some(StableStringRewriteBuiltin::StripTags),
        "substr_replace" => Some(StableStringRewriteBuiltin::SubstrReplace),
        "str_split" => Some(StableStringRewriteBuiltin::StrSplit),
        "version_compare" => Some(StableStringRewriteBuiltin::VersionCompare),
        _ => None,
    }
}

/// Exact stateless HTML entity codecs over authoritative native bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableHtmlCodecBuiltin {
    SpecialChars,
    Entities,
    EntityDecode,
    SpecialCharsDecode,
    TranslationTable,
}

impl StableHtmlCodecBuiltin {
    pub(super) const COUNT: usize = 5;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::SpecialChars => 0,
            Self::Entities => 1,
            Self::EntityDecode => 2,
            Self::SpecialCharsDecode => 3,
            Self::TranslationTable => 4,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::SpecialChars => "phrust_native_htmlspecialchars",
            Self::Entities => "phrust_native_htmlentities",
            Self::EntityDecode => "phrust_native_html_entity_decode",
            Self::SpecialCharsDecode => "phrust_native_htmlspecialchars_decode",
            Self::TranslationTable => "phrust_native_get_html_translation_table",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::SpecialChars | Self::Entities => arity >= 1 && arity <= 4,
            Self::EntityDecode => arity >= 1 && arity <= 3,
            Self::SpecialCharsDecode => arity == 1 || arity == 2,
            Self::TranslationTable => arity <= 3,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::SpecialChars,
            Self::Entities,
            Self::EntityDecode,
            Self::SpecialCharsDecode,
            Self::TranslationTable,
        ]
    }
}

pub(super) fn stable_builtin_html_codec(
    target: &RegionCallTarget,
) -> Option<StableHtmlCodecBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "htmlspecialchars" => Some(StableHtmlCodecBuiltin::SpecialChars),
        "htmlentities" => Some(StableHtmlCodecBuiltin::Entities),
        "html_entity_decode" => Some(StableHtmlCodecBuiltin::EntityDecode),
        "htmlspecialchars_decode" => Some(StableHtmlCodecBuiltin::SpecialCharsDecode),
        "get_html_translation_table" => Some(StableHtmlCodecBuiltin::TranslationTable),
        _ => None,
    }
}

/// Exact URL/query transforms over authoritative native strings and arrays.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableUrlQueryBuiltin {
    ParseUrl,
    ParseStr,
    HttpBuildQuery,
}

impl StableUrlQueryBuiltin {
    pub(super) const COUNT: usize = 3;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::ParseUrl => 0,
            Self::ParseStr => 1,
            Self::HttpBuildQuery => 2,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::ParseUrl => "phrust_native_parse_url",
            Self::ParseStr => "phrust_native_parse_str",
            Self::HttpBuildQuery => "phrust_native_http_build_query",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::ParseUrl => arity == 1 || arity == 2,
            Self::ParseStr => arity == 2,
            Self::HttpBuildQuery => arity >= 1 && arity <= 4,
        }
    }

    pub(super) const fn argument_is_by_reference(self, index: usize) -> bool {
        matches!(self, Self::ParseStr) && index == 1
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::ParseUrl, Self::ParseStr, Self::HttpBuildQuery]
    }
}

pub(super) fn stable_builtin_url_query(target: &RegionCallTarget) -> Option<StableUrlQueryBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "parse_url" => Some(StableUrlQueryBuiltin::ParseUrl),
        "parse_str" => Some(StableUrlQueryBuiltin::ParseStr),
        "http_build_query" => Some(StableUrlQueryBuiltin::HttpBuildQuery),
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
    IsFile,
    IsDir,
    IsReadable,
    IsWritable,
    Filesize,
    Filemtime,
    FileGetContents,
    FilePutContents,
    Rename,
    Unlink,
    Mkdir,
    Rmdir,
    Touch,
    Fopen,
    Fwrite,
    Fclose,
    Fread,
    Fgets,
    Fgetc,
    Feof,
    Fflush,
    Fseek,
    Ftell,
    Ftruncate,
    Rewind,
    StreamGetContents,
    StreamCopyToStream,
    IsLink,
    FilePerms,
    FileOwner,
    FileGroup,
    FileType,
    DiskFreeSpace,
    DiskTotalSpace,
    Pathinfo,
    Stat,
    Lstat,
    File,
    Glob,
    OpenDir,
    ReadDir,
    RewindDir,
    CloseDir,
    ScanDir,
    StreamGetMetaData,
    StreamGetWrappers,
    StreamIsLocal,
    StreamResolveIncludePath,
    StreamContextCreate,
    StreamContextGetDefault,
    StreamContextGetOptions,
    StreamContextSetDefault,
    StreamContextSetOption,
    StreamContextSetOptions,
    StreamFilterAppend,
    StreamFilterPrepend,
    StreamFilterRemove,
    StreamIsAtty,
    StreamSetTimeout,
    Chmod,
    Symlink,
    Readfile,
    IsUploadedFile,
    MoveUploadedFile,
    Tempnam,
    Tmpfile,
}

impl StablePathBuiltin {
    pub(super) const COUNT: usize = 70;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Basename => 0,
            Self::Dirname => 1,
            Self::Realpath => 2,
            Self::FileExists => 3,
            Self::IsFile => 4,
            Self::IsDir => 5,
            Self::IsReadable => 6,
            Self::IsWritable => 7,
            Self::Filesize => 8,
            Self::Filemtime => 9,
            Self::FileGetContents => 10,
            Self::FilePutContents => 11,
            Self::Rename => 12,
            Self::Unlink => 13,
            Self::Mkdir => 14,
            Self::Rmdir => 15,
            Self::Touch => 16,
            Self::Fopen => 17,
            Self::Fwrite => 18,
            Self::Fclose => 19,
            Self::Fread => 20,
            Self::Fgets => 21,
            Self::Fgetc => 22,
            Self::Feof => 23,
            Self::Fflush => 24,
            Self::Fseek => 25,
            Self::Ftell => 26,
            Self::Ftruncate => 27,
            Self::Rewind => 28,
            Self::StreamGetContents => 29,
            Self::StreamCopyToStream => 30,
            Self::IsLink => 31,
            Self::FilePerms => 32,
            Self::FileOwner => 33,
            Self::FileGroup => 34,
            Self::FileType => 35,
            Self::DiskFreeSpace => 36,
            Self::DiskTotalSpace => 37,
            Self::Pathinfo => 38,
            Self::Stat => 39,
            Self::Lstat => 40,
            Self::File => 41,
            Self::Glob => 42,
            Self::OpenDir => 43,
            Self::ReadDir => 44,
            Self::RewindDir => 45,
            Self::CloseDir => 46,
            Self::ScanDir => 47,
            Self::StreamGetMetaData => 48,
            Self::StreamGetWrappers => 49,
            Self::StreamIsLocal => 50,
            Self::StreamResolveIncludePath => 51,
            Self::StreamContextCreate => 52,
            Self::StreamContextGetDefault => 53,
            Self::StreamContextGetOptions => 54,
            Self::StreamContextSetDefault => 55,
            Self::StreamContextSetOption => 56,
            Self::StreamContextSetOptions => 57,
            Self::StreamFilterAppend => 58,
            Self::StreamFilterPrepend => 59,
            Self::StreamFilterRemove => 60,
            Self::StreamIsAtty => 61,
            Self::StreamSetTimeout => 62,
            Self::Chmod => 63,
            Self::Symlink => 64,
            Self::Readfile => 65,
            Self::IsUploadedFile => 66,
            Self::MoveUploadedFile => 67,
            Self::Tempnam => 68,
            Self::Tmpfile => 69,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Basename => "phrust_native_basename",
            Self::Dirname => "phrust_native_dirname",
            Self::Realpath => "phrust_native_realpath",
            Self::FileExists => "phrust_native_file_exists",
            Self::IsFile => "phrust_native_is_file",
            Self::IsDir => "phrust_native_is_dir",
            Self::IsReadable => "phrust_native_is_readable",
            Self::IsWritable => "phrust_native_is_writable",
            Self::Filesize => "phrust_native_filesize",
            Self::Filemtime => "phrust_native_filemtime",
            Self::FileGetContents => "phrust_native_file_get_contents",
            Self::FilePutContents => "phrust_native_file_put_contents",
            Self::Rename => "phrust_native_rename",
            Self::Unlink => "phrust_native_unlink",
            Self::Mkdir => "phrust_native_mkdir",
            Self::Rmdir => "phrust_native_rmdir",
            Self::Touch => "phrust_native_touch",
            Self::Fopen => "phrust_native_fopen",
            Self::Fwrite => "phrust_native_fwrite",
            Self::Fclose => "phrust_native_fclose",
            Self::Fread => "phrust_native_fread",
            Self::Fgets => "phrust_native_fgets",
            Self::Fgetc => "phrust_native_fgetc",
            Self::Feof => "phrust_native_feof",
            Self::Fflush => "phrust_native_fflush",
            Self::Fseek => "phrust_native_fseek",
            Self::Ftell => "phrust_native_ftell",
            Self::Ftruncate => "phrust_native_ftruncate",
            Self::Rewind => "phrust_native_rewind",
            Self::StreamGetContents => "phrust_native_stream_get_contents",
            Self::StreamCopyToStream => "phrust_native_stream_copy_to_stream",
            Self::IsLink => "phrust_native_is_link",
            Self::FilePerms => "phrust_native_fileperms",
            Self::FileOwner => "phrust_native_fileowner",
            Self::FileGroup => "phrust_native_filegroup",
            Self::FileType => "phrust_native_filetype",
            Self::DiskFreeSpace => "phrust_native_disk_free_space",
            Self::DiskTotalSpace => "phrust_native_disk_total_space",
            Self::Pathinfo => "phrust_native_pathinfo",
            Self::Stat => "phrust_native_stat",
            Self::Lstat => "phrust_native_lstat",
            Self::File => "phrust_native_file",
            Self::Glob => "phrust_native_glob",
            Self::OpenDir => "phrust_native_opendir",
            Self::ReadDir => "phrust_native_readdir",
            Self::RewindDir => "phrust_native_rewinddir",
            Self::CloseDir => "phrust_native_closedir",
            Self::ScanDir => "phrust_native_scandir",
            Self::StreamGetMetaData => "phrust_native_stream_get_meta_data",
            Self::StreamGetWrappers => "phrust_native_stream_get_wrappers",
            Self::StreamIsLocal => "phrust_native_stream_is_local",
            Self::StreamResolveIncludePath => "phrust_native_stream_resolve_include_path",
            Self::StreamContextCreate => "phrust_native_stream_context_create",
            Self::StreamContextGetDefault => "phrust_native_stream_context_get_default",
            Self::StreamContextGetOptions => "phrust_native_stream_context_get_options",
            Self::StreamContextSetDefault => "phrust_native_stream_context_set_default",
            Self::StreamContextSetOption => "phrust_native_stream_context_set_option",
            Self::StreamContextSetOptions => "phrust_native_stream_context_set_options",
            Self::StreamFilterAppend => "phrust_native_stream_filter_append",
            Self::StreamFilterPrepend => "phrust_native_stream_filter_prepend",
            Self::StreamFilterRemove => "phrust_native_stream_filter_remove",
            Self::StreamIsAtty => "phrust_native_stream_isatty",
            Self::StreamSetTimeout => "phrust_native_stream_set_timeout",
            Self::Chmod => "phrust_native_chmod",
            Self::Symlink => "phrust_native_symlink",
            Self::Readfile => "phrust_native_readfile",
            Self::IsUploadedFile => "phrust_native_is_uploaded_file",
            Self::MoveUploadedFile => "phrust_native_move_uploaded_file",
            Self::Tempnam => "phrust_native_tempnam",
            Self::Tmpfile => "phrust_native_tmpfile",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Basename | Self::Dirname => arity == 1 || arity == 2,
            Self::Realpath
            | Self::FileExists
            | Self::IsFile
            | Self::IsDir
            | Self::IsReadable
            | Self::IsWritable
            | Self::IsLink
            | Self::FilePerms
            | Self::FileOwner
            | Self::FileGroup
            | Self::FileType
            | Self::DiskFreeSpace
            | Self::DiskTotalSpace
            | Self::Stat
            | Self::Lstat
            | Self::Filesize
            | Self::Filemtime
            | Self::OpenDir
            | Self::ReadDir
            | Self::RewindDir
            | Self::CloseDir
            | Self::StreamGetMetaData
            | Self::StreamIsLocal
            | Self::StreamResolveIncludePath
            | Self::StreamContextGetOptions
            | Self::StreamContextSetDefault
            | Self::StreamFilterRemove
            | Self::StreamIsAtty
            | Self::Readfile
            | Self::IsUploadedFile => arity == 1,
            Self::StreamGetWrappers | Self::Tmpfile => arity == 0,
            Self::StreamContextCreate | Self::StreamContextGetDefault => arity <= 1,
            Self::StreamContextSetOption => arity == 2 || arity == 4,
            Self::StreamContextSetOptions => arity == 2,
            Self::StreamFilterAppend | Self::StreamFilterPrepend => arity >= 2 && arity <= 4,
            Self::StreamSetTimeout => arity == 2 || arity == 3,
            Self::Chmod | Self::Symlink | Self::MoveUploadedFile | Self::Tempnam => arity == 2,
            Self::Pathinfo => arity == 1 || arity == 2,
            Self::File => arity >= 1 && arity <= 3,
            Self::Glob => arity == 1 || arity == 2,
            Self::ScanDir => arity == 1 || arity == 2,
            Self::FileGetContents => arity >= 1 && arity <= 5,
            Self::FilePutContents => arity >= 2 && arity <= 4,
            Self::Rename => arity == 2,
            Self::Mkdir => arity >= 1 && arity <= 4,
            Self::Unlink | Self::Rmdir | Self::Touch => arity == 1,
            // Optional fopen include-path/context shapes retain their one
            // baseline continuation until those capabilities are published.
            Self::Fopen => arity == 2,
            Self::Fwrite => arity == 2 || arity == 3,
            Self::Fclose | Self::Fgetc | Self::Feof | Self::Fflush | Self::Ftell | Self::Rewind => {
                arity == 1
            }
            Self::Fread | Self::Ftruncate => arity == 2,
            Self::Fgets => arity == 1 || arity == 2,
            Self::Fseek => arity == 2 || arity == 3,
            Self::StreamGetContents => arity >= 1 && arity <= 3,
            Self::StreamCopyToStream => arity >= 2 && arity <= 4,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Basename,
            Self::Dirname,
            Self::Realpath,
            Self::FileExists,
            Self::IsFile,
            Self::IsDir,
            Self::IsReadable,
            Self::IsWritable,
            Self::Filesize,
            Self::Filemtime,
            Self::FileGetContents,
            Self::FilePutContents,
            Self::Rename,
            Self::Unlink,
            Self::Mkdir,
            Self::Rmdir,
            Self::Touch,
            Self::Fopen,
            Self::Fwrite,
            Self::Fclose,
            Self::Fread,
            Self::Fgets,
            Self::Fgetc,
            Self::Feof,
            Self::Fflush,
            Self::Fseek,
            Self::Ftell,
            Self::Ftruncate,
            Self::Rewind,
            Self::StreamGetContents,
            Self::StreamCopyToStream,
            Self::IsLink,
            Self::FilePerms,
            Self::FileOwner,
            Self::FileGroup,
            Self::FileType,
            Self::DiskFreeSpace,
            Self::DiskTotalSpace,
            Self::Pathinfo,
            Self::Stat,
            Self::Lstat,
            Self::File,
            Self::Glob,
            Self::OpenDir,
            Self::ReadDir,
            Self::RewindDir,
            Self::CloseDir,
            Self::ScanDir,
            Self::StreamGetMetaData,
            Self::StreamGetWrappers,
            Self::StreamIsLocal,
            Self::StreamResolveIncludePath,
            Self::StreamContextCreate,
            Self::StreamContextGetDefault,
            Self::StreamContextGetOptions,
            Self::StreamContextSetDefault,
            Self::StreamContextSetOption,
            Self::StreamContextSetOptions,
            Self::StreamFilterAppend,
            Self::StreamFilterPrepend,
            Self::StreamFilterRemove,
            Self::StreamIsAtty,
            Self::StreamSetTimeout,
            Self::Chmod,
            Self::Symlink,
            Self::Readfile,
            Self::IsUploadedFile,
            Self::MoveUploadedFile,
            Self::Tempnam,
            Self::Tmpfile,
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
        "is_file" => Some(StablePathBuiltin::IsFile),
        "is_dir" => Some(StablePathBuiltin::IsDir),
        "is_readable" => Some(StablePathBuiltin::IsReadable),
        "is_writable" => Some(StablePathBuiltin::IsWritable),
        "is_link" => Some(StablePathBuiltin::IsLink),
        "fileperms" => Some(StablePathBuiltin::FilePerms),
        "fileowner" => Some(StablePathBuiltin::FileOwner),
        "filegroup" => Some(StablePathBuiltin::FileGroup),
        "filetype" => Some(StablePathBuiltin::FileType),
        "disk_free_space" => Some(StablePathBuiltin::DiskFreeSpace),
        "disk_total_space" => Some(StablePathBuiltin::DiskTotalSpace),
        "pathinfo" => Some(StablePathBuiltin::Pathinfo),
        "stat" => Some(StablePathBuiltin::Stat),
        "lstat" => Some(StablePathBuiltin::Lstat),
        "file" => Some(StablePathBuiltin::File),
        "glob" => Some(StablePathBuiltin::Glob),
        "opendir" => Some(StablePathBuiltin::OpenDir),
        "readdir" => Some(StablePathBuiltin::ReadDir),
        "rewinddir" => Some(StablePathBuiltin::RewindDir),
        "closedir" => Some(StablePathBuiltin::CloseDir),
        "scandir" => Some(StablePathBuiltin::ScanDir),
        "stream_get_meta_data" => Some(StablePathBuiltin::StreamGetMetaData),
        "stream_get_wrappers" => Some(StablePathBuiltin::StreamGetWrappers),
        "stream_is_local" => Some(StablePathBuiltin::StreamIsLocal),
        "stream_resolve_include_path" => Some(StablePathBuiltin::StreamResolveIncludePath),
        "stream_context_create" => Some(StablePathBuiltin::StreamContextCreate),
        "stream_context_get_default" => Some(StablePathBuiltin::StreamContextGetDefault),
        "stream_context_get_options" => Some(StablePathBuiltin::StreamContextGetOptions),
        "stream_context_set_default" => Some(StablePathBuiltin::StreamContextSetDefault),
        "stream_context_set_option" => Some(StablePathBuiltin::StreamContextSetOption),
        "stream_context_set_options" => Some(StablePathBuiltin::StreamContextSetOptions),
        "stream_filter_append" => Some(StablePathBuiltin::StreamFilterAppend),
        "stream_filter_prepend" => Some(StablePathBuiltin::StreamFilterPrepend),
        "stream_filter_remove" => Some(StablePathBuiltin::StreamFilterRemove),
        "stream_isatty" => Some(StablePathBuiltin::StreamIsAtty),
        "stream_set_timeout" => Some(StablePathBuiltin::StreamSetTimeout),
        "chmod" => Some(StablePathBuiltin::Chmod),
        "symlink" => Some(StablePathBuiltin::Symlink),
        "readfile" => Some(StablePathBuiltin::Readfile),
        "is_uploaded_file" => Some(StablePathBuiltin::IsUploadedFile),
        "move_uploaded_file" => Some(StablePathBuiltin::MoveUploadedFile),
        "tempnam" => Some(StablePathBuiltin::Tempnam),
        "tmpfile" => Some(StablePathBuiltin::Tmpfile),
        "filesize" => Some(StablePathBuiltin::Filesize),
        "filemtime" => Some(StablePathBuiltin::Filemtime),
        "file_get_contents" => Some(StablePathBuiltin::FileGetContents),
        "file_put_contents" => Some(StablePathBuiltin::FilePutContents),
        "rename" => Some(StablePathBuiltin::Rename),
        "unlink" => Some(StablePathBuiltin::Unlink),
        "mkdir" => Some(StablePathBuiltin::Mkdir),
        "rmdir" => Some(StablePathBuiltin::Rmdir),
        "touch" => Some(StablePathBuiltin::Touch),
        "fopen" => Some(StablePathBuiltin::Fopen),
        "fwrite" => Some(StablePathBuiltin::Fwrite),
        "fclose" => Some(StablePathBuiltin::Fclose),
        "fread" => Some(StablePathBuiltin::Fread),
        "fgets" => Some(StablePathBuiltin::Fgets),
        "fgetc" => Some(StablePathBuiltin::Fgetc),
        "feof" => Some(StablePathBuiltin::Feof),
        "fflush" => Some(StablePathBuiltin::Fflush),
        "fseek" => Some(StablePathBuiltin::Fseek),
        "ftell" => Some(StablePathBuiltin::Ftell),
        "ftruncate" => Some(StablePathBuiltin::Ftruncate),
        "rewind" => Some(StablePathBuiltin::Rewind),
        "stream_get_contents" => Some(StablePathBuiltin::StreamGetContents),
        "stream_copy_to_stream" => Some(StablePathBuiltin::StreamCopyToStream),
        _ => None,
    }
}

/// Exact request-local output-buffer operations selected at compile time.
///
/// The default output-buffer operations consume only the authoritative
/// request output stack and native string plane. `ob_start()` callback,
/// chunk-size, and flags shapes retain their one baseline continuation until
/// a native output-handler stack is published.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableOutputBufferBuiltin {
    Start,
    GetClean,
    GetContents,
    GetFlush,
    GetLength,
    GetLevel,
    EndFlush,
    EndClean,
    PhpInfo,
}

impl StableOutputBufferBuiltin {
    pub(super) const COUNT: usize = 9;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Start => 0,
            Self::GetClean => 1,
            Self::GetContents => 2,
            Self::GetFlush => 3,
            Self::GetLength => 4,
            Self::GetLevel => 5,
            Self::EndFlush => 6,
            Self::EndClean => 7,
            Self::PhpInfo => 8,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Start => "phrust_native_ob_start",
            Self::GetClean => "phrust_native_ob_get_clean",
            Self::GetContents => "phrust_native_ob_get_contents",
            Self::GetFlush => "phrust_native_ob_get_flush",
            Self::GetLength => "phrust_native_ob_get_length",
            Self::GetLevel => "phrust_native_ob_get_level",
            Self::EndFlush => "phrust_native_ob_end_flush",
            Self::EndClean => "phrust_native_ob_end_clean",
            Self::PhpInfo => "phrust_native_phpinfo",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        // Optional ob_start arguments select output-handler semantics that are
        // deliberately baseline-only until their state has a native record.
        match self {
            Self::PhpInfo => arity <= 1,
            _ => arity == 0,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Start,
            Self::GetClean,
            Self::GetContents,
            Self::GetFlush,
            Self::GetLength,
            Self::GetLevel,
            Self::EndFlush,
            Self::EndClean,
            Self::PhpInfo,
        ]
    }
}

pub(super) fn stable_builtin_output_buffer(
    target: &RegionCallTarget,
) -> Option<StableOutputBufferBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "ob_start" => Some(StableOutputBufferBuiltin::Start),
        "ob_get_clean" => Some(StableOutputBufferBuiltin::GetClean),
        "ob_get_contents" => Some(StableOutputBufferBuiltin::GetContents),
        "ob_get_flush" => Some(StableOutputBufferBuiltin::GetFlush),
        "ob_get_length" => Some(StableOutputBufferBuiltin::GetLength),
        "ob_get_level" => Some(StableOutputBufferBuiltin::GetLevel),
        "ob_end_flush" => Some(StableOutputBufferBuiltin::EndFlush),
        "ob_end_clean" => Some(StableOutputBufferBuiltin::EndClean),
        "phpinfo" => Some(StableOutputBufferBuiltin::PhpInfo),
        _ => None,
    }
}

pub(super) fn stable_builtin_var_dump(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("var_dump")
}

pub(super) fn stable_builtin_var_export(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("var_export")
}

pub(super) fn stable_builtin_mysqli_set_charset(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("mysqli_set_charset")
}

pub(super) fn stable_builtin_mysqli_query(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("mysqli_query")
}

pub(super) fn stable_builtin_mysqli_fetch_array(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("mysqli_fetch_array")
}

pub(super) fn stable_builtin_mysqli_fetch_object(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("mysqli_fetch_object")
}

pub(super) fn stable_builtin_mysqli_character_set_name(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("mysqli_character_set_name")
}

pub(super) fn stable_builtin_mysqli_fetch_field(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("mysqli_fetch_field")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableMysqliResultCountBuiltin {
    NumFields,
    NumRows,
}

impl StableMysqliResultCountBuiltin {
    pub(super) const COUNT: usize = 2;

    pub(super) const fn index(self) -> usize {
        self as usize
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::NumFields => "phrust_native_mysqli_num_fields",
            Self::NumRows => "phrust_native_mysqli_num_rows",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::NumFields, Self::NumRows]
    }
}

pub(super) fn stable_builtin_mysqli_result_count(
    target: &RegionCallTarget,
) -> Option<StableMysqliResultCountBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "mysqli_num_fields" => Some(StableMysqliResultCountBuiltin::NumFields),
        "mysqli_num_rows" => Some(StableMysqliResultCountBuiltin::NumRows),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableMysqliConnectStatusBuiltin {
    Errno,
    Error,
}

impl StableMysqliConnectStatusBuiltin {
    pub(super) const COUNT: usize = 2;

    pub(super) const fn index(self) -> usize {
        self as usize
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Errno => "phrust_native_mysqli_connect_errno",
            Self::Error => "phrust_native_mysqli_connect_error",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::Errno, Self::Error]
    }
}

pub(super) fn stable_builtin_mysqli_connect_status(
    target: &RegionCallTarget,
) -> Option<StableMysqliConnectStatusBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "mysqli_connect_errno" => Some(StableMysqliConnectStatusBuiltin::Errno),
        "mysqli_connect_error" => Some(StableMysqliConnectStatusBuiltin::Error),
        _ => None,
    }
}

pub(super) fn stable_builtin_mysqli_select_db(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("mysqli_select_db")
}

pub(super) fn stable_builtin_mysqli_real_escape_string(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("mysqli_real_escape_string")
}

pub(super) fn stable_builtin_mysqli_free_result(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("mysqli_free_result")
}

pub(super) fn stable_builtin_mysqli_more_results(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("mysqli_more_results")
}

pub(super) fn stable_builtin_mysqli_next_result(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("mysqli_next_result")
}

pub(super) fn stable_builtin_mysqli_report(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("mysqli_report")
}

pub(super) fn stable_builtin_mysqli_init(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("mysqli_init")
}

pub(super) fn stable_builtin_mysqli_options(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("mysqli_options")
}

pub(super) fn stable_builtin_mysqli_real_connect(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("mysqli_real_connect")
}

pub(super) fn stable_builtin_mysqli_close(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("mysqli_close")
}

pub(super) fn stable_builtin_mysqli_get_server_info(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("mysqli_get_server_info")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableMysqliConnectionStatusBuiltin {
    Error,
    Errno,
    AffectedRows,
    InsertId,
    FieldCount,
}

impl StableMysqliConnectionStatusBuiltin {
    pub(super) const COUNT: usize = 5;

    pub(super) const fn index(self) -> usize {
        self as usize
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Error => "phrust_native_mysqli_error",
            Self::Errno => "phrust_native_mysqli_errno",
            Self::AffectedRows => "phrust_native_mysqli_affected_rows",
            Self::InsertId => "phrust_native_mysqli_insert_id",
            Self::FieldCount => "phrust_native_mysqli_field_count",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Error,
            Self::Errno,
            Self::AffectedRows,
            Self::InsertId,
            Self::FieldCount,
        ]
    }
}

pub(super) fn stable_builtin_mysqli_connection_status(
    target: &RegionCallTarget,
) -> Option<StableMysqliConnectionStatusBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "mysqli_error" => Some(StableMysqliConnectionStatusBuiltin::Error),
        "mysqli_errno" => Some(StableMysqliConnectionStatusBuiltin::Errno),
        "mysqli_affected_rows" => Some(StableMysqliConnectionStatusBuiltin::AffectedRows),
        "mysqli_insert_id" => Some(StableMysqliConnectionStatusBuiltin::InsertId),
        "mysqli_field_count" => Some(StableMysqliConnectionStatusBuiltin::FieldCount),
        _ => None,
    }
}

pub(super) fn stable_builtin_print(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("print")
}

pub(super) fn stable_builtin_print_r(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("print_r")
}

/// Final zero-argument accessors implemented by PHP's internal Throwable
/// hierarchy.  Every variant maps to one fixed native symbol; the receiver
/// check remains inside that exact leaf so an unrelated user method with the
/// same spelling continues through generated callable resolution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableThrowableMethod {
    Message,
    Code,
    File,
    Line,
    Previous,
    Trace,
}

impl StableThrowableMethod {
    pub(super) const COUNT: usize = 6;

    pub(super) const fn index(self) -> usize {
        self as usize
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Message => "phrust_native_throwable_get_message",
            Self::Code => "phrust_native_throwable_get_code",
            Self::File => "phrust_native_throwable_get_file",
            Self::Line => "phrust_native_throwable_get_line",
            Self::Previous => "phrust_native_throwable_get_previous",
            Self::Trace => "phrust_native_throwable_get_trace",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Message,
            Self::Code,
            Self::File,
            Self::Line,
            Self::Previous,
            Self::Trace,
        ]
    }
}

pub(super) fn stable_throwable_method(target: &RegionCallTarget) -> Option<StableThrowableMethod> {
    let RegionCallTarget::Method { method, .. } = target else {
        return None;
    };
    match method.to_ascii_lowercase().as_str() {
        "getmessage" => Some(StableThrowableMethod::Message),
        "getcode" => Some(StableThrowableMethod::Code),
        "getfile" => Some(StableThrowableMethod::File),
        "getline" => Some(StableThrowableMethod::Line),
        "getprevious" => Some(StableThrowableMethod::Previous),
        "gettrace" => Some(StableThrowableMethod::Trace),
        _ => None,
    }
}

pub(super) fn stable_internal_throwable_constructor(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Constructor { class_name, .. } = target else {
        return false;
    };
    matches!(
        class_name
            .trim_start_matches('\\')
            .to_ascii_lowercase()
            .as_str(),
        "exception"
            | "logicexception"
            | "badfunctioncallexception"
            | "badmethodcallexception"
            | "domainexception"
            | "invalidargumentexception"
            | "lengthexception"
            | "outofrangeexception"
            | "runtimeexception"
            | "outofboundsexception"
            | "overflowexception"
            | "rangeexception"
            | "underflowexception"
            | "unexpectedvalueexception"
            | "error"
            | "compileerror"
            | "parseerror"
            | "typeerror"
            | "argumentcounterror"
            | "valueerror"
            | "arithmeticerror"
            | "divisionbyzeroerror"
            | "unhandledmatcherror"
            | "fibererror"
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableLengthBuiltin {
    String,
}

pub(super) fn stable_builtin_length(target: &RegionCallTarget) -> Option<StableLengthBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
    if normalized.contains('\\') {
        return None;
    }
    match normalized.as_str() {
        "strlen" => Some(StableLengthBuiltin::String),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableStringPredicateBuiltin {
    Contains,
    StartsWith,
    EndsWith,
}

pub(super) fn stable_builtin_string_predicate(
    target: &RegionCallTarget,
) -> Option<StableStringPredicateBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "str_contains" => Some(StableStringPredicateBuiltin::Contains),
        "str_starts_with" => Some(StableStringPredicateBuiltin::StartsWith),
        "str_ends_with" => Some(StableStringPredicateBuiltin::EndsWith),
        _ => None,
    }
}

/// ASCII-only case conversion builtins whose PHP 8 semantics can be emitted
/// directly over the request-owned native string arena.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableAsciiCaseBuiltin {
    Lower,
    Upper,
}

pub(super) fn stable_builtin_ascii_case(
    target: &RegionCallTarget,
) -> Option<StableAsciiCaseBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "strtolower" => Some(StableAsciiCaseBuiltin::Lower),
        "strtoupper" => Some(StableAsciiCaseBuiltin::Upper),
        _ => None,
    }
}

/// Byte-preserving transforms over one native string.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableStringTransformBuiltin {
    Reverse,
    LowercaseFirst,
    UppercaseFirst,
}

pub(super) fn stable_builtin_string_transform(
    target: &RegionCallTarget,
) -> Option<StableStringTransformBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "strrev" => Some(StableStringTransformBuiltin::Reverse),
        "lcfirst" => Some(StableStringTransformBuiltin::LowercaseFirst),
        "ucfirst" => Some(StableStringTransformBuiltin::UppercaseFirst),
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

/// Native byte comparisons with fixed compile-time identities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableStringCompareBuiltin {
    Binary,
    AsciiCaseInsensitive,
    BinaryBounded,
    AsciiCaseInsensitiveBounded,
}

impl StableStringCompareBuiltin {
    pub(super) const fn case_insensitive(self) -> bool {
        matches!(
            self,
            Self::AsciiCaseInsensitive | Self::AsciiCaseInsensitiveBounded
        )
    }

    pub(super) const fn bounded(self) -> bool {
        matches!(
            self,
            Self::BinaryBounded | Self::AsciiCaseInsensitiveBounded
        )
    }
}

pub(super) fn stable_builtin_string_compare(
    target: &RegionCallTarget,
) -> Option<StableStringCompareBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "strcmp" => Some(StableStringCompareBuiltin::Binary),
        "strcasecmp" => Some(StableStringCompareBuiltin::AsciiCaseInsensitive),
        "strncmp" => Some(StableStringCompareBuiltin::BinaryBounded),
        "strncasecmp" => Some(StableStringCompareBuiltin::AsciiCaseInsensitiveBounded),
        _ => None,
    }
}

/// Byte-position builtins with exact positional native lowerings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableStringPositionBuiltin {
    Forward,
    ForwardAsciiCaseInsensitive,
    Reverse,
    ReverseAsciiCaseInsensitive,
}

impl StableStringPositionBuiltin {
    pub(super) const fn case_insensitive(self) -> bool {
        matches!(
            self,
            Self::ForwardAsciiCaseInsensitive | Self::ReverseAsciiCaseInsensitive
        )
    }

    pub(super) const fn reverse(self) -> bool {
        matches!(self, Self::Reverse | Self::ReverseAsciiCaseInsensitive)
    }
}

pub(super) fn stable_builtin_string_position(
    target: &RegionCallTarget,
) -> Option<StableStringPositionBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "strpos" => Some(StableStringPositionBuiltin::Forward),
        "stripos" => Some(StableStringPositionBuiltin::ForwardAsciiCaseInsensitive),
        "strrpos" => Some(StableStringPositionBuiltin::Reverse),
        "strripos" => Some(StableStringPositionBuiltin::ReverseAsciiCaseInsensitive),
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

/// Native byte-slice transformations. `substr` has its own argument plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableDefaultTrimBuiltin {
    Both,
    Left,
    Right,
}

impl StableDefaultTrimBuiltin {
    pub(super) const fn trims_left(self) -> bool {
        matches!(self, Self::Both | Self::Left)
    }

    pub(super) const fn trims_right(self) -> bool {
        matches!(self, Self::Both | Self::Right)
    }
}

pub(super) fn stable_builtin_default_trim(
    target: &RegionCallTarget,
) -> Option<StableDefaultTrimBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "trim" => Some(StableDefaultTrimBuiltin::Both),
        "ltrim" => Some(StableDefaultTrimBuiltin::Left),
        "rtrim" => Some(StableDefaultTrimBuiltin::Right),
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
/// array. The identity is resolved at publication/lowering time and is never
/// carried into generated code as a generic operation ID.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableArrayProjectionBuiltin {
    Keys,
    Values,
}

pub(super) fn stable_builtin_array_projection(
    target: &RegionCallTarget,
) -> Option<StableArrayProjectionBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "array_keys" => Some(StableArrayProjectionBuiltin::Keys),
        "array_values" => Some(StableArrayProjectionBuiltin::Values),
        _ => None,
    }
}

/// Case-folds only string keys while preserving integer keys and PHP's
/// collision/overwrite ordering. The case selector remains a normal typed
/// operand; no builtin or operation identifier reaches generated code.
pub(super) fn stable_builtin_array_change_key_case(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("array_change_key_case")
}

/// Scalar aggregates over authoritative native array entries.
///
/// Each aggregate has one fixed compiled target; the generated artifact never
/// carries a generic array-operation identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableArrayAggregateBuiltin {
    Sum,
    Count,
    SizeOf,
}

impl StableArrayAggregateBuiltin {
    pub(super) const COUNT: usize = 3;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Sum => 0,
            Self::Count => 1,
            Self::SizeOf => 2,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Sum => "phrust_native_array_sum",
            Self::Count => "phrust_native_count",
            Self::SizeOf => "phrust_native_sizeof",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Sum => arity == 1,
            Self::Count | Self::SizeOf => matches!(arity, 1 | 2),
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::Sum, Self::Count, Self::SizeOf]
    }
}

pub(super) fn stable_builtin_array_aggregate(
    target: &RegionCallTarget,
) -> Option<StableArrayAggregateBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "array_sum" => Some(StableArrayAggregateBuiltin::Sum),
        "count" => Some(StableArrayAggregateBuiltin::Count),
        "sizeof" => Some(StableArrayAggregateBuiltin::SizeOf),
        _ => None,
    }
}

/// Recursive array overlays over authoritative direct array graphs.
///
/// Optimizing lowering folds an arbitrary argument list through one fixed
/// binary target per PHP operation. The left argument is an owned native
/// accumulator consumed by the handler; the right argument is borrowed.
/// This keeps variadic calls native without a generic operation identifier or
/// a bounded "exact builtin" argument adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableRecursiveArrayBuiltin {
    Merge,
    Replace,
}

impl StableRecursiveArrayBuiltin {
    pub(super) const COUNT: usize = 2;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Merge => 0,
            Self::Replace => 1,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Merge => "phrust_native_array_merge_recursive",
            Self::Replace => "phrust_native_array_replace_recursive",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::Merge, Self::Replace]
    }
}

pub(super) fn stable_builtin_recursive_array(
    target: &RegionCallTarget,
) -> Option<StableRecursiveArrayBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "array_merge_recursive" => Some(StableRecursiveArrayBuiltin::Merge),
        "array_replace_recursive" => Some(StableRecursiveArrayBuiltin::Replace),
        _ => None,
    }
}

/// Direct constructors whose result is an authoritative native array.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableArrayConstructorBuiltin {
    Fill,
    FillKeys,
    Combine,
    Flip,
}

pub(super) fn stable_builtin_array_constructor(
    target: &RegionCallTarget,
) -> Option<StableArrayConstructorBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "array_fill" => Some(StableArrayConstructorBuiltin::Fill),
        "array_fill_keys" => Some(StableArrayConstructorBuiltin::FillKeys),
        "array_combine" => Some(StableArrayConstructorBuiltin::Combine),
        "array_flip" => Some(StableArrayConstructorBuiltin::Flip),
        _ => None,
    }
}

/// Representation-complete array shape operations. Each operation keeps a
/// fixed compile-time identity rather than entering a shared numeric ABI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableArrayShapeBuiltin {
    Range,
    Pad,
    Chunk,
    Column,
    Unique,
}

pub(super) fn stable_builtin_array_shape(
    target: &RegionCallTarget,
) -> Option<StableArrayShapeBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "range" => Some(StableArrayShapeBuiltin::Range),
        "array_pad" => Some(StableArrayShapeBuiltin::Pad),
        "array_chunk" => Some(StableArrayShapeBuiltin::Chunk),
        "array_column" => Some(StableArrayShapeBuiltin::Column),
        "array_unique" => Some(StableArrayShapeBuiltin::Unique),
        _ => None,
    }
}

/// Callback-free array sorts over authoritative direct entries. Each
/// operation has a fixed ABI; comparison mode remains a PHP-visible argument
/// and unsupported modes are rejected before optimizer entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableArraySortBuiltin {
    Asort,
    Arsort,
    Ksort,
    Krsort,
    Natsort,
    Natcasesort,
    Sort,
    Rsort,
}

impl StableArraySortBuiltin {
    pub(super) const COUNT: usize = 8;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Asort => 0,
            Self::Arsort => 1,
            Self::Ksort => 2,
            Self::Krsort => 3,
            Self::Natsort => 4,
            Self::Natcasesort => 5,
            Self::Sort => 6,
            Self::Rsort => 7,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Asort => "phrust_native_asort",
            Self::Arsort => "phrust_native_arsort",
            Self::Ksort => "phrust_native_ksort",
            Self::Krsort => "phrust_native_krsort",
            Self::Natsort => "phrust_native_natsort",
            Self::Natcasesort => "phrust_native_natcasesort",
            Self::Sort => "phrust_native_sort",
            Self::Rsort => "phrust_native_rsort",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Natsort | Self::Natcasesort => arity == 1,
            _ => arity == 1 || arity == 2,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Asort,
            Self::Arsort,
            Self::Ksort,
            Self::Krsort,
            Self::Natsort,
            Self::Natcasesort,
            Self::Sort,
            Self::Rsort,
        ]
    }
}

pub(super) fn stable_builtin_array_sort(
    target: &RegionCallTarget,
) -> Option<StableArraySortBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "asort" => Some(StableArraySortBuiltin::Asort),
        "arsort" => Some(StableArraySortBuiltin::Arsort),
        "ksort" => Some(StableArraySortBuiltin::Ksort),
        "krsort" => Some(StableArraySortBuiltin::Krsort),
        "natsort" => Some(StableArraySortBuiltin::Natsort),
        "natcasesort" => Some(StableArraySortBuiltin::Natcasesort),
        "sort" => Some(StableArraySortBuiltin::Sort),
        "rsort" => Some(StableArraySortBuiltin::Rsort),
        _ => None,
    }
}

/// Variadic coordinated sort over two or more authoritative native arrays.
///
/// Unlike the single-array sort family this has one stable slice ABI because
/// PHP interleaves an arbitrary number of by-reference arrays with their
/// direction and comparison flags.
pub(super) fn stable_builtin_array_multisort(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("array_multisort")
}

/// Introspection over the active native PHP call frame. The frame already
/// carries authoritative native encodings; these fixed handlers expose that
/// view without entering the generic builtin dispatcher or materializing
/// Rust `Value` trees.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableFrameIntrospectionBuiltin {
    NumArgs,
    GetArg,
    GetArgs,
    DebugBacktrace,
}

impl StableFrameIntrospectionBuiltin {
    pub(super) const COUNT: usize = 4;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::NumArgs => 0,
            Self::GetArg => 1,
            Self::GetArgs => 2,
            Self::DebugBacktrace => 3,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::NumArgs => "phrust_native_func_num_args",
            Self::GetArg => "phrust_native_func_get_arg",
            Self::GetArgs => "phrust_native_func_get_args",
            Self::DebugBacktrace => "phrust_native_debug_backtrace",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::GetArg => arity == 1,
            Self::NumArgs | Self::GetArgs => arity == 0,
            Self::DebugBacktrace => arity <= 2,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::NumArgs,
            Self::GetArg,
            Self::GetArgs,
            Self::DebugBacktrace,
        ]
    }
}

pub(super) fn stable_builtin_frame_introspection(
    target: &RegionCallTarget,
) -> Option<StableFrameIntrospectionBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "func_num_args" => Some(StableFrameIntrospectionBuiltin::NumArgs),
        "func_get_arg" => Some(StableFrameIntrospectionBuiltin::GetArg),
        "func_get_args" => Some(StableFrameIntrospectionBuiltin::GetArgs),
        "debug_backtrace" => Some(StableFrameIntrospectionBuiltin::DebugBacktrace),
        _ => None,
    }
}

/// Stable object identity reads over authoritative direct object and closure
/// owners. Hash and integer forms have separate fixed ABIs; neither operation
/// enters generic SPL dispatch or materializes declared object properties.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableObjectIdentityBuiltin {
    Hash,
    Id,
}

impl StableObjectIdentityBuiltin {
    pub(super) const COUNT: usize = 2;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Hash => 0,
            Self::Id => 1,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Hash => "phrust_native_spl_object_hash",
            Self::Id => "phrust_native_spl_object_id",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::Hash, Self::Id]
    }
}

pub(super) fn stable_builtin_object_identity(
    target: &RegionCallTarget,
) -> Option<StableObjectIdentityBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "spl_object_hash" => Some(StableObjectIdentityBuiltin::Hash),
        "spl_object_id" => Some(StableObjectIdentityBuiltin::Id),
        _ => None,
    }
}

/// Callable-shape queries over authoritative native strings, objects, arrays,
/// references, and prepared callable records.
///
/// The optional callable-name output remains a native reference operand; the
/// exact handler updates that reference directly instead of routing through
/// the generic builtin binder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableCallableQueryBuiltin {
    IsCallable,
}

impl StableCallableQueryBuiltin {
    pub(super) const COUNT: usize = 1;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::IsCallable => 0,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::IsCallable => "phrust_native_is_callable",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::IsCallable => arity >= 1 && arity <= 3,
        }
    }

    pub(super) const fn argument_is_by_reference(self, index: usize) -> bool {
        match self {
            Self::IsCallable => index == 2,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::IsCallable]
    }
}

pub(super) fn stable_builtin_callable_query(
    target: &RegionCallTarget,
) -> Option<StableCallableQueryBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "is_callable" => Some(StableCallableQueryBuiltin::IsCallable),
        _ => None,
    }
}

/// Request-scoped error/exception handler stacks over authoritative native
/// callable owners. Each builtin has a dedicated ABI; no operation ID or
/// generic prepared-builtin dispatcher participates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableCallbackHandlerBuiltin {
    SetError,
    RestoreError,
    SetException,
    RestoreException,
    GetException,
    TriggerError,
}

impl StableCallbackHandlerBuiltin {
    pub(super) const COUNT: usize = 6;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::SetError => 0,
            Self::RestoreError => 1,
            Self::SetException => 2,
            Self::RestoreException => 3,
            Self::GetException => 4,
            Self::TriggerError => 5,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::SetError => "phrust_native_set_error_handler",
            Self::RestoreError => "phrust_native_restore_error_handler",
            Self::SetException => "phrust_native_set_exception_handler",
            Self::RestoreException => "phrust_native_restore_exception_handler",
            Self::GetException => "phrust_native_get_exception_handler",
            Self::TriggerError => "phrust_native_trigger_error",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::SetError => arity == 1 || arity == 2,
            Self::SetException => arity == 1,
            Self::RestoreError | Self::RestoreException | Self::GetException => arity == 0,
            Self::TriggerError => arity == 1 || arity == 2,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::SetError,
            Self::RestoreError,
            Self::SetException,
            Self::RestoreException,
            Self::GetException,
            Self::TriggerError,
        ]
    }
}

pub(super) fn stable_builtin_callback_handler(
    target: &RegionCallTarget,
) -> Option<StableCallbackHandlerBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "set_error_handler" => Some(StableCallbackHandlerBuiltin::SetError),
        "restore_error_handler" => Some(StableCallbackHandlerBuiltin::RestoreError),
        "set_exception_handler" => Some(StableCallbackHandlerBuiltin::SetException),
        "restore_exception_handler" => Some(StableCallbackHandlerBuiltin::RestoreException),
        "get_exception_handler" => Some(StableCallbackHandlerBuiltin::GetException),
        "trigger_error" | "user_error" => Some(StableCallbackHandlerBuiltin::TriggerError),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableAutoloadCallbackBuiltin {
    Register,
    Unregister,
    Functions,
}

impl StableAutoloadCallbackBuiltin {
    pub(super) const COUNT: usize = 3;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Register => 0,
            Self::Unregister => 1,
            Self::Functions => 2,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Register => "phrust_native_spl_autoload_register",
            Self::Unregister => "phrust_native_spl_autoload_unregister",
            Self::Functions => "phrust_native_spl_autoload_functions",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Register => arity >= 1 && arity <= 3,
            Self::Unregister => arity == 1,
            Self::Functions => arity == 0,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::Register, Self::Unregister, Self::Functions]
    }
}

pub(super) fn stable_builtin_autoload_callback(
    target: &RegionCallTarget,
) -> Option<StableAutoloadCallbackBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "spl_autoload_register" => Some(StableAutoloadCallbackBuiltin::Register),
        "spl_autoload_unregister" => Some(StableAutoloadCallbackBuiltin::Unregister),
        "spl_autoload_functions" => Some(StableAutoloadCallbackBuiltin::Functions),
        _ => None,
    }
}

pub(super) fn stable_builtin_shutdown_callback(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("register_shutdown_function")
}

/// PHP wire serialization over authoritative native scalar/array graphs.
/// Object hooks, reference records, request-specific float precision, and
/// malformed-input diagnostics retain one baseline continuation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableSerializationBuiltin {
    Serialize,
    Unserialize,
}

impl StableSerializationBuiltin {
    pub(super) const COUNT: usize = 2;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Serialize => 0,
            Self::Unserialize => 1,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Serialize => "phrust_native_serialize",
            Self::Unserialize => "phrust_native_unserialize",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::Serialize, Self::Unserialize]
    }
}

pub(super) fn stable_builtin_serialization(
    target: &RegionCallTarget,
) -> Option<StableSerializationBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "serialize" => Some(StableSerializationBuiltin::Serialize),
        "unserialize" => Some(StableSerializationBuiltin::Unserialize),
        _ => None,
    }
}

/// Stable object-property projections over the authoritative native object
/// layout. Visible and mangled forms have separate fixed ABIs so optimizing
/// code never enters generic builtin dispatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableObjectVarsBuiltin {
    Visible,
    Mangled,
}

impl StableObjectVarsBuiltin {
    pub(super) const COUNT: usize = 2;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Visible => 0,
            Self::Mangled => 1,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Visible => "phrust_native_get_object_vars",
            Self::Mangled => "phrust_native_get_mangled_object_vars",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::Visible, Self::Mangled]
    }
}

pub(super) fn stable_builtin_object_vars(
    target: &RegionCallTarget,
) -> Option<StableObjectVarsBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "get_object_vars" => Some(StableObjectVarsBuiltin::Visible),
        "get_mangled_object_vars" => Some(StableObjectVarsBuiltin::Mangled),
        _ => None,
    }
}

pub(super) fn stable_builtin_get_class(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("get_class")
}

/// Immutable class method/default-property projections. The caller function
/// is a fixed numeric ABI argument used only for PHP visibility checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableClassMetadataBuiltin {
    Methods,
    Vars,
}

impl StableClassMetadataBuiltin {
    pub(super) const COUNT: usize = 2;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Methods => 0,
            Self::Vars => 1,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Methods => "phrust_native_get_class_methods",
            Self::Vars => "phrust_native_get_class_vars",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::Methods, Self::Vars]
    }
}

pub(super) fn stable_builtin_class_metadata(
    target: &RegionCallTarget,
) -> Option<StableClassMetadataBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "get_class_methods" => Some(StableClassMetadataBuiltin::Methods),
        "get_class_vars" => Some(StableClassMetadataBuiltin::Vars),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableClassLineageBuiltin {
    ParentClass,
    IsSubclassOf,
    IsA,
    Implements,
}

impl StableClassLineageBuiltin {
    pub(super) const COUNT: usize = 4;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::ParentClass => 0,
            Self::IsSubclassOf => 1,
            Self::IsA => 2,
            Self::Implements => 3,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::ParentClass => "phrust_native_get_parent_class",
            Self::IsSubclassOf => "phrust_native_is_subclass_of",
            Self::IsA => "phrust_native_is_a",
            Self::Implements => "phrust_native_class_implements",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::ParentClass => arity == 1,
            Self::IsSubclassOf | Self::IsA => arity == 2 || arity == 3,
            Self::Implements => arity == 1 || arity == 2,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::ParentClass,
            Self::IsSubclassOf,
            Self::IsA,
            Self::Implements,
        ]
    }
}

pub(super) fn stable_builtin_class_lineage(
    target: &RegionCallTarget,
) -> Option<StableClassLineageBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "get_parent_class" => Some(StableClassLineageBuiltin::ParentClass),
        "is_subclass_of" => Some(StableClassLineageBuiltin::IsSubclassOf),
        "is_a" => Some(StableClassLineageBuiltin::IsA),
        "class_implements" => Some(StableClassLineageBuiltin::Implements),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableExtensionQueryBuiltin {
    IsLoaded,
    LoadedNames,
}

impl StableExtensionQueryBuiltin {
    pub(super) const COUNT: usize = 2;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::IsLoaded => 0,
            Self::LoadedNames => 1,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::IsLoaded => "phrust_native_extension_loaded",
            Self::LoadedNames => "phrust_native_get_loaded_extensions",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::IsLoaded => arity == 1,
            Self::LoadedNames => arity <= 1,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::IsLoaded, Self::LoadedNames]
    }
}

pub(super) fn stable_builtin_extension_query(
    target: &RegionCallTarget,
) -> Option<StableExtensionQueryBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "extension_loaded" => Some(StableExtensionQueryBuiltin::IsLoaded),
        "get_loaded_extensions" => Some(StableExtensionQueryBuiltin::LoadedNames),
        _ => None,
    }
}

/// Exact request-memory observations over the authoritative native output
/// capability. The optional PHP `real_usage` flag is accepted for signature
/// compatibility but does not select a generic operation at runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableMemoryQueryBuiltin {
    Usage,
    PeakUsage,
}

impl StableMemoryQueryBuiltin {
    pub(super) const COUNT: usize = 2;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Usage => 0,
            Self::PeakUsage => 1,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Usage => "phrust_native_memory_get_usage",
            Self::PeakUsage => "phrust_native_memory_get_peak_usage",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        arity <= 1
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::Usage, Self::PeakUsage]
    }
}

pub(super) fn stable_builtin_memory_query(
    target: &RegionCallTarget,
) -> Option<StableMemoryQueryBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "memory_get_usage" => Some(StableMemoryQueryBuiltin::Usage),
        "memory_get_peak_usage" => Some(StableMemoryQueryBuiltin::PeakUsage),
        _ => None,
    }
}

/// Complete request-local cycle-collector control/query family. Each variant
/// selects one fixed ABI; no operation ID reaches generated code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableGcBuiltin {
    CollectCycles,
    Disable,
    Enable,
    Enabled,
    MemCaches,
    Status,
}

impl StableGcBuiltin {
    pub(super) const COUNT: usize = 6;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::CollectCycles => 0,
            Self::Disable => 1,
            Self::Enable => 2,
            Self::Enabled => 3,
            Self::MemCaches => 4,
            Self::Status => 5,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::CollectCycles => "phrust_native_gc_collect_cycles",
            Self::Disable => "phrust_native_gc_disable",
            Self::Enable => "phrust_native_gc_enable",
            Self::Enabled => "phrust_native_gc_enabled",
            Self::MemCaches => "phrust_native_gc_mem_caches",
            Self::Status => "phrust_native_gc_status",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::CollectCycles,
            Self::Disable,
            Self::Enable,
            Self::Enabled,
            Self::MemCaches,
            Self::Status,
        ]
    }
}

pub(super) fn stable_builtin_gc(target: &RegionCallTarget) -> Option<StableGcBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "gc_collect_cycles" => Some(StableGcBuiltin::CollectCycles),
        "gc_disable" => Some(StableGcBuiltin::Disable),
        "gc_enable" => Some(StableGcBuiltin::Enable),
        "gc_enabled" => Some(StableGcBuiltin::Enabled),
        "gc_mem_caches" => Some(StableGcBuiltin::MemCaches),
        "gc_status" => Some(StableGcBuiltin::Status),
        _ => None,
    }
}

/// Complete resource-introspection family over authoritative request-owned
/// resource handles. Each variant selects one fixed native ABI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableResourceQueryBuiltin {
    Id,
    Type,
    All,
}

impl StableResourceQueryBuiltin {
    pub(super) const COUNT: usize = 3;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Id => 0,
            Self::Type => 1,
            Self::All => 2,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Id => "phrust_native_get_resource_id",
            Self::Type => "phrust_native_get_resource_type",
            Self::All => "phrust_native_get_resources",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Id | Self::Type => arity == 1,
            Self::All => arity <= 1,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::Id, Self::Type, Self::All]
    }
}

pub(super) fn stable_builtin_resource_query(
    target: &RegionCallTarget,
) -> Option<StableResourceQueryBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "get_resource_id" => Some(StableResourceQueryBuiltin::Id),
        "get_resource_type" => Some(StableResourceQueryBuiltin::Type),
        "get_resources" => Some(StableResourceQueryBuiltin::All),
        _ => None,
    }
}

/// Exact fileinfo operations over one prepared native `finfo` object shape.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableFileinfoBuiltin {
    Open,
    Close,
    Buffer,
    File,
    SetFlags,
    ExifImageType,
    ImageTypeToMimeType,
    GetImageSize,
}

impl StableFileinfoBuiltin {
    pub(super) const COUNT: usize = 8;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Open => 0,
            Self::Close => 1,
            Self::Buffer => 2,
            Self::File => 3,
            Self::SetFlags => 4,
            Self::ExifImageType => 5,
            Self::ImageTypeToMimeType => 6,
            Self::GetImageSize => 7,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Open => "phrust_native_finfo_open",
            Self::Close => "phrust_native_finfo_close",
            Self::Buffer => "phrust_native_finfo_buffer",
            Self::File => "phrust_native_finfo_file",
            Self::SetFlags => "phrust_native_finfo_set_flags",
            Self::ExifImageType => "phrust_native_exif_imagetype",
            Self::ImageTypeToMimeType => "phrust_native_image_type_to_mime_type",
            Self::GetImageSize => "phrust_native_getimagesize",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Open => arity <= 2,
            Self::Close => arity == 1,
            Self::Buffer | Self::File => arity >= 2 && arity <= 4,
            Self::SetFlags => arity == 2,
            Self::ExifImageType | Self::ImageTypeToMimeType => arity == 1,
            Self::GetImageSize => arity >= 1 && arity <= 2,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Open,
            Self::Close,
            Self::Buffer,
            Self::File,
            Self::SetFlags,
            Self::ExifImageType,
            Self::ImageTypeToMimeType,
            Self::GetImageSize,
        ]
    }
}

pub(super) fn stable_builtin_fileinfo(target: &RegionCallTarget) -> Option<StableFileinfoBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "finfo_open" => Some(StableFileinfoBuiltin::Open),
        "finfo_close" => Some(StableFileinfoBuiltin::Close),
        "finfo_buffer" => Some(StableFileinfoBuiltin::Buffer),
        "finfo_file" => Some(StableFileinfoBuiltin::File),
        "finfo_set_flags" => Some(StableFileinfoBuiltin::SetFlags),
        "exif_imagetype" => Some(StableFileinfoBuiltin::ExifImageType),
        "image_type_to_mime_type" => Some(StableFileinfoBuiltin::ImageTypeToMimeType),
        "getimagesize" => Some(StableFileinfoBuiltin::GetImageSize),
        _ => None,
    }
}

/// Request-local PHP error-state observation and reset. The stored diagnostic
/// is already a native record; exact handlers publish its array view directly
/// instead of reconstructing a Rust `Value`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableErrorStateBuiltin {
    GetLast,
    ClearLast,
}

impl StableErrorStateBuiltin {
    pub(super) const COUNT: usize = 2;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::GetLast => 0,
            Self::ClearLast => 1,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::GetLast => "phrust_native_error_get_last",
            Self::ClearLast => "phrust_native_error_clear_last",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::GetLast, Self::ClearLast]
    }
}

pub(super) fn stable_builtin_error_state(
    target: &RegionCallTarget,
) -> Option<StableErrorStateBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "error_get_last" => Some(StableErrorStateBuiltin::GetLast),
        "error_clear_last" => Some(StableErrorStateBuiltin::ClearLast),
        _ => None,
    }
}

pub(super) fn stable_builtin_settype(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    normalized.eq_ignore_ascii_case("settype") && !normalized.contains('\\')
}

/// Stable reads and writes through the request-published configuration
/// capability. Every operation has its own exact ABI; the enum exists only
/// in compilation metadata and is never passed as a runtime operation ID.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableConfigurationBuiltin {
    IniGet,
    IniGetAll,
    CfgVar,
    IncludePath,
    IniSet,
    SetIncludePath,
    TimezoneGet,
    TimezoneSet,
}

impl StableConfigurationBuiltin {
    pub(super) const COUNT: usize = 8;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::IniGet => 0,
            Self::IniGetAll => 1,
            Self::CfgVar => 2,
            Self::IncludePath => 3,
            Self::IniSet => 4,
            Self::SetIncludePath => 5,
            Self::TimezoneGet => 6,
            Self::TimezoneSet => 7,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::IniGet => "phrust_native_ini_get",
            Self::IniGetAll => "phrust_native_ini_get_all",
            Self::CfgVar => "phrust_native_get_cfg_var",
            Self::IncludePath => "phrust_native_get_include_path",
            Self::IniSet => "phrust_native_ini_set",
            Self::SetIncludePath => "phrust_native_set_include_path",
            Self::TimezoneGet => "phrust_native_date_default_timezone_get",
            Self::TimezoneSet => "phrust_native_date_default_timezone_set",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::IniGet | Self::CfgVar | Self::SetIncludePath | Self::TimezoneSet => arity == 1,
            Self::IniGetAll => arity <= 2,
            Self::IncludePath | Self::TimezoneGet => arity == 0,
            Self::IniSet => arity == 2,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::IniGet,
            Self::IniGetAll,
            Self::CfgVar,
            Self::IncludePath,
            Self::IniSet,
            Self::SetIncludePath,
            Self::TimezoneGet,
            Self::TimezoneSet,
        ]
    }
}

pub(super) fn stable_builtin_configuration(
    target: &RegionCallTarget,
) -> Option<StableConfigurationBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "ini_get" => Some(StableConfigurationBuiltin::IniGet),
        "ini_get_all" => Some(StableConfigurationBuiltin::IniGetAll),
        "get_cfg_var" => Some(StableConfigurationBuiltin::CfgVar),
        "get_include_path" => Some(StableConfigurationBuiltin::IncludePath),
        "ini_set" => Some(StableConfigurationBuiltin::IniSet),
        "set_include_path" => Some(StableConfigurationBuiltin::SetIncludePath),
        "date_default_timezone_get" => Some(StableConfigurationBuiltin::TimezoneGet),
        "date_default_timezone_set" => Some(StableConfigurationBuiltin::TimezoneSet),
        _ => None,
    }
}

/// Exact access to the request-owned HTTP response state.
///
/// Each variant has a distinct native symbol and the response capability is
/// published once with the request. Generated code never supplies an
/// operation ID or enters prepared-builtin dispatch for an admitted call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableHttpResponseBuiltin {
    Header,
    HeaderRemove,
    HeadersList,
    HeadersSent,
    ResponseCode,
}

impl StableHttpResponseBuiltin {
    pub(super) const COUNT: usize = 5;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Header => 0,
            Self::HeaderRemove => 1,
            Self::HeadersList => 2,
            Self::HeadersSent => 3,
            Self::ResponseCode => 4,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Header => "phrust_native_header",
            Self::HeaderRemove => "phrust_native_header_remove",
            Self::HeadersList => "phrust_native_headers_list",
            Self::HeadersSent => "phrust_native_headers_sent",
            Self::ResponseCode => "phrust_native_http_response_code",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Header => arity >= 1 && arity <= 3,
            Self::HeaderRemove | Self::ResponseCode => arity <= 1,
            Self::HeadersList | Self::HeadersSent => arity == 0,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Header,
            Self::HeaderRemove,
            Self::HeadersList,
            Self::HeadersSent,
            Self::ResponseCode,
        ]
    }
}

pub(super) fn stable_builtin_http_response(
    target: &RegionCallTarget,
) -> Option<StableHttpResponseBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "header" => Some(StableHttpResponseBuiltin::Header),
        "header_remove" => Some(StableHttpResponseBuiltin::HeaderRemove),
        "headers_list" => Some(StableHttpResponseBuiltin::HeadersList),
        "headers_sent" => Some(StableHttpResponseBuiltin::HeadersSent),
        "http_response_code" => Some(StableHttpResponseBuiltin::ResponseCode),
        _ => None,
    }
}

/// Exact cookie header construction over native scalar/direct-array values.
///
/// Cookies retain their seven-argument PHP signature through a dedicated
/// fixed ABI instead of truncating into the shared six-argument handler ABI.
/// Raw-vs-encoded identity remains compile-time metadata only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableCookieBuiltin {
    Encoded,
    Raw,
}

impl StableCookieBuiltin {
    pub(super) const COUNT: usize = 2;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Encoded => 0,
            Self::Raw => 1,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Encoded => "phrust_native_setcookie",
            Self::Raw => "phrust_native_setrawcookie",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        arity >= 1 && arity <= 7
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [Self::Encoded, Self::Raw]
    }
}

pub(super) fn stable_builtin_cookie(target: &RegionCallTarget) -> Option<StableCookieBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "setcookie" => Some(StableCookieBuiltin::Encoded),
        "setrawcookie" => Some(StableCookieBuiltin::Raw),
        _ => None,
    }
}

/// Exact wall-clock reads over authoritative native values.
///
/// Each PHP-visible result shape has its own fixed native entry. The enum is
/// compilation metadata only and is never passed to generated code as an
/// operation ID.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableClockBuiltin {
    Time,
    Microtime,
    Hrtime,
    Usleep,
    SetTimeLimit,
    Uniqid,
}

impl StableClockBuiltin {
    pub(super) const COUNT: usize = 6;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Time => 0,
            Self::Microtime => 1,
            Self::Hrtime => 2,
            Self::Usleep => 3,
            Self::SetTimeLimit => 4,
            Self::Uniqid => 5,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Time => "phrust_native_time",
            Self::Microtime => "phrust_native_microtime",
            Self::Hrtime => "phrust_native_hrtime",
            Self::Usleep => "phrust_native_usleep",
            Self::SetTimeLimit => "phrust_native_set_time_limit",
            Self::Uniqid => "phrust_native_uniqid",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Time => arity == 0,
            Self::Microtime | Self::Hrtime => arity <= 1,
            Self::Usleep | Self::SetTimeLimit => arity == 1,
            Self::Uniqid => arity <= 2,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Time,
            Self::Microtime,
            Self::Hrtime,
            Self::Usleep,
            Self::SetTimeLimit,
            Self::Uniqid,
        ]
    }
}

pub(super) fn stable_builtin_clock(target: &RegionCallTarget) -> Option<StableClockBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "time" => Some(StableClockBuiltin::Time),
        "microtime" => Some(StableClockBuiltin::Microtime),
        "hrtime" => Some(StableClockBuiltin::Hrtime),
        "usleep" => Some(StableClockBuiltin::Usleep),
        "set_time_limit" => Some(StableClockBuiltin::SetTimeLimit),
        "uniqid" => Some(StableClockBuiltin::Uniqid),
        _ => None,
    }
}

/// Exact procedural date/time operations over authoritative native scalars
/// and the already-published request timezone capability.
///
/// Every admitted operation below has a distinct native symbol; generated
/// code never supplies a date operation ID. Object construction consumes an
/// immutable class plan published before request execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableDateBuiltin {
    Checkdate,
    Date,
    Gmdate,
    Strtotime,
    Mktime,
    Gmmktime,
    TimezoneIdentifiers,
    DateCreate,
    TimezoneOpen,
}

impl StableDateBuiltin {
    pub(super) const COUNT: usize = 9;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Checkdate => 0,
            Self::Date => 1,
            Self::Gmdate => 2,
            Self::Strtotime => 3,
            Self::Mktime => 4,
            Self::Gmmktime => 5,
            Self::TimezoneIdentifiers => 6,
            Self::DateCreate => 7,
            Self::TimezoneOpen => 8,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Checkdate => "phrust_native_checkdate",
            Self::Date => "phrust_native_date",
            Self::Gmdate => "phrust_native_gmdate",
            Self::Strtotime => "phrust_native_strtotime",
            Self::Mktime => "phrust_native_mktime",
            Self::Gmmktime => "phrust_native_gmmktime",
            Self::TimezoneIdentifiers => "phrust_native_timezone_identifiers_list",
            Self::DateCreate => "phrust_native_date_create",
            Self::TimezoneOpen => "phrust_native_timezone_open",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Checkdate => arity == 3,
            Self::Date | Self::Gmdate | Self::Strtotime => arity == 1 || arity == 2,
            Self::Mktime | Self::Gmmktime => arity >= 1 && arity <= 6,
            Self::TimezoneIdentifiers => arity <= 2,
            Self::DateCreate => arity <= 2,
            Self::TimezoneOpen => arity == 1,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Checkdate,
            Self::Date,
            Self::Gmdate,
            Self::Strtotime,
            Self::Mktime,
            Self::Gmmktime,
            Self::TimezoneIdentifiers,
            Self::DateCreate,
            Self::TimezoneOpen,
        ]
    }
}

pub(super) fn stable_builtin_date(target: &RegionCallTarget) -> Option<StableDateBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "checkdate" => Some(StableDateBuiltin::Checkdate),
        "date" => Some(StableDateBuiltin::Date),
        "gmdate" => Some(StableDateBuiltin::Gmdate),
        "strtotime" => Some(StableDateBuiltin::Strtotime),
        "mktime" => Some(StableDateBuiltin::Mktime),
        "gmmktime" => Some(StableDateBuiltin::Gmmktime),
        "timezone_identifiers_list" => Some(StableDateBuiltin::TimezoneIdentifiers),
        "date_create" => Some(StableDateBuiltin::DateCreate),
        "timezone_open" => Some(StableDateBuiltin::TimezoneOpen),
        _ => None,
    }
}

/// Exact random operations backed by one explicitly published request
/// capability. Each result shape and mutating array operation has a fixed
/// native entry; no operation ID enters generated code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableRandomBuiltin {
    RandomBytes,
    RandomInt,
    Rand,
    MtRand,
    GetRandMax,
    MtGetRandMax,
    ArrayRand,
    Shuffle,
}

impl StableRandomBuiltin {
    pub(super) const COUNT: usize = 8;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::RandomBytes => 0,
            Self::RandomInt => 1,
            Self::Rand => 2,
            Self::MtRand => 3,
            Self::GetRandMax => 4,
            Self::MtGetRandMax => 5,
            Self::ArrayRand => 6,
            Self::Shuffle => 7,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::RandomBytes => "phrust_native_random_bytes",
            Self::RandomInt => "phrust_native_random_int",
            Self::Rand => "phrust_native_rand",
            Self::MtRand => "phrust_native_mt_rand",
            Self::GetRandMax => "phrust_native_getrandmax",
            Self::MtGetRandMax => "phrust_native_mt_getrandmax",
            Self::ArrayRand => "phrust_native_array_rand",
            Self::Shuffle => "phrust_native_shuffle",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::RandomBytes => arity == 1,
            Self::RandomInt => arity == 2,
            Self::Rand | Self::MtRand => arity == 0 || arity == 2,
            Self::GetRandMax | Self::MtGetRandMax => arity == 0,
            Self::ArrayRand => arity == 1 || arity == 2,
            Self::Shuffle => arity == 1,
        }
    }

    pub(super) const fn argument_is_by_reference(self, index: usize) -> bool {
        matches!(self, Self::Shuffle) && index == 0
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::RandomBytes,
            Self::RandomInt,
            Self::Rand,
            Self::MtRand,
            Self::GetRandMax,
            Self::MtGetRandMax,
            Self::ArrayRand,
            Self::Shuffle,
        ]
    }
}

pub(super) fn stable_builtin_random(target: &RegionCallTarget) -> Option<StableRandomBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "random_bytes" => Some(StableRandomBuiltin::RandomBytes),
        "random_int" => Some(StableRandomBuiltin::RandomInt),
        "rand" => Some(StableRandomBuiltin::Rand),
        "mt_rand" => Some(StableRandomBuiltin::MtRand),
        "getrandmax" => Some(StableRandomBuiltin::GetRandMax),
        "mt_getrandmax" => Some(StableRandomBuiltin::MtGetRandMax),
        "array_rand" => Some(StableRandomBuiltin::ArrayRand),
        "shuffle" => Some(StableRandomBuiltin::Shuffle),
        _ => None,
    }
}

/// Stable operations over explicitly published request context.
///
/// Each variant lowers to its own exact ABI. The enum is compile-time
/// metadata only: generated code never supplies an operation ID and the
/// optimizing artifact cannot reach the generic builtin dispatcher.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableRequestQueryBuiltin {
    TempDir,
    CurrentDirectory,
    Environment,
    SapiName,
    Uname,
    CurrentUser,
    IncludedFiles,
    ChangeDirectory,
    Umask,
    ClearStatCache,
}

impl StableRequestQueryBuiltin {
    pub(super) const COUNT: usize = 10;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::TempDir => 0,
            Self::CurrentDirectory => 1,
            Self::Environment => 2,
            Self::SapiName => 3,
            Self::Uname => 4,
            Self::CurrentUser => 5,
            Self::IncludedFiles => 6,
            Self::ChangeDirectory => 7,
            Self::Umask => 8,
            Self::ClearStatCache => 9,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::TempDir => "phrust_native_sys_get_temp_dir",
            Self::CurrentDirectory => "phrust_native_getcwd",
            Self::Environment => "phrust_native_getenv",
            Self::SapiName => "phrust_native_php_sapi_name",
            Self::Uname => "phrust_native_php_uname",
            Self::CurrentUser => "phrust_native_get_current_user",
            Self::IncludedFiles => "phrust_native_get_included_files",
            Self::ChangeDirectory => "phrust_native_chdir",
            Self::Umask => "phrust_native_umask",
            Self::ClearStatCache => "phrust_native_clearstatcache",
        }
    }

    pub(super) const fn accepts_arity(self, arity: usize) -> bool {
        match self {
            Self::Environment | Self::Uname => arity <= 1,
            Self::ChangeDirectory => arity == 1,
            Self::Umask => arity <= 1,
            Self::ClearStatCache => arity <= 2,
            Self::TempDir
            | Self::CurrentDirectory
            | Self::SapiName
            | Self::CurrentUser
            | Self::IncludedFiles => arity == 0,
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::TempDir,
            Self::CurrentDirectory,
            Self::Environment,
            Self::SapiName,
            Self::Uname,
            Self::CurrentUser,
            Self::IncludedFiles,
            Self::ChangeDirectory,
            Self::Umask,
            Self::ClearStatCache,
        ]
    }
}

pub(super) fn stable_builtin_request_query(
    target: &RegionCallTarget,
) -> Option<StableRequestQueryBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "sys_get_temp_dir" => Some(StableRequestQueryBuiltin::TempDir),
        "getcwd" => Some(StableRequestQueryBuiltin::CurrentDirectory),
        "getenv" => Some(StableRequestQueryBuiltin::Environment),
        "php_sapi_name" => Some(StableRequestQueryBuiltin::SapiName),
        "php_uname" => Some(StableRequestQueryBuiltin::Uname),
        "get_current_user" => Some(StableRequestQueryBuiltin::CurrentUser),
        "get_included_files" | "get_required_files" => {
            Some(StableRequestQueryBuiltin::IncludedFiles)
        }
        "chdir" => Some(StableRequestQueryBuiltin::ChangeDirectory),
        "umask" => Some(StableRequestQueryBuiltin::Umask),
        "clearstatcache" => Some(StableRequestQueryBuiltin::ClearStatCache),
        _ => None,
    }
}

/// Immutable declaration-name inventories published by the compiled unit.
///
/// Values are constructed directly as native arrays. Runtime-valued
/// constants deliberately remain a separate storage family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableDeclarationInventoryBuiltin {
    Functions,
    Classes,
    Interfaces,
    Traits,
}

impl StableDeclarationInventoryBuiltin {
    pub(super) const COUNT: usize = 4;

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Functions => 0,
            Self::Classes => 1,
            Self::Interfaces => 2,
            Self::Traits => 3,
        }
    }

    pub(super) const fn symbol(self) -> &'static str {
        match self {
            Self::Functions => "phrust_native_get_defined_functions",
            Self::Classes => "phrust_native_get_declared_classes",
            Self::Interfaces => "phrust_native_get_declared_interfaces",
            Self::Traits => "phrust_native_get_declared_traits",
        }
    }

    pub(super) const fn all() -> [Self; Self::COUNT] {
        [
            Self::Functions,
            Self::Classes,
            Self::Interfaces,
            Self::Traits,
        ]
    }
}

pub(super) fn stable_builtin_declaration_inventory(
    target: &RegionCallTarget,
) -> Option<StableDeclarationInventoryBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "get_defined_functions" => Some(StableDeclarationInventoryBuiltin::Functions),
        "get_declared_classes" => Some(StableDeclarationInventoryBuiltin::Classes),
        "get_declared_interfaces" => Some(StableDeclarationInventoryBuiltin::Interfaces),
        "get_declared_traits" => Some(StableDeclarationInventoryBuiltin::Traits),
        _ => None,
    }
}

pub(super) fn stable_builtin_constant_inventory(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("get_defined_constants")
}

pub(super) fn stable_builtin_compact(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("compact")
}

pub(super) fn stable_builtin_get_defined_vars(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("get_defined_vars")
}

/// Compile-time family identity for every fixed builtin whose implementation
/// lives behind the exact native control ABI.
///
/// This value never enters generated code. Publication uses it to require one
/// complete operand/resource/ownership plan before optimizing lowering is
/// allowed to declare the family's fixed symbol.
pub(super) fn stable_exact_control_builtin_family(
    target: &RegionCallTarget,
) -> Option<&'static str> {
    if stable_builtin_symbol_query(target).is_some() {
        Some("symbol-query")
    } else if stable_builtin_pcre(target).is_some() {
        Some("pcre")
    } else if stable_builtin_json(target).is_some() {
        Some("json")
    } else if stable_builtin_format(target).is_some() {
        Some("format")
    } else if stable_builtin_hash(target).is_some() {
        Some("hash")
    } else if stable_builtin_byte_codec(target).is_some() {
        Some("byte-codec")
    } else if stable_builtin_string_search_compare(target).is_some() {
        Some("string-search-compare")
    } else if stable_builtin_string_rewrite(target).is_some() {
        Some("string-rewrite")
    } else if stable_builtin_html_codec(target).is_some() {
        Some("html-codec")
    } else if stable_builtin_url_query(target).is_some() {
        Some("url-query")
    } else if stable_builtin_array_aggregate(target).is_some() {
        Some("array-aggregate")
    } else if stable_builtin_recursive_array(target).is_some() {
        Some("recursive-array")
    } else if stable_builtin_array_sort(target).is_some() || stable_builtin_array_multisort(target)
    {
        Some("array-sort")
    } else if stable_builtin_object_identity(target).is_some() {
        Some("object-identity")
    } else if stable_builtin_callable_query(target).is_some() {
        Some("callable-query")
    } else if stable_builtin_callback_handler(target).is_some()
        || stable_builtin_autoload_callback(target).is_some()
        || stable_builtin_shutdown_callback(target)
    {
        Some("callback-control")
    } else if stable_builtin_serialization(target).is_some() {
        Some("serialization")
    } else if stable_builtin_tokenizer(target).is_some() {
        Some("tokenizer")
    } else if stable_builtin_mbstring(target).is_some() {
        Some("mbstring")
    } else if stable_builtin_bcmath(target).is_some() {
        Some("bcmath")
    } else if stable_builtin_filter(target).is_some() {
        Some("filter")
    } else if stable_builtin_session(target).is_some() {
        Some("session")
    } else if stable_builtin_object_vars(target).is_some() {
        Some("object-vars")
    } else if stable_builtin_class_metadata(target).is_some() {
        Some("class-metadata")
    } else if stable_builtin_class_lineage(target).is_some() {
        Some("class-lineage")
    } else if stable_builtin_extension_query(target).is_some() {
        Some("extension-query")
    } else if stable_builtin_memory_query(target).is_some() {
        Some("memory-query")
    } else if stable_builtin_gc(target).is_some() {
        Some("gc")
    } else if stable_builtin_resource_query(target).is_some() {
        Some("resource-query")
    } else if stable_builtin_error_state(target).is_some() {
        Some("error-state")
    } else if stable_builtin_settype(target) {
        Some("settype")
    } else if stable_builtin_configuration(target).is_some() {
        Some("configuration")
    } else if stable_builtin_http_response(target).is_some() {
        Some("http-response")
    } else if stable_builtin_cookie(target).is_some() {
        Some("cookie")
    } else if stable_builtin_clock(target).is_some() {
        Some("clock")
    } else if stable_builtin_date(target).is_some() {
        Some("date")
    } else if stable_builtin_random(target).is_some() {
        Some("random")
    } else if stable_builtin_request_query(target).is_some() {
        Some("request-query")
    } else if stable_builtin_declaration_inventory(target).is_some()
        || stable_builtin_constant_inventory(target)
    {
        Some("declaration-inventory")
    } else if stable_builtin_compact(target) || stable_builtin_get_defined_vars(target) {
        Some("frame-values")
    } else if stable_builtin_frame_introspection(target).is_some() {
        Some("frame-introspection")
    } else if stable_builtin_base_conversion(target).is_some() {
        Some("base-conversion")
    } else if stable_builtin_network_address(target).is_some() {
        Some("network-address")
    } else if stable_builtin_compression_codec(target).is_some() {
        Some("compression-codec")
    } else if stable_builtin_path(target).is_some() {
        Some("path")
    } else if stable_builtin_output_buffer(target).is_some() {
        Some("output-buffer")
    } else {
        None
    }
}

/// Non-callback array set and overlay operations over authoritative direct
/// entries. Callback comparators and recursive overlays remain distinct
/// baseline semantics instead of being smuggled through this fixed family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableArraySetBuiltin {
    Diff,
    DiffAssoc,
    DiffKey,
    Intersect,
    IntersectAssoc,
    IntersectKey,
    Replace,
}

impl StableArraySetBuiltin {
    pub(super) const fn requires_two_arrays(self) -> bool {
        !matches!(self, Self::Replace)
    }

    pub(super) const fn value_sensitive(self) -> bool {
        matches!(
            self,
            Self::Diff | Self::DiffAssoc | Self::Intersect | Self::IntersectAssoc
        )
    }

    pub(super) const fn keeps_match(self) -> bool {
        matches!(
            self,
            Self::Intersect | Self::IntersectAssoc | Self::IntersectKey
        )
    }
}

pub(super) fn stable_builtin_array_set(target: &RegionCallTarget) -> Option<StableArraySetBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "array_diff" => Some(StableArraySetBuiltin::Diff),
        "array_diff_assoc" => Some(StableArraySetBuiltin::DiffAssoc),
        "array_diff_key" => Some(StableArraySetBuiltin::DiffKey),
        "array_intersect" => Some(StableArraySetBuiltin::Intersect),
        "array_intersect_assoc" => Some(StableArraySetBuiltin::IntersectAssoc),
        "array_intersect_key" => Some(StableArraySetBuiltin::IntersectKey),
        "array_replace" => Some(StableArraySetBuiltin::Replace),
        _ => None,
    }
}

/// Callback-neutral array transforms. The selector distinguishes
/// `array_map(null, $array)` from `array_filter($array[, null])`. Callable
/// forms are owned by `RegionArrayCallbackCall`, including runtime-prepared
/// same-unit callables, and never enter this neutral-only selector.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableCallbackNeutralArrayBuiltin {
    MapNull,
    FilterTruthy,
}

pub(super) fn stable_builtin_callback_neutral_array(
    target: &RegionCallTarget,
) -> Option<StableCallbackNeutralArrayBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "array_map" => Some(StableCallbackNeutralArrayBuiltin::MapNull),
        "array_filter" => Some(StableCallbackNeutralArrayBuiltin::FilterTruthy),
        _ => None,
    }
}

/// Strict native array membership operations. The selector distinguishes a
/// boolean membership result from the matching key result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableArrayLookupBuiltin {
    InArray,
    Search,
}

pub(super) fn stable_builtin_array_lookup(
    target: &RegionCallTarget,
) -> Option<StableArrayLookupBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "in_array" => Some(StableArrayLookupBuiltin::InArray),
        "array_search" => Some(StableArrayLookupBuiltin::Search),
        _ => None,
    }
}

/// Array-key queries that preserve the source key representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableArrayEdgeKeyBuiltin {
    First,
    Last,
}

pub(super) fn stable_builtin_array_edge_key(
    target: &RegionCallTarget,
) -> Option<StableArrayEdgeKeyBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "array_key_first" => Some(StableArrayEdgeKeyBuiltin::First),
        "array_key_last" => Some(StableArrayEdgeKeyBuiltin::Last),
        _ => None,
    }
}

/// PHP array internal-pointer operations. Read-only selectors consume the
/// authoritative native slot; mutating selectors require an exact caller
/// local and update that slot after COW separation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableArrayPointerBuiltin {
    Current,
    Key,
    Next,
    Reset,
    Prev,
    End,
}

impl StableArrayPointerBuiltin {
    pub(super) const fn is_read_only(self) -> bool {
        matches!(self, Self::Current | Self::Key)
    }
}

pub(super) fn stable_builtin_array_pointer(
    target: &RegionCallTarget,
) -> Option<StableArrayPointerBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "current" => Some(StableArrayPointerBuiltin::Current),
        "key" => Some(StableArrayPointerBuiltin::Key),
        "next" => Some(StableArrayPointerBuiltin::Next),
        "reset" => Some(StableArrayPointerBuiltin::Reset),
        "prev" => Some(StableArrayPointerBuiltin::Prev),
        "end" => Some(StableArrayPointerBuiltin::End),
        _ => None,
    }
}

/// Exact local-mutating array deque operations over authoritative entries.
/// Pop/shift move one element owner into the result; push/unshift retain the
/// prepared positional values once for the array. Numeric-key reindexing for
/// the front operations happens directly in the stable entry range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableArrayStackBuiltin {
    Pop,
    Push,
    Shift,
    Unshift,
}

impl StableArrayStackBuiltin {
    pub(super) const fn minimum_arity(self) -> usize {
        match self {
            Self::Pop | Self::Shift => 1,
            Self::Push | Self::Unshift => 2,
        }
    }
}

pub(super) fn stable_builtin_array_stack(
    target: &RegionCallTarget,
) -> Option<StableArrayStackBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "array_pop" => Some(StableArrayStackBuiltin::Pop),
        "array_push" => Some(StableArrayStackBuiltin::Push),
        "array_shift" => Some(StableArrayStackBuiltin::Shift),
        "array_unshift" => Some(StableArrayStackBuiltin::Unshift),
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

pub(super) fn stable_builtin_array_splice(target: &RegionCallTarget) -> bool {
    let RegionCallTarget::Function { name, .. } = target else {
        return false;
    };
    let normalized = name.trim_start_matches('\\');
    !normalized.contains('\\') && normalized.eq_ignore_ascii_case("array_splice")
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StableStringSpanBuiltin {
    Included,
    Excluded,
}

pub(super) fn stable_builtin_string_span(
    target: &RegionCallTarget,
) -> Option<StableStringSpanBuiltin> {
    let RegionCallTarget::Function { name, .. } = target else {
        return None;
    };
    let normalized = name.trim_start_matches('\\');
    if normalized.contains('\\') {
        return None;
    }
    match normalized.to_ascii_lowercase().as_str() {
        "strspn" => Some(StableStringSpanBuiltin::Included),
        "strcspn" => Some(StableStringSpanBuiltin::Excluded),
        _ => None,
    }
}
