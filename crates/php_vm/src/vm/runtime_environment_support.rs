use super::*;

#[cold]
pub(super) fn is_autoload_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "spl_autoload"
            | "spl_autoload_extensions"
            | "spl_autoload_register"
            | "spl_autoload_unregister"
            | "spl_autoload_functions"
            | "spl_autoload_call"
    )
}

pub(super) fn is_symbol_introspection_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "define"
            | "defined"
            | "constant"
            | "extension_loaded"
            | "function_exists"
            | "compact"
            | "clone"
            | "class_exists"
            | "class_alias"
            | "call_user_func"
            | "call_user_func_array"
            | "forward_static_call"
            | "debug_backtrace"
            | "debug_print_backtrace"
            | "func_get_arg"
            | "func_get_args"
            | "func_num_args"
            | "get_called_class"
            | "interface_exists"
            | "trait_exists"
            | "enum_exists"
            | "method_exists"
            | "property_exists"
            | "is_callable"
            | "is_a"
            | "is_subclass_of"
            | "get_class"
            | "get_class_methods"
            | "get_class_vars"
            | "get_parent_class"
            | "class_parents"
            | "class_implements"
            | "get_declared_classes"
            | "get_declared_interfaces"
            | "get_declared_traits"
            | "get_defined_functions"
            | "get_defined_constants"
            | "get_defined_vars"
            | "get_extension_funcs"
            | "get_included_files"
            | "get_loaded_extensions"
            | "get_required_files"
            | "phpversion"
            | "zend_version"
            | "get_mangled_object_vars"
            | "get_object_vars"
    )
}

pub(super) fn is_config_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "ignore_user_abort" | "ini_get" | "ini_set" | "ini_get_all" | "get_cfg_var"
    )
}

pub(super) fn is_error_handling_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "error_reporting"
            | "error_log"
            | "set_error_handler"
            | "get_error_handler"
            | "restore_error_handler"
            | "error_get_last"
            | "register_shutdown_function"
            | "trigger_error"
            | "user_error"
            | "set_exception_handler"
            | "get_exception_handler"
            | "restore_exception_handler"
    )
}

pub(super) fn is_output_buffering_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "ob_start"
            | "ob_get_contents"
            | "ob_get_clean"
            | "ob_get_flush"
            | "ob_get_length"
            | "ob_get_level"
            | "ob_end_clean"
            | "ob_end_flush"
            | "flush"
    )
}

pub(super) fn is_environment_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "getenv"
            | "putenv"
            | "php_sapi_name"
            | "php_uname"
            | "get_current_user"
            | "getmyuid"
            | "getmygid"
    )
}

pub(super) fn is_process_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "proc_open"
            | "proc_close"
            | "proc_get_status"
            | "popen"
            | "pclose"
            | "shell_exec"
            | "exec"
            | "passthru"
            | "system"
    )
}

pub(super) fn error_handler_callback_from_value(
    compiled: &CompiledUnit,
    value: Value,
) -> Result<CallableValue, String> {
    match value {
        Value::Callable(callable) => match *callable {
            CallableValue::UserFunction { name } => {
                let normalized = normalize_function_name(&name);
                if compiled.lookup_function(&normalized).is_some() {
                    Ok(CallableValue::UserFunction { name: normalized })
                } else if BuiltinRegistry::new().contains(&normalized) {
                    Ok(CallableValue::InternalBuiltin { name: normalized })
                } else {
                    Err(format!(
                        "E_PHP_VM_ERROR_INVALID_CALLBACK: function {name} is not callable"
                    ))
                }
            }
            CallableValue::Closure(payload) => Ok(CallableValue::Closure(payload)),
            CallableValue::InternalBuiltin { name } => {
                if BuiltinRegistry::new().contains(&name) {
                    Ok(CallableValue::InternalBuiltin { name })
                } else {
                    Err(format!(
                        "E_PHP_VM_ERROR_INVALID_CALLBACK: builtin {name} is not callable"
                    ))
                }
            }
            other_callable => Err(format!(
                "E_PHP_VM_ERROR_INVALID_CALLBACK: value of type {} is not callable",
                value_type_name(&Value::Callable(Box::new(other_callable)))
            )),
        },
        Value::String(name) => {
            let name = normalize_function_name(&name.to_string_lossy());
            if compiled.lookup_function(&name).is_some() {
                Ok(CallableValue::UserFunction { name })
            } else if BuiltinRegistry::new().contains(&name) {
                Ok(CallableValue::InternalBuiltin { name })
            } else {
                Err(format!(
                    "E_PHP_VM_ERROR_INVALID_CALLBACK: function {name} is not callable"
                ))
            }
        }
        other => Err(format!(
            "E_PHP_VM_ERROR_INVALID_CALLBACK: value of type {} is not callable",
            value_type_name(&other)
        )),
    }
}

