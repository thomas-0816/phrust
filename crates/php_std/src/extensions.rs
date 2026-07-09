use super::*;

fn with_generated_classes(
    mut extension: ExtensionDescriptor,
    source_extension: &'static str,
) -> ExtensionDescriptor {
    for metadata in generated::arginfo::GENERATED_CLASSES
        .iter()
        .filter(|metadata| metadata.extension == source_extension)
    {
        if extension
            .classes()
            .iter()
            .any(|class| class.name().eq_ignore_ascii_case(metadata.name))
        {
            continue;
        }

        let kind = match metadata.kind {
            "class" => ClassKind::Class,
            "interface" => ClassKind::Interface,
            "trait" => ClassKind::Trait,
            "enum" => ClassKind::Enum,
            _ => continue,
        };
        extension =
            extension.with_class(ClassDescriptor::new(metadata.name, source_extension, kind));
    }
    extension
}

pub(super) fn standard_library_core_extension() -> ExtensionDescriptor {
    with_generated_classes(
        ExtensionDescriptor::new("core")
            .with_class(ClassDescriptor::new("Closure", "core", ClassKind::Class))
            .with_class(ClassDescriptor::new("stdClass", "core", ClassKind::Class))
            .with_class(ClassDescriptor::new(
                "Throwable",
                "core",
                ClassKind::Interface,
            ))
            .with_class(ClassDescriptor::new("Exception", "core", ClassKind::Class))
            .with_class(ClassDescriptor::new("Error", "core", ClassKind::Class))
            .with_class(ClassDescriptor::new("TypeError", "core", ClassKind::Class))
            .with_class(ClassDescriptor::new("ValueError", "core", ClassKind::Class))
            .with_class(ClassDescriptor::new(
                "ErrorException",
                "core",
                ClassKind::Class,
            ))
            .with_class(ClassDescriptor::new("ParseError", "core", ClassKind::Class))
            .with_class(ClassDescriptor::new(
                "ArithmeticError",
                "core",
                ClassKind::Class,
            ))
            .with_class(ClassDescriptor::new(
                "DivisionByZeroError",
                "core",
                ClassKind::Class,
            ))
            .with_function(FunctionDescriptor::php("class_alias", "core"))
            .with_function(FunctionDescriptor::php("class_exists", "core"))
            .with_function(FunctionDescriptor::php("clone", "core"))
            .with_function(FunctionDescriptor::php("debug_backtrace", "core"))
            .with_function(FunctionDescriptor::php("debug_print_backtrace", "core"))
            .with_function(FunctionDescriptor::php("define", "core"))
            .with_function(FunctionDescriptor::php("defined", "core"))
            .with_function(FunctionDescriptor::php("die", "core"))
            .with_function(FunctionDescriptor::php("enum_exists", "core"))
            .with_function(FunctionDescriptor::php("error_reporting", "core"))
            .with_function(FunctionDescriptor::php("exit", "core"))
            .with_function(FunctionDescriptor::php("extension_loaded", "core"))
            .with_function(FunctionDescriptor::php("func_get_arg", "core"))
            .with_function(FunctionDescriptor::php("func_get_args", "core"))
            .with_function(FunctionDescriptor::php("func_num_args", "core"))
            .with_function(FunctionDescriptor::php("function_exists", "core"))
            .with_function(FunctionDescriptor::php("gc_collect_cycles", "core"))
            .with_function(FunctionDescriptor::php("gc_disable", "core"))
            .with_function(FunctionDescriptor::php("gc_enable", "core"))
            .with_function(FunctionDescriptor::php("gc_enabled", "core"))
            .with_function(FunctionDescriptor::php("gc_mem_caches", "core"))
            .with_function(FunctionDescriptor::php("gc_status", "core"))
            .with_function(FunctionDescriptor::php("get_called_class", "core"))
            .with_function(FunctionDescriptor::php("get_class", "core"))
            .with_function(FunctionDescriptor::php("get_class_methods", "core"))
            .with_function(FunctionDescriptor::php("get_class_vars", "core"))
            .with_function(FunctionDescriptor::php("get_declared_classes", "core"))
            .with_function(FunctionDescriptor::php("get_declared_interfaces", "core"))
            .with_function(FunctionDescriptor::php("get_declared_traits", "core"))
            .with_function(FunctionDescriptor::php("get_defined_constants", "core"))
            .with_function(FunctionDescriptor::php("get_defined_functions", "core"))
            .with_function(FunctionDescriptor::php("get_defined_vars", "core"))
            .with_function(FunctionDescriptor::php("get_error_handler", "core"))
            .with_function(FunctionDescriptor::php("get_exception_handler", "core"))
            .with_function(FunctionDescriptor::php("get_extension_funcs", "core"))
            .with_function(FunctionDescriptor::php("get_included_files", "core"))
            .with_function(FunctionDescriptor::php("get_loaded_extensions", "core"))
            .with_function(FunctionDescriptor::php("get_mangled_object_vars", "core"))
            .with_function(FunctionDescriptor::php("get_object_vars", "core"))
            .with_function(FunctionDescriptor::php("get_parent_class", "core"))
            .with_function(FunctionDescriptor::php("get_required_files", "core"))
            .with_function(FunctionDescriptor::php("get_resource_id", "core"))
            .with_function(FunctionDescriptor::php("get_resource_type", "core"))
            .with_function(FunctionDescriptor::php("get_resources", "core"))
            .with_function(FunctionDescriptor::php("interface_exists", "core"))
            .with_function(FunctionDescriptor::php("is_a", "core"))
            .with_function(FunctionDescriptor::php("is_subclass_of", "core"))
            .with_function(FunctionDescriptor::php("method_exists", "core"))
            .with_function(FunctionDescriptor::php("property_exists", "core"))
            .with_function(FunctionDescriptor::php("restore_error_handler", "core"))
            .with_function(FunctionDescriptor::php("restore_exception_handler", "core"))
            .with_function(FunctionDescriptor::php("set_error_handler", "core"))
            .with_function(FunctionDescriptor::php("set_exception_handler", "core"))
            .with_function(FunctionDescriptor::php("strcasecmp", "core"))
            .with_function(FunctionDescriptor::php("strcmp", "core"))
            .with_function(FunctionDescriptor::php("strlen", "core"))
            .with_function(FunctionDescriptor::php("strncasecmp", "core"))
            .with_function(FunctionDescriptor::php("strncmp", "core"))
            .with_function(FunctionDescriptor::php("trait_exists", "core"))
            .with_function(FunctionDescriptor::php("trigger_error", "core"))
            .with_function(FunctionDescriptor::php("user_error", "core"))
            .with_function(FunctionDescriptor::php("zend_version", "core"))
            .with_constant(ConstantDescriptor::with_value(
                "TRUE",
                "core",
                ConstantValue::Bool(true),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "FALSE",
                "core",
                ConstantValue::Bool(false),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "NULL",
                "core",
                ConstantValue::Null,
            ))
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
                "PHP_FLOAT_DIG",
                "core",
                ConstantValue::Int(constants::PHP_FLOAT_DIG),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_FLOAT_EPSILON",
                "core",
                ConstantValue::Float(constants::PHP_FLOAT_EPSILON),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_FLOAT_MAX",
                "core",
                ConstantValue::Float(constants::PHP_FLOAT_MAX),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_FLOAT_MIN",
                "core",
                ConstantValue::Float(constants::PHP_FLOAT_MIN),
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
                "PHP_EXTENSION_DIR",
                "core",
                ConstantValue::String(constants::PHP_EXTENSION_DIR),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PEAR_EXTENSION_DIR",
                "core",
                ConstantValue::String(constants::PEAR_EXTENSION_DIR),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PEAR_INSTALL_DIR",
                "core",
                ConstantValue::String(constants::PEAR_INSTALL_DIR),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_BINDIR",
                "core",
                ConstantValue::String(constants::PHP_BINDIR),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_BUILD_DATE",
                "core",
                ConstantValue::String(constants::PHP_BUILD_DATE),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_CLI_PROCESS_TITLE",
                "core",
                ConstantValue::Bool(constants::PHP_CLI_PROCESS_TITLE),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_CONFIG_FILE_PATH",
                "core",
                ConstantValue::String(constants::PHP_CONFIG_FILE_PATH),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_CONFIG_FILE_SCAN_DIR",
                "core",
                ConstantValue::String(constants::PHP_CONFIG_FILE_SCAN_DIR),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_DATADIR",
                "core",
                ConstantValue::String(constants::PHP_DATADIR),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_DEBUG",
                "core",
                ConstantValue::Bool(constants::PHP_DEBUG),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_EXTRA_VERSION",
                "core",
                ConstantValue::String(constants::PHP_EXTRA_VERSION),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_FD_SETSIZE",
                "core",
                ConstantValue::Int(constants::PHP_FD_SETSIZE),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_LIBDIR",
                "core",
                ConstantValue::String(constants::PHP_LIBDIR),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_LOCALSTATEDIR",
                "core",
                ConstantValue::String(constants::PHP_LOCALSTATEDIR),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_MANDIR",
                "core",
                ConstantValue::String(constants::PHP_MANDIR),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_PREFIX",
                "core",
                ConstantValue::String(constants::PHP_PREFIX),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_SBINDIR",
                "core",
                ConstantValue::String(constants::PHP_SBINDIR),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_SHLIB_SUFFIX",
                "core",
                ConstantValue::String(constants::PHP_SHLIB_SUFFIX),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_SYSCONFDIR",
                "core",
                ConstantValue::String(constants::PHP_SYSCONFDIR),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_ZTS",
                "core",
                ConstantValue::Bool(constants::PHP_ZTS),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "ZEND_DEBUG_BUILD",
                "core",
                ConstantValue::Bool(constants::ZEND_DEBUG_BUILD),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "ZEND_THREAD_SAFE",
                "core",
                ConstantValue::Bool(constants::ZEND_THREAD_SAFE),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "ZEND_VM_KIND",
                "core",
                ConstantValue::String(constants::ZEND_VM_KIND),
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
                "PHP_OUTPUT_HANDLER_CONT",
                "core",
                ConstantValue::Int(constants::PHP_OUTPUT_HANDLER_CONT),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_OUTPUT_HANDLER_WRITE",
                "core",
                ConstantValue::Int(constants::PHP_OUTPUT_HANDLER_WRITE),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_OUTPUT_HANDLER_START",
                "core",
                ConstantValue::Int(constants::PHP_OUTPUT_HANDLER_START),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_OUTPUT_HANDLER_CLEAN",
                "core",
                ConstantValue::Int(constants::PHP_OUTPUT_HANDLER_CLEAN),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_OUTPUT_HANDLER_FLUSH",
                "core",
                ConstantValue::Int(constants::PHP_OUTPUT_HANDLER_FLUSH),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_OUTPUT_HANDLER_FINAL",
                "core",
                ConstantValue::Int(constants::PHP_OUTPUT_HANDLER_FINAL),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_OUTPUT_HANDLER_END",
                "core",
                ConstantValue::Int(constants::PHP_OUTPUT_HANDLER_END),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_OUTPUT_HANDLER_CLEANABLE",
                "core",
                ConstantValue::Int(constants::PHP_OUTPUT_HANDLER_CLEANABLE),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_OUTPUT_HANDLER_FLUSHABLE",
                "core",
                ConstantValue::Int(constants::PHP_OUTPUT_HANDLER_FLUSHABLE),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_OUTPUT_HANDLER_REMOVABLE",
                "core",
                ConstantValue::Int(constants::PHP_OUTPUT_HANDLER_REMOVABLE),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_OUTPUT_HANDLER_STDFLAGS",
                "core",
                ConstantValue::Int(constants::PHP_OUTPUT_HANDLER_STDFLAGS),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_OUTPUT_HANDLER_STARTED",
                "core",
                ConstantValue::Int(constants::PHP_OUTPUT_HANDLER_STARTED),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_OUTPUT_HANDLER_DISABLED",
                "core",
                ConstantValue::Int(constants::PHP_OUTPUT_HANDLER_DISABLED),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PHP_OUTPUT_HANDLER_PROCESSED",
                "core",
                ConstantValue::Int(constants::PHP_OUTPUT_HANDLER_PROCESSED),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "DEBUG_BACKTRACE_PROVIDE_OBJECT",
                "core",
                ConstantValue::Int(constants::DEBUG_BACKTRACE_PROVIDE_OBJECT),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "DEBUG_BACKTRACE_IGNORE_ARGS",
                "core",
                ConstantValue::Int(constants::DEBUG_BACKTRACE_IGNORE_ARGS),
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
            .with_constant(ConstantDescriptor::with_value(
                "UPLOAD_ERR_OK",
                "core",
                ConstantValue::Int(constants::UPLOAD_ERR_OK),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "UPLOAD_ERR_INI_SIZE",
                "core",
                ConstantValue::Int(constants::UPLOAD_ERR_INI_SIZE),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "UPLOAD_ERR_FORM_SIZE",
                "core",
                ConstantValue::Int(constants::UPLOAD_ERR_FORM_SIZE),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "UPLOAD_ERR_PARTIAL",
                "core",
                ConstantValue::Int(constants::UPLOAD_ERR_PARTIAL),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "UPLOAD_ERR_NO_FILE",
                "core",
                ConstantValue::Int(constants::UPLOAD_ERR_NO_FILE),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "UPLOAD_ERR_NO_TMP_DIR",
                "core",
                ConstantValue::Int(constants::UPLOAD_ERR_NO_TMP_DIR),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "UPLOAD_ERR_CANT_WRITE",
                "core",
                ConstantValue::Int(constants::UPLOAD_ERR_CANT_WRITE),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "UPLOAD_ERR_EXTENSION",
                "core",
                ConstantValue::Int(constants::UPLOAD_ERR_EXTENSION),
            ))
            .with_constant(ConstantDescriptor::new("STDIN", "core"))
            .with_constant(ConstantDescriptor::new("STDOUT", "core"))
            .with_constant(ConstantDescriptor::new("STDERR", "core")),
        "core",
    )
}

