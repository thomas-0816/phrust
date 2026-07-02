use super::*;

pub(super) fn standard_library_core_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("core")
        .with_class(ClassDescriptor::new("Closure", "core", ClassKind::Class))
        .with_class(ClassDescriptor::new("stdClass", "core", ClassKind::Class))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_VERSION",
            "core",
            ConstantValue::String(constants::PHP_VERSION),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_VERSION_ID",
            "core",
            ConstantValue::Int(constants::PHP_VERSION_ID),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_MAJOR_VERSION",
            "core",
            ConstantValue::Int(constants::PHP_MAJOR_VERSION),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_MINOR_VERSION",
            "core",
            ConstantValue::Int(constants::PHP_MINOR_VERSION),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_RELEASE_VERSION",
            "core",
            ConstantValue::Int(constants::PHP_RELEASE_VERSION),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_INT_MAX",
            "core",
            ConstantValue::Int(constants::PHP_INT_MAX),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_INT_MIN",
            "core",
            ConstantValue::Int(constants::PHP_INT_MIN),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_INT_SIZE",
            "core",
            ConstantValue::Int(constants::PHP_INT_SIZE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "INF",
            "core",
            ConstantValue::Float(constants::INF),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "NAN",
            "core",
            ConstantValue::Float(constants::NAN),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "DIRECTORY_SEPARATOR",
            "core",
            ConstantValue::String(constants::DIRECTORY_SEPARATOR),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PATH_SEPARATOR",
            "core",
            ConstantValue::String(constants::PATH_SEPARATOR),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_OS",
            "core",
            ConstantValue::String(constants::PHP_OS),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_OS_FAMILY",
            "core",
            ConstantValue::String(constants::PHP_OS_FAMILY),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_EOL",
            "core",
            ConstantValue::String(constants::PHP_EOL),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_SAPI",
            "core",
            ConstantValue::String(constants::PHP_SAPI),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_BINARY",
            "core",
            ConstantValue::String(constants::PHP_BINARY),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "DEFAULT_INCLUDE_PATH",
            "core",
            ConstantValue::String(constants::DEFAULT_INCLUDE_PATH),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_MAXPATHLEN",
            "core",
            ConstantValue::Int(constants::PHP_MAXPATHLEN),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "E_ERROR",
            "core",
            ConstantValue::Int(constants::E_ERROR),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "E_WARNING",
            "core",
            ConstantValue::Int(constants::E_WARNING),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "E_PARSE",
            "core",
            ConstantValue::Int(constants::E_PARSE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "E_NOTICE",
            "core",
            ConstantValue::Int(constants::E_NOTICE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "E_CORE_ERROR",
            "core",
            ConstantValue::Int(constants::E_CORE_ERROR),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "E_CORE_WARNING",
            "core",
            ConstantValue::Int(constants::E_CORE_WARNING),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "E_COMPILE_ERROR",
            "core",
            ConstantValue::Int(constants::E_COMPILE_ERROR),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "E_COMPILE_WARNING",
            "core",
            ConstantValue::Int(constants::E_COMPILE_WARNING),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "E_USER_ERROR",
            "core",
            ConstantValue::Int(constants::E_USER_ERROR),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "E_USER_WARNING",
            "core",
            ConstantValue::Int(constants::E_USER_WARNING),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "E_USER_NOTICE",
            "core",
            ConstantValue::Int(constants::E_USER_NOTICE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "E_STRICT",
            "core",
            ConstantValue::Int(constants::E_STRICT),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "E_RECOVERABLE_ERROR",
            "core",
            ConstantValue::Int(constants::E_RECOVERABLE_ERROR),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "E_DEPRECATED",
            "core",
            ConstantValue::Int(constants::E_DEPRECATED),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "E_USER_DEPRECATED",
            "core",
            ConstantValue::Int(constants::E_USER_DEPRECATED),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "E_ALL",
            "core",
            ConstantValue::Int(constants::E_ALL),
        ))
}

pub(super) fn standard_library_standard_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("standard")
        .with_constant(ConstantDescriptor::with_value(
            "SORT_ASC",
            "standard",
            ConstantValue::Int(constants::SORT_ASC),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SORT_DESC",
            "standard",
            ConstantValue::Int(constants::SORT_DESC),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SORT_REGULAR",
            "standard",
            ConstantValue::Int(constants::SORT_REGULAR),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SORT_NUMERIC",
            "standard",
            ConstantValue::Int(constants::SORT_NUMERIC),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SORT_STRING",
            "standard",
            ConstantValue::Int(constants::SORT_STRING),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SORT_LOCALE_STRING",
            "standard",
            ConstantValue::Int(constants::SORT_LOCALE_STRING),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SORT_NATURAL",
            "standard",
            ConstantValue::Int(constants::SORT_NATURAL),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SORT_FLAG_CASE",
            "standard",
            ConstantValue::Int(constants::SORT_FLAG_CASE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LC_ALL",
            "standard",
            ConstantValue::Int(constants::LC_ALL),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LC_CTYPE",
            "standard",
            ConstantValue::Int(constants::LC_CTYPE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LC_NUMERIC",
            "standard",
            ConstantValue::Int(constants::LC_NUMERIC),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LC_TIME",
            "standard",
            ConstantValue::Int(constants::LC_TIME),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LC_COLLATE",
            "standard",
            ConstantValue::Int(constants::LC_COLLATE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LC_MONETARY",
            "standard",
            ConstantValue::Int(constants::LC_MONETARY),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LC_MESSAGES",
            "standard",
            ConstantValue::Int(constants::LC_MESSAGES),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CASE_LOWER",
            "standard",
            ConstantValue::Int(constants::CASE_LOWER),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CASE_UPPER",
            "standard",
            ConstantValue::Int(constants::CASE_UPPER),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "COUNT_NORMAL",
            "standard",
            ConstantValue::Int(constants::COUNT_NORMAL),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "COUNT_RECURSIVE",
            "standard",
            ConstantValue::Int(constants::COUNT_RECURSIVE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ARRAY_FILTER_USE_BOTH",
            "standard",
            ConstantValue::Int(constants::ARRAY_FILTER_USE_BOTH),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ARRAY_FILTER_USE_KEY",
            "standard",
            ConstantValue::Int(constants::ARRAY_FILTER_USE_KEY),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "STR_PAD_LEFT",
            "standard",
            ConstantValue::Int(constants::STR_PAD_LEFT),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "STR_PAD_RIGHT",
            "standard",
            ConstantValue::Int(constants::STR_PAD_RIGHT),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "STR_PAD_BOTH",
            "standard",
            ConstantValue::Int(constants::STR_PAD_BOTH),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "M_E",
            "standard",
            ConstantValue::Float(constants::M_E),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "M_LOG2E",
            "standard",
            ConstantValue::Float(constants::M_LOG2E),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "M_LOG10E",
            "standard",
            ConstantValue::Float(constants::M_LOG10E),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "M_LN2",
            "standard",
            ConstantValue::Float(constants::M_LN2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "M_LN10",
            "standard",
            ConstantValue::Float(constants::M_LN10),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "M_PI",
            "standard",
            ConstantValue::Float(constants::M_PI),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "M_PI_2",
            "standard",
            ConstantValue::Float(constants::M_PI_2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "M_PI_4",
            "standard",
            ConstantValue::Float(constants::M_PI_4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "M_1_PI",
            "standard",
            ConstantValue::Float(constants::M_1_PI),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "M_2_PI",
            "standard",
            ConstantValue::Float(constants::M_2_PI),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "M_SQRTPI",
            "standard",
            ConstantValue::Float(constants::M_SQRTPI),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "M_2_SQRTPI",
            "standard",
            ConstantValue::Float(constants::M_2_SQRTPI),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "M_LNPI",
            "standard",
            ConstantValue::Float(constants::M_LNPI),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "M_EULER",
            "standard",
            ConstantValue::Float(constants::M_EULER),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "M_SQRT2",
            "standard",
            ConstantValue::Float(constants::M_SQRT2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "M_SQRT1_2",
            "standard",
            ConstantValue::Float(constants::M_SQRT1_2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "M_SQRT3",
            "standard",
            ConstantValue::Float(constants::M_SQRT3),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_ROUND_HALF_UP",
            "standard",
            ConstantValue::Int(constants::PHP_ROUND_HALF_UP),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_ROUND_HALF_DOWN",
            "standard",
            ConstantValue::Int(constants::PHP_ROUND_HALF_DOWN),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_ROUND_HALF_EVEN",
            "standard",
            ConstantValue::Int(constants::PHP_ROUND_HALF_EVEN),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_ROUND_HALF_ODD",
            "standard",
            ConstantValue::Int(constants::PHP_ROUND_HALF_ODD),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILE_APPEND",
            "standard",
            ConstantValue::Int(constants::FILE_APPEND),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILE_USE_INCLUDE_PATH",
            "standard",
            ConstantValue::Int(constants::FILE_USE_INCLUDE_PATH),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILE_IGNORE_NEW_LINES",
            "standard",
            ConstantValue::Int(constants::FILE_IGNORE_NEW_LINES),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILE_SKIP_EMPTY_LINES",
            "standard",
            ConstantValue::Int(constants::FILE_SKIP_EMPTY_LINES),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILE_NO_DEFAULT_CONTEXT",
            "standard",
            ConstantValue::Int(constants::FILE_NO_DEFAULT_CONTEXT),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LOCK_SH",
            "standard",
            ConstantValue::Int(constants::LOCK_SH),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LOCK_EX",
            "standard",
            ConstantValue::Int(constants::LOCK_EX),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LOCK_UN",
            "standard",
            ConstantValue::Int(constants::LOCK_UN),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LOCK_NB",
            "standard",
            ConstantValue::Int(constants::LOCK_NB),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SEEK_SET",
            "standard",
            ConstantValue::Int(constants::SEEK_SET),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SEEK_CUR",
            "standard",
            ConstantValue::Int(constants::SEEK_CUR),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SEEK_END",
            "standard",
            ConstantValue::Int(constants::SEEK_END),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "GLOB_BRACE",
            "standard",
            ConstantValue::Int(constants::GLOB_BRACE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "GLOB_MARK",
            "standard",
            ConstantValue::Int(constants::GLOB_MARK),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "GLOB_NOSORT",
            "standard",
            ConstantValue::Int(constants::GLOB_NOSORT),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "GLOB_NOCHECK",
            "standard",
            ConstantValue::Int(constants::GLOB_NOCHECK),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "GLOB_NOESCAPE",
            "standard",
            ConstantValue::Int(constants::GLOB_NOESCAPE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "GLOB_ERR",
            "standard",
            ConstantValue::Int(constants::GLOB_ERR),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "GLOB_ONLYDIR",
            "standard",
            ConstantValue::Int(constants::GLOB_ONLYDIR),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PATHINFO_DIRNAME",
            "standard",
            ConstantValue::Int(constants::PATHINFO_DIRNAME),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PATHINFO_BASENAME",
            "standard",
            ConstantValue::Int(constants::PATHINFO_BASENAME),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PATHINFO_EXTENSION",
            "standard",
            ConstantValue::Int(constants::PATHINFO_EXTENSION),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PATHINFO_FILENAME",
            "standard",
            ConstantValue::Int(constants::PATHINFO_FILENAME),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "INI_USER",
            "standard",
            ConstantValue::Int(constants::INI_USER),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "INI_PERDIR",
            "standard",
            ConstantValue::Int(constants::INI_PERDIR),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "INI_SYSTEM",
            "standard",
            ConstantValue::Int(constants::INI_SYSTEM),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "INI_ALL",
            "standard",
            ConstantValue::Int(constants::INI_ALL),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "INI_SCANNER_NORMAL",
            "standard",
            ConstantValue::Int(constants::INI_SCANNER_NORMAL),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "INI_SCANNER_RAW",
            "standard",
            ConstantValue::Int(constants::INI_SCANNER_RAW),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "INI_SCANNER_TYPED",
            "standard",
            ConstantValue::Int(constants::INI_SCANNER_TYPED),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FNM_NOESCAPE",
            "standard",
            ConstantValue::Int(constants::FNM_NOESCAPE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FNM_PATHNAME",
            "standard",
            ConstantValue::Int(constants::FNM_PATHNAME),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FNM_PERIOD",
            "standard",
            ConstantValue::Int(constants::FNM_PERIOD),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FNM_CASEFOLD",
            "standard",
            ConstantValue::Int(constants::FNM_CASEFOLD),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "HTML_ENTITIES",
            "standard",
            ConstantValue::Int(constants::HTML_ENTITIES),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ENT_COMPAT",
            "standard",
            ConstantValue::Int(constants::ENT_COMPAT),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ENT_QUOTES",
            "standard",
            ConstantValue::Int(constants::ENT_QUOTES),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ENT_NOQUOTES",
            "standard",
            ConstantValue::Int(constants::ENT_NOQUOTES),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ENT_IGNORE",
            "standard",
            ConstantValue::Int(constants::ENT_IGNORE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ENT_SUBSTITUTE",
            "standard",
            ConstantValue::Int(constants::ENT_SUBSTITUTE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ENT_DISALLOWED",
            "standard",
            ConstantValue::Int(constants::ENT_DISALLOWED),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ENT_HTML401",
            "standard",
            ConstantValue::Int(constants::ENT_HTML401),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ENT_XML1",
            "standard",
            ConstantValue::Int(constants::ENT_XML1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ENT_XHTML",
            "standard",
            ConstantValue::Int(constants::ENT_XHTML),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ENT_HTML5",
            "standard",
            ConstantValue::Int(constants::ENT_HTML5),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CHAR_MAX",
            "standard",
            ConstantValue::Int(constants::CHAR_MAX),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "HTML_SPECIALCHARS",
            "standard",
            ConstantValue::Int(constants::HTML_SPECIALCHARS),
        ))
        .with_class(ClassDescriptor::new(
            "RoundingMode",
            "standard",
            ClassKind::Enum,
        ))
        .with_function(FunctionDescriptor::php("abs", "standard"))
        .with_function(FunctionDescriptor::php("acos", "standard"))
        .with_function(FunctionDescriptor::php("acosh", "standard"))
        .with_function(FunctionDescriptor::php("array_all", "standard"))
        .with_function(FunctionDescriptor::php("array_any", "standard"))
        .with_function(FunctionDescriptor::php("array_chunk", "standard"))
        .with_function(FunctionDescriptor::php("array_column", "standard"))
        .with_function(FunctionDescriptor::php("array_filter", "standard"))
        .with_function(FunctionDescriptor::php("array_fill", "standard"))
        .with_function(FunctionDescriptor::php("array_find", "standard"))
        .with_function(FunctionDescriptor::php("array_find_key", "standard"))
        .with_function(FunctionDescriptor::php("array_flip", "standard"))
        .with_function(FunctionDescriptor::php("array_is_list", "standard"))
        .with_function(FunctionDescriptor::php("array_key_exists", "standard"))
        .with_function(FunctionDescriptor::php("array_key_first", "standard"))
        .with_function(FunctionDescriptor::php("array_key_last", "standard"))
        .with_function(FunctionDescriptor::php("array_keys", "standard"))
        .with_function(FunctionDescriptor::php("array_map", "standard"))
        .with_function(FunctionDescriptor::php("array_merge", "standard"))
        .with_function(FunctionDescriptor::php("array_merge_recursive", "standard"))
        .with_function(FunctionDescriptor::php("array_pad", "standard"))
        .with_function(FunctionDescriptor::php("array_pop", "standard"))
        .with_function(FunctionDescriptor::php("array_push", "standard"))
        .with_function(FunctionDescriptor::php("array_rand", "standard"))
        .with_function(FunctionDescriptor::php("array_reduce", "standard"))
        .with_function(FunctionDescriptor::php("array_replace", "standard"))
        .with_function(FunctionDescriptor::php(
            "array_replace_recursive",
            "standard",
        ))
        .with_function(FunctionDescriptor::php("array_reverse", "standard"))
        .with_function(FunctionDescriptor::php("array_search", "standard"))
        .with_function(FunctionDescriptor::php("array_shift", "standard"))
        .with_function(FunctionDescriptor::php("array_slice", "standard"))
        .with_function(FunctionDescriptor::php("array_splice", "standard"))
        .with_function(FunctionDescriptor::php("array_unshift", "standard"))
        .with_function(FunctionDescriptor::php("array_values", "standard"))
        .with_function(FunctionDescriptor::php("array_walk", "standard"))
        .with_function(FunctionDescriptor::php("array_walk_recursive", "standard"))
        .with_function(FunctionDescriptor::php("arsort", "standard"))
        .with_function(FunctionDescriptor::php("asin", "standard"))
        .with_function(FunctionDescriptor::php("asinh", "standard"))
        .with_function(FunctionDescriptor::php("asort", "standard"))
        .with_function(FunctionDescriptor::php("atan", "standard"))
        .with_function(FunctionDescriptor::php("atan2", "standard"))
        .with_function(FunctionDescriptor::php("atanh", "standard"))
        .with_function(FunctionDescriptor::php("base64_decode", "standard"))
        .with_function(FunctionDescriptor::php("base64_encode", "standard"))
        .with_function(FunctionDescriptor::php("base_convert", "standard"))
        .with_function(FunctionDescriptor::php("basename", "standard"))
        .with_function(FunctionDescriptor::php("bin2hex", "standard"))
        .with_function(FunctionDescriptor::php("bindec", "standard"))
        .with_function(FunctionDescriptor::php("boolval", "standard"))
        .with_function(FunctionDescriptor::php("ceil", "standard"))
        .with_function(FunctionDescriptor::php("chdir", "standard"))
        .with_function(FunctionDescriptor::php("chmod", "standard"))
        .with_function(FunctionDescriptor::php("chr", "standard"))
        .with_function(FunctionDescriptor::php("class_exists", "standard"))
        .with_function(FunctionDescriptor::php("call_user_func", "standard"))
        .with_function(FunctionDescriptor::php("call_user_func_array", "standard"))
        .with_function(FunctionDescriptor::php("clearstatcache", "standard"))
        .with_function(FunctionDescriptor::php("closedir", "standard"))
        .with_function(FunctionDescriptor::php("constant", "standard"))
        .with_function(FunctionDescriptor::php("copy", "standard"))
        .with_function(FunctionDescriptor::php("cos", "standard"))
        .with_function(FunctionDescriptor::php("cosh", "standard"))
        .with_function(FunctionDescriptor::php("count", "standard"))
        .with_function(FunctionDescriptor::php("crc32", "standard"))
        .with_function(FunctionDescriptor::php("debug_backtrace", "standard"))
        .with_function(FunctionDescriptor::php("debug_print_backtrace", "standard"))
        .with_function(FunctionDescriptor::php("decbin", "standard"))
        .with_function(FunctionDescriptor::php("dechex", "standard"))
        .with_function(FunctionDescriptor::php("decoct", "standard"))
        .with_function(FunctionDescriptor::php("deg2rad", "standard"))
        .with_function(FunctionDescriptor::php("define", "standard"))
        .with_function(FunctionDescriptor::php("defined", "standard"))
        .with_function(FunctionDescriptor::php("dirname", "standard"))
        .with_function(FunctionDescriptor::php("dir", "standard"))
        .with_function(FunctionDescriptor::php("disk_free_space", "standard"))
        .with_function(FunctionDescriptor::php("disk_total_space", "standard"))
        .with_function(FunctionDescriptor::php("enum_exists", "standard"))
        .with_function(FunctionDescriptor::php("error_reporting", "standard"))
        .with_function(FunctionDescriptor::php("exec", "standard"))
        .with_function(FunctionDescriptor::php("exp", "standard"))
        .with_function(FunctionDescriptor::php("expm1", "standard"))
        .with_function(FunctionDescriptor::php("explode", "standard"))
        .with_function(FunctionDescriptor::php("extension_loaded", "standard"))
        .with_function(FunctionDescriptor::php("fclose", "standard"))
        .with_function(FunctionDescriptor::php("feof", "standard"))
        .with_function(FunctionDescriptor::php("fflush", "standard"))
        .with_function(FunctionDescriptor::php("fgetc", "standard"))
        .with_function(FunctionDescriptor::php("fgets", "standard"))
        .with_function(FunctionDescriptor::php("file_exists", "standard"))
        .with_function(FunctionDescriptor::php("file_get_contents", "standard"))
        .with_function(FunctionDescriptor::php("file_put_contents", "standard"))
        .with_function(FunctionDescriptor::php("filegroup", "standard"))
        .with_function(FunctionDescriptor::php("filemtime", "standard"))
        .with_function(FunctionDescriptor::php("fileowner", "standard"))
        .with_function(FunctionDescriptor::php("fileperms", "standard"))
        .with_function(FunctionDescriptor::php("filesize", "standard"))
        .with_function(FunctionDescriptor::php("filetype", "standard"))
        .with_function(FunctionDescriptor::php("floor", "standard"))
        .with_function(FunctionDescriptor::php("floatval", "standard"))
        .with_function(FunctionDescriptor::php("flush", "standard"))
        .with_function(FunctionDescriptor::php("fdiv", "standard"))
        .with_function(FunctionDescriptor::php("fmod", "standard"))
        .with_function(FunctionDescriptor::php("fopen", "standard"))
        .with_function(FunctionDescriptor::php("fpow", "standard"))
        .with_function(FunctionDescriptor::php("fprintf", "standard"))
        .with_function(FunctionDescriptor::php("fread", "standard"))
        .with_function(FunctionDescriptor::php("fseek", "standard"))
        .with_function(FunctionDescriptor::php("ftell", "standard"))
        .with_function(FunctionDescriptor::php("function_exists", "standard"))
        .with_function(FunctionDescriptor::php("forward_static_call", "standard"))
        .with_function(FunctionDescriptor::php("func_get_arg", "standard"))
        .with_function(FunctionDescriptor::php("func_get_args", "standard"))
        .with_function(FunctionDescriptor::php("func_num_args", "standard"))
        .with_function(FunctionDescriptor::php("fwrite", "standard"))
        .with_function(FunctionDescriptor::php("get_current_user", "standard"))
        .with_function(FunctionDescriptor::php("get_cfg_var", "standard"))
        .with_function(FunctionDescriptor::php("get_called_class", "standard"))
        .with_function(FunctionDescriptor::php("get_class", "standard"))
        .with_function(FunctionDescriptor::php("get_class_methods", "standard"))
        .with_function(FunctionDescriptor::php("get_class_vars", "standard"))
        .with_function(FunctionDescriptor::php("get_debug_type", "standard"))
        .with_function(FunctionDescriptor::php("get_declared_classes", "standard"))
        .with_function(FunctionDescriptor::php(
            "get_declared_interfaces",
            "standard",
        ))
        .with_function(FunctionDescriptor::php("get_declared_traits", "standard"))
        .with_function(FunctionDescriptor::php("get_loaded_extensions", "standard"))
        .with_function(FunctionDescriptor::php(
            "get_mangled_object_vars",
            "standard",
        ))
        .with_function(FunctionDescriptor::php("get_object_vars", "standard"))
        .with_function(FunctionDescriptor::php("get_parent_class", "standard"))
        .with_function(FunctionDescriptor::php("getrandmax", "standard"))
        .with_function(FunctionDescriptor::php("get_resource_id", "standard"))
        .with_function(FunctionDescriptor::php("get_resource_type", "standard"))
        .with_function(FunctionDescriptor::php("getimagesize", "standard"))
        .with_function(FunctionDescriptor::php(
            "getimagesizefromstring",
            "standard",
        ))
        .with_function(FunctionDescriptor::php("getcwd", "standard"))
        .with_function(FunctionDescriptor::php("getenv", "standard"))
        .with_function(FunctionDescriptor::php("gettype", "standard"))
        .with_function(FunctionDescriptor::php("glob", "standard"))
        .with_function(FunctionDescriptor::php("header", "standard"))
        .with_function(FunctionDescriptor::php("header_remove", "standard"))
        .with_function(FunctionDescriptor::php("headers_list", "standard"))
        .with_function(FunctionDescriptor::php("headers_sent", "standard"))
        .with_function(FunctionDescriptor::php("hex2bin", "standard"))
        .with_function(FunctionDescriptor::php("hexdec", "standard"))
        .with_function(FunctionDescriptor::php("htmlentities", "standard"))
        .with_function(FunctionDescriptor::php("htmlspecialchars", "standard"))
        .with_function(FunctionDescriptor::php(
            "htmlspecialchars_decode",
            "standard",
        ))
        .with_function(FunctionDescriptor::php("hypot", "standard"))
        .with_function(FunctionDescriptor::php("hrtime", "standard"))
        .with_function(FunctionDescriptor::php("http_build_query", "standard"))
        .with_function(FunctionDescriptor::php("http_response_code", "standard"))
        .with_function(FunctionDescriptor::php("implode", "standard"))
        .with_function(FunctionDescriptor::php("in_array", "standard"))
        .with_function(FunctionDescriptor::php("ini_get", "standard"))
        .with_function(FunctionDescriptor::php("ini_get_all", "standard"))
        .with_function(FunctionDescriptor::php("ini_set", "standard"))
        .with_function(FunctionDescriptor::php("intdiv", "standard"))
        .with_function(FunctionDescriptor::php("interface_exists", "standard"))
        .with_function(FunctionDescriptor::php("is_a", "core"))
        .with_function(FunctionDescriptor::php("intval", "standard"))
        .with_function(FunctionDescriptor::php("is_array", "standard"))
        .with_function(FunctionDescriptor::php("is_bool", "standard"))
        .with_function(FunctionDescriptor::php("is_countable", "standard"))
        .with_function(FunctionDescriptor::php("is_dir", "standard"))
        .with_function(FunctionDescriptor::php("is_file", "standard"))
        .with_function(FunctionDescriptor::php("is_finite", "standard"))
        .with_function(FunctionDescriptor::php("is_float", "standard"))
        .with_function(FunctionDescriptor::php("is_infinite", "standard"))
        .with_function(FunctionDescriptor::php("is_int", "standard"))
        .with_function(FunctionDescriptor::php("is_iterable", "standard"))
        .with_function(FunctionDescriptor::php("is_link", "standard"))
        .with_function(FunctionDescriptor::php("is_nan", "standard"))
        .with_function(FunctionDescriptor::php("is_null", "standard"))
        .with_function(FunctionDescriptor::php("is_object", "standard"))
        .with_function(FunctionDescriptor::php("is_readable", "standard"))
        .with_function(FunctionDescriptor::php("is_resource", "standard"))
        .with_function(FunctionDescriptor::php("is_scalar", "standard"))
        .with_function(FunctionDescriptor::php("is_string", "standard"))
        .with_function(FunctionDescriptor::php("is_subclass_of", "standard"))
        .with_function(FunctionDescriptor::php("is_writable", "standard"))
        .with_function(FunctionDescriptor::php("krsort", "standard"))
        .with_function(FunctionDescriptor::php("ksort", "standard"))
        .with_function(FunctionDescriptor::php("lcfirst", "standard"))
        .with_function(FunctionDescriptor::php("lstat", "standard"))
        .with_function(FunctionDescriptor::php("log", "standard"))
        .with_function(FunctionDescriptor::php("log10", "standard"))
        .with_function(FunctionDescriptor::php("log1p", "standard"))
        .with_function(FunctionDescriptor::php("ltrim", "standard"))
        .with_function(FunctionDescriptor::php("max", "standard"))
        .with_function(FunctionDescriptor::php("md5", "standard"))
        .with_function(FunctionDescriptor::php("memory_get_peak_usage", "standard"))
        .with_function(FunctionDescriptor::php("memory_get_usage", "standard"))
        .with_function(FunctionDescriptor::php("method_exists", "standard"))
        .with_function(FunctionDescriptor::php("min", "standard"))
        .with_function(FunctionDescriptor::php("mkdir", "standard"))
        .with_function(FunctionDescriptor::php("mime_content_type", "standard"))
        .with_function(FunctionDescriptor::php("natcasesort", "standard"))
        .with_function(FunctionDescriptor::php("natsort", "standard"))
        .with_function(FunctionDescriptor::php("number_format", "standard"))
        .with_function(FunctionDescriptor::php("ob_end_clean", "standard"))
        .with_function(FunctionDescriptor::php("ob_end_flush", "standard"))
        .with_function(FunctionDescriptor::php("ob_get_clean", "standard"))
        .with_function(FunctionDescriptor::php("ob_get_contents", "standard"))
        .with_function(FunctionDescriptor::php("ob_get_length", "standard"))
        .with_function(FunctionDescriptor::php("ob_get_level", "standard"))
        .with_function(FunctionDescriptor::php("ob_start", "standard"))
        .with_function(FunctionDescriptor::php("octdec", "standard"))
        .with_function(FunctionDescriptor::php("opendir", "standard"))
        .with_function(FunctionDescriptor::php("ord", "standard"))
        .with_function(FunctionDescriptor::php("pathinfo", "standard"))
        .with_function(FunctionDescriptor::php("parse_str", "standard"))
        .with_function(FunctionDescriptor::php("parse_url", "standard"))
        .with_function(FunctionDescriptor::php("passthru", "standard"))
        .with_function(FunctionDescriptor::php("pclose", "standard"))
        .with_function(FunctionDescriptor::php("php_sapi_name", "standard"))
        .with_function(FunctionDescriptor::php("php_uname", "standard"))
        .with_function(FunctionDescriptor::php("phpversion", "standard"))
        .with_function(FunctionDescriptor::php("password_hash", "standard"))
        .with_function(FunctionDescriptor::php("password_needs_rehash", "standard"))
        .with_function(FunctionDescriptor::php("password_verify", "standard"))
        .with_function(FunctionDescriptor::php("pi", "standard"))
        .with_function(FunctionDescriptor::php("popen", "standard"))
        .with_function(FunctionDescriptor::php("print", "standard"))
        .with_function(FunctionDescriptor::php("print_r", "standard"))
        .with_function(FunctionDescriptor::php("printf", "standard"))
        .with_function(FunctionDescriptor::php("pow", "standard"))
        .with_function(FunctionDescriptor::php("property_exists", "standard"))
        .with_function(FunctionDescriptor::php("proc_close", "standard"))
        .with_function(FunctionDescriptor::php("proc_get_status", "standard"))
        .with_function(FunctionDescriptor::php("proc_open", "standard"))
        .with_function(FunctionDescriptor::php("putenv", "standard"))
        .with_function(FunctionDescriptor::php("rad2deg", "standard"))
        .with_function(FunctionDescriptor::php("rawurldecode", "standard"))
        .with_function(FunctionDescriptor::php("rawurlencode", "standard"))
        .with_function(FunctionDescriptor::php("range", "standard"))
        .with_function(FunctionDescriptor::php("readdir", "standard"))
        .with_function(FunctionDescriptor::php("readfile", "standard"))
        .with_function(FunctionDescriptor::php("realpath", "standard"))
        .with_function(FunctionDescriptor::php("rename", "standard"))
        .with_function(FunctionDescriptor::php("restore_error_handler", "standard"))
        .with_function(FunctionDescriptor::php(
            "restore_exception_handler",
            "standard",
        ))
        .with_function(FunctionDescriptor::php("rewind", "standard"))
        .with_function(FunctionDescriptor::php("rewinddir", "standard"))
        .with_function(FunctionDescriptor::php("rmdir", "standard"))
        .with_function(FunctionDescriptor::php("round", "standard"))
        .with_function(FunctionDescriptor::php("rsort", "standard"))
        .with_function(FunctionDescriptor::php("rtrim", "standard"))
        .with_function(FunctionDescriptor::php("scandir", "standard"))
        .with_function(FunctionDescriptor::php("serialize", "standard"))
        .with_function(FunctionDescriptor::php("set_error_handler", "standard"))
        .with_function(FunctionDescriptor::php("set_exception_handler", "standard"))
        .with_function(FunctionDescriptor::php("setcookie", "standard"))
        .with_function(FunctionDescriptor::php("setrawcookie", "standard"))
        .with_function(FunctionDescriptor::php("set_time_limit", "standard"))
        .with_function(FunctionDescriptor::php("sha1", "standard"))
        .with_function(FunctionDescriptor::php("shell_exec", "standard"))
        .with_function(FunctionDescriptor::php("sin", "standard"))
        .with_function(FunctionDescriptor::php("sinh", "standard"))
        .with_function(FunctionDescriptor::php("sizeof", "standard"))
        .with_function(FunctionDescriptor::php("sort", "standard"))
        .with_function(FunctionDescriptor::php("sprintf", "standard"))
        .with_function(FunctionDescriptor::php("sqrt", "standard"))
        .with_function(FunctionDescriptor::php("stat", "standard"))
        .with_function(FunctionDescriptor::php("symlink", "standard"))
        .with_function(FunctionDescriptor::php("stream_context_create", "standard"))
        .with_function(FunctionDescriptor::php(
            "stream_context_get_default",
            "standard",
        ))
        .with_function(FunctionDescriptor::php(
            "stream_context_get_options",
            "standard",
        ))
        .with_function(FunctionDescriptor::php(
            "stream_context_set_default",
            "standard",
        ))
        .with_function(FunctionDescriptor::php(
            "stream_context_set_option",
            "standard",
        ))
        .with_function(FunctionDescriptor::php("stream_copy_to_stream", "standard"))
        .with_function(FunctionDescriptor::php("stream_get_contents", "standard"))
        .with_function(FunctionDescriptor::php("stream_get_meta_data", "standard"))
        .with_function(FunctionDescriptor::php("stream_get_wrappers", "standard"))
        .with_function(FunctionDescriptor::php("stream_is_local", "standard"))
        .with_function(FunctionDescriptor::php("stream_isatty", "standard"))
        .with_function(FunctionDescriptor::php("stream_set_timeout", "standard"))
        .with_function(FunctionDescriptor::php(
            "stream_resolve_include_path",
            "standard",
        ))
        .with_function(FunctionDescriptor::php("str_contains", "standard"))
        .with_function(FunctionDescriptor::php("str_ends_with", "standard"))
        .with_function(FunctionDescriptor::php("str_pad", "standard"))
        .with_function(FunctionDescriptor::php("str_repeat", "standard"))
        .with_function(FunctionDescriptor::php("str_replace", "standard"))
        .with_function(FunctionDescriptor::php("str_starts_with", "standard"))
        .with_function(FunctionDescriptor::php("strcasecmp", "standard"))
        .with_function(FunctionDescriptor::php("strcmp", "standard"))
        .with_function(FunctionDescriptor::php("stripos", "standard"))
        .with_function(FunctionDescriptor::php("strlen", "standard"))
        .with_function(FunctionDescriptor::php("strncasecmp", "standard"))
        .with_function(FunctionDescriptor::php("strncmp", "standard"))
        .with_function(FunctionDescriptor::php("strpos", "standard"))
        .with_function(FunctionDescriptor::php("strrev", "standard"))
        .with_function(FunctionDescriptor::php("strrpos", "standard"))
        .with_function(FunctionDescriptor::php("strtolower", "standard"))
        .with_function(FunctionDescriptor::php("strval", "standard"))
        .with_function(FunctionDescriptor::php("strtoupper", "standard"))
        .with_function(FunctionDescriptor::php("strtr", "standard"))
        .with_function(FunctionDescriptor::php("substr", "standard"))
        .with_function(FunctionDescriptor::php("system", "standard"))
        .with_function(FunctionDescriptor::php("sys_get_temp_dir", "standard"))
        .with_function(FunctionDescriptor::php("tan", "standard"))
        .with_function(FunctionDescriptor::php("tanh", "standard"))
        .with_function(FunctionDescriptor::php("tempnam", "standard"))
        .with_function(FunctionDescriptor::php("tmpfile", "standard"))
        .with_function(FunctionDescriptor::php("touch", "standard"))
        .with_function(FunctionDescriptor::php("trim", "standard"))
        .with_function(FunctionDescriptor::php("trigger_error", "standard"))
        .with_function(FunctionDescriptor::php("trait_exists", "standard"))
        .with_function(FunctionDescriptor::php("uasort", "standard"))
        .with_function(FunctionDescriptor::php("uksort", "standard"))
        .with_function(FunctionDescriptor::php("umask", "standard"))
        .with_function(FunctionDescriptor::php("unlink", "standard"))
        .with_function(FunctionDescriptor::php("unserialize", "standard"))
        .with_function(FunctionDescriptor::php("urldecode", "standard"))
        .with_function(FunctionDescriptor::php("urlencode", "standard"))
        .with_function(FunctionDescriptor::php("usort", "standard"))
        .with_function(FunctionDescriptor::php("ucfirst", "standard"))
        .with_function(FunctionDescriptor::php("ucwords", "standard"))
        .with_function(FunctionDescriptor::php("user_error", "standard"))
        .with_function(FunctionDescriptor::php("var_dump", "standard"))
        .with_function(FunctionDescriptor::php("var_export", "standard"))
        .with_function(FunctionDescriptor::php("version_compare", "standard"))
        .with_function(FunctionDescriptor::php("vprintf", "standard"))
        .with_function(FunctionDescriptor::php("vsprintf", "standard"))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_URL_SCHEME",
            "standard",
            ConstantValue::Int(constants::PHP_URL_SCHEME),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_URL_HOST",
            "standard",
            ConstantValue::Int(constants::PHP_URL_HOST),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_URL_PORT",
            "standard",
            ConstantValue::Int(constants::PHP_URL_PORT),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_URL_USER",
            "standard",
            ConstantValue::Int(constants::PHP_URL_USER),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_URL_PASS",
            "standard",
            ConstantValue::Int(constants::PHP_URL_PASS),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_URL_PATH",
            "standard",
            ConstantValue::Int(constants::PHP_URL_PATH),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_URL_QUERY",
            "standard",
            ConstantValue::Int(constants::PHP_URL_QUERY),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_URL_FRAGMENT",
            "standard",
            ConstantValue::Int(constants::PHP_URL_FRAGMENT),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PASSWORD_DEFAULT",
            "standard",
            ConstantValue::String(constants::PASSWORD_DEFAULT),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PASSWORD_BCRYPT",
            "standard",
            ConstantValue::String(constants::PASSWORD_BCRYPT),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PASSWORD_BCRYPT_DEFAULT_COST",
            "standard",
            ConstantValue::Int(constants::PASSWORD_BCRYPT_DEFAULT_COST),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_GIF",
            "standard",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_JPEG",
            "standard",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_PNG",
            "standard",
            ConstantValue::Int(3),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_WEBP",
            "standard",
            ConstantValue::Int(18),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_AVIF",
            "standard",
            ConstantValue::Int(19),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "UPLOAD_ERR_OK",
            "standard",
            ConstantValue::Int(constants::UPLOAD_ERR_OK),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "UPLOAD_ERR_INI_SIZE",
            "standard",
            ConstantValue::Int(constants::UPLOAD_ERR_INI_SIZE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "UPLOAD_ERR_FORM_SIZE",
            "standard",
            ConstantValue::Int(constants::UPLOAD_ERR_FORM_SIZE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "UPLOAD_ERR_PARTIAL",
            "standard",
            ConstantValue::Int(constants::UPLOAD_ERR_PARTIAL),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "UPLOAD_ERR_NO_FILE",
            "standard",
            ConstantValue::Int(constants::UPLOAD_ERR_NO_FILE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "UPLOAD_ERR_NO_TMP_DIR",
            "standard",
            ConstantValue::Int(constants::UPLOAD_ERR_NO_TMP_DIR),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "UPLOAD_ERR_CANT_WRITE",
            "standard",
            ConstantValue::Int(constants::UPLOAD_ERR_CANT_WRITE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "UPLOAD_ERR_EXTENSION",
            "standard",
            ConstantValue::Int(constants::UPLOAD_ERR_EXTENSION),
        ))
}

pub(super) fn standard_library_json_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("json")
        .with_function(FunctionDescriptor::php("json_decode", "json"))
        .with_function(FunctionDescriptor::php("json_encode", "json"))
        .with_function(FunctionDescriptor::php("json_last_error", "json"))
        .with_function(FunctionDescriptor::php("json_last_error_msg", "json"))
        .with_function(FunctionDescriptor::php("json_validate", "json"))
        .with_class(ClassDescriptor::new(
            "JsonException",
            "json",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "JsonSerializable",
            "json",
            ClassKind::Interface,
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_BIGINT_AS_STRING",
            "json",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_HEX_TAG",
            "json",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_HEX_AMP",
            "json",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_HEX_APOS",
            "json",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_HEX_QUOT",
            "json",
            ConstantValue::Int(8),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_FORCE_OBJECT",
            "json",
            ConstantValue::Int(16),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_NUMERIC_CHECK",
            "json",
            ConstantValue::Int(32),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_ERROR_DEPTH",
            "json",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_ERROR_NONE",
            "json",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_ERROR_STATE_MISMATCH",
            "json",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_ERROR_CTRL_CHAR",
            "json",
            ConstantValue::Int(3),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_ERROR_SYNTAX",
            "json",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_ERROR_UTF8",
            "json",
            ConstantValue::Int(5),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_ERROR_RECURSION",
            "json",
            ConstantValue::Int(6),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_ERROR_INF_OR_NAN",
            "json",
            ConstantValue::Int(7),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_ERROR_UNSUPPORTED_TYPE",
            "json",
            ConstantValue::Int(8),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_ERROR_INVALID_PROPERTY_NAME",
            "json",
            ConstantValue::Int(9),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_ERROR_UTF16",
            "json",
            ConstantValue::Int(10),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_OBJECT_AS_ARRAY",
            "json",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_PRETTY_PRINT",
            "json",
            ConstantValue::Int(128),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_PRESERVE_ZERO_FRACTION",
            "json",
            ConstantValue::Int(1024),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_PARTIAL_OUTPUT_ON_ERROR",
            "json",
            ConstantValue::Int(512),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_THROW_ON_ERROR",
            "json",
            ConstantValue::Int(4_194_304),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_UNESCAPED_SLASHES",
            "json",
            ConstantValue::Int(64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_UNESCAPED_UNICODE",
            "json",
            ConstantValue::Int(256),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_UNESCAPED_LINE_TERMINATORS",
            "json",
            ConstantValue::Int(2048),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_INVALID_UTF8_IGNORE",
            "json",
            ConstantValue::Int(1_048_576),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "JSON_INVALID_UTF8_SUBSTITUTE",
            "json",
            ConstantValue::Int(2_097_152),
        ))
}

pub(super) fn standard_library_pcre_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("pcre")
        .with_function(FunctionDescriptor::php("preg_grep", "pcre"))
        .with_function(FunctionDescriptor::php("preg_last_error", "pcre"))
        .with_function(FunctionDescriptor::php("preg_last_error_msg", "pcre"))
        .with_function(FunctionDescriptor::php("preg_match", "pcre"))
        .with_function(FunctionDescriptor::php("preg_match_all", "pcre"))
        .with_function(FunctionDescriptor::php("preg_quote", "pcre"))
        .with_function(FunctionDescriptor::php("preg_replace", "pcre"))
        .with_function(FunctionDescriptor::php("preg_replace_callback", "pcre"))
        .with_function(FunctionDescriptor::php("preg_split", "pcre"))
        .with_constant(ConstantDescriptor::with_value(
            "PREG_BAD_UTF8_ERROR",
            "pcre",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PREG_BAD_UTF8_OFFSET_ERROR",
            "pcre",
            ConstantValue::Int(5),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PREG_BACKTRACK_LIMIT_ERROR",
            "pcre",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PREG_GREP_INVERT",
            "pcre",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PREG_INTERNAL_ERROR",
            "pcre",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PREG_JIT_STACKLIMIT_ERROR",
            "pcre",
            ConstantValue::Int(6),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PREG_NO_ERROR",
            "pcre",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PREG_OFFSET_CAPTURE",
            "pcre",
            ConstantValue::Int(256),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PREG_PATTERN_ORDER",
            "pcre",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PREG_RECURSION_LIMIT_ERROR",
            "pcre",
            ConstantValue::Int(3),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PREG_SET_ORDER",
            "pcre",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PREG_SPLIT_DELIM_CAPTURE",
            "pcre",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PREG_SPLIT_NO_EMPTY",
            "pcre",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PREG_SPLIT_OFFSET_CAPTURE",
            "pcre",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PREG_UNMATCHED_AS_NULL",
            "pcre",
            ConstantValue::Int(512),
        ))
}

pub(super) fn standard_library_session_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("session")
        .with_constant(ConstantDescriptor::with_value(
            "PHP_SESSION_DISABLED",
            "session",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_SESSION_NONE",
            "session",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_SESSION_ACTIVE",
            "session",
            ConstantValue::Int(2),
        ))
        .with_function(FunctionDescriptor::php("session_destroy", "session"))
        .with_function(FunctionDescriptor::php("session_cache_expire", "session"))
        .with_function(FunctionDescriptor::php("session_cache_limiter", "session"))
        .with_function(FunctionDescriptor::php("session_commit", "session"))
        .with_function(FunctionDescriptor::php("session_id", "session"))
        .with_function(FunctionDescriptor::php("session_module_name", "session"))
        .with_function(FunctionDescriptor::php("session_name", "session"))
        .with_function(FunctionDescriptor::php("session_save_path", "session"))
        .with_function(FunctionDescriptor::php("session_start", "session"))
        .with_function(FunctionDescriptor::php("session_status", "session"))
        .with_function(FunctionDescriptor::php("session_write_close", "session"))
}

pub(super) fn standard_library_pdo_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("pdo")
        .with_function(FunctionDescriptor::php("pdo_drivers", "pdo"))
        .with_class(ClassDescriptor::new("PDO", "pdo", ClassKind::Class))
        .with_class(ClassDescriptor::new(
            "PDOException",
            "pdo",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new("PDORow", "pdo", ClassKind::Class))
        .with_class(ClassDescriptor::new(
            "PDOStatement",
            "pdo",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_pdo_sqlite_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("pdo_sqlite").with_class(ClassDescriptor::new(
        "PDO_SQLite_Ext",
        "pdo_sqlite",
        ClassKind::Class,
    ))
}

pub(super) fn standard_library_mysqli_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("mysqli")
        .with_constant(ConstantDescriptor::with_value(
            "MYSQLI_ASSOC",
            "mysqli",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MYSQLI_NUM",
            "mysqli",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MYSQLI_BOTH",
            "mysqli",
            ConstantValue::Int(3),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MYSQLI_REPORT_OFF",
            "mysqli",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MYSQLI_REPORT_ERROR",
            "mysqli",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MYSQLI_REPORT_STRICT",
            "mysqli",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MYSQLI_REPORT_INDEX",
            "mysqli",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MYSQLI_OPT_CONNECT_TIMEOUT",
            "mysqli",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MYSQLI_INIT_COMMAND",
            "mysqli",
            ConstantValue::Int(3),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MYSQLI_READ_DEFAULT_FILE",
            "mysqli",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MYSQLI_READ_DEFAULT_GROUP",
            "mysqli",
            ConstantValue::Int(5),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MYSQLI_SET_CHARSET_NAME",
            "mysqli",
            ConstantValue::Int(7),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MYSQLI_STORE_RESULT",
            "mysqli",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MYSQLI_USE_RESULT",
            "mysqli",
            ConstantValue::Int(1),
        ))
        .with_function(FunctionDescriptor::php("mysqli_close", "mysqli"))
        .with_function(FunctionDescriptor::php(
            "mysqli_character_set_name",
            "mysqli",
        ))
        .with_function(FunctionDescriptor::php("mysqli_connect", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_connect_errno", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_connect_error", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_data_seek", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_errno", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_error", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_fetch_fields", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_fetch_object", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_field_count", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_get_charset", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_get_client_info", "mysqli"))
        .with_function(FunctionDescriptor::php(
            "mysqli_get_client_version",
            "mysqli",
        ))
        .with_function(FunctionDescriptor::php("mysqli_get_host_info", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_get_server_info", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_escape_string", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_fetch_array", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_fetch_assoc", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_fetch_row", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_free_result", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_init", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_num_fields", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_num_rows", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_options", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_prepare", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_query", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_real_connect", "mysqli"))
        .with_function(FunctionDescriptor::php(
            "mysqli_real_escape_string",
            "mysqli",
        ))
        .with_function(FunctionDescriptor::php("mysqli_report", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_select_db", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_set_charset", "mysqli"))
        .with_function(FunctionDescriptor::php(
            "mysqli_stmt_affected_rows",
            "mysqli",
        ))
        .with_function(FunctionDescriptor::php("mysqli_stmt_bind_param", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_stmt_bind_result", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_stmt_close", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_stmt_errno", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_stmt_error", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_stmt_execute", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_stmt_fetch", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_stmt_free_result", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_stmt_get_result", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_stmt_init", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_stmt_insert_id", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_stmt_num_rows", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_stmt_prepare", "mysqli"))
        .with_function(FunctionDescriptor::php("mysqli_stmt_sqlstate", "mysqli"))
        .with_class(ClassDescriptor::new("mysqli", "mysqli", ClassKind::Class))
        .with_class(ClassDescriptor::new(
            "mysqli_driver",
            "mysqli",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "mysqli_result",
            "mysqli",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "mysqli_stmt",
            "mysqli",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "mysqli_warning",
            "mysqli",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_curl_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("curl")
        .with_constant(ConstantDescriptor::with_value(
            "CURLM_OK",
            "curl",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLM_BAD_HANDLE",
            "curl",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_URL",
            "curl",
            ConstantValue::Int(10002),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_RETURNTRANSFER",
            "curl",
            ConstantValue::Int(19913),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_TIMEOUT",
            "curl",
            ConstantValue::Int(13),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_TIMEOUT_MS",
            "curl",
            ConstantValue::Int(155),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_FOLLOWLOCATION",
            "curl",
            ConstantValue::Int(52),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_HEADER",
            "curl",
            ConstantValue::Int(42),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_NOBODY",
            "curl",
            ConstantValue::Int(44),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_USERAGENT",
            "curl",
            ConstantValue::Int(10018),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_REFERER",
            "curl",
            ConstantValue::Int(10016),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_ENCODING",
            "curl",
            ConstantValue::Int(10102),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_HTTP_VERSION",
            "curl",
            ConstantValue::Int(84),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_CONNECTTIMEOUT",
            "curl",
            ConstantValue::Int(78),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_CONNECTTIMEOUT_MS",
            "curl",
            ConstantValue::Int(156),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_MAXREDIRS",
            "curl",
            ConstantValue::Int(68),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_FAILONERROR",
            "curl",
            ConstantValue::Int(45),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_HTTPHEADER",
            "curl",
            ConstantValue::Int(10023),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_POST",
            "curl",
            ConstantValue::Int(47),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_POSTFIELDS",
            "curl",
            ConstantValue::Int(10015),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_CUSTOMREQUEST",
            "curl",
            ConstantValue::Int(10036),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_SSL_VERIFYPEER",
            "curl",
            ConstantValue::Int(64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_SSL_VERIFYHOST",
            "curl",
            ConstantValue::Int(81),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLINFO_EFFECTIVE_URL",
            "curl",
            ConstantValue::Int(1048577),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLINFO_HTTP_CODE",
            "curl",
            ConstantValue::Int(2097154),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLINFO_RESPONSE_CODE",
            "curl",
            ConstantValue::Int(2097154),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLINFO_HEADER_SIZE",
            "curl",
            ConstantValue::Int(2097163),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLINFO_TOTAL_TIME",
            "curl",
            ConstantValue::Int(3145731),
        ))
        .with_function(FunctionDescriptor::php("curl_close", "curl"))
        .with_function(FunctionDescriptor::php("curl_copy_handle", "curl"))
        .with_function(FunctionDescriptor::php("curl_errno", "curl"))
        .with_function(FunctionDescriptor::php("curl_error", "curl"))
        .with_function(FunctionDescriptor::php("curl_escape", "curl"))
        .with_function(FunctionDescriptor::php("curl_exec", "curl"))
        .with_function(FunctionDescriptor::php("curl_getinfo", "curl"))
        .with_function(FunctionDescriptor::php("curl_init", "curl"))
        .with_function(FunctionDescriptor::php("curl_multi_add_handle", "curl"))
        .with_function(FunctionDescriptor::php("curl_multi_close", "curl"))
        .with_function(FunctionDescriptor::php("curl_multi_exec", "curl"))
        .with_function(FunctionDescriptor::php("curl_multi_init", "curl"))
        .with_function(FunctionDescriptor::php("curl_multi_strerror", "curl"))
        .with_function(FunctionDescriptor::php("curl_reset", "curl"))
        .with_function(FunctionDescriptor::php("curl_setopt", "curl"))
        .with_function(FunctionDescriptor::php("curl_setopt_array", "curl"))
        .with_function(FunctionDescriptor::php("curl_unescape", "curl"))
        .with_function(FunctionDescriptor::php("curl_version", "curl"))
        .with_class(ClassDescriptor::new("CurlHandle", "curl", ClassKind::Class))
        .with_class(ClassDescriptor::new(
            "CurlMultiHandle",
            "curl",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_openssl_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("openssl")
        .with_constant(ConstantDescriptor::with_value(
            "OPENSSL_ALGO_SHA1",
            "openssl",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "OPENSSL_ALGO_SHA256",
            "openssl",
            ConstantValue::Int(7),
        ))
        .with_function(FunctionDescriptor::php("openssl_digest", "openssl"))
        .with_function(FunctionDescriptor::php(
            "openssl_cipher_iv_length",
            "openssl",
        ))
        .with_function(FunctionDescriptor::php(
            "openssl_get_cipher_methods",
            "openssl",
        ))
        .with_function(FunctionDescriptor::php("openssl_get_md_methods", "openssl"))
        .with_function(FunctionDescriptor::php(
            "openssl_pkey_get_public",
            "openssl",
        ))
        .with_function(FunctionDescriptor::php("openssl_get_publickey", "openssl"))
        .with_function(FunctionDescriptor::php("openssl_error_string", "openssl"))
        .with_function(FunctionDescriptor::php(
            "openssl_random_pseudo_bytes",
            "openssl",
        ))
        .with_function(FunctionDescriptor::php("openssl_verify", "openssl"))
}

pub(super) fn standard_library_phar_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("phar")
        .with_class(ClassDescriptor::new("Phar", "phar", ClassKind::Class))
        .with_class(ClassDescriptor::new("PharData", "phar", ClassKind::Class))
        .with_class(ClassDescriptor::new(
            "PharFileInfo",
            "phar",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_sqlite3_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("sqlite3")
        .with_constant(ConstantDescriptor::with_value(
            "SQLITE3_ASSOC",
            "sqlite3",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SQLITE3_NUM",
            "sqlite3",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SQLITE3_BOTH",
            "sqlite3",
            ConstantValue::Int(3),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SQLITE3_INTEGER",
            "sqlite3",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SQLITE3_FLOAT",
            "sqlite3",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SQLITE3_TEXT",
            "sqlite3",
            ConstantValue::Int(3),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SQLITE3_BLOB",
            "sqlite3",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SQLITE3_NULL",
            "sqlite3",
            ConstantValue::Int(5),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SQLITE3_OPEN_READONLY",
            "sqlite3",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SQLITE3_OPEN_READWRITE",
            "sqlite3",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SQLITE3_OPEN_CREATE",
            "sqlite3",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SQLITE3_DETERMINISTIC",
            "sqlite3",
            ConstantValue::Int(2048),
        ))
        .with_class(ClassDescriptor::new("SQLite3", "sqlite3", ClassKind::Class))
        .with_class(ClassDescriptor::new(
            "SQLite3Exception",
            "sqlite3",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "SQLite3Result",
            "sqlite3",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "SQLite3Stmt",
            "sqlite3",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_mbstring_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("mbstring")
        .enabled_by_default(true)
        .with_function(FunctionDescriptor::php("mb_check_encoding", "mbstring"))
        .with_function(FunctionDescriptor::php("mb_convert_encoding", "mbstring"))
        .with_function(FunctionDescriptor::php("mb_detect_encoding", "mbstring"))
        .with_function(FunctionDescriptor::php("mb_internal_encoding", "mbstring"))
        .with_function(FunctionDescriptor::php("mb_strlen", "mbstring"))
        .with_function(FunctionDescriptor::php("mb_strtolower", "mbstring"))
        .with_function(FunctionDescriptor::php("mb_strtoupper", "mbstring"))
        .with_function(FunctionDescriptor::php("mb_strpos", "mbstring"))
        .with_function(FunctionDescriptor::php("mb_substr", "mbstring"))
}

pub(super) fn standard_library_intl_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("intl")
        .enabled_by_default(true)
        .with_constant(ConstantDescriptor::with_value(
            "INTL_ICU_DATA_VERSION",
            "intl",
            ConstantValue::String("76.1"),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "INTL_ICU_VERSION",
            "intl",
            ConstantValue::String("76.1"),
        ))
        .with_function(FunctionDescriptor::php("grapheme_substr", "intl"))
        .with_function(FunctionDescriptor::php("grapheme_strlen", "intl"))
        .with_function(FunctionDescriptor::php("intl_get_error_code", "intl"))
        .with_function(FunctionDescriptor::php("intl_get_error_message", "intl"))
        .with_function(FunctionDescriptor::php(
            "locale_get_primary_language",
            "intl",
        ))
        .with_function(FunctionDescriptor::php("normalizer_is_normalized", "intl"))
        .with_function(FunctionDescriptor::php("normalizer_normalize", "intl"))
        .with_function(FunctionDescriptor::php(
            "transliterator_transliterate",
            "intl",
        ))
        .with_class(ClassDescriptor::new("Collator", "intl", ClassKind::Class))
        .with_class(ClassDescriptor::new("IntlChar", "intl", ClassKind::Class))
        .with_class(ClassDescriptor::new("Locale", "intl", ClassKind::Class))
        .with_class(ClassDescriptor::new("Normalizer", "intl", ClassKind::Class))
        .with_class(ClassDescriptor::new(
            "NumberFormatter",
            "intl",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_xml_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("xml")
        .enabled_by_default(true)
        .with_constant(ConstantDescriptor::with_value(
            "XML_OPTION_CASE_FOLDING",
            "xml",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "XML_OPTION_TARGET_ENCODING",
            "xml",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "XML_OPTION_SKIP_TAGSTART",
            "xml",
            ConstantValue::Int(3),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "XML_OPTION_SKIP_WHITE",
            "xml",
            ConstantValue::Int(4),
        ))
        .with_function(FunctionDescriptor::php("xml_parser_create", "xml"))
        .with_function(FunctionDescriptor::php("xml_parser_create_ns", "xml"))
        .with_function(FunctionDescriptor::php("xml_parser_get_option", "xml"))
        .with_function(FunctionDescriptor::php("xml_parser_set_option", "xml"))
        .with_function(FunctionDescriptor::php("xml_parse", "xml"))
        .with_class(ClassDescriptor::new("XMLParser", "xml", ClassKind::Class))
}

pub(super) fn standard_library_dom_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("dom")
        .enabled_by_default(true)
        .with_class(ClassDescriptor::new("DOMDocument", "dom", ClassKind::Class))
        .with_class(ClassDescriptor::new("DOMElement", "dom", ClassKind::Class))
        .with_class(ClassDescriptor::new("DOMNode", "dom", ClassKind::Class))
        .with_class(ClassDescriptor::new("DOMNodeList", "dom", ClassKind::Class))
}

pub(super) fn standard_library_simplexml_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("simplexml")
        .enabled_by_default(true)
        .with_function(FunctionDescriptor::php("simplexml_load_file", "simplexml"))
        .with_function(FunctionDescriptor::php(
            "simplexml_load_string",
            "simplexml",
        ))
        .with_class(ClassDescriptor::new(
            "SimpleXMLElement",
            "simplexml",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_xmlreader_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("xmlreader")
        .enabled_by_default(true)
        .with_class(ClassDescriptor::new(
            "XMLReader",
            "xmlreader",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_xmlwriter_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("xmlwriter")
        .enabled_by_default(true)
        .with_class(ClassDescriptor::new(
            "XMLWriter",
            "xmlwriter",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_hash_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("hash")
        .with_function(FunctionDescriptor::php("hash", "hash"))
        .with_function(FunctionDescriptor::php("hash_algos", "hash"))
        .with_function(FunctionDescriptor::php("hash_equals", "hash"))
        .with_function(FunctionDescriptor::php("hash_hmac", "hash"))
}

pub(super) fn standard_library_ctype_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("ctype")
        .with_function(FunctionDescriptor::php("ctype_alnum", "ctype"))
        .with_function(FunctionDescriptor::php("ctype_alpha", "ctype"))
        .with_function(FunctionDescriptor::php("ctype_cntrl", "ctype"))
        .with_function(FunctionDescriptor::php("ctype_digit", "ctype"))
        .with_function(FunctionDescriptor::php("ctype_graph", "ctype"))
        .with_function(FunctionDescriptor::php("ctype_lower", "ctype"))
        .with_function(FunctionDescriptor::php("ctype_print", "ctype"))
        .with_function(FunctionDescriptor::php("ctype_punct", "ctype"))
        .with_function(FunctionDescriptor::php("ctype_space", "ctype"))
        .with_function(FunctionDescriptor::php("ctype_upper", "ctype"))
        .with_function(FunctionDescriptor::php("ctype_xdigit", "ctype"))
}

pub(super) fn standard_library_filter_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("filter")
        .with_function(FunctionDescriptor::php("filter_input", "filter"))
        .with_function(FunctionDescriptor::php("filter_var", "filter"))
        .with_constant(ConstantDescriptor::with_value(
            "INPUT_POST",
            "filter",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "INPUT_GET",
            "filter",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "INPUT_COOKIE",
            "filter",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "INPUT_ENV",
            "filter",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "INPUT_SERVER",
            "filter",
            ConstantValue::Int(5),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_DEFAULT",
            "filter",
            ConstantValue::Int(516),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_VALIDATE_BOOL",
            "filter",
            ConstantValue::Int(258),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_VALIDATE_BOOLEAN",
            "filter",
            ConstantValue::Int(258),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_VALIDATE_INT",
            "filter",
            ConstantValue::Int(257),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_VALIDATE_FLOAT",
            "filter",
            ConstantValue::Int(259),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_VALIDATE_URL",
            "filter",
            ConstantValue::Int(273),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_VALIDATE_EMAIL",
            "filter",
            ConstantValue::Int(274),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_VALIDATE_IP",
            "filter",
            ConstantValue::Int(275),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_SANITIZE_EMAIL",
            "filter",
            ConstantValue::Int(517),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_SANITIZE_URL",
            "filter",
            ConstantValue::Int(518),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_SANITIZE_NUMBER_INT",
            "filter",
            ConstantValue::Int(519),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_NULL_ON_FAILURE",
            "filter",
            ConstantValue::Int(134_217_728),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_IPV4",
            "filter",
            ConstantValue::Int(1_048_576),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_IPV6",
            "filter",
            ConstantValue::Int(2_097_152),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_PATH_REQUIRED",
            "filter",
            ConstantValue::Int(262_144),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_QUERY_REQUIRED",
            "filter",
            ConstantValue::Int(524_288),
        ))
}

pub(super) fn standard_library_iconv_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("iconv")
        .with_function(FunctionDescriptor::php("iconv", "iconv"))
        .with_function(FunctionDescriptor::php("iconv_get_encoding", "iconv"))
        .with_function(FunctionDescriptor::php("iconv_set_encoding", "iconv"))
        .with_function(FunctionDescriptor::php("iconv_strlen", "iconv"))
        .with_function(FunctionDescriptor::php("iconv_strpos", "iconv"))
        .with_function(FunctionDescriptor::php("iconv_substr", "iconv"))
        .with_constant(ConstantDescriptor::with_value(
            "ICONV_IMPL",
            "iconv",
            ConstantValue::String("phrust"),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ICONV_VERSION",
            "iconv",
            ConstantValue::String("bounded-utf8-ascii-latin1"),
        ))
}

pub(super) fn standard_library_sodium_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("sodium")
        .with_function(FunctionDescriptor::php("sodium_base642bin", "sodium"))
        .with_function(FunctionDescriptor::php("sodium_bin2base64", "sodium"))
        .with_function(FunctionDescriptor::php("sodium_bin2hex", "sodium"))
        .with_function(FunctionDescriptor::php(
            "sodium_crypto_generichash",
            "sodium",
        ))
        .with_function(FunctionDescriptor::php(
            "sodium_crypto_generichash_keygen",
            "sodium",
        ))
        .with_function(FunctionDescriptor::php(
            "sodium_crypto_sign_detached",
            "sodium",
        ))
        .with_function(FunctionDescriptor::php(
            "sodium_crypto_sign_verify_detached",
            "sodium",
        ))
        .with_function(FunctionDescriptor::php("sodium_hex2bin", "sodium"))
        .with_constant(ConstantDescriptor::with_value(
            "SODIUM_BASE64_VARIANT_ORIGINAL",
            "sodium",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SODIUM_BASE64_VARIANT_ORIGINAL_NO_PADDING",
            "sodium",
            ConstantValue::Int(3),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SODIUM_BASE64_VARIANT_URLSAFE",
            "sodium",
            ConstantValue::Int(5),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SODIUM_BASE64_VARIANT_URLSAFE_NO_PADDING",
            "sodium",
            ConstantValue::Int(7),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SODIUM_CRYPTO_GENERICHASH_BYTES",
            "sodium",
            ConstantValue::Int(32),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SODIUM_CRYPTO_GENERICHASH_BYTES_MIN",
            "sodium",
            ConstantValue::Int(16),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SODIUM_CRYPTO_GENERICHASH_BYTES_MAX",
            "sodium",
            ConstantValue::Int(64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SODIUM_CRYPTO_SIGN_BYTES",
            "sodium",
            ConstantValue::Int(64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES",
            "sodium",
            ConstantValue::Int(32),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SODIUM_CRYPTO_SIGN_SECRETKEYBYTES",
            "sodium",
            ConstantValue::Int(64),
        ))
}

pub(super) fn standard_library_bcmath_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("bcmath")
        .with_function(FunctionDescriptor::php("bcadd", "bcmath"))
        .with_function(FunctionDescriptor::php("bccomp", "bcmath"))
        .with_function(FunctionDescriptor::php("bcdiv", "bcmath"))
        .with_function(FunctionDescriptor::php("bcmod", "bcmath"))
        .with_function(FunctionDescriptor::php("bcmul", "bcmath"))
        .with_function(FunctionDescriptor::php("bcpow", "bcmath"))
        .with_function(FunctionDescriptor::php("bcscale", "bcmath"))
        .with_function(FunctionDescriptor::php("bcsub", "bcmath"))
}

pub(super) fn standard_library_gmp_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("gmp")
        .with_class(ClassDescriptor::new("GMP", "gmp", ClassKind::Class))
        .with_function(FunctionDescriptor::php("gmp_abs", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_add", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_cmp", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_div_q", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_init", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_intval", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_mod", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_mul", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_neg", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_pow", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_strval", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_sub", "gmp"))
}

pub(super) fn standard_library_apcu_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("apcu")
        .with_function(FunctionDescriptor::php("apcu_add", "apcu"))
        .with_function(FunctionDescriptor::php("apcu_clear_cache", "apcu"))
        .with_function(FunctionDescriptor::php("apcu_delete", "apcu"))
        .with_function(FunctionDescriptor::php("apcu_enabled", "apcu"))
        .with_function(FunctionDescriptor::php("apcu_exists", "apcu"))
        .with_function(FunctionDescriptor::php("apcu_fetch", "apcu"))
        .with_function(FunctionDescriptor::php("apcu_store", "apcu"))
}

pub(super) fn standard_library_redis_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("redis").with_class(ClassDescriptor::new(
        "Redis",
        "redis",
        ClassKind::Class,
    ))
}

pub(super) fn standard_library_memcached_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("memcached").with_class(ClassDescriptor::new(
        "Memcached",
        "memcached",
        ClassKind::Class,
    ))
}

pub(super) fn standard_library_ftp_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("ftp")
        .with_function(FunctionDescriptor::php("ftp_connect", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_ssl_connect", "ftp"))
        .with_class(ClassDescriptor::new(
            "FTP\\Connection",
            "ftp",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_sockets_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("sockets")
        .with_function(FunctionDescriptor::php("socket_create", "sockets"))
        .with_function(FunctionDescriptor::php("socket_last_error", "sockets"))
        .with_function(FunctionDescriptor::php("socket_strerror", "sockets"))
        .with_constant(ConstantDescriptor::with_value(
            "AF_INET",
            "sockets",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SOCK_STREAM",
            "sockets",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SOL_TCP",
            "sockets",
            ConstantValue::Int(6),
        ))
}

pub(super) fn standard_library_zlib_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("zlib")
        .with_function(FunctionDescriptor::php("gzclose", "zlib"))
        .with_function(FunctionDescriptor::php("gzdeflate", "zlib"))
        .with_function(FunctionDescriptor::php("gzcompress", "zlib"))
        .with_function(FunctionDescriptor::php("gzdecode", "zlib"))
        .with_function(FunctionDescriptor::php("gzencode", "zlib"))
        .with_function(FunctionDescriptor::php("gzopen", "zlib"))
        .with_function(FunctionDescriptor::php("gzread", "zlib"))
        .with_function(FunctionDescriptor::php("gzwrite", "zlib"))
        .with_function(FunctionDescriptor::php("gzinflate", "zlib"))
        .with_function(FunctionDescriptor::php("gzuncompress", "zlib"))
        .with_function(FunctionDescriptor::php("zlib_decode", "zlib"))
        .with_function(FunctionDescriptor::php("zlib_encode", "zlib"))
        .with_constant(ConstantDescriptor::with_value(
            "FORCE_DEFLATE",
            "zlib",
            ConstantValue::Int(15),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FORCE_GZIP",
            "zlib",
            ConstantValue::Int(31),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_ENCODING_RAW",
            "zlib",
            ConstantValue::Int(-15),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_ENCODING_GZIP",
            "zlib",
            ConstantValue::Int(31),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_ENCODING_DEFLATE",
            "zlib",
            ConstantValue::Int(15),
        ))
}

pub(super) fn standard_library_zip_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("zip").with_class(ClassDescriptor::new(
        "ZipArchive",
        "zip",
        ClassKind::Class,
    ))
}

pub(super) fn standard_library_fileinfo_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("fileinfo")
        .with_function(FunctionDescriptor::php("finfo_buffer", "fileinfo"))
        .with_function(FunctionDescriptor::php("finfo_close", "fileinfo"))
        .with_function(FunctionDescriptor::php("finfo_file", "fileinfo"))
        .with_function(FunctionDescriptor::php("finfo_open", "fileinfo"))
        .with_constant(ConstantDescriptor::with_value(
            "FILEINFO_NONE",
            "fileinfo",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILEINFO_MIME_TYPE",
            "fileinfo",
            ConstantValue::Int(16),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILEINFO_MIME_ENCODING",
            "fileinfo",
            ConstantValue::Int(1024),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILEINFO_MIME",
            "fileinfo",
            ConstantValue::Int(1040),
        ))
}

pub(super) fn standard_library_exif_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("exif")
        .with_function(FunctionDescriptor::php("exif_imagetype", "exif"))
        .with_function(FunctionDescriptor::php("exif_read_data", "exif"))
}

pub(super) fn standard_library_gd_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("gd")
        .with_function(FunctionDescriptor::php("gd_info", "gd"))
        .with_function(FunctionDescriptor::php("imagecopyresampled", "gd"))
        .with_function(FunctionDescriptor::php("imagecreatefromjpeg", "gd"))
        .with_function(FunctionDescriptor::php("imagecreatefrompng", "gd"))
        .with_function(FunctionDescriptor::php("imagecreatefromstring", "gd"))
        .with_function(FunctionDescriptor::php("imagecreatetruecolor", "gd"))
        .with_function(FunctionDescriptor::php("imagedestroy", "gd"))
        .with_function(FunctionDescriptor::php("imagejpeg", "gd"))
        .with_function(FunctionDescriptor::php("imagepng", "gd"))
        .with_function(FunctionDescriptor::php("imagesx", "gd"))
        .with_function(FunctionDescriptor::php("imagesy", "gd"))
        .with_class(ClassDescriptor::new("GdImage", "gd", ClassKind::Class))
}

pub(super) fn standard_library_random_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("random")
        .with_function(FunctionDescriptor::php("random_bytes", "random"))
        .with_function(FunctionDescriptor::php("random_int", "random"))
}

pub(super) fn standard_library_date_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("date")
        .with_constant(ConstantDescriptor::with_value(
            "DATE_ATOM",
            "date",
            ConstantValue::String(constants::DATE_ATOM),
        ))
        .with_function(FunctionDescriptor::php("checkdate", "date"))
        .with_function(FunctionDescriptor::php("date", "date"))
        .with_function(FunctionDescriptor::php("date_default_timezone_get", "date"))
        .with_function(FunctionDescriptor::php("date_default_timezone_set", "date"))
        .with_function(FunctionDescriptor::php("strtotime", "date"))
        .with_function(FunctionDescriptor::php("time", "date"))
        .with_function(FunctionDescriptor::php("timezone_identifiers_list", "date"))
        .with_class(ClassDescriptor::new(
            "DateInterval",
            "date",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new("DateTime", "date", ClassKind::Class))
        .with_class(ClassDescriptor::new(
            "DateTimeImmutable",
            "date",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "DateTimeInterface",
            "date",
            ClassKind::Interface,
        ))
        .with_class(ClassDescriptor::new(
            "DateTimeZone",
            "date",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_spl_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("spl")
        .with_function(FunctionDescriptor::php("class_implements", "spl"))
        .with_function(FunctionDescriptor::php("iterator_apply", "spl"))
        .with_function(FunctionDescriptor::php("iterator_count", "spl"))
        .with_function(FunctionDescriptor::php("iterator_to_array", "spl"))
        .with_function(FunctionDescriptor::php("spl_autoload_call", "spl"))
        .with_function(FunctionDescriptor::php("spl_autoload_functions", "spl"))
        .with_function(FunctionDescriptor::php("spl_autoload_register", "spl"))
        .with_function(FunctionDescriptor::php("spl_autoload_unregister", "spl"))
        .with_function(FunctionDescriptor::php("spl_object_hash", "spl"))
        .with_function(FunctionDescriptor::php("spl_object_id", "spl"))
        .with_class(ClassDescriptor::new(
            "ArrayAccess",
            "spl",
            ClassKind::Interface,
        ))
        .with_class(ClassDescriptor::new(
            "AppendIterator",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "ArrayIterator",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new("ArrayObject", "spl", ClassKind::Class))
        .with_class(ClassDescriptor::new(
            "BadFunctionCallException",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "BadMethodCallException",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "Countable",
            "spl",
            ClassKind::Interface,
        ))
        .with_class(ClassDescriptor::new(
            "DomainException",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "EmptyIterator",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "InvalidArgumentException",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "Iterator",
            "spl",
            ClassKind::Interface,
        ))
        .with_class(ClassDescriptor::new(
            "IteratorAggregate",
            "spl",
            ClassKind::Interface,
        ))
        .with_class(ClassDescriptor::new(
            "IteratorIterator",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "LengthException",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "LimitIterator",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "LogicException",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "OutOfBoundsException",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "OutOfRangeException",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "OverflowException",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "RangeException",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "RecursiveArrayIterator",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "RecursiveIterator",
            "spl",
            ClassKind::Interface,
        ))
        .with_class(ClassDescriptor::new(
            "RuntimeException",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "SeekableIterator",
            "spl",
            ClassKind::Interface,
        ))
        .with_class(ClassDescriptor::new(
            "Serializable",
            "spl",
            ClassKind::Interface,
        ))
        .with_class(ClassDescriptor::new(
            "SplDoublyLinkedList",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new("SplFileInfo", "spl", ClassKind::Class))
        .with_class(ClassDescriptor::new(
            "SplFileObject",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "SplFixedArray",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "SplObjectStorage",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new("SplQueue", "spl", ClassKind::Class))
        .with_class(ClassDescriptor::new("SplStack", "spl", ClassKind::Class))
        .with_class(ClassDescriptor::new(
            "SplTempFileObject",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "Traversable",
            "spl",
            ClassKind::Interface,
        ))
        .with_class(ClassDescriptor::new(
            "UnderflowException",
            "spl",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "UnexpectedValueException",
            "spl",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_test_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("test")
        .enabled_by_default(false)
        .with_function(FunctionDescriptor::internal_test(
            "__php_std_test_probe",
            "test",
        ))
}

pub(super) fn reflection_extension() -> ExtensionDescriptor {
    let mut extension = ExtensionDescriptor::new("reflection");
    for class in generated::arginfo::GENERATED_CLASSES
        .iter()
        .filter(|class| class.extension == "reflection")
    {
        extension = extension.with_class(ClassDescriptor::new(
            class.name,
            "reflection",
            generated_class_kind(class.kind),
        ));
    }
    extension
}

fn generated_class_kind(kind: &str) -> ClassKind {
    match kind {
        "interface" => ClassKind::Interface,
        "trait" => ClassKind::Trait,
        "enum" => ClassKind::Enum,
        _ => ClassKind::Class,
    }
}

pub(super) fn tokenizer_extension() -> ExtensionDescriptor {
    let mut extension = ExtensionDescriptor::new("tokenizer")
        .with_function(FunctionDescriptor::php("token_get_all", "tokenizer"))
        .with_function(FunctionDescriptor::php("token_name", "tokenizer"))
        .with_class(ClassDescriptor::new(
            "PhpToken",
            "tokenizer",
            ClassKind::Class,
        ))
        .with_constant(ConstantDescriptor::with_value(
            "TOKEN_PARSE",
            "tokenizer",
            ConstantValue::Int(php_runtime::api::tokenizer::TOKEN_PARSE),
        ));
    for (index, token_name) in php_lexer::TOKENIZER_TOKEN_NAMES.iter().enumerate() {
        extension = extension.with_constant(ConstantDescriptor::with_value(
            token_name.as_php_name(),
            "tokenizer",
            ConstantValue::Int(php_lexer::TOKENIZER_TOKEN_ID_BASE + index as i64),
        ));
    }
    extension
}