pub(super) fn autoload_callback_from_value(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    value: Value,
) -> Result<CallableValue, String> {
    match value {
        Value::Callable(callable) => match *callable {
            CallableValue::UserFunction { name } => {
                let normalized = normalize_function_name(&name);
                if compiled.lookup_function(&normalized).is_some()
                    || dynamic_function_in_state(state, &normalized).is_some()
                    || BuiltinRegistry::new().contains(&normalized)
                {
                    Ok(CallableValue::UserFunction { name: normalized })
                } else {
                    Err(format!(
                        "function \"{name}\" not found or invalid function name"
                    ))
                }
            }
            CallableValue::Closure(payload) => Ok(CallableValue::Closure(payload)),
            CallableValue::InternalBuiltin { name } => {
                if BuiltinRegistry::new().contains(&name) {
                    Ok(CallableValue::InternalBuiltin { name })
                } else {
                    Err(format!("builtin {name} is not callable"))
                }
            }
            other_callable => Err(format!(
                "value of type {} is not callable",
                value_type_name(&Value::Callable(Box::new(other_callable)))
            )),
        },
        Value::String(name) => {
            let name = name.to_string_lossy();
            if let Some((class_name, method)) = name.split_once("::") {
                autoload_class_method_callback(compiled, state, class_name, method, true)
            } else {
                let normalized = normalize_function_name(&name);
                if compiled.lookup_function(&normalized).is_some()
                    || dynamic_function_in_state(state, &normalized).is_some()
                {
                    Ok(CallableValue::UserFunction { name: normalized })
                } else if BuiltinRegistry::new().contains(&normalized)
                    || is_autoload_builtin_name(&normalized)
                {
                    Ok(CallableValue::InternalBuiltin { name: normalized })
                } else {
                    Err(format!(
                        "function \"{name}\" not found or invalid function name"
                    ))
                }
            }
        }
        Value::Array(array) => {
            let elements = array
                .iter()
                .map(|(_, value)| value.clone())
                .collect::<Vec<_>>();
            let [target, method]: [Value; 2] = match elements.try_into() {
                Ok(elements) => elements,
                Err(_) => {
                    return Err("callable arrays must contain exactly target and method".to_owned());
                }
            };
            let Some(method) = callable_string_value(method) else {
                return Err("callable array method must be string".to_owned());
            };
            match callable_resolve_reference(target) {
                Value::Object(object) => {
                    let class_name = object.class_name();
                    let resolved = autoload_resolve_method(compiled, state, &class_name, &method)?;
                    if resolved.method.flags.is_static {
                        Ok(CallableValue::BoundMethod {
                            target: CallableMethodTarget::Class(object.display_name()),
                            method,
                            scope: Some(normalize_class_name(&class_name)),
                        })
                    } else {
                        Ok(CallableValue::BoundMethod {
                            target: CallableMethodTarget::Object(object),
                            method,
                            scope: None,
                        })
                    }
                }
                Value::String(class_name) => {
                    let class_name = class_name.to_string_lossy();
                    autoload_class_method_callback(compiled, state, &class_name, &method, true)
                }
                other => Err(format!(
                    "callable array target must be object or class string, got {}",
                    value_type_name(&other)
                )),
            }
        }
        Value::Object(object) => {
            if lookup_method_in_state(compiled, state, &object.class_name(), "__invoke")
                .map(|method| method.is_some())?
            {
                Ok(CallableValue::BoundMethod {
                    target: CallableMethodTarget::Object(object),
                    method: "__invoke".to_owned(),
                    scope: None,
                })
            } else {
                Err(format!(
                    "object of class {} is not callable",
                    object.class_name()
                ))
            }
        }
        other => Err(format!(
            "value of type {} is not callable",
            value_type_name(&other)
        )),
    }
}

pub(super) fn autoload_invalid_callback_error(function_name: &str, reason: &str) -> String {
    format!(
        "E_PHP_VM_AUTOLOAD_INVALID_CALLBACK: {function_name}(): Argument #1 ($callback) must be a valid callback or null, {reason}"
    )
}

pub(super) fn autoload_class_method_callback(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    method: &str,
    require_static: bool,
) -> Result<CallableValue, String> {
    let resolved = autoload_resolve_method(compiled, state, class_name, method)?;
    if require_static && !resolved.method.flags.is_static {
        return Err(format!(
            "non-static method {}::{}() cannot be called statically",
            resolved.class.display_name, method
        ));
    }
    let target_display = lookup_class_in_state(compiled, state, class_name)
        .map(|class| class.display_name.clone())
        .unwrap_or_else(|| display_class_name(class_name));
    Ok(CallableValue::BoundMethod {
        target: CallableMethodTarget::Class(target_display),
        method: method.to_owned(),
        scope: Some(normalize_class_name(&resolved.class.name)),
    })
}

pub(super) fn autoload_resolve_method(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    method: &str,
) -> Result<ResolvedMethodOwned, String> {
    let display_name = callable_class_display_name(compiled, state, class_name);
    let Some(class) = lookup_class_in_state(compiled, state, class_name) else {
        return Err(format!("class {display_name} does not exist"));
    };
    let Some(resolved) =
        lookup_resolved_method_in_state(compiled, state, &class.name, method, None)?
    else {
        return Err(format!(
            "class {} does not have a method \"{method}\"",
            class.display_name
        ));
    };
    if resolved.method.flags.is_private || resolved.method.flags.is_protected {
        return Err(format!(
            "cannot access {} method {}::{}()",
            method_visibility_name(resolved.method.flags),
            resolved.class.display_name,
            method
        ));
    }
    Ok(resolved)
}

pub(super) fn class_like_exists_direct(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
    kind: AutoloadClassLookupKind,
) -> bool {
    if lookup_class_in_state(compiled, state, class_name).is_some_and(|class| match kind {
        AutoloadClassLookupKind::ClassLike => true,
        AutoloadClassLookupKind::Interface => class.flags.is_interface,
        AutoloadClassLookupKind::Enum => class.flags.is_enum,
        AutoloadClassLookupKind::Trait => class.flags.is_trait,
        AutoloadClassLookupKind::Class => !class.flags.is_interface && !class.flags.is_trait,
    }) {
        return true;
    }

    php_std::ExtensionRegistry::standard_library()
        .enabled_class(class_name)
        .is_some_and(|class| match kind {
            AutoloadClassLookupKind::ClassLike => true,
            AutoloadClassLookupKind::Interface => class.kind() == php_std::ClassKind::Interface,
            AutoloadClassLookupKind::Enum => class.kind() == php_std::ClassKind::Enum,
            AutoloadClassLookupKind::Trait => class.kind() == php_std::ClassKind::Trait,
            AutoloadClassLookupKind::Class => class.kind() == php_std::ClassKind::Class,
        })
}