pub(super) fn standard_library_standard_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("standard")
        .with_constant(ConstantDescriptor::with_value(
            "INF",
            "standard",
            ConstantValue::Float(constants::INF),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "NAN",
            "standard",
            ConstantValue::Float(constants::NAN),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "DIRECTORY_SEPARATOR",
            "standard",
            ConstantValue::String(constants::DIRECTORY_SEPARATOR),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PATH_SEPARATOR",
            "standard",
            ConstantValue::String(constants::PATH_SEPARATOR),
        ))
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
        .with_function(FunctionDescriptor::php("assert", "standard"))
        .with_function(FunctionDescriptor::php("acos", "standard"))
        .with_function(FunctionDescriptor::php("acosh", "standard"))
        .with_function(FunctionDescriptor::php("addcslashes", "standard"))
        .with_function(FunctionDescriptor::php("array_all", "standard"))
        .with_function(FunctionDescriptor::php("array_any", "standard"))
        .with_function(FunctionDescriptor::php("array_chunk", "standard"))
        .with_function(FunctionDescriptor::php("array_column", "standard"))
        .with_function(FunctionDescriptor::php("array_diff_key", "standard"))
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
        .with_function(FunctionDescriptor::php("chgrp", "standard"))
        .with_function(FunctionDescriptor::php("chmod", "standard"))
        .with_function(FunctionDescriptor::php("chown", "standard"))
        .with_function(FunctionDescriptor::php("chr", "standard"))
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
        .with_function(FunctionDescriptor::php("debug_zval_dump", "standard"))
        .with_function(FunctionDescriptor::php("decbin", "standard"))
        .with_function(FunctionDescriptor::php("dechex", "standard"))
        .with_function(FunctionDescriptor::php("decoct", "standard"))
        .with_function(FunctionDescriptor::php("deg2rad", "standard"))
        .with_function(FunctionDescriptor::php("dirname", "standard"))
        .with_function(FunctionDescriptor::php("dir", "standard"))
        .with_function(FunctionDescriptor::php("disk_free_space", "standard"))
        .with_function(FunctionDescriptor::php("disk_total_space", "standard"))
        .with_function(FunctionDescriptor::php("error_log", "standard"))
        .with_function(FunctionDescriptor::php("exec", "standard"))
        .with_function(FunctionDescriptor::php("exp", "standard"))
        .with_function(FunctionDescriptor::php("expm1", "standard"))
        .with_function(FunctionDescriptor::php("explode", "standard"))
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
        .with_function(FunctionDescriptor::php("forward_static_call", "standard"))
        .with_function(FunctionDescriptor::php("fwrite", "standard"))
        .with_function(FunctionDescriptor::php("get_current_user", "standard"))
        .with_function(FunctionDescriptor::php("get_cfg_var", "standard"))
        .with_function(FunctionDescriptor::php("get_debug_type", "standard"))
        .with_function(FunctionDescriptor::php("getrandmax", "standard"))
        .with_function(FunctionDescriptor::php("getimagesize", "standard"))
        .with_function(FunctionDescriptor::php(
            "getimagesizefromstring",
            "standard",
        ))
        .with_function(FunctionDescriptor::php("getcwd", "standard"))
        .with_function(FunctionDescriptor::php("getenv", "standard"))
        .with_function(FunctionDescriptor::php("gethostbyname", "standard"))
        .with_function(FunctionDescriptor::php("gettype", "standard"))
        .with_function(FunctionDescriptor::php("glob", "standard"))
        .with_function(FunctionDescriptor::php("header", "standard"))
        .with_function(FunctionDescriptor::php("header_remove", "standard"))
        .with_function(FunctionDescriptor::php("headers_list", "standard"))
        .with_function(FunctionDescriptor::php("headers_sent", "standard"))
        .with_function(FunctionDescriptor::php("hex2bin", "standard"))
        .with_function(FunctionDescriptor::php("hexdec", "standard"))
        .with_function(FunctionDescriptor::php("html_entity_decode", "standard"))
        .with_function(FunctionDescriptor::php(
            "get_html_translation_table",
            "standard",
        ))
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
        .with_function(FunctionDescriptor::php("join", "standard"))
        .with_function(FunctionDescriptor::php("in_array", "standard"))
        .with_function(FunctionDescriptor::php("ini_get", "standard"))
        .with_function(FunctionDescriptor::php("ini_get_all", "standard"))
        .with_function(FunctionDescriptor::php("ini_set", "standard"))
        .with_function(FunctionDescriptor::php("intdiv", "standard"))
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
        .with_function(FunctionDescriptor::php("rewind", "standard"))
        .with_function(FunctionDescriptor::php("rewinddir", "standard"))
        .with_function(FunctionDescriptor::php("rmdir", "standard"))
        .with_function(FunctionDescriptor::php("round", "standard"))
        .with_function(FunctionDescriptor::php("rsort", "standard"))
        .with_function(FunctionDescriptor::php("rtrim", "standard"))
        .with_function(FunctionDescriptor::php("scandir", "standard"))
        .with_function(FunctionDescriptor::php("serialize", "standard"))
        .with_function(FunctionDescriptor::php("setcookie", "standard"))
        .with_function(FunctionDescriptor::php("setrawcookie", "standard"))
        .with_function(FunctionDescriptor::php("set_time_limit", "standard"))
        .with_function(FunctionDescriptor::php("ignore_user_abort", "standard"))
        .with_function(FunctionDescriptor::php("sha1", "standard"))
        .with_function(FunctionDescriptor::php("shell_exec", "standard"))
        .with_function(FunctionDescriptor::php("sin", "standard"))
        .with_function(FunctionDescriptor::php("sinh", "standard"))
        .with_function(FunctionDescriptor::php("sizeof", "standard"))
        .with_function(FunctionDescriptor::php("sleep", "standard"))
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
        .with_function(FunctionDescriptor::php(
            "stream_context_set_options",
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
        .with_function(FunctionDescriptor::php(
            "stream_wrapper_register",
            "standard",
        ))
        .with_function(FunctionDescriptor::php("str_contains", "standard"))
        .with_function(FunctionDescriptor::php("str_ends_with", "standard"))
        .with_function(FunctionDescriptor::php("str_pad", "standard"))
        .with_function(FunctionDescriptor::php("str_repeat", "standard"))
        .with_function(FunctionDescriptor::php("str_replace", "standard"))
        .with_function(FunctionDescriptor::php("str_split", "standard"))
        .with_function(FunctionDescriptor::php("str_starts_with", "standard"))
        .with_function(FunctionDescriptor::php("stripos", "standard"))
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
        .with_function(FunctionDescriptor::php("var_dump", "standard"))
        .with_function(FunctionDescriptor::php("var_export", "standard"))
        .with_function(FunctionDescriptor::php("version_compare", "standard"))
        .with_function(FunctionDescriptor::php("vprintf", "standard"))
        .with_function(FunctionDescriptor::php("vsprintf", "standard"))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_QUERY_RFC1738",
            "standard",
            ConstantValue::Int(constants::PHP_QUERY_RFC1738),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_QUERY_RFC3986",
            "standard",
            ConstantValue::Int(constants::PHP_QUERY_RFC3986),
        ))
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
            ConstantValue::Int(constants::IMAGETYPE_GIF),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_JPEG",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_JPEG),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_PNG",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_PNG),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_SWF",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_SWF),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_PSD",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_PSD),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_BMP",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_BMP),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_TIFF_II",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_TIFF_II),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_TIFF_MM",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_TIFF_MM),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_JPC",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_JPC),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_JP2",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_JP2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_JPX",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_JPX),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_JB2",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_JB2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_SWC",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_SWC),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_IFF",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_IFF),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_WBMP",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_WBMP),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_JPEG2000",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_JPEG2000),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_XBM",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_XBM),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_ICO",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_ICO),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_WEBP",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_WEBP),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_AVIF",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_AVIF),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_HEIF",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_HEIF),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_SVG",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_SVG),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_UNKNOWN",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_UNKNOWN),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMAGETYPE_COUNT",
            "standard",
            ConstantValue::Int(constants::IMAGETYPE_COUNT),
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
            "JSON_ERROR_NON_BACKED_ENUM",
            "json",
            ConstantValue::Int(11),
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
        .with_function(FunctionDescriptor::php("preg_filter", "pcre"))
        .with_function(FunctionDescriptor::php("preg_grep", "pcre"))
        .with_function(FunctionDescriptor::php("preg_last_error", "pcre"))
        .with_function(FunctionDescriptor::php("preg_last_error_msg", "pcre"))
        .with_function(FunctionDescriptor::php("preg_match", "pcre"))
        .with_function(FunctionDescriptor::php("preg_match_all", "pcre"))
        .with_function(FunctionDescriptor::php("preg_quote", "pcre"))
        .with_function(FunctionDescriptor::php("preg_replace", "pcre"))
        .with_function(FunctionDescriptor::php("preg_replace_callback", "pcre"))
        .with_function(FunctionDescriptor::php(
            "preg_replace_callback_array",
            "pcre",
        ))
        .with_function(FunctionDescriptor::php("preg_split", "pcre"))
        .with_constant(ConstantDescriptor::with_value(
            "PCRE_JIT_SUPPORT",
            "pcre",
            ConstantValue::Bool(true),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PCRE_VERSION",
            "pcre",
            ConstantValue::String("10.44 2024-06-07"),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PCRE_VERSION_MAJOR",
            "pcre",
            ConstantValue::Int(10),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PCRE_VERSION_MINOR",
            "pcre",
            ConstantValue::Int(44),
        ))
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
        .with_function(FunctionDescriptor::php(
            "session_get_cookie_params",
            "session",
        ))
        .with_function(FunctionDescriptor::php("session_module_name", "session"))
        .with_function(FunctionDescriptor::php("session_name", "session"))
        .with_function(FunctionDescriptor::php("session_save_path", "session"))
        .with_function(FunctionDescriptor::php(
            "session_set_cookie_params",
            "session",
        ))
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

pub(super) fn standard_library_pdo_mysql_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("pdo_mysql").with_class(ClassDescriptor::new(
        "Pdo\\Mysql",
        "pdo_mysql",
        ClassKind::Class,
    ))
}