pub(super) fn class_dependency_display_name(
    compiled: &CompiledUnit,
    class: &php_ir::module::ClassEntry,
    normalized_dependency: &str,
) -> String {
    let normalized_dependency = normalize_class_name(normalized_dependency);
    if class
        .parent
        .as_deref()
        .is_some_and(|parent| normalize_class_name(parent) == normalized_dependency)
        && let Some(display) = class.parent_display_name.as_deref()
    {
        return display.to_owned();
    }
    let Some(file) = compiled.unit().files.get(class.span.file.index()) else {
        return normalized_dependency;
    };
    let Ok(source) = std::fs::read_to_string(&file.path) else {
        return normalized_dependency;
    };
    if let Some(display) =
        class_dependency_import_display_name(&source, class, &normalized_dependency)
    {
        return display;
    }
    let start = (class.span.start as usize).min(source.len());
    let end = (class.span.end as usize).min(source.len()).max(start);
    let declaration = source[start..end]
        .split_once('{')
        .map_or(&source[start..end], |(head, _)| head);
    for token in php_name_tokens(declaration) {
        if normalize_class_name(&token) == normalized_dependency {
            return display_class_name(&token);
        }
        if class_name_tail(&token).eq_ignore_ascii_case(class_name_tail(&normalized_dependency))
            && let Some(display) = class_dependency_namespace_display_name(
                &source,
                class,
                &token,
                &normalized_dependency,
            )
        {
            return display;
        }
    }
    normalized_dependency
}

pub(super) fn class_dependency_import_display_name(
    source: &str,
    class: &php_ir::module::ClassEntry,
    normalized_dependency: &str,
) -> Option<String> {
    let header_end = (class.span.start as usize).min(source.len());
    let header = source.get(..header_end)?;
    let dependency_tail = class_name_tail(normalized_dependency);
    for line in header.lines() {
        let line = line.trim();
        let Some(imports) = line.strip_prefix("use ") else {
            continue;
        };
        if let Some(display) = class_dependency_import_display_name_from_imports(
            imports,
            dependency_tail,
            normalized_dependency,
        ) {
            return Some(display);
        }
    }
    for statement in header.split(';') {
        let statement = statement.trim();
        let Some(imports) = statement.strip_prefix("use ") else {
            continue;
        };
        if let Some(display) = class_dependency_import_display_name_from_imports(
            imports,
            dependency_tail,
            normalized_dependency,
        ) {
            return Some(display);
        }
    }
    None
}

pub(super) fn class_dependency_import_display_name_from_imports(
    imports: &str,
    dependency_tail: &str,
    normalized_dependency: &str,
) -> Option<String> {
    let imports = imports.trim().trim_end_matches(';').trim();
    if imports.starts_with("function ") || imports.starts_with("const ") {
        return None;
    }
    for import in imports.split(',') {
        let import = import.trim();
        if import.contains('{') || import.contains('}') {
            continue;
        }
        let (name, alias) = split_import_alias(import);
        let name = name.trim().trim_start_matches('\\');
        if name.is_empty() {
            continue;
        }
        let alias = alias
            .map(str::trim)
            .filter(|alias| !alias.is_empty())
            .unwrap_or_else(|| class_name_tail(name));
        if alias.eq_ignore_ascii_case(dependency_tail)
            && normalize_class_name(name) == normalized_dependency
        {
            return Some(name.to_owned());
        }
    }
    None
}

pub(super) fn class_dependency_namespace_display_name(
    source: &str,
    class: &php_ir::module::ClassEntry,
    token: &str,
    normalized_dependency: &str,
) -> Option<String> {
    if token.contains('\\') {
        return None;
    }
    let namespace = class_declaration_namespace_display_name(source, class)?;
    let candidate = format!("{namespace}\\{token}");
    (normalize_class_name(&candidate) == normalized_dependency).then_some(candidate)
}

pub(super) fn class_declaration_namespace_display_name(
    source: &str,
    class: &php_ir::module::ClassEntry,
) -> Option<String> {
    let header_end = (class.span.start as usize).min(source.len());
    let header = source.get(..header_end)?;
    for statement in header.split(';') {
        let statement = statement.trim();
        let marker = "namespace ";
        let Some(index) = statement.find(marker) else {
            continue;
        };
        let namespace = statement[index + marker.len()..].trim();
        if namespace.is_empty() || namespace.starts_with('{') {
            continue;
        }
        let namespace = namespace
            .split_whitespace()
            .next()
            .unwrap_or(namespace)
            .trim_matches('{')
            .trim();
        if !namespace.is_empty() {
            return Some(namespace.trim_start_matches('\\').to_owned());
        }
    }
    None
}

pub(super) fn split_import_alias(import: &str) -> (&str, Option<&str>) {
    let lower = import.to_ascii_lowercase();
    if let Some(index) = lower.rfind(" as ") {
        (&import[..index], Some(&import[index + 4..]))
    } else {
        (import, None)
    }
}

pub(super) fn class_name_tail(name: &str) -> &str {
    name.trim_start_matches('\\')
        .rsplit('\\')
        .next()
        .unwrap_or(name)
}

pub(super) fn should_defer_class_dependency_validation(class: &php_ir::module::ClassEntry) -> bool {
    class.name.starts_with("__phrust_anonymous_") || class.display_name.starts_with("anonymous#")
}

pub(super) fn php_name_tokens(source: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in source.chars() {
        if ch == '\\' || ch == '_' || ch.is_alphanumeric() {
            current.push(ch);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

pub(super) fn is_valid_autoload_class_name(class_name: &str) -> bool {
    let name = class_name.strip_prefix('\\').unwrap_or(class_name);
    if name.is_empty() {
        return false;
    }
    name.split('\\')
        .all(|segment| is_valid_autoload_class_name_segment(segment.as_bytes()))
}

pub(super) fn is_valid_autoload_class_name_segment(segment: &[u8]) -> bool {
    let Some((&first, rest)) = segment.split_first() else {
        return false;
    };
    is_php_name_start_byte(first) && rest.iter().copied().all(is_php_name_byte)
}

pub(super) fn is_php_name_start_byte(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic() || byte >= 0x80
}

pub(super) fn is_php_name_byte(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric() || byte >= 0x80
}

impl AutoloadClassLookupKind {
    pub(super) const fn exists_function_name(self) -> &'static str {
        match self {
            Self::ClassLike => "class_exists",
            Self::Class => "class_exists",
            Self::Interface => "interface_exists",
            Self::Trait => "trait_exists",
            Self::Enum => "enum_exists",
        }
    }
}

pub(super) fn autoload_trace_origin_from_call_site(
    compiled: &CompiledUnit,
    function_name: &'static str,
    call_site: Option<(u64, FunctionId, BlockId, InstrId)>,
) -> Option<AutoloadTraceOrigin> {
    let (_, function_id, block_id, instruction_id) = call_site?;
    let function = compiled.unit().functions.get(function_id.index())?;
    let block = function.blocks.get(block_id.index())?;
    let instruction = block
        .instructions
        .iter()
        .find(|instruction| instruction.id == instruction_id)?;
    Some(AutoloadTraceOrigin {
        function_name,
        span: instruction.span,
    })
}

pub(super) fn capture_autoload_trace(
    compiled: &CompiledUnit,
    stack: &CallStack,
    callback: &CallableValue,
    class_name: &str,
    origin: AutoloadTraceOrigin,
) -> String {
    let class_arg = format_trace_arg(&Value::string(class_name.as_bytes().to_vec()));
    let callback = autoload_trace_callback_name(callback);
    let mut lines = vec![format!("#0 [internal function]: {callback}({class_arg})")];
    let file = compiled
        .unit()
        .files
        .get(origin.span.file.index())
        .map(|file| file.path.clone())
        .unwrap_or_default();
    let line = source_span_display_line(compiled, origin.span, false)
        .unwrap_or_else(|| i64::from(origin.span.start));
    lines.push(format!(
        "#1 {file}({line}): {}({class_arg})",
        origin.function_name
    ));
    let rest = capture_backtrace_string_from_index(compiled, stack, 2);
    if !rest.is_empty() {
        lines.push(rest);
    }
    lines.join("\n")
}

pub(super) fn autoload_trace_callback_name(callback: &CallableValue) -> String {
    match callback {
        CallableValue::UserFunction { name } | CallableValue::InternalBuiltin { name } => {
            name.clone()
        }
        CallableValue::Closure(_) => "{closure}".to_owned(),
        CallableValue::BoundMethod { target, method, .. } => {
            let target = match target {
                CallableMethodTarget::Object(object) => object.display_name(),
                CallableMethodTarget::Class(class_name) => class_name.clone(),
            };
            format!("{target}->{method}")
        }
        CallableValue::MethodPlaceholder { target }
        | CallableValue::UnresolvedDynamic { target } => target.clone(),
    }
}

pub(super) fn dynamic_class_owner_in_state(
    state: &ExecutionState,
    class_name: &str,
) -> Option<CompiledUnit> {
    let unit_index = dynamic_class_owner_index_in_state(state, class_name)?;
    state.dynamic_units.get(unit_index).cloned()
}

pub(super) fn class_owner_in_state(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    class_name: &str,
) -> CompiledUnit {
    dynamic_class_owner_in_state(state, class_name).unwrap_or_else(|| compiled.clone())
}

pub(super) fn destructor_entry_owner(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    entry: &DestructorEntry,
) -> CompiledUnit {
    entry
        .owner_dynamic_unit_index
        .and_then(|unit_index| state.dynamic_units.get(unit_index).cloned())
        .unwrap_or_else(|| compiled.clone())
}

pub(super) fn dynamic_class_owner_index_in_state(
    state: &ExecutionState,
    class_name: &str,
) -> Option<usize> {
    let normalized = normalize_class_name(class_name);
    if let Some(entry) = dynamic_class_entry_by_normalized_name(state, &normalized)
        && state.dynamic_units.get(entry.unit_index).is_some()
    {
        return Some(entry.unit_index);
    }
    state.dynamic_units.iter().rposition(|unit| {
        unit.lookup_class(&normalized)
            .is_some_and(|class| normalize_class_name(&class.name) == normalized)
    })
}

pub(super) fn ini_option_name(value: &Value) -> Result<String, String> {
    to_string(value).map(|name| name.to_string_lossy())
}

pub(super) fn session_ini_cannot_change_when_active(option: &str) -> bool {
    matches!(
        option.to_ascii_lowercase().as_str(),
        "session.save_path"
            | "session.name"
            | "session.save_handler"
            | "session.gc_probability"
            | "session.gc_divisor"
            | "session.gc_maxlifetime"
            | "session.serialize_handler"
            | "session.sid_length"
            | "session.sid_bits_per_character"
            | "session.use_strict_mode"
            | "session.cookie_lifetime"
            | "session.cookie_path"
            | "session.cookie_domain"
            | "session.cookie_secure"
            | "session.cookie_partitioned"
            | "session.cookie_httponly"
            | "session.cookie_samesite"
            | "session.use_cookies"
            | "session.use_only_cookies"
            | "session.referer_check"
            | "session.cache_expire"
            | "session.cache_limiter"
            | "session.use_trans_sid"
            | "session.lazy_write"
    )
}

pub(super) fn session_sid_ini_deprecation(option: &str, value: &str) -> Option<String> {
    let (canonical, default) = if option.eq_ignore_ascii_case("session.sid_length") {
        ("session.sid_length", 32)
    } else if option.eq_ignore_ascii_case("session.sid_bits_per_character") {
        ("session.sid_bits_per_character", 4)
    } else {
        return None;
    };
    let parsed = value.trim().parse::<i64>().unwrap_or(0);
    (parsed != default).then(|| format!("ini_set(): {canonical} INI setting is deprecated"))
}

pub(super) fn session_serialize_handler_ini_error(option: &str, value: &str) -> Option<String> {
    if !option.eq_ignore_ascii_case("session.serialize_handler") {
        return None;
    }
    match value {
        "php" | "php_binary" | "php_serialize" => None,
        _ => Some(format!(
            "ini_set(): Serialization handler \"{value}\" cannot be found"
        )),
    }
}

pub(super) fn ini_set_effective_value(option: &str, value: String, cwd: &Path) -> String {
    if option.eq_ignore_ascii_case("open_basedir") {
        return normalize_open_basedir_ini_value(&value, cwd);
    }
    value
}

pub(super) fn normalize_open_basedir_ini_value(value: &str, cwd: &Path) -> String {
    value
        .split(open_basedir_separator())
        .map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                String::new()
            } else {
                canonicalize_open_basedir_path(entry, cwd)
                    .to_string_lossy()
                    .into_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(&open_basedir_separator().to_string())
}

pub(super) fn session_save_path_open_basedir_ini_error(
    option: &str,
    value: &str,
    cwd: &Path,
    registry: &IniRegistry,
) -> Option<String> {
    if !option.eq_ignore_ascii_case("session.save_path") {
        return None;
    }
    let save_path = session_save_path_directory(value)?;
    let open_basedir = registry.get("open_basedir")?.trim();
    if open_basedir.is_empty() || open_basedir_allows_path(&save_path, open_basedir, cwd) {
        return None;
    }
    Some(format!(
        "ini_set(): open_basedir restriction in effect. File({save_path}) is not within the allowed path(s): ({open_basedir})"
    ))
}

pub(super) fn session_save_path_directory(raw_path: &str) -> Option<String> {
    let path = raw_path
        .split(';')
        .next_back()
        .unwrap_or(raw_path)
        .trim()
        .to_owned();
    (!path.is_empty()).then_some(path)
}

pub(super) fn open_basedir_allows_path(path: &str, open_basedir: &str, cwd: &Path) -> bool {
    let candidate = canonicalize_open_basedir_path(path, cwd);
    open_basedir
        .split(open_basedir_separator())
        .filter_map(|entry| {
            let entry = entry.trim();
            (!entry.is_empty()).then(|| canonicalize_open_basedir_path(entry, cwd))
        })
        .any(|allowed| candidate == allowed || candidate.starts_with(&allowed))
}

pub(super) fn canonicalize_open_basedir_path(path: &str, cwd: &Path) -> PathBuf {
    let path = Path::new(path);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    fs::canonicalize(&absolute).unwrap_or_else(|_| normalize_open_basedir_path(&absolute))
}

pub(super) fn normalize_open_basedir_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

pub(super) fn open_basedir_separator() -> char {
    if cfg!(windows) { ';' } else { ':' }
}

pub(super) fn apply_float_string_precision(registry: &IniRegistry) {
    if let Some(precision) = registry
        .get("precision")
        .and_then(|value| value.trim().parse::<i32>().ok())
    {
        set_float_string_precision(precision);
    }
}

pub(super) fn ini_get_all_array(
    registry: &IniRegistry,
    details: bool,
    extension: Option<&str>,
) -> PhpArray {
    let mut output = PhpArray::new();
    let entries = match extension {
        Some(extension) => registry.entries_for_extension(extension),
        None => registry.entries(),
    };
    for entry in entries {
        let value = if details {
            let mut detail = PhpArray::new();
            detail.insert(
                php_string_key("global_value"),
                Value::string(entry.global_value),
            );
            detail.insert(
                php_string_key("local_value"),
                Value::string(entry.local_value),
            );
            detail.insert(php_string_key("access"), Value::Int(entry.access));
            Value::Array(detail)
        } else {
            Value::string(entry.local_value)
        };
        output.insert(php_string_key(entry.name), value);
    }
    output
}

pub(super) fn trim_error_handler_args(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    callback: &CallableValue,
    values: Vec<Value>,
) -> Vec<Value> {
    let Some(max_args) = error_handler_callback_max_args(compiled, state, callback) else {
        return values;
    };
    values.into_iter().take(max_args).collect()
}

pub(super) fn error_handler_callback_max_args(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    callback: &CallableValue,
) -> Option<usize> {
    match callback {
        CallableValue::UserFunction { name } => {
            if let Some(function) = compiled
                .lookup_function(name)
                .and_then(|function| compiled.unit().functions.get(function.index()))
            {
                return Some(user_function_max_positional_args(function));
            }
            dynamic_function_in_state(state, name).and_then(|(owner, function)| {
                owner
                    .unit()
                    .functions
                    .get(function.index())
                    .map(user_function_max_positional_args)
            })
        }
        CallableValue::Closure(payload) => compiled
            .unit()
            .functions
            .get(FunctionId::new(payload.function).index())
            .map(user_function_max_positional_args),
        CallableValue::InternalBuiltin { .. }
        | CallableValue::BoundMethod { .. }
        | CallableValue::MethodPlaceholder { .. }
        | CallableValue::UnresolvedDynamic { .. } => None,
    }
}

pub(super) fn user_function_max_positional_args(function: &IrFunction) -> usize {
    if function.params.iter().any(|param| param.variadic) {
        usize::MAX
    } else {
        function.params.len()
    }
}

pub(super) fn state_include_path(state: &ExecutionState) -> Arc<Vec<PathBuf>> {
    Arc::clone(&state.parsed_include_path)
}

pub(super) fn parse_ini_include_path(ini: &IniRegistry) -> Arc<Vec<PathBuf>> {
    Arc::new(
        ini.get("include_path")
            .unwrap_or(".")
            .split(':')
            .filter(|entry| !entry.is_empty())
            .map(PathBuf::from)
            .collect(),
    )
}

pub(super) fn php_string_key(value: &str) -> ArrayKey {
    ArrayKey::String(PhpString::from_test_str(value))
}

pub(super) fn php_object_vars_key(value: &str) -> ArrayKey {
    ArrayKey::from_php_string(PhpString::from_test_str(value))
}

pub(super) fn object_from_value(value: &Value) -> Option<ObjectRef> {
    match value {
        Value::Object(object) => Some(object.clone()),
        Value::Reference(cell) => object_from_value(&cell.get()),
        _ => None,
    }
}

pub(super) fn class_name_for_is_a_subject(
    value: &Value,
    allow_string: bool,
) -> Result<Option<String>, String> {
    match effective_value(value) {
        Value::Object(object) => Ok(Some(object.class_name())),
        Value::Callable(_) => Ok(Some("Closure".to_owned())),
        value if allow_string => to_string(&value).map(|name| Some(name.to_string_lossy())),
        _ => Ok(None),
    }
}

pub(super) fn object_vars_array(
    compiled: &CompiledUnit,
    stack: &CallStack,
    object: &ObjectRef,
    mangled: bool,
) -> PhpArray {
    let mut array = PhpArray::new();
    let class = compiled.lookup_class(&object.class_name());
    let scope = current_scope_class(compiled, stack);

    for (storage_name, value) in object.properties_snapshot() {
        if !mangled && is_spl_internal_storage_property(object, &storage_name) {
            continue;
        }
        if let Some((declaring_class, property)) = private_storage_parts(&storage_name) {
            if mangled {
                let display_class =
                    class_display_name(compiled, &declaring_class).unwrap_or(declaring_class);
                array.insert(
                    ArrayKey::String(PhpString::from_test_str(&format!(
                        "\0{display_class}\0{property}"
                    ))),
                    value,
                );
            } else if scope.as_deref().is_some_and(|scope| {
                normalize_class_name(scope) == normalize_class_name(&declaring_class)
            }) {
                array.insert(php_string_key(&property), value);
            }
            continue;
        }

        let property = class.and_then(|class| {
            lookup_property_in_hierarchy(compiled, class, &storage_name, None)
                .ok()
                .flatten()
        });
        if mangled {
            let key = property
                .as_ref()
                .and_then(|resolved| {
                    if resolved.property.flags.is_protected {
                        Some(ArrayKey::String(PhpString::from_test_str(&format!(
                            "\0*\0{}",
                            resolved.property.name
                        ))))
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| php_string_key(&storage_name));
            array.insert(key, value);
            continue;
        }

        let visible = property.as_ref().is_none_or(|resolved| {
            class_member_visible(
                compiled,
                scope.as_deref(),
                resolved.class,
                resolved.property.flags.is_private,
                resolved.property.flags.is_protected,
            )
        });
        if visible {
            let key = property
                .as_ref()
                .map(|resolved| php_string_key(resolved.property.name.as_str()))
                .unwrap_or_else(|| php_object_vars_key(storage_name.as_str()));
            array.insert(key, value);
        }
    }

    array
}

pub(super) fn is_spl_internal_storage_property(object: &ObjectRef, storage_name: &str) -> bool {
    if spl_runtime_marker(object).is_none() {
        return false;
    }
    matches!(
        storage_name,
        SPL_RUNTIME_CLASS_PROPERTY
            | "__append_entry_iterator_indices"
            | "__append_iterators"
            | "__attached_iterator_ids"
            | "__attached_iterators"
            | "__entries"
            | "__entry_depths"
            | "__extract_flags"
            | "__file_info_class"
            | "__flags"
            | "__inner_iterator"
            | "__iterator_class"
            | "__iterator_count"
            | "__limit_count"
            | "__limit_offset"
            | "__position"
            | "__regex_accept_pre_parent"
            | "__regex_flags"
            | "__regex_last_accept_result"
            | "__regex_mode"
            | "__regex_pattern"
            | "__rii_array_string_warning_positions"
            | "__rii_checked_child_results"
            | "__rii_checked_child_positions"
            | "__rii_direct_at_root"
            | "__rii_direct_root_consumed"
            | "__rii_end_iteration_called"
            | "__rii_entered_child_positions"
            | "__rii_flags"
            | "__rii_hook_depth"
            | "__rii_hook_iterators"
            | "__rii_iteration_active"
            | "__rii_last_call_has_children"
            | "__rii_mode"
            | "__rii_notified_position"
            | "__rii_pruned_branches"
            | "__rti_flags"
            | "__rti_prefix_parts"
            | "__snapshot_source_id"
            | "__storage"
            | "__sub_iterators"
    )
}

pub(super) fn private_storage_parts(storage_name: &str) -> Option<(String, String)> {
    storage_name
        .strip_prefix("private:")
        .and_then(|rest| rest.split_once(':'))
        .map(|(class, property)| (class.to_owned(), property.to_owned()))
}

pub(super) fn sleep_property_value(
    properties: &[(String, Value)],
    selected_name: &str,
) -> Option<(String, Value)> {
    properties.iter().find_map(|(storage_name, value)| {
        if storage_name == selected_name {
            return Some((storage_name.clone(), value.clone()));
        }
        if let Some((owner, property)) = private_storage_parts(storage_name)
            && (property == selected_name || selected_name == format!("\0{owner}\0{property}"))
        {
            return Some((storage_name.clone(), value.clone()));
        }
        if selected_name == format!("\0*\0{storage_name}") {
            return Some((storage_name.clone(), value.clone()));
        }
        None
    })
}

pub(super) fn eval_failure(
    output: &OutputBuffer,
    message: impl Into<String>,
    stack_trace: Vec<RuntimeStackFrame>,
) -> VmResult {
    let message = message.into();
    VmResult::runtime_error_with_diagnostic(
        output.clone(),
        message.clone(),
        RuntimeDiagnostic::new(
            eval_failure_id(&message).to_owned(),
            RuntimeSeverity::FatalError,
            message,
            RuntimeSourceSpan::default(),
            stack_trace,
            None,
        ),
    )
}

pub(super) fn eval_failure_id(message: &str) -> &str {
    message
        .split_once(':')
        .and_then(|(id, _)| id.starts_with("E_").then_some(id))
        .unwrap_or("E_PHP_VM_EVAL_ERROR")
}

pub(super) fn current_source_path(compiled: &CompiledUnit, stack: &CallStack) -> Option<PathBuf> {
    let frame = stack.current()?;
    let function = compiled.unit().functions.get(frame.function.index())?;
    let file = compiled.unit().files.get(function.span.file.index())?;
    Some(PathBuf::from(&file.path))
}

pub(super) fn dense_instruction_span(
    dense: &DenseBytecodeUnit,
    instruction: &DenseInstruction,
) -> IrSpan {
    dense
        .spans
        .get(instruction.span.index())
        .copied()
        .unwrap_or_default()
}

pub(super) fn is_synthetic_eof_return(
    function: &IrFunction,
    terminator_span: IrSpan,
    return_value: Option<&Value>,
) -> bool {
    function.flags.is_top_level
        && terminator_span == function.span
        && matches!(return_value, None | Some(Value::Null))
}

pub(super) fn include_return_value(
    return_value: Option<Value>,
    returned_explicitly: bool,
) -> Option<Value> {
    if returned_explicitly {
        Some(return_value.unwrap_or(Value::Null))
    } else {
        None
    }
}

pub(super) fn shared_locals_from_current_frame(
    compiled: &CompiledUnit,
    stack: &CallStack,
) -> HashMap<String, Slot> {
    let Some(frame) = stack.current() else {
        return HashMap::new();
    };
    let Some(function) = compiled.unit().functions.get(frame.function.index()) else {
        return HashMap::new();
    };
    function
        .locals
        .iter()
        .enumerate()
        .filter_map(|(index, name)| {
            frame
                .locals
                .get_slot(LocalId::new(index as u32))
                .map(|slot| (name.clone(), slot.clone()))
        })
        .collect()
}

pub(super) fn import_shared_locals(
    function: &IrFunction,
    stack: &mut CallStack,
    state: &mut ExecutionState,
    shared: &HashMap<String, Slot>,
    bind_missing_globals: bool,
) {
    let Some(frame) = stack.current_mut() else {
        return;
    };
    for (index, name) in function.locals.iter().enumerate() {
        if let Some(slot) = shared.get(name) {
            let _ = frame
                .locals
                .set_slot(LocalId::new(index as u32), slot.clone());
        } else if bind_missing_globals && name != "GLOBALS" {
            let cell = state
                .globals
                .ensure_slot(name.clone(), Value::Uninitialized);
            let _ = frame
                .locals
                .bind_reference_cell(LocalId::new(index as u32), cell);
        }
    }
}

pub(super) fn current_frame_is_top_level(compiled: &CompiledUnit, stack: &CallStack) -> bool {
    let Some(frame) = stack.current() else {
        return false;
    };
    compiled
        .unit()
        .functions
        .get(frame.function.index())
        .is_some_and(|function| function.flags.is_top_level)
}

pub(super) fn auto_start_session_if_configured(
    state: &mut ExecutionState,
    source_span: RuntimeSourceSpan,
) {
    if !ini_bool(&state.ini, "session.auto_start")
        || state.request.session.status() == php_runtime::PHP_SESSION_ACTIVE
    {
        return;
    }
    if state.request.session.needs_lazy_load() {
        let id = state.request.session.id().to_owned();
        if let Some(loader) = &state.request.session_loader
            && let Ok(data) = loader.load(&id)
        {
            state.request.session.load_data(data);
        }
    }
    let id_length = session_sid_length_from_ini(&state.ini);
    let strict_mode = ini_bool(&state.ini, "session.use_strict_mode");
    state
        .request
        .session
        .start_with_policy(id_length, strict_mode);
    state.request.session.mark_started_automatically();
    let location = php_runtime::PhpDiagnosticLocation::from_span(&source_span);
    state
        .request
        .session
        .record_start_location(location.file, location.line);
    state
        .globals
        .set("_SESSION", state.request.session.data_value());
}

pub(super) fn session_sid_length_from_ini(ini: &IniRegistry) -> usize {
    ini.get("session.sid_length")
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| (22..=256).contains(value))
        .unwrap_or(32)
}

pub(super) fn ini_bool(ini: &IniRegistry, name: &str) -> bool {
    ini.get(name).is_some_and(|value| {
        !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "off" | "no"
        )
    })
}

pub(super) fn seed_runtime_globals(globals: &mut GlobalSymbolTable, context: &RuntimeContext) {
    for name in [
        "argc", "argv", "_SERVER", "_ENV", "_GET", "_POST", "_COOKIE", "_FILES", "_REQUEST",
        "_SESSION",
    ] {
        if let Some(value) = context.global_value(name) {
            globals.set(name, value);
        }
    }
}

pub(super) fn sync_session_state_from_globals(state: &mut ExecutionState) {
    let Some(Value::Array(array)) = state.globals.get("_SESSION") else {
        return;
    };
    state.request.session.set_data(array);
}

pub(super) fn env_entries_array(entries: &[(String, String)]) -> PhpArray {
    let mut array = PhpArray::new();
    for (key, value) in entries {
        array.insert(
            ArrayKey::String(PhpString::from_test_str(key)),
            Value::string(value.clone()),
        );
    }
    array
}

pub(super) fn set_env_entry(
    entries: &mut Vec<(String, String)>,
    key: String,
    value: Option<String>,
) {
    entries.retain(|(entry_key, _)| entry_key != &key);
    if let Some(value) = value {
        entries.push((key, value));
        entries.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    }
}

pub(super) fn php_uname_value(mode: &str) -> String {
    match mode.chars().next().unwrap_or('a').to_ascii_lowercase() {
        's' => "Phrust".to_string(),
        'n' => "localhost".to_string(),
        'r' => php_source::reference_php_version().to_string(),
        'v' => "Stdlib".to_string(),
        'm' => "generic".to_string(),
        _ => format!(
            "Phrust localhost {} Stdlib generic",
            php_source::reference_php_version()
        ),
    }
}

pub(super) fn script_owner_uid(script_path: Option<&str>) -> i64 {
    script_path
        .and_then(|path| fs::metadata(path).ok())
        .map_or_else(current_directory_uid, |metadata| {
            metadata_owner_uid(&metadata)
        })
}

pub(super) fn script_owner_gid(script_path: Option<&str>) -> i64 {
    script_path
        .and_then(|path| fs::metadata(path).ok())
        .map_or_else(current_directory_gid, |metadata| {
            metadata_owner_gid(&metadata)
        })
}

#[cfg(unix)]
pub(super) fn metadata_owner_uid(metadata: &fs::Metadata) -> i64 {
    use std::os::unix::fs::MetadataExt as _;

    metadata.uid() as i64
}

#[cfg(not(unix))]
pub(super) fn metadata_owner_uid(_metadata: &fs::Metadata) -> i64 {
    0
}

#[cfg(unix)]
pub(super) fn metadata_owner_gid(metadata: &fs::Metadata) -> i64 {
    use std::os::unix::fs::MetadataExt as _;

    metadata.gid() as i64
}

#[cfg(not(unix))]
pub(super) fn metadata_owner_gid(_metadata: &fs::Metadata) -> i64 {
    0
}

pub(super) fn current_directory_uid() -> i64 {
    fs::metadata(".").map_or(0, |metadata| metadata_owner_uid(&metadata))
}

pub(super) fn current_directory_gid() -> i64 {
    fs::metadata(".").map_or(0, |metadata| metadata_owner_gid(&metadata))
}

pub(super) fn validate_process_arity(name: &str, argc: usize) -> Option<String> {
    let valid = match name {
        "proc_open" => (3..=6).contains(&argc),
        "proc_close" | "proc_get_status" | "pclose" => argc == 1,
        "popen" => argc == 2,
        "shell_exec" | "system" => argc == 1,
        "exec" => (1..=3).contains(&argc),
        "passthru" => (1..=2).contains(&argc),
        _ => false,
    };
    if valid {
        None
    } else {
        Some(format!(
            "E_PHP_VM_PROCESS_ARITY: {name} received {argc} argument(s)"
        ))
    }
}

pub(super) fn process_disabled_result(
    output: &OutputBuffer,
    name: &str,
    stack_trace: Vec<RuntimeStackFrame>,
) -> VmResult {
    process_warning_result(
        output,
        name,
        "E_PHP_VM_PROCESS_CAPABILITY_DISABLED",
        format!("{name}(): process execution is disabled by runtime capabilities"),
        process_failure_value(name),
        stack_trace,
    )
}

pub(super) fn process_unsupported_mock_result(
    output: &OutputBuffer,
    name: &str,
    stack_trace: Vec<RuntimeStackFrame>,
) -> VmResult {
    process_warning_result(
        output,
        name,
        "E_PHP_VM_PROCESS_RESOURCE_MOCK_UNSUPPORTED",
        format!("{name}(): process resource APIs are not implemented by the standard-library mock"),
        process_failure_value(name),
        stack_trace,
    )
}

pub(super) fn process_warning_result(
    _output: &OutputBuffer,
    _name: &str,
    id: &'static str,
    message: String,
    return_value: Value,
    stack_trace: Vec<RuntimeStackFrame>,
) -> VmResult {
    VmResult::success_with_diagnostics_no_output(
        Some(return_value),
        vec![RuntimeDiagnostic::new(
            id,
            RuntimeSeverity::Warning,
            message,
            RuntimeSourceSpan::default(),
            stack_trace,
            Some(php_runtime::PhpReferenceClassification::Warning),
        )],
    )
}

pub(super) fn process_failure_value(name: &str) -> Value {
    match name {
        "shell_exec" | "passthru" => Value::Bool(false),
        _ => Value::Bool(false),
    }
}

pub(super) fn process_output_lines_array(output: &str) -> Value {
    Value::packed_array(
        output
            .lines()
            .map(|line| Value::string(line.to_owned()))
            .collect(),
    )
}

pub(super) fn process_last_output_line(output: &str) -> String {
    output.lines().last().unwrap_or_default().to_owned()
}

pub(super) fn assign_process_ref_arg(
    stack: &mut CallStack,
    arg: &CallArgument,
    value: Value,
) -> Result<(), String> {
    let Some(local) = arg.by_ref_local else {
        return Ok(());
    };
    let frame = stack.current_mut().ok_or_else(|| {
        "E_PHP_VM_NO_ACTIVE_FRAME: cannot bind process reference argument".to_owned()
    })?;
    let _source = layout_source::enter(layout_source::BY_REF_ARGUMENT_BINDING);
    frame.locals.ensure_reference_cell(local)?.set(value);
    Ok(())
}

pub(super) fn should_skip_top_level_auto_global_bind(
    function: &IrFunction,
    instruction: &Instruction,
) -> bool {
    let InstructionKind::BindGlobal { local, name } = &instruction.kind else {
        return false;
    };
    function.flags.is_top_level
        && is_auto_global_name(name)
        && function
            .locals
            .get(local.index())
            .is_some_and(|local_name| local_name == name)
}

pub(super) fn is_auto_global_name(name: &str) -> bool {
    matches!(
        name,
        "argc"
            | "argv"
            | "_SERVER"
            | "_ENV"
            | "_GET"
            | "_POST"
            | "_COOKIE"
            | "_FILES"
            | "_REQUEST"
            | "_SESSION"
    )
}

pub(super) fn bind_top_level_global_locals(
    function: &IrFunction,
    stack: &mut CallStack,
    state: &mut ExecutionState,
) {
    let Some(frame) = stack.current_mut() else {
        return;
    };
    for (index, name) in function.locals.iter().enumerate() {
        if name == "GLOBALS" {
            continue;
        }
        let cell = state
            .globals
            .ensure_slot(name.clone(), Value::Uninitialized);
        let _ = frame
            .locals
            .bind_reference_cell(LocalId::new(index as u32), cell);
    }
}

pub(super) fn export_shared_locals_at_frame(
    function: &IrFunction,
    stack: &CallStack,
    frame_index: usize,
    shared: &mut HashMap<String, Slot>,
) {
    let Some(frame) = stack.frames().get(frame_index) else {
        return;
    };
    for (index, name) in function.locals.iter().enumerate() {
        if let Some(slot) = frame.locals.get_slot(LocalId::new(index as u32)) {
            shared.insert(name.clone(), slot.clone());
        }
    }
}

pub(super) fn export_shared_locals(
    function: &IrFunction,
    stack: &CallStack,
    shared: &mut HashMap<String, Slot>,
) {
    let Some(frame_index) = stack.len().checked_sub(1) else {
        return;
    };
    export_shared_locals_at_frame(function, stack, frame_index, shared);
}

pub(super) fn write_shared_locals_to_current_frame(
    compiled: &CompiledUnit,
    stack: &mut CallStack,
    shared: &HashMap<String, Slot>,
) {
    let Some(frame) = stack.current_mut() else {
        return;
    };
    let Some(function) = compiled.unit().functions.get(frame.function.index()) else {
        return;
    };
    for (index, name) in function.locals.iter().enumerate() {
        if let Some(slot) = shared.get(name) {
            let _ = frame
                .locals
                .set_slot(LocalId::new(index as u32), slot.clone());
        }
    }
}