pub(super) fn standard_library_pdo_pgsql_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("pdo_pgsql")
        .with_class(ClassDescriptor::new(
            "Pdo\\Pgsql",
            "pdo_pgsql",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "PDO_PGSql_Ext",
            "pdo_pgsql",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_pgsql_extension() -> ExtensionDescriptor {
    with_generated_classes(
        ExtensionDescriptor::new("pgsql")
            .with_function(FunctionDescriptor::php("pg_affected_rows", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_close", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_connect", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_escape_bytea", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_escape_identifier", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_escape_literal", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_escape_string", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_execute", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_fetch_array", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_fetch_assoc", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_fetch_object", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_fetch_result", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_fetch_row", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_free_result", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_last_error", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_num_fields", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_num_rows", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_prepare", "pgsql"))
            .with_function(FunctionDescriptor::php("pg_query", "pgsql"))
            .with_constant(ConstantDescriptor::with_value(
                "PGSQL_ASSOC",
                "pgsql",
                ConstantValue::Int(1),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PGSQL_NUM",
                "pgsql",
                ConstantValue::Int(2),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PGSQL_BOTH",
                "pgsql",
                ConstantValue::Int(3),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PGSQL_CONNECTION_OK",
                "pgsql",
                ConstantValue::Int(0),
            )),
        "pgsql",
    )
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
        .with_function(FunctionDescriptor::php("mysqli_more_results", "mysqli"))
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
            "CURLM_CALL_MULTI_PERFORM",
            "curl",
            ConstantValue::Int(-1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLE_OK",
            "curl",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLE_UNSUPPORTED_PROTOCOL",
            "curl",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLE_WRITE_ERROR",
            "curl",
            ConstantValue::Int(23),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLE_BAD_CONTENT_ENCODING",
            "curl",
            ConstantValue::Int(61),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLSHE_OK",
            "curl",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLSHE_BAD_OPTION",
            "curl",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURL_VERSION_SSL",
            "curl",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURL_HTTP_VERSION_1_0",
            "curl",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURL_HTTP_VERSION_1_1",
            "curl",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLAUTH_BASIC",
            "curl",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLAUTH_ANY",
            "curl",
            ConstantValue::Int(-17),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLPROTO_HTTP",
            "curl",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLPROTO_HTTPS",
            "curl",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLPROXY_HTTP",
            "curl",
            ConstantValue::Int(0),
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
            "CURLOPT_HEADERFUNCTION",
            "curl",
            ConstantValue::Int(20079),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_WRITEFUNCTION",
            "curl",
            ConstantValue::Int(20011),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_BUFFERSIZE",
            "curl",
            ConstantValue::Int(98),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_CAINFO",
            "curl",
            ConstantValue::Int(10065),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_HTTPAUTH",
            "curl",
            ConstantValue::Int(107),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_PROTOCOLS",
            "curl",
            ConstantValue::Int(181),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_PROXY",
            "curl",
            ConstantValue::Int(10004),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_PROXYAUTH",
            "curl",
            ConstantValue::Int(111),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_PROXYPORT",
            "curl",
            ConstantValue::Int(59),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_PROXYTYPE",
            "curl",
            ConstantValue::Int(101),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_PROXYUSERPWD",
            "curl",
            ConstantValue::Int(10006),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_REDIR_PROTOCOLS",
            "curl",
            ConstantValue::Int(182),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_USERPWD",
            "curl",
            ConstantValue::Int(10005),
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
            "CURLOPT_PRIVATE",
            "curl",
            ConstantValue::Int(10103),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLOPT_SHARE",
            "curl",
            ConstantValue::Int(10100),
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
        .with_constant(ConstantDescriptor::with_value(
            "CURLINFO_PRIVATE",
            "curl",
            ConstantValue::Int(1048597),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLSHOPT_SHARE",
            "curl",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURLSHOPT_UNSHARE",
            "curl",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURL_LOCK_DATA_COOKIE",
            "curl",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CURL_LOCK_DATA_DNS",
            "curl",
            ConstantValue::Int(3),
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
        .with_function(FunctionDescriptor::php("curl_share_close", "curl"))
        .with_function(FunctionDescriptor::php("curl_share_errno", "curl"))
        .with_function(FunctionDescriptor::php("curl_share_init", "curl"))
        .with_function(FunctionDescriptor::php("curl_share_setopt", "curl"))
        .with_function(FunctionDescriptor::php("curl_share_strerror", "curl"))
        .with_function(FunctionDescriptor::php("curl_strerror", "curl"))
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
        .with_class(ClassDescriptor::new(
            "CurlShareHandle",
            "curl",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_openssl_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("openssl")
        .with_constant(ConstantDescriptor::with_value(
            "OPENSSL_ALGO_MD5",
            "openssl",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "OPENSSL_ALGO_SHA1",
            "openssl",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "OPENSSL_ALGO_SHA224",
            "openssl",
            ConstantValue::Int(6),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "OPENSSL_ALGO_SHA256",
            "openssl",
            ConstantValue::Int(7),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "OPENSSL_ALGO_SHA384",
            "openssl",
            ConstantValue::Int(8),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "OPENSSL_ALGO_SHA512",
            "openssl",
            ConstantValue::Int(9),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "OPENSSL_RAW_DATA",
            "openssl",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "OPENSSL_ZERO_PADDING",
            "openssl",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "OPENSSL_DONT_ZERO_PAD_KEY",
            "openssl",
            ConstantValue::Int(4),
        ))
        .with_function(FunctionDescriptor::php("openssl_decrypt", "openssl"))
        .with_function(FunctionDescriptor::php("openssl_digest", "openssl"))
        .with_function(FunctionDescriptor::php("openssl_encrypt", "openssl"))
        .with_function(FunctionDescriptor::php(
            "openssl_cipher_iv_length",
            "openssl",
        ))
        .with_function(FunctionDescriptor::php(
            "openssl_cipher_key_length",
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
        .with_function(FunctionDescriptor::php("mb_encoding_aliases", "mbstring"))
        .with_function(FunctionDescriptor::php("mb_internal_encoding", "mbstring"))
        .with_function(FunctionDescriptor::php("mb_list_encodings", "mbstring"))
        .with_function(FunctionDescriptor::php("mb_strlen", "mbstring"))
        .with_function(FunctionDescriptor::php("mb_strtolower", "mbstring"))
        .with_function(FunctionDescriptor::php("mb_strtoupper", "mbstring"))
        .with_function(FunctionDescriptor::php("mb_stripos", "mbstring"))
        .with_function(FunctionDescriptor::php("mb_strpos", "mbstring"))
        .with_function(FunctionDescriptor::php("mb_substr_count", "mbstring"))
        .with_function(FunctionDescriptor::php(
            "mb_substitute_character",
            "mbstring",
        ))
        .with_function(FunctionDescriptor::php("mb_substr", "mbstring"))
}

pub(super) fn standard_library_intl_extension() -> ExtensionDescriptor {
    with_generated_classes(
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
            )),
        "intl",
    )
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
        .with_constant(ConstantDescriptor::with_value(
            "XML_OPTION_PARSE_HUGE",
            "xml",
            ConstantValue::Int(5),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "XML_SAX_IMPL",
            "xml",
            ConstantValue::String("libxml"),
        ))
        .with_function(FunctionDescriptor::php("xml_parser_create", "xml"))
        .with_function(FunctionDescriptor::php("xml_parser_create_ns", "xml"))
        .with_function(FunctionDescriptor::php("xml_parser_get_option", "xml"))
        .with_function(FunctionDescriptor::php("xml_parser_set_option", "xml"))
        .with_function(FunctionDescriptor::php("xml_parser_free", "xml"))
        .with_function(FunctionDescriptor::php("xml_parse", "xml"))
        .with_function(FunctionDescriptor::php("xml_parse_into_struct", "xml"))
        .with_function(FunctionDescriptor::php("xml_set_element_handler", "xml"))
        .with_function(FunctionDescriptor::php(
            "xml_set_character_data_handler",
            "xml",
        ))
        .with_function(FunctionDescriptor::php("xml_set_default_handler", "xml"))
        .with_function(FunctionDescriptor::php("xml_get_error_code", "xml"))
        .with_function(FunctionDescriptor::php("xml_error_string", "xml"))
        .with_function(FunctionDescriptor::php("xml_get_current_byte_index", "xml"))
        .with_function(FunctionDescriptor::php(
            "xml_get_current_line_number",
            "xml",
        ))
        .with_function(FunctionDescriptor::php(
            "xml_get_current_column_number",
            "xml",
        ))
        .with_class(ClassDescriptor::new("XMLParser", "xml", ClassKind::Class))
}

pub(super) fn standard_library_dom_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("dom")
        .enabled_by_default(true)
        .with_class(ClassDescriptor::new("DOMDocument", "dom", ClassKind::Class))
        .with_class(ClassDescriptor::new("DOMElement", "dom", ClassKind::Class))
        .with_class(ClassDescriptor::new("DOMAttr", "dom", ClassKind::Class))
        .with_class(ClassDescriptor::new("DOMNode", "dom", ClassKind::Class))
        .with_class(ClassDescriptor::new("DOMText", "dom", ClassKind::Class))
        .with_class(ClassDescriptor::new("DOMComment", "dom", ClassKind::Class))
        .with_class(ClassDescriptor::new(
            "DOMCdataSection",
            "dom",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new("DOMNodeList", "dom", ClassKind::Class))
}

pub(super) fn standard_library_simplexml_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("simplexml")
        .enabled_by_default(true)
        .with_function(FunctionDescriptor::php("dom_import_simplexml", "simplexml"))
        .with_function(FunctionDescriptor::php("simplexml_import_dom", "simplexml"))
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
        .with_function(FunctionDescriptor::php(
            "xmlwriter_open_memory",
            "xmlwriter",
        ))
        .with_function(FunctionDescriptor::php(
            "xmlwriter_start_document",
            "xmlwriter",
        ))
        .with_function(FunctionDescriptor::php(
            "xmlwriter_start_element",
            "xmlwriter",
        ))
        .with_function(FunctionDescriptor::php(
            "xmlwriter_write_attribute",
            "xmlwriter",
        ))
        .with_function(FunctionDescriptor::php("xmlwriter_text", "xmlwriter"))
        .with_function(FunctionDescriptor::php(
            "xmlwriter_write_comment",
            "xmlwriter",
        ))
        .with_function(FunctionDescriptor::php(
            "xmlwriter_write_cdata",
            "xmlwriter",
        ))
        .with_function(FunctionDescriptor::php(
            "xmlwriter_write_element",
            "xmlwriter",
        ))
        .with_function(FunctionDescriptor::php(
            "xmlwriter_end_document",
            "xmlwriter",
        ))
        .with_function(FunctionDescriptor::php(
            "xmlwriter_output_memory",
            "xmlwriter",
        ))
        .with_class(ClassDescriptor::new(
            "XMLWriter",
            "xmlwriter",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_xsl_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("xsl")
        .with_class(ClassDescriptor::new(
            "XSLTProcessor",
            "xsl",
            ClassKind::Class,
        ))
        .with_constant(ConstantDescriptor::with_value(
            "XSL_CLONE_AUTO",
            "xsl",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "XSL_CLONE_NEVER",
            "xsl",
            ConstantValue::Int(-1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "XSL_CLONE_ALWAYS",
            "xsl",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "XSL_SECPREF_NONE",
            "xsl",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "XSL_SECPREF_READ_FILE",
            "xsl",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "XSL_SECPREF_WRITE_FILE",
            "xsl",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "XSL_SECPREF_CREATE_DIRECTORY",
            "xsl",
            ConstantValue::Int(8),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "XSL_SECPREF_READ_NETWORK",
            "xsl",
            ConstantValue::Int(16),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "XSL_SECPREF_WRITE_NETWORK",
            "xsl",
            ConstantValue::Int(32),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "XSL_SECPREF_DEFAULT",
            "xsl",
            ConstantValue::Int(44),
        ))
}

fn mhash_constant(name: &'static str, value: i64) -> ConstantDescriptor {
    let message = match name {
        "MHASH_CRC32" => {
            "Constant MHASH_CRC32 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_MD5" => {
            "Constant MHASH_MD5 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_SHA1" => {
            "Constant MHASH_SHA1 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_HAVAL256" => {
            "Constant MHASH_HAVAL256 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_RIPEMD160" => {
            "Constant MHASH_RIPEMD160 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_TIGER" => {
            "Constant MHASH_TIGER is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_GOST" => {
            "Constant MHASH_GOST is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_CRC32B" => {
            "Constant MHASH_CRC32B is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_HAVAL224" => {
            "Constant MHASH_HAVAL224 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_HAVAL192" => {
            "Constant MHASH_HAVAL192 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_HAVAL160" => {
            "Constant MHASH_HAVAL160 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_HAVAL128" => {
            "Constant MHASH_HAVAL128 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_TIGER128" => {
            "Constant MHASH_TIGER128 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_TIGER160" => {
            "Constant MHASH_TIGER160 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_MD4" => {
            "Constant MHASH_MD4 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_SHA256" => {
            "Constant MHASH_SHA256 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_ADLER32" => {
            "Constant MHASH_ADLER32 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_SHA224" => {
            "Constant MHASH_SHA224 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_SHA512" => {
            "Constant MHASH_SHA512 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_SHA384" => {
            "Constant MHASH_SHA384 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_WHIRLPOOL" => {
            "Constant MHASH_WHIRLPOOL is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_RIPEMD128" => {
            "Constant MHASH_RIPEMD128 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_RIPEMD256" => {
            "Constant MHASH_RIPEMD256 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_RIPEMD320" => {
            "Constant MHASH_RIPEMD320 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_SNEFRU256" => {
            "Constant MHASH_SNEFRU256 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_MD2" => {
            "Constant MHASH_MD2 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_FNV132" => {
            "Constant MHASH_FNV132 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_FNV1A32" => {
            "Constant MHASH_FNV1A32 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_FNV164" => {
            "Constant MHASH_FNV164 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_FNV1A64" => {
            "Constant MHASH_FNV1A64 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_JOAAT" => {
            "Constant MHASH_JOAAT is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_CRC32C" => {
            "Constant MHASH_CRC32C is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_MURMUR3A" => {
            "Constant MHASH_MURMUR3A is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_MURMUR3C" => {
            "Constant MHASH_MURMUR3C is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_MURMUR3F" => {
            "Constant MHASH_MURMUR3F is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_XXH32" => {
            "Constant MHASH_XXH32 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_XXH64" => {
            "Constant MHASH_XXH64 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_XXH3" => {
            "Constant MHASH_XXH3 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        "MHASH_XXH128" => {
            "Constant MHASH_XXH128 is deprecated since 8.5, as the mhash*() functions were deprecated"
        }
        unknown => panic!("unknown mhash constant {unknown}"),
    };
    ConstantDescriptor::with_value(name, "hash", ConstantValue::Int(value)).deprecated(message)
}

pub(super) fn standard_library_hash_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("hash")
        .with_class(ClassDescriptor::new(
            "HashContext",
            "hash",
            ClassKind::Class,
        ))
        .with_constant(ConstantDescriptor::with_value(
            "HASH_HMAC",
            "hash",
            ConstantValue::Int(1),
        ))
        .with_constant(mhash_constant("MHASH_CRC32", 0))
        .with_constant(mhash_constant("MHASH_MD5", 1))
        .with_constant(mhash_constant("MHASH_SHA1", 2))
        .with_constant(mhash_constant("MHASH_HAVAL256", 3))
        .with_constant(mhash_constant("MHASH_RIPEMD160", 5))
        .with_constant(mhash_constant("MHASH_TIGER", 7))
        .with_constant(mhash_constant("MHASH_GOST", 8))
        .with_constant(mhash_constant("MHASH_CRC32B", 9))
        .with_constant(mhash_constant("MHASH_HAVAL224", 10))
        .with_constant(mhash_constant("MHASH_HAVAL192", 11))
        .with_constant(mhash_constant("MHASH_HAVAL160", 12))
        .with_constant(mhash_constant("MHASH_HAVAL128", 13))
        .with_constant(mhash_constant("MHASH_TIGER128", 14))
        .with_constant(mhash_constant("MHASH_TIGER160", 15))
        .with_constant(mhash_constant("MHASH_MD4", 16))
        .with_constant(mhash_constant("MHASH_SHA256", 17))
        .with_constant(mhash_constant("MHASH_ADLER32", 18))
        .with_constant(mhash_constant("MHASH_SHA224", 19))
        .with_constant(mhash_constant("MHASH_SHA512", 20))
        .with_constant(mhash_constant("MHASH_SHA384", 21))
        .with_constant(mhash_constant("MHASH_WHIRLPOOL", 22))
        .with_constant(mhash_constant("MHASH_RIPEMD128", 23))
        .with_constant(mhash_constant("MHASH_RIPEMD256", 24))
        .with_constant(mhash_constant("MHASH_RIPEMD320", 25))
        .with_constant(mhash_constant("MHASH_SNEFRU256", 27))
        .with_constant(mhash_constant("MHASH_MD2", 28))
        .with_constant(mhash_constant("MHASH_FNV132", 29))
        .with_constant(mhash_constant("MHASH_FNV1A32", 30))
        .with_constant(mhash_constant("MHASH_FNV164", 31))
        .with_constant(mhash_constant("MHASH_FNV1A64", 32))
        .with_constant(mhash_constant("MHASH_JOAAT", 33))
        .with_constant(mhash_constant("MHASH_CRC32C", 34))
        .with_constant(mhash_constant("MHASH_MURMUR3A", 35))
        .with_constant(mhash_constant("MHASH_MURMUR3C", 36))
        .with_constant(mhash_constant("MHASH_MURMUR3F", 37))
        .with_constant(mhash_constant("MHASH_XXH32", 38))
        .with_constant(mhash_constant("MHASH_XXH64", 39))
        .with_constant(mhash_constant("MHASH_XXH3", 40))
        .with_constant(mhash_constant("MHASH_XXH128", 41))
        .with_function(FunctionDescriptor::php("hash", "hash"))
        .with_function(FunctionDescriptor::php("hash_algos", "hash"))
        .with_function(FunctionDescriptor::php("hash_copy", "hash"))
        .with_function(FunctionDescriptor::php("hash_equals", "hash"))
        .with_function(FunctionDescriptor::php("hash_file", "hash"))
        .with_function(FunctionDescriptor::php("hash_final", "hash"))
        .with_function(FunctionDescriptor::php("hash_hmac", "hash"))
        .with_function(FunctionDescriptor::php("hash_hmac_algos", "hash"))
        .with_function(FunctionDescriptor::php("hash_hmac_file", "hash"))
        .with_function(FunctionDescriptor::php("hash_hkdf", "hash"))
        .with_function(FunctionDescriptor::php("hash_init", "hash"))
        .with_function(FunctionDescriptor::php("hash_pbkdf2", "hash"))
        .with_function(FunctionDescriptor::php("hash_update", "hash"))
        .with_function(FunctionDescriptor::php("hash_update_file", "hash"))
        .with_function(FunctionDescriptor::php("hash_update_stream", "hash"))
        .with_function(FunctionDescriptor::php("mhash", "hash"))
        .with_function(FunctionDescriptor::php("mhash_count", "hash"))
        .with_function(FunctionDescriptor::php("mhash_get_block_size", "hash"))
        .with_function(FunctionDescriptor::php("mhash_get_hash_name", "hash"))
        .with_function(FunctionDescriptor::php("mhash_keygen_s2k", "hash"))
}

pub(super) fn standard_library_gettext_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("gettext")
        .with_function(FunctionDescriptor::php("_", "gettext"))
        .with_function(FunctionDescriptor::php(
            "bind_textdomain_codeset",
            "gettext",
        ))
        .with_function(FunctionDescriptor::php("bindtextdomain", "gettext"))
        .with_function(FunctionDescriptor::php("dcgettext", "gettext"))
        .with_function(FunctionDescriptor::php("dcngettext", "gettext"))
        .with_function(FunctionDescriptor::php("dgettext", "gettext"))
        .with_function(FunctionDescriptor::php("dngettext", "gettext"))
        .with_function(FunctionDescriptor::php("gettext", "gettext"))
        .with_function(FunctionDescriptor::php("ngettext", "gettext"))
        .with_function(FunctionDescriptor::php("textdomain", "gettext"))
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

pub(super) fn standard_library_calendar_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("calendar")
        .with_constant(ConstantDescriptor::with_value(
            "CAL_GREGORIAN",
            "calendar",
            ConstantValue::Int(constants::CAL_GREGORIAN),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_JULIAN",
            "calendar",
            ConstantValue::Int(constants::CAL_JULIAN),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_JEWISH",
            "calendar",
            ConstantValue::Int(constants::CAL_JEWISH),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_FRENCH",
            "calendar",
            ConstantValue::Int(constants::CAL_FRENCH),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_NUM_CALS",
            "calendar",
            ConstantValue::Int(constants::CAL_NUM_CALS),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_DOW_DAYNO",
            "calendar",
            ConstantValue::Int(constants::CAL_DOW_DAYNO),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_DOW_LONG",
            "calendar",
            ConstantValue::Int(constants::CAL_DOW_LONG),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_DOW_SHORT",
            "calendar",
            ConstantValue::Int(constants::CAL_DOW_SHORT),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_MONTH_GREGORIAN_SHORT",
            "calendar",
            ConstantValue::Int(constants::CAL_MONTH_GREGORIAN_SHORT),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_MONTH_GREGORIAN_LONG",
            "calendar",
            ConstantValue::Int(constants::CAL_MONTH_GREGORIAN_LONG),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_MONTH_JULIAN_SHORT",
            "calendar",
            ConstantValue::Int(constants::CAL_MONTH_JULIAN_SHORT),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_MONTH_JULIAN_LONG",
            "calendar",
            ConstantValue::Int(constants::CAL_MONTH_JULIAN_LONG),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_MONTH_JEWISH",
            "calendar",
            ConstantValue::Int(constants::CAL_MONTH_JEWISH),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_MONTH_FRENCH",
            "calendar",
            ConstantValue::Int(constants::CAL_MONTH_FRENCH),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_EASTER_DEFAULT",
            "calendar",
            ConstantValue::Int(constants::CAL_EASTER_DEFAULT),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_EASTER_ROMAN",
            "calendar",
            ConstantValue::Int(constants::CAL_EASTER_ROMAN),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_EASTER_ALWAYS_GREGORIAN",
            "calendar",
            ConstantValue::Int(constants::CAL_EASTER_ALWAYS_GREGORIAN),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_EASTER_ALWAYS_JULIAN",
            "calendar",
            ConstantValue::Int(constants::CAL_EASTER_ALWAYS_JULIAN),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_JEWISH_ADD_ALAFIM_GERESH",
            "calendar",
            ConstantValue::Int(constants::CAL_JEWISH_ADD_ALAFIM_GERESH),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_JEWISH_ADD_ALAFIM",
            "calendar",
            ConstantValue::Int(constants::CAL_JEWISH_ADD_ALAFIM),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CAL_JEWISH_ADD_GERESHAYIM",
            "calendar",
            ConstantValue::Int(constants::CAL_JEWISH_ADD_GERESHAYIM),
        ))
        .with_function(FunctionDescriptor::php("cal_days_in_month", "calendar"))
        .with_function(FunctionDescriptor::php("cal_from_jd", "calendar"))
        .with_function(FunctionDescriptor::php("cal_info", "calendar"))
        .with_function(FunctionDescriptor::php("cal_to_jd", "calendar"))
        .with_function(FunctionDescriptor::php("easter_date", "calendar"))
        .with_function(FunctionDescriptor::php("easter_days", "calendar"))
        .with_function(FunctionDescriptor::php("frenchtojd", "calendar"))
        .with_function(FunctionDescriptor::php("gregoriantojd", "calendar"))
        .with_function(FunctionDescriptor::php("jddayofweek", "calendar"))
        .with_function(FunctionDescriptor::php("jdmonthname", "calendar"))
        .with_function(FunctionDescriptor::php("jdtofrench", "calendar"))
        .with_function(FunctionDescriptor::php("jdtogregorian", "calendar"))
        .with_function(FunctionDescriptor::php("jdtojewish", "calendar"))
        .with_function(FunctionDescriptor::php("jdtojulian", "calendar"))
        .with_function(FunctionDescriptor::php("jdtounix", "calendar"))
        .with_function(FunctionDescriptor::php("jewishtojd", "calendar"))
        .with_function(FunctionDescriptor::php("juliantojd", "calendar"))
        .with_function(FunctionDescriptor::php("unixtojd", "calendar"))
}

pub(super) fn standard_library_filter_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("filter")
        .with_function(FunctionDescriptor::php("filter_has_var", "filter"))
        .with_function(FunctionDescriptor::php("filter_input", "filter"))
        .with_function(FunctionDescriptor::php("filter_input_array", "filter"))
        .with_function(FunctionDescriptor::php("filter_var_array", "filter"))
        .with_function(FunctionDescriptor::php("filter_list", "filter"))
        .with_function(FunctionDescriptor::php("filter_id", "filter"))
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
            "FILTER_UNSAFE_RAW",
            "filter",
            ConstantValue::Int(516),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_CALLBACK",
            "filter",
            ConstantValue::Int(1_024),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_NONE",
            "filter",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_REQUIRE_ARRAY",
            "filter",
            ConstantValue::Int(16_777_216),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_REQUIRE_SCALAR",
            "filter",
            ConstantValue::Int(33_554_432),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FORCE_ARRAY",
            "filter",
            ConstantValue::Int(67_108_864),
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
            "FILTER_VALIDATE_REGEXP",
            "filter",
            ConstantValue::Int(272),
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
            "FILTER_VALIDATE_MAC",
            "filter",
            ConstantValue::Int(276),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_VALIDATE_DOMAIN",
            "filter",
            ConstantValue::Int(277),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_SANITIZE_STRING",
            "filter",
            ConstantValue::Int(513),
        ).deprecated(
            "Constant FILTER_SANITIZE_STRING is deprecated since 8.1, use htmlspecialchars() instead",
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_SANITIZE_STRIPPED",
            "filter",
            ConstantValue::Int(513),
        ).deprecated(
            "Constant FILTER_SANITIZE_STRIPPED is deprecated since 8.1, use htmlspecialchars() instead",
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_SANITIZE_ENCODED",
            "filter",
            ConstantValue::Int(514),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_SANITIZE_SPECIAL_CHARS",
            "filter",
            ConstantValue::Int(515),
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
            "FILTER_SANITIZE_NUMBER_FLOAT",
            "filter",
            ConstantValue::Int(520),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_SANITIZE_FULL_SPECIAL_CHARS",
            "filter",
            ConstantValue::Int(522),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_SANITIZE_ADD_SLASHES",
            "filter",
            ConstantValue::Int(523),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_NULL_ON_FAILURE",
            "filter",
            ConstantValue::Int(134_217_728),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_ALLOW_OCTAL",
            "filter",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_ALLOW_HEX",
            "filter",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_STRIP_LOW",
            "filter",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_STRIP_HIGH",
            "filter",
            ConstantValue::Int(8),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_ENCODE_LOW",
            "filter",
            ConstantValue::Int(16),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_ENCODE_HIGH",
            "filter",
            ConstantValue::Int(32),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_ENCODE_AMP",
            "filter",
            ConstantValue::Int(64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_NO_ENCODE_QUOTES",
            "filter",
            ConstantValue::Int(128),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_EMPTY_STRING_NULL",
            "filter",
            ConstantValue::Int(256),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_STRIP_BACKTICK",
            "filter",
            ConstantValue::Int(512),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_ALLOW_FRACTION",
            "filter",
            ConstantValue::Int(4_096),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_ALLOW_THOUSAND",
            "filter",
            ConstantValue::Int(8_192),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_ALLOW_SCIENTIFIC",
            "filter",
            ConstantValue::Int(16_384),
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
            "FILTER_FLAG_NO_RES_RANGE",
            "filter",
            ConstantValue::Int(4_194_304),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_NO_PRIV_RANGE",
            "filter",
            ConstantValue::Int(8_388_608),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_GLOBAL_RANGE",
            "filter",
            ConstantValue::Int(536_870_912),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_HOSTNAME",
            "filter",
            ConstantValue::Int(1_048_576),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILTER_FLAG_EMAIL_UNICODE",
            "filter",
            ConstantValue::Int(1_048_576),
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
        .with_function(FunctionDescriptor::php("iconv_mime_decode", "iconv"))
        .with_function(FunctionDescriptor::php(
            "iconv_mime_decode_headers",
            "iconv",
        ))
        .with_function(FunctionDescriptor::php("iconv_mime_encode", "iconv"))
        .with_function(FunctionDescriptor::php("iconv_set_encoding", "iconv"))
        .with_function(FunctionDescriptor::php("iconv_strlen", "iconv"))
        .with_function(FunctionDescriptor::php("iconv_strpos", "iconv"))
        .with_function(FunctionDescriptor::php("iconv_strrpos", "iconv"))
        .with_function(FunctionDescriptor::php("iconv_substr", "iconv"))
        .with_constant(ConstantDescriptor::with_value(
            "ICONV_MIME_DECODE_STRICT",
            "iconv",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ICONV_MIME_DECODE_CONTINUE_ON_ERROR",
            "iconv",
            ConstantValue::Int(2),
        ))
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
        .with_constant(ConstantDescriptor::with_value(
            "SODIUM_LIBRARY_VERSION",
            "sodium",
            ConstantValue::String("1.0.20"),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SODIUM_LIBRARY_MAJOR_VERSION",
            "sodium",
            ConstantValue::Int(10),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SODIUM_LIBRARY_MINOR_VERSION",
            "sodium",
            ConstantValue::Int(5),
        ))
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
        .with_function(FunctionDescriptor::php("bcpowmod", "bcmath"))
        .with_function(FunctionDescriptor::php("bcscale", "bcmath"))
        .with_function(FunctionDescriptor::php("bcsqrt", "bcmath"))
        .with_function(FunctionDescriptor::php("bcsub", "bcmath"))
}

pub(super) fn standard_library_gmp_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("gmp")
        .with_class(ClassDescriptor::new("GMP", "gmp", ClassKind::Class))
        .with_function(FunctionDescriptor::php("gmp_abs", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_add", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_and", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_binomial", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_cmp", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_com", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_div", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_div_q", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_div_qr", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_div_r", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_divexact", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_export", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_fact", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_gcd", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_gcdext", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_hamdist", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_import", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_init", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_intval", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_invert", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_jacobi", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_kronecker", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_lcm", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_legendre", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_mod", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_mul", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_neg", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_nextprime", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_or", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_perfect_power", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_perfect_square", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_popcount", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_pow", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_powm", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_prob_prime", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_random_bits", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_random_range", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_random_seed", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_root", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_rootrem", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_scan0", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_scan1", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_sign", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_sqrt", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_sqrtrem", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_strval", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_sub", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_testbit", "gmp"))
        .with_function(FunctionDescriptor::php("gmp_xor", "gmp"))
        .with_constant(ConstantDescriptor::with_value(
            "GMP_ROUND_ZERO",
            "gmp",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "GMP_ROUND_PLUSINF",
            "gmp",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "GMP_ROUND_MINUSINF",
            "gmp",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "GMP_MSW_FIRST",
            "gmp",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "GMP_LSW_FIRST",
            "gmp",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "GMP_LITTLE_ENDIAN",
            "gmp",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "GMP_BIG_ENDIAN",
            "gmp",
            ConstantValue::Int(8),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "GMP_NATIVE_ENDIAN",
            "gmp",
            ConstantValue::Int(16),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "GMP_VERSION",
            "gmp",
            ConstantValue::String("6.3.0"),
        ))
}

pub(super) fn standard_library_posix_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("posix")
        .with_function(FunctionDescriptor::php("posix_access", "posix"))
        .with_function(FunctionDescriptor::php("posix_ctermid", "posix"))
        .with_function(FunctionDescriptor::php("posix_eaccess", "posix"))
        .with_function(FunctionDescriptor::php("posix_errno", "posix"))
        .with_function(FunctionDescriptor::php("posix_fpathconf", "posix"))
        .with_function(FunctionDescriptor::php("posix_get_last_error", "posix"))
        .with_function(FunctionDescriptor::php("posix_getcwd", "posix"))
        .with_function(FunctionDescriptor::php("posix_getegid", "posix"))
        .with_function(FunctionDescriptor::php("posix_geteuid", "posix"))
        .with_function(FunctionDescriptor::php("posix_getgid", "posix"))
        .with_function(FunctionDescriptor::php("posix_getgrgid", "posix"))
        .with_function(FunctionDescriptor::php("posix_getgrnam", "posix"))
        .with_function(FunctionDescriptor::php("posix_getgroups", "posix"))
        .with_function(FunctionDescriptor::php("posix_getlogin", "posix"))
        .with_function(FunctionDescriptor::php("posix_getpgid", "posix"))
        .with_function(FunctionDescriptor::php("posix_getpgrp", "posix"))
        .with_function(FunctionDescriptor::php("posix_getpid", "posix"))
        .with_function(FunctionDescriptor::php("posix_getppid", "posix"))
        .with_function(FunctionDescriptor::php("posix_getpwnam", "posix"))
        .with_function(FunctionDescriptor::php("posix_getpwuid", "posix"))
        .with_function(FunctionDescriptor::php("posix_getrlimit", "posix"))
        .with_function(FunctionDescriptor::php("posix_getsid", "posix"))
        .with_function(FunctionDescriptor::php("posix_getuid", "posix"))
        .with_function(FunctionDescriptor::php("posix_initgroups", "posix"))
        .with_function(FunctionDescriptor::php("posix_isatty", "posix"))
        .with_function(FunctionDescriptor::php("posix_kill", "posix"))
        .with_function(FunctionDescriptor::php("posix_mkfifo", "posix"))
        .with_function(FunctionDescriptor::php("posix_mknod", "posix"))
        .with_function(FunctionDescriptor::php("posix_pathconf", "posix"))
        .with_function(FunctionDescriptor::php("posix_setegid", "posix"))
        .with_function(FunctionDescriptor::php("posix_seteuid", "posix"))
        .with_function(FunctionDescriptor::php("posix_setgid", "posix"))
        .with_function(FunctionDescriptor::php("posix_setpgid", "posix"))
        .with_function(FunctionDescriptor::php("posix_setrlimit", "posix"))
        .with_function(FunctionDescriptor::php("posix_setsid", "posix"))
        .with_function(FunctionDescriptor::php("posix_setuid", "posix"))
        .with_function(FunctionDescriptor::php("posix_strerror", "posix"))
        .with_function(FunctionDescriptor::php("posix_sysconf", "posix"))
        .with_function(FunctionDescriptor::php("posix_times", "posix"))
        .with_function(FunctionDescriptor::php("posix_ttyname", "posix"))
        .with_function(FunctionDescriptor::php("posix_uname", "posix"))
        .with_constant(ConstantDescriptor::with_value(
            "POSIX_F_OK",
            "posix",
            ConstantValue::Int(libc::F_OK as i64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "POSIX_X_OK",
            "posix",
            ConstantValue::Int(libc::X_OK as i64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "POSIX_W_OK",
            "posix",
            ConstantValue::Int(libc::W_OK as i64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "POSIX_R_OK",
            "posix",
            ConstantValue::Int(libc::R_OK as i64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "POSIX_PC_NAME_MAX",
            "posix",
            ConstantValue::Int(libc::_PC_NAME_MAX as i64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "POSIX_PC_PATH_MAX",
            "posix",
            ConstantValue::Int(libc::_PC_PATH_MAX as i64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "POSIX_SC_NPROCESSORS_ONLN",
            "posix",
            ConstantValue::Int(libc::_SC_NPROCESSORS_ONLN as i64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "POSIX_SC_OPEN_MAX",
            "posix",
            ConstantValue::Int(libc::_SC_OPEN_MAX as i64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "POSIX_RLIMIT_CORE",
            "posix",
            ConstantValue::Int(libc::RLIMIT_CORE as i64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "POSIX_RLIMIT_CPU",
            "posix",
            ConstantValue::Int(libc::RLIMIT_CPU as i64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "POSIX_RLIMIT_DATA",
            "posix",
            ConstantValue::Int(libc::RLIMIT_DATA as i64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "POSIX_RLIMIT_FSIZE",
            "posix",
            ConstantValue::Int(libc::RLIMIT_FSIZE as i64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "POSIX_RLIMIT_NOFILE",
            "posix",
            ConstantValue::Int(libc::RLIMIT_NOFILE as i64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "POSIX_RLIMIT_RSS",
            "posix",
            ConstantValue::Int(libc::RLIMIT_RSS as i64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "POSIX_RLIMIT_STACK",
            "posix",
            ConstantValue::Int(libc::RLIMIT_STACK as i64),
        ))
}

pub(super) fn standard_library_shmop_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("shmop")
        .with_class(ClassDescriptor::new("Shmop", "shmop", ClassKind::Class))
        .with_function(FunctionDescriptor::php("shmop_close", "shmop"))
        .with_function(FunctionDescriptor::php("shmop_delete", "shmop"))
        .with_function(FunctionDescriptor::php("shmop_open", "shmop"))
        .with_function(FunctionDescriptor::php("shmop_read", "shmop"))
        .with_function(FunctionDescriptor::php("shmop_size", "shmop"))
        .with_function(FunctionDescriptor::php("shmop_write", "shmop"))
}

pub(super) fn standard_library_pcntl_extension() -> ExtensionDescriptor {
    with_pcntl_platform_constants(
        ExtensionDescriptor::new("pcntl")
            .with_function(FunctionDescriptor::php("pcntl_alarm", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_async_signals", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_errno", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_exec", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_fork", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_get_last_error", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_getpriority", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_setpriority", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_signal", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_signal_dispatch", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_signal_get_handler", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_strerror", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_wait", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_waitpid", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_wexitstatus", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_wifcontinued", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_wifexited", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_wifsignaled", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_wifstopped", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_wstopsig", "pcntl"))
            .with_function(FunctionDescriptor::php("pcntl_wtermsig", "pcntl"))
            .with_constant(ConstantDescriptor::with_value(
                "PRIO_PROCESS",
                "pcntl",
                ConstantValue::Int(libc::PRIO_PROCESS as i64),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PRIO_PGRP",
                "pcntl",
                ConstantValue::Int(libc::PRIO_PGRP as i64),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PRIO_USER",
                "pcntl",
                ConstantValue::Int(libc::PRIO_USER as i64),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PCNTL_ECHILD",
                "pcntl",
                ConstantValue::Int(libc::ECHILD as i64),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PCNTL_EINVAL",
                "pcntl",
                ConstantValue::Int(libc::EINVAL as i64),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PCNTL_EINTR",
                "pcntl",
                ConstantValue::Int(libc::EINTR as i64),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "SIG_DFL",
                "pcntl",
                ConstantValue::Int(libc::SIG_DFL as i64),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "SIG_IGN",
                "pcntl",
                ConstantValue::Int(libc::SIG_IGN as i64),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "SIG_ERR",
                "pcntl",
                ConstantValue::Int(libc::SIG_ERR as i64),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "SIGALRM",
                "pcntl",
                ConstantValue::Int(libc::SIGALRM as i64),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "SIGCHLD",
                "pcntl",
                ConstantValue::Int(libc::SIGCHLD as i64),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "SIGINT",
                "pcntl",
                ConstantValue::Int(libc::SIGINT as i64),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "SIGTERM",
                "pcntl",
                ConstantValue::Int(libc::SIGTERM as i64),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "SIGUSR1",
                "pcntl",
                ConstantValue::Int(libc::SIGUSR1 as i64),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "SIGUSR2",
                "pcntl",
                ConstantValue::Int(libc::SIGUSR2 as i64),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "WNOHANG",
                "pcntl",
                ConstantValue::Int(libc::WNOHANG as i64),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "WUNTRACED",
                "pcntl",
                ConstantValue::Int(libc::WUNTRACED as i64),
            )),
    )
}

fn with_pcntl_platform_constants(mut extension: ExtensionDescriptor) -> ExtensionDescriptor {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        extension = extension
            .with_constant(ConstantDescriptor::with_value(
                "PRIO_DARWIN_BG",
                "pcntl",
                ConstantValue::Int(0x1000),
            ))
            .with_constant(ConstantDescriptor::with_value(
                "PRIO_DARWIN_THREAD",
                "pcntl",
                ConstantValue::Int(3),
            ));
    }
    extension
}

pub(super) fn standard_library_readline_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("readline")
        .with_constant(ConstantDescriptor::with_value(
            "READLINE_LIB",
            "readline",
            ConstantValue::String("phrust"),
        ))
        .with_function(FunctionDescriptor::php("readline", "readline"))
        .with_function(FunctionDescriptor::php("readline_add_history", "readline"))
        .with_function(FunctionDescriptor::php(
            "readline_callback_handler_install",
            "readline",
        ))
        .with_function(FunctionDescriptor::php(
            "readline_callback_handler_remove",
            "readline",
        ))
        .with_function(FunctionDescriptor::php(
            "readline_callback_read_char",
            "readline",
        ))
        .with_function(FunctionDescriptor::php(
            "readline_clear_history",
            "readline",
        ))
        .with_function(FunctionDescriptor::php(
            "readline_completion_function",
            "readline",
        ))
        .with_function(FunctionDescriptor::php("readline_info", "readline"))
        .with_function(FunctionDescriptor::php("readline_list_history", "readline"))
        .with_function(FunctionDescriptor::php("readline_on_new_line", "readline"))
        .with_function(FunctionDescriptor::php("readline_read_history", "readline"))
        .with_function(FunctionDescriptor::php("readline_redisplay", "readline"))
        .with_function(FunctionDescriptor::php(
            "readline_write_history",
            "readline",
        ))
}

pub(super) fn standard_library_sysvmsg_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("sysvmsg")
        .with_class(ClassDescriptor::new(
            "SysvMessageQueue",
            "sysvmsg",
            ClassKind::Class,
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MSG_IPC_NOWAIT",
            "sysvmsg",
            ConstantValue::Int(libc::IPC_NOWAIT as i64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MSG_EAGAIN",
            "sysvmsg",
            ConstantValue::Int(libc::EAGAIN as i64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MSG_ENOMSG",
            "sysvmsg",
            ConstantValue::Int(libc::ENOMSG as i64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MSG_NOERROR",
            "sysvmsg",
            ConstantValue::Int(0o10000),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MSG_EXCEPT",
            "sysvmsg",
            ConstantValue::Int(0o20000),
        ))
        .with_function(FunctionDescriptor::php("msg_get_queue", "sysvmsg"))
        .with_function(FunctionDescriptor::php("msg_queue_exists", "sysvmsg"))
        .with_function(FunctionDescriptor::php("msg_receive", "sysvmsg"))
        .with_function(FunctionDescriptor::php("msg_remove_queue", "sysvmsg"))
        .with_function(FunctionDescriptor::php("msg_send", "sysvmsg"))
        .with_function(FunctionDescriptor::php("msg_set_queue", "sysvmsg"))
        .with_function(FunctionDescriptor::php("msg_stat_queue", "sysvmsg"))
}

pub(super) fn standard_library_sysvsem_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("sysvsem")
        .with_class(ClassDescriptor::new(
            "SysvSemaphore",
            "sysvsem",
            ClassKind::Class,
        ))
        .with_function(FunctionDescriptor::php("sem_acquire", "sysvsem"))
        .with_function(FunctionDescriptor::php("sem_get", "sysvsem"))
        .with_function(FunctionDescriptor::php("sem_release", "sysvsem"))
        .with_function(FunctionDescriptor::php("sem_remove", "sysvsem"))
}

pub(super) fn standard_library_sysvshm_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("sysvshm")
        .with_class(ClassDescriptor::new(
            "SysvSharedMemory",
            "sysvshm",
            ClassKind::Class,
        ))
        .with_function(FunctionDescriptor::php("shm_attach", "sysvshm"))
        .with_function(FunctionDescriptor::php("shm_detach", "sysvshm"))
        .with_function(FunctionDescriptor::php("shm_get_var", "sysvshm"))
        .with_function(FunctionDescriptor::php("shm_has_var", "sysvshm"))
        .with_function(FunctionDescriptor::php("shm_put_var", "sysvshm"))
        .with_function(FunctionDescriptor::php("shm_remove", "sysvshm"))
        .with_function(FunctionDescriptor::php("shm_remove_var", "sysvshm"))
}

pub(super) fn standard_library_apcu_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("apcu")
        .with_function(FunctionDescriptor::php("apcu_add", "apcu"))
        .with_function(FunctionDescriptor::php("apcu_cache_info", "apcu"))
        .with_function(FunctionDescriptor::php("apcu_clear_cache", "apcu"))
        .with_function(FunctionDescriptor::php("apcu_dec", "apcu"))
        .with_function(FunctionDescriptor::php("apcu_delete", "apcu"))
        .with_function(FunctionDescriptor::php("apcu_enabled", "apcu"))
        .with_function(FunctionDescriptor::php("apcu_exists", "apcu"))
        .with_function(FunctionDescriptor::php("apcu_fetch", "apcu"))
        .with_function(FunctionDescriptor::php("apcu_inc", "apcu"))
        .with_function(FunctionDescriptor::php("apcu_sma_info", "apcu"))
        .with_function(FunctionDescriptor::php("apcu_store", "apcu"))
}

pub(super) fn standard_library_redis_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("redis")
        .with_class(ClassDescriptor::new("Redis", "redis", ClassKind::Class))
        .with_class(ClassDescriptor::new(
            "RedisException",
            "redis",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_memcached_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("memcached")
        .with_class(ClassDescriptor::new(
            "Memcached",
            "memcached",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "MemcachedException",
            "memcached",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_imagick_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("imagick")
        .with_class(ClassDescriptor::new("Imagick", "imagick", ClassKind::Class))
        .with_class(ClassDescriptor::new(
            "ImagickDraw",
            "imagick",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "ImagickPixel",
            "imagick",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "ImagickPixelIterator",
            "imagick",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "ImagickException",
            "imagick",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_igbinary_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("igbinary")
        .with_function(FunctionDescriptor::php("igbinary_serialize", "igbinary"))
        .with_function(FunctionDescriptor::php("igbinary_unserialize", "igbinary"))
}

pub(super) fn standard_library_msgpack_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("msgpack")
        .with_constant(ConstantDescriptor::with_value(
            "MESSAGEPACK_OPT_PHPONLY",
            "msgpack",
            ConstantValue::Int(-1001),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MESSAGEPACK_OPT_ASSOC",
            "msgpack",
            ConstantValue::Int(-1002),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MESSAGEPACK_OPT_FORCE_F32",
            "msgpack",
            ConstantValue::Int(-1003),
        ))
        .with_function(FunctionDescriptor::php("msgpack_pack", "msgpack"))
        .with_function(FunctionDescriptor::php("msgpack_serialize", "msgpack"))
        .with_function(FunctionDescriptor::php("msgpack_unserialize", "msgpack"))
        .with_function(FunctionDescriptor::php("msgpack_unpack", "msgpack"))
        .with_class(ClassDescriptor::new(
            "MessagePack",
            "msgpack",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "MessagePackUnpacker",
            "msgpack",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_opcache_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("opcache")
        .with_function(FunctionDescriptor::php("opcache_compile_file", "opcache"))
        .with_function(FunctionDescriptor::php(
            "opcache_get_configuration",
            "opcache",
        ))
        .with_function(FunctionDescriptor::php("opcache_get_status", "opcache"))
        .with_function(FunctionDescriptor::php("opcache_invalidate", "opcache"))
        .with_function(FunctionDescriptor::php(
            "opcache_is_script_cached",
            "opcache",
        ))
        .with_function(FunctionDescriptor::php(
            "opcache_is_script_cached_in_file_cache",
            "opcache",
        ))
        .with_function(FunctionDescriptor::php("opcache_jit_blacklist", "opcache"))
        .with_function(FunctionDescriptor::php("opcache_reset", "opcache"))
}

pub(super) fn standard_library_soap_extension() -> ExtensionDescriptor {
    with_soap_constants(
        ExtensionDescriptor::new("soap")
            .with_function(FunctionDescriptor::php("is_soap_fault", "soap"))
            .with_function(FunctionDescriptor::php("use_soap_error_handler", "soap"))
            .with_class(ClassDescriptor::new("SoapClient", "soap", ClassKind::Class))
            .with_class(ClassDescriptor::new("SoapServer", "soap", ClassKind::Class))
            .with_class(ClassDescriptor::new("SoapFault", "soap", ClassKind::Class))
            .with_class(ClassDescriptor::new("SoapHeader", "soap", ClassKind::Class))
            .with_class(ClassDescriptor::new("SoapParam", "soap", ClassKind::Class))
            .with_class(ClassDescriptor::new("SoapVar", "soap", ClassKind::Class))
            .with_class(ClassDescriptor::new("Soap\\Sdl", "soap", ClassKind::Class))
            .with_class(ClassDescriptor::new(
                "Soap\\SoapClient",
                "soap",
                ClassKind::Class,
            ))
            .with_class(ClassDescriptor::new(
                "Soap\\SoapServer",
                "soap",
                ClassKind::Class,
            ))
            .with_class(ClassDescriptor::new(
                "Soap\\SoapFault",
                "soap",
                ClassKind::Class,
            ))
            .with_class(ClassDescriptor::new(
                "Soap\\SoapHeader",
                "soap",
                ClassKind::Class,
            ))
            .with_class(ClassDescriptor::new(
                "Soap\\SoapParam",
                "soap",
                ClassKind::Class,
            ))
            .with_class(ClassDescriptor::new(
                "Soap\\SoapVar",
                "soap",
                ClassKind::Class,
            ))
            .with_class(ClassDescriptor::new("Soap\\Url", "soap", ClassKind::Class)),
    )
}

fn with_soap_constants(mut extension: ExtensionDescriptor) -> ExtensionDescriptor {
    const INT_CONSTANTS: &[(&str, i64)] = &[
        ("SOAP_1_1", 1),
        ("SOAP_1_2", 2),
        ("SOAP_PERSISTENCE_SESSION", 1),
        ("SOAP_PERSISTENCE_REQUEST", 2),
        ("SOAP_FUNCTIONS_ALL", 999),
        ("SOAP_ENCODED", 1),
        ("SOAP_LITERAL", 2),
        ("SOAP_RPC", 1),
        ("SOAP_DOCUMENT", 2),
        ("SOAP_ACTOR_NEXT", 1),
        ("SOAP_ACTOR_NONE", 2),
        ("SOAP_ACTOR_UNLIMATERECEIVER", 3),
        ("SOAP_COMPRESSION_ACCEPT", 32),
        ("SOAP_COMPRESSION_GZIP", 0),
        ("SOAP_COMPRESSION_DEFLATE", 16),
        ("SOAP_AUTHENTICATION_BASIC", 0),
        ("SOAP_AUTHENTICATION_DIGEST", 1),
        ("UNKNOWN_TYPE", 999_998),
        ("XSD_STRING", 101),
        ("XSD_BOOLEAN", 102),
        ("XSD_DECIMAL", 103),
        ("XSD_FLOAT", 104),
        ("XSD_DOUBLE", 105),
        ("XSD_DURATION", 106),
        ("XSD_DATETIME", 107),
        ("XSD_TIME", 108),
        ("XSD_DATE", 109),
        ("XSD_GYEARMONTH", 110),
        ("XSD_GYEAR", 111),
        ("XSD_GMONTHDAY", 112),
        ("XSD_GDAY", 113),
        ("XSD_GMONTH", 114),
        ("XSD_HEXBINARY", 115),
        ("XSD_BASE64BINARY", 116),
        ("XSD_ANYURI", 117),
        ("XSD_QNAME", 118),
        ("XSD_NOTATION", 119),
        ("XSD_NORMALIZEDSTRING", 120),
        ("XSD_TOKEN", 121),
        ("XSD_LANGUAGE", 122),
        ("XSD_NMTOKEN", 123),
        ("XSD_NAME", 124),
        ("XSD_NCNAME", 125),
        ("XSD_ID", 126),
        ("XSD_IDREF", 127),
        ("XSD_IDREFS", 128),
        ("XSD_ENTITY", 129),
        ("XSD_ENTITIES", 130),
        ("XSD_INTEGER", 131),
        ("XSD_NONPOSITIVEINTEGER", 132),
        ("XSD_NEGATIVEINTEGER", 133),
        ("XSD_LONG", 134),
        ("XSD_INT", 135),
        ("XSD_SHORT", 136),
        ("XSD_BYTE", 137),
        ("XSD_NONNEGATIVEINTEGER", 138),
        ("XSD_UNSIGNEDLONG", 139),
        ("XSD_UNSIGNEDINT", 140),
        ("XSD_UNSIGNEDSHORT", 141),
        ("XSD_UNSIGNEDBYTE", 142),
        ("XSD_POSITIVEINTEGER", 143),
        ("XSD_NMTOKENS", 144),
        ("XSD_ANYTYPE", 145),
        ("XSD_ANYXML", 147),
        ("APACHE_MAP", 200),
        ("SOAP_ENC_OBJECT", 301),
        ("SOAP_ENC_ARRAY", 300),
        ("XSD_1999_TIMEINSTANT", 401),
        ("SOAP_SINGLE_ELEMENT_ARRAYS", 1),
        ("SOAP_WAIT_ONE_WAY_CALLS", 2),
        ("SOAP_USE_XSI_ARRAY_TYPE", 4),
        ("WSDL_CACHE_NONE", 0),
        ("WSDL_CACHE_DISK", 1),
        ("WSDL_CACHE_MEMORY", 2),
        ("WSDL_CACHE_BOTH", 3),
        ("SOAP_SSL_METHOD_TLS", 0),
        ("SOAP_SSL_METHOD_SSLv2", 1),
        ("SOAP_SSL_METHOD_SSLv3", 2),
        ("SOAP_SSL_METHOD_SSLv23", 3),
    ];
    for (name, value) in INT_CONSTANTS {
        extension = extension.with_constant(ConstantDescriptor::with_value(
            name,
            "soap",
            ConstantValue::Int(*value),
        ));
    }
    extension
        .with_constant(ConstantDescriptor::with_value(
            "XSD_NAMESPACE",
            "soap",
            ConstantValue::String("http://www.w3.org/2001/XMLSchema"),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "XSD_1999_NAMESPACE",
            "soap",
            ConstantValue::String("http://www.w3.org/1999/XMLSchema"),
        ))
}

pub(super) fn standard_library_ftp_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("ftp")
        .with_function(FunctionDescriptor::php("ftp_alloc", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_append", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_cdup", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_chdir", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_chmod", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_close", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_connect", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_delete", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_exec", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_fget", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_fput", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_get", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_get_option", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_login", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_mdtm", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_mkdir", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_mlsd", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_nb_continue", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_nb_fget", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_nb_fput", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_nb_get", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_nb_put", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_nlist", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_pasv", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_put", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_pwd", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_quit", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_raw", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_rawlist", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_rename", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_rmdir", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_set_option", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_site", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_size", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_ssl_connect", "ftp"))
        .with_function(FunctionDescriptor::php("ftp_systype", "ftp"))
        .with_class(ClassDescriptor::new(
            "FTP\\Connection",
            "ftp",
            ClassKind::Class,
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FTP_ASCII",
            "ftp",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FTP_TEXT",
            "ftp",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FTP_BINARY",
            "ftp",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FTP_IMAGE",
            "ftp",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FTP_AUTORESUME",
            "ftp",
            ConstantValue::Int(-1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FTP_TIMEOUT_SEC",
            "ftp",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FTP_AUTOSEEK",
            "ftp",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FTP_USEPASVADDRESS",
            "ftp",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FTP_FAILED",
            "ftp",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FTP_FINISHED",
            "ftp",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FTP_MOREDATA",
            "ftp",
            ConstantValue::Int(2),
        ))
}

pub(super) fn standard_library_ldap_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("ldap")
        .with_function(FunctionDescriptor::php("ldap_8859_to_t61", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_add", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_add_ext", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_bind", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_bind_ext", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_close", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_compare", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_connect", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_count_entries", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_count_references", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_delete", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_delete_ext", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_dn2ufn", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_err2str", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_errno", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_error", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_escape", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_exop", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_exop_passwd", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_exop_refresh", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_exop_sync", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_exop_whoami", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_explode_dn", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_first_attribute", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_first_entry", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_first_reference", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_free_result", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_get_attributes", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_get_dn", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_get_entries", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_get_option", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_get_values", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_get_values_len", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_list", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_mod_add", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_mod_add_ext", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_mod_del", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_mod_del_ext", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_mod_replace", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_mod_replace_ext", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_modify", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_modify_batch", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_next_attribute", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_next_entry", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_next_reference", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_parse_exop", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_parse_reference", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_parse_result", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_read", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_rename", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_rename_ext", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_sasl_bind", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_search", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_set_option", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_set_rebind_proc", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_start_tls", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_t61_to_8859", "ldap"))
        .with_function(FunctionDescriptor::php("ldap_unbind", "ldap"))
        .with_class(ClassDescriptor::new(
            "LDAP\\Connection",
            "ldap",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "LDAP\\Result",
            "ldap",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "LDAP\\ResultEntry",
            "ldap",
            ClassKind::Class,
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_DEREF_NEVER",
            "ldap",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_DEREF_SEARCHING",
            "ldap",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_DEREF_FINDING",
            "ldap",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_DEREF_ALWAYS",
            "ldap",
            ConstantValue::Int(3),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_MODIFY_BATCH_ADD",
            "ldap",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_MODIFY_BATCH_REMOVE",
            "ldap",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_MODIFY_BATCH_REMOVE_ALL",
            "ldap",
            ConstantValue::Int(18),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_MODIFY_BATCH_REPLACE",
            "ldap",
            ConstantValue::Int(3),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_MODIFY_BATCH_ATTRIB",
            "ldap",
            ConstantValue::String("attrib"),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_MODIFY_BATCH_MODTYPE",
            "ldap",
            ConstantValue::String("modtype"),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_MODIFY_BATCH_VALUES",
            "ldap",
            ConstantValue::String("values"),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_ESCAPE_FILTER",
            "ldap",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_ESCAPE_DN",
            "ldap",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_OPT_DEREF",
            "ldap",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_OPT_PROTOCOL_VERSION",
            "ldap",
            ConstantValue::Int(17),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_OPT_REFERRALS",
            "ldap",
            ConstantValue::Int(8),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_OPT_X_TLS_REQUIRE_CERT",
            "ldap",
            ConstantValue::Int(24582),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_OPT_X_TLS_NEVER",
            "ldap",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_OPT_X_TLS_HARD",
            "ldap",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_OPT_X_TLS_DEMAND",
            "ldap",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_OPT_X_TLS_ALLOW",
            "ldap",
            ConstantValue::Int(3),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "LDAP_OPT_X_TLS_TRY",
            "ldap",
            ConstantValue::Int(4),
        ))
}

pub(super) fn standard_library_imap_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("imap")
        .with_function(FunctionDescriptor::php("imap_8bit", "imap"))
        .with_function(FunctionDescriptor::php("imap_alerts", "imap"))
        .with_function(FunctionDescriptor::php("imap_append", "imap"))
        .with_function(FunctionDescriptor::php("imap_base64", "imap"))
        .with_function(FunctionDescriptor::php("imap_binary", "imap"))
        .with_function(FunctionDescriptor::php("imap_check", "imap"))
        .with_function(FunctionDescriptor::php("imap_close", "imap"))
        .with_function(FunctionDescriptor::php("imap_delete", "imap"))
        .with_function(FunctionDescriptor::php("imap_errors", "imap"))
        .with_function(FunctionDescriptor::php("imap_expunge", "imap"))
        .with_function(FunctionDescriptor::php("imap_fetch_overview", "imap"))
        .with_function(FunctionDescriptor::php("imap_fetchbody", "imap"))
        .with_function(FunctionDescriptor::php("imap_fetchheader", "imap"))
        .with_function(FunctionDescriptor::php("imap_fetchstructure", "imap"))
        .with_function(FunctionDescriptor::php("imap_gc", "imap"))
        .with_function(FunctionDescriptor::php("imap_headerinfo", "imap"))
        .with_function(FunctionDescriptor::php("imap_headers", "imap"))
        .with_function(FunctionDescriptor::php("imap_last_error", "imap"))
        .with_function(FunctionDescriptor::php("imap_list", "imap"))
        .with_function(FunctionDescriptor::php("imap_listscan", "imap"))
        .with_function(FunctionDescriptor::php("imap_mail_copy", "imap"))
        .with_function(FunctionDescriptor::php("imap_mail_move", "imap"))
        .with_function(FunctionDescriptor::php("imap_mailboxmsginfo", "imap"))
        .with_function(FunctionDescriptor::php("imap_num_msg", "imap"))
        .with_function(FunctionDescriptor::php("imap_num_recent", "imap"))
        .with_function(FunctionDescriptor::php("imap_open", "imap"))
        .with_function(FunctionDescriptor::php("imap_ping", "imap"))
        .with_function(FunctionDescriptor::php("imap_qprint", "imap"))
        .with_function(FunctionDescriptor::php("imap_reopen", "imap"))
        .with_function(FunctionDescriptor::php("imap_search", "imap"))
        .with_function(FunctionDescriptor::php("imap_sort", "imap"))
        .with_function(FunctionDescriptor::php("imap_status", "imap"))
        .with_function(FunctionDescriptor::php("imap_undelete", "imap"))
        .with_function(FunctionDescriptor::php("imap_utf8", "imap"))
        .with_function(FunctionDescriptor::php("imap_utf7_decode", "imap"))
        .with_function(FunctionDescriptor::php("imap_utf7_encode", "imap"))
        .with_class(ClassDescriptor::new(
            "IMAP\\Connection",
            "imap",
            ClassKind::Class,
        ))
        .with_constant(ConstantDescriptor::with_value(
            "NIL",
            "imap",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "OP_DEBUG",
            "imap",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "OP_READONLY",
            "imap",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "OP_ANONYMOUS",
            "imap",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "OP_HALFOPEN",
            "imap",
            ConstantValue::Int(64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "OP_EXPUNGE",
            "imap",
            ConstantValue::Int(32768),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CL_EXPUNGE",
            "imap",
            ConstantValue::Int(32768),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FT_UID",
            "imap",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FT_PEEK",
            "imap",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FT_INTERNAL",
            "imap",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FT_PREFETCHTEXT",
            "imap",
            ConstantValue::Int(32),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ST_UID",
            "imap",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "CP_UID",
            "imap",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SE_UID",
            "imap",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SA_MESSAGES",
            "imap",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SA_RECENT",
            "imap",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SA_UNSEEN",
            "imap",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SA_UIDNEXT",
            "imap",
            ConstantValue::Int(8),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SA_UIDVALIDITY",
            "imap",
            ConstantValue::Int(16),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SA_ALL",
            "imap",
            ConstantValue::Int(31),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SORTDATE",
            "imap",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SORTARRIVAL",
            "imap",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SORTFROM",
            "imap",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SORTSUBJECT",
            "imap",
            ConstantValue::Int(3),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SORTTO",
            "imap",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SORTCC",
            "imap",
            ConstantValue::Int(5),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SORTSIZE",
            "imap",
            ConstantValue::Int(6),
        ))
}

pub(super) fn standard_library_ssh2_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("ssh2")
        .with_function(FunctionDescriptor::php("ssh2_auth_hostbased_file", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_auth_none", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_auth_password", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_auth_pubkey_file", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_connect", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_disconnect", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_exec", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_fingerprint", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_forward_accept", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_forward_listen", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_methods_negotiated", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_publickey_add", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_publickey_init", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_publickey_list", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_publickey_remove", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_scp_recv", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_scp_send", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_sftp", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_sftp_chmod", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_sftp_lstat", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_sftp_mkdir", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_sftp_readlink", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_sftp_realpath", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_sftp_rename", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_sftp_rmdir", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_sftp_stat", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_sftp_symlink", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_sftp_unlink", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_shell", "ssh2"))
        .with_function(FunctionDescriptor::php("ssh2_tunnel", "ssh2"))
        .with_class(ClassDescriptor::new(
            "SSH2\\Session",
            "ssh2",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new("SSH2\\Sftp", "ssh2", ClassKind::Class))
        .with_class(ClassDescriptor::new(
            "SSH2\\Channel",
            "ssh2",
            ClassKind::Class,
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SSH2_FINGERPRINT_MD5",
            "ssh2",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SSH2_FINGERPRINT_SHA1",
            "ssh2",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SSH2_FINGERPRINT_HEX",
            "ssh2",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SSH2_FINGERPRINT_RAW",
            "ssh2",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SSH2_TERM_UNIT_CHARS",
            "ssh2",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SSH2_TERM_UNIT_PIXELS",
            "ssh2",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SSH2_DEFAULT_TERMINAL",
            "ssh2",
            ConstantValue::String("vanilla"),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SSH2_DEFAULT_TERM_WIDTH",
            "ssh2",
            ConstantValue::Int(80),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SSH2_DEFAULT_TERM_HEIGHT",
            "ssh2",
            ConstantValue::Int(25),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SSH2_DEFAULT_TERM_UNIT",
            "ssh2",
            ConstantValue::Int(0),
        ))
}

pub(super) fn standard_library_sockets_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("sockets")
        .with_function(FunctionDescriptor::php("inet_ntop", "sockets"))
        .with_function(FunctionDescriptor::php("inet_pton", "sockets"))
        .with_function(FunctionDescriptor::php("socket_accept", "sockets"))
        .with_function(FunctionDescriptor::php("socket_bind", "sockets"))
        .with_function(FunctionDescriptor::php("socket_clear_error", "sockets"))
        .with_function(FunctionDescriptor::php("socket_close", "sockets"))
        .with_function(FunctionDescriptor::php("socket_connect", "sockets"))
        .with_function(FunctionDescriptor::php("socket_create", "sockets"))
        .with_function(FunctionDescriptor::php("socket_getpeername", "sockets"))
        .with_function(FunctionDescriptor::php("socket_getsockname", "sockets"))
        .with_function(FunctionDescriptor::php("socket_last_error", "sockets"))
        .with_function(FunctionDescriptor::php("socket_listen", "sockets"))
        .with_function(FunctionDescriptor::php("socket_read", "sockets"))
        .with_function(FunctionDescriptor::php("socket_recv", "sockets"))
        .with_function(FunctionDescriptor::php("socket_send", "sockets"))
        .with_function(FunctionDescriptor::php("socket_shutdown", "sockets"))
        .with_function(FunctionDescriptor::php("socket_strerror", "sockets"))
        .with_function(FunctionDescriptor::php("socket_write", "sockets"))
        .with_class(ClassDescriptor::new(
            "AddressInfo",
            "sockets",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new("Socket", "sockets", ClassKind::Class))
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
            "PHP_NORMAL_READ",
            "sockets",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "PHP_BINARY_READ",
            "sockets",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MSG_OOB",
            "sockets",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "MSG_WAITALL",
            "sockets",
            ConstantValue::Int(64),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SOL_SOCKET",
            "sockets",
            ConstantValue::Int(0xffff),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SOL_TCP",
            "sockets",
            ConstantValue::Int(6),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SHUT_RD",
            "sockets",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SHUT_WR",
            "sockets",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "SHUT_RDWR",
            "sockets",
            ConstantValue::Int(2),
        ))
}

pub(super) fn standard_library_zlib_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("zlib")
        .with_class(ClassDescriptor::new(
            "DeflateContext",
            "zlib",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "InflateContext",
            "zlib",
            ClassKind::Class,
        ))
        .with_function(FunctionDescriptor::php("deflate_add", "zlib"))
        .with_function(FunctionDescriptor::php("deflate_init", "zlib"))
        .with_function(FunctionDescriptor::php("gzclose", "zlib"))
        .with_function(FunctionDescriptor::php("gzdeflate", "zlib"))
        .with_function(FunctionDescriptor::php("gzcompress", "zlib"))
        .with_function(FunctionDescriptor::php("gzdecode", "zlib"))
        .with_function(FunctionDescriptor::php("gzencode", "zlib"))
        .with_function(FunctionDescriptor::php("gzeof", "zlib"))
        .with_function(FunctionDescriptor::php("gzfile", "zlib"))
        .with_function(FunctionDescriptor::php("gzgetc", "zlib"))
        .with_function(FunctionDescriptor::php("gzgets", "zlib"))
        .with_function(FunctionDescriptor::php("gzopen", "zlib"))
        .with_function(FunctionDescriptor::php("gzpassthru", "zlib"))
        .with_function(FunctionDescriptor::php("gzputs", "zlib"))
        .with_function(FunctionDescriptor::php("gzread", "zlib"))
        .with_function(FunctionDescriptor::php("gzrewind", "zlib"))
        .with_function(FunctionDescriptor::php("gzseek", "zlib"))
        .with_function(FunctionDescriptor::php("gztell", "zlib"))
        .with_function(FunctionDescriptor::php("gzwrite", "zlib"))
        .with_function(FunctionDescriptor::php("gzinflate", "zlib"))
        .with_function(FunctionDescriptor::php("gzuncompress", "zlib"))
        .with_function(FunctionDescriptor::php("inflate_add", "zlib"))
        .with_function(FunctionDescriptor::php("inflate_get_read_len", "zlib"))
        .with_function(FunctionDescriptor::php("inflate_get_status", "zlib"))
        .with_function(FunctionDescriptor::php("inflate_init", "zlib"))
        .with_function(FunctionDescriptor::php("readgzfile", "zlib"))
        .with_function(FunctionDescriptor::php("zlib_decode", "zlib"))
        .with_function(FunctionDescriptor::php("zlib_encode", "zlib"))
        .with_function(FunctionDescriptor::php("zlib_get_coding_type", "zlib"))
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
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_NO_FLUSH",
            "zlib",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_PARTIAL_FLUSH",
            "zlib",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_SYNC_FLUSH",
            "zlib",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_FULL_FLUSH",
            "zlib",
            ConstantValue::Int(3),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_BLOCK",
            "zlib",
            ConstantValue::Int(5),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_FINISH",
            "zlib",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_FILTERED",
            "zlib",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_HUFFMAN_ONLY",
            "zlib",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_RLE",
            "zlib",
            ConstantValue::Int(3),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_FIXED",
            "zlib",
            ConstantValue::Int(4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_DEFAULT_STRATEGY",
            "zlib",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_VERSION",
            "zlib",
            ConstantValue::String("1.3.1"),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_VERNUM",
            "zlib",
            ConstantValue::Int(4880),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_OK",
            "zlib",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_STREAM_END",
            "zlib",
            ConstantValue::Int(1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_NEED_DICT",
            "zlib",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_ERRNO",
            "zlib",
            ConstantValue::Int(-1),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_STREAM_ERROR",
            "zlib",
            ConstantValue::Int(-2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_DATA_ERROR",
            "zlib",
            ConstantValue::Int(-3),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_MEM_ERROR",
            "zlib",
            ConstantValue::Int(-4),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_BUF_ERROR",
            "zlib",
            ConstantValue::Int(-5),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "ZLIB_VERSION_ERROR",
            "zlib",
            ConstantValue::Int(-6),
        ))
}

pub(super) fn standard_library_zip_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("zip")
        .with_class(ClassDescriptor::new("ZipArchive", "zip", ClassKind::Class))
        .with_function(FunctionDescriptor::php("zip_close", "zip"))
        .with_function(FunctionDescriptor::php("zip_entry_close", "zip"))
        .with_function(FunctionDescriptor::php("zip_entry_compressedsize", "zip"))
        .with_function(FunctionDescriptor::php(
            "zip_entry_compressionmethod",
            "zip",
        ))
        .with_function(FunctionDescriptor::php("zip_entry_filesize", "zip"))
        .with_function(FunctionDescriptor::php("zip_entry_name", "zip"))
        .with_function(FunctionDescriptor::php("zip_entry_open", "zip"))
        .with_function(FunctionDescriptor::php("zip_entry_read", "zip"))
        .with_function(FunctionDescriptor::php("zip_open", "zip"))
        .with_function(FunctionDescriptor::php("zip_read", "zip"))
}

pub(super) fn standard_library_fileinfo_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("fileinfo")
        .with_function(FunctionDescriptor::php("finfo_buffer", "fileinfo"))
        .with_function(FunctionDescriptor::php("finfo_close", "fileinfo"))
        .with_function(FunctionDescriptor::php("finfo_file", "fileinfo"))
        .with_function(FunctionDescriptor::php("finfo_open", "fileinfo"))
        .with_function(FunctionDescriptor::php("finfo_set_flags", "fileinfo"))
        .with_constant(ConstantDescriptor::with_value(
            "FILEINFO_NONE",
            "fileinfo",
            ConstantValue::Int(0),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILEINFO_SYMLINK",
            "fileinfo",
            ConstantValue::Int(2),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILEINFO_DEVICES",
            "fileinfo",
            ConstantValue::Int(8),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILEINFO_MIME_TYPE",
            "fileinfo",
            ConstantValue::Int(16),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILEINFO_CONTINUE",
            "fileinfo",
            ConstantValue::Int(32),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILEINFO_PRESERVE_ATIME",
            "fileinfo",
            ConstantValue::Int(128),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILEINFO_RAW",
            "fileinfo",
            ConstantValue::Int(256),
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
        .with_constant(ConstantDescriptor::with_value(
            "FILEINFO_APPLE",
            "fileinfo",
            ConstantValue::Int(2048),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "FILEINFO_EXTENSION",
            "fileinfo",
            ConstantValue::Int(16777216),
        ))
}

pub(super) fn standard_library_ffi_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("ffi")
        .with_class(ClassDescriptor::new("FFI", "ffi", ClassKind::Class))
        .with_class(ClassDescriptor::new("FFI\\CData", "ffi", ClassKind::Class))
        .with_class(ClassDescriptor::new("FFI\\CType", "ffi", ClassKind::Class))
        .with_class(ClassDescriptor::new(
            "FFI\\Exception",
            "ffi",
            ClassKind::Class,
        ))
        .with_class(ClassDescriptor::new(
            "FFI\\ParserException",
            "ffi",
            ClassKind::Class,
        ))
}

pub(super) fn standard_library_exif_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("exif")
        .with_constant(ConstantDescriptor::with_value(
            "EXIF_USE_MBSTRING",
            "exif",
            ConstantValue::Bool(false),
        ))
        .with_function(FunctionDescriptor::php("exif_imagetype", "exif"))
        .with_function(FunctionDescriptor::php("exif_read_data", "exif"))
        .with_function(FunctionDescriptor::php("exif_tagname", "exif"))
        .with_function(FunctionDescriptor::php("exif_thumbnail", "exif"))
}

pub(super) fn standard_library_gd_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("gd")
        .with_constant(ConstantDescriptor::with_value(
            "IMG_GIF",
            "gd",
            ConstantValue::Int(constants::IMG_GIF),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMG_JPG",
            "gd",
            ConstantValue::Int(constants::IMG_JPG),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMG_JPEG",
            "gd",
            ConstantValue::Int(constants::IMG_JPEG),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMG_PNG",
            "gd",
            ConstantValue::Int(constants::IMG_PNG),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMG_WEBP",
            "gd",
            ConstantValue::Int(constants::IMG_WEBP),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "IMG_AVIF",
            "gd",
            ConstantValue::Int(constants::IMG_AVIF),
        ))
        .with_function(FunctionDescriptor::php("gd_info", "gd"))
        .with_function(FunctionDescriptor::php("imagecopyresampled", "gd"))
        .with_function(FunctionDescriptor::php("imagecreatefromjpeg", "gd"))
        .with_function(FunctionDescriptor::php("imagecreatefrompng", "gd"))
        .with_function(FunctionDescriptor::php("imagecreatefromstring", "gd"))
        .with_function(FunctionDescriptor::php("imagecreatetruecolor", "gd"))
        .with_function(FunctionDescriptor::php("imagetypes", "gd"))
        .with_function(FunctionDescriptor::php("imagedestroy", "gd"))
        .with_function(FunctionDescriptor::php("imagejpeg", "gd"))
        .with_function(FunctionDescriptor::php("imagepng", "gd"))
        .with_function(FunctionDescriptor::php("imagesx", "gd"))
        .with_function(FunctionDescriptor::php("imagesy", "gd"))
        .with_class(ClassDescriptor::new("GdImage", "gd", ClassKind::Class))
}

pub(super) fn standard_library_random_extension() -> ExtensionDescriptor {
    with_generated_classes(
        ExtensionDescriptor::new("random")
            .with_function(FunctionDescriptor::php("random_bytes", "random"))
            .with_function(FunctionDescriptor::php("random_int", "random")),
        "random",
    )
}

pub(super) fn standard_library_date_extension() -> ExtensionDescriptor {
    ExtensionDescriptor::new("date")
        .with_constant(ConstantDescriptor::with_value(
            "DATE_ATOM",
            "date",
            ConstantValue::String(constants::DATE_ATOM),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "DATE_COOKIE",
            "date",
            ConstantValue::String(constants::DATE_COOKIE),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "DATE_ISO8601",
            "date",
            ConstantValue::String(constants::DATE_ISO8601),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "DATE_ISO8601_EXPANDED",
            "date",
            ConstantValue::String(constants::DATE_ISO8601_EXPANDED),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "DATE_RFC1036",
            "date",
            ConstantValue::String(constants::DATE_RFC1036),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "DATE_RFC1123",
            "date",
            ConstantValue::String(constants::DATE_RFC1123),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "DATE_RFC2822",
            "date",
            ConstantValue::String(constants::DATE_RFC2822),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "DATE_RFC3339",
            "date",
            ConstantValue::String(constants::DATE_RFC3339),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "DATE_RFC3339_EXTENDED",
            "date",
            ConstantValue::String(constants::DATE_RFC3339_EXTENDED),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "DATE_RFC7231",
            "date",
            ConstantValue::String(constants::DATE_RFC7231),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "DATE_RFC822",
            "date",
            ConstantValue::String(constants::DATE_RFC822),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "DATE_RFC850",
            "date",
            ConstantValue::String(constants::DATE_RFC850),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "DATE_RSS",
            "date",
            ConstantValue::String(constants::DATE_RSS),
        ))
        .with_constant(ConstantDescriptor::with_value(
            "DATE_W3C",
            "date",
            ConstantValue::String(constants::DATE_W3C),
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
    with_generated_classes(
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
            )),
        "spl",
    )
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
    with_generated_classes(ExtensionDescriptor::new("reflection"), "reflection")
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
    extension = extension.with_constant(ConstantDescriptor::with_value(
        "T_PAAMAYIM_NEKUDOTAYIM",
        "tokenizer",
        ConstantValue::Int(php_runtime::api::tokenizer::token_name_id(
            php_lexer::TokenName::DoubleColon,
        )),
    ));
    extension
}
