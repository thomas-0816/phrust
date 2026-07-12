use super::builtin_adapter::builtin_source_span;
use super::prelude::*;

impl Vm {
    pub(super) fn call_config_builtin(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        args: Vec<CallArgument>,
        call_span: Option<php_ir::IrSpan>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let values = match call_args_to_positional(name, args) {
            Ok(args) => args,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        match name {
            "ignore_user_abort" => {
                if values.len() > 1 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_CONFIG_ARITY: ignore_user_abort expects zero or one argument",
                    );
                }
                let previous = state
                    .ini
                    .get("ignore_user_abort")
                    .is_some_and(|value| value != "0");
                if let Some(value) = values.first()
                    && !matches!(value, Value::Null)
                {
                    let next = match to_bool(value) {
                        Ok(true) => "1",
                        Ok(false) => "0",
                        Err(message) => {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    };
                    let _ = state.ini.set("ignore_user_abort", next);
                }
                VmResult::success_no_output(Some(Value::Int(i64::from(previous))))
            }
            "ini_get" => {
                if values.len() != 1 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_CONFIG_ARITY: ini_get expects one argument",
                    );
                }
                let option = match ini_option_name(&values[0]) {
                    Ok(option) => option,
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                let value = state
                    .ini
                    .get(&option)
                    .map(Value::string)
                    .unwrap_or(Value::Bool(false));
                VmResult::success_no_output(Some(value))
            }
            "ini_set" => {
                if values.len() != 2 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_CONFIG_ARITY: ini_set expects two arguments",
                    );
                }
                let option = match ini_option_name(&values[0]) {
                    Ok(option) => option,
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                let value = match to_string(&values[1]) {
                    Ok(value) => value.to_string_lossy(),
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                let mut diagnostics = Vec::new();
                if session_ini_cannot_change_when_active(&option)
                    && state.request.session.status() == php_runtime::api::PHP_SESSION_ACTIVE
                {
                    let (started_file, started_line) = state
                        .request
                        .session
                        .started_location()
                        .map(|(file, line)| (file.to_owned(), line))
                        .unwrap_or_else(|| ("Unknown".to_owned(), 0));
                    let diagnostic = RuntimeDiagnostic::new(
                        "E_PHP_VM_SESSION_INI_ACTIVE",
                        RuntimeSeverity::Warning,
                        format!(
                            "ini_set(): Session ini settings cannot be changed when a session is active (started from {started_file} on line {started_line})"
                        ),
                        builtin_source_span(compiled, call_span),
                        stack_trace(compiled, stack),
                        None,
                    );
                    let handled = match self.dispatch_error_handler(
                        compiled,
                        output,
                        stack,
                        state,
                        php_runtime::api::PHP_E_WARNING,
                        &diagnostic,
                    ) {
                        Ok(handled) => handled,
                        Err(result) => return result,
                    };
                    if !handled && error_reporting_allows(state, php_runtime::api::PHP_E_WARNING) {
                        emit_vm_diagnostic(
                            output,
                            state,
                            &diagnostic,
                            php_runtime::api::PhpDiagnosticChannel::Warning,
                            php_runtime::api::PHP_E_WARNING,
                        );
                    }
                    diagnostics.push(diagnostic);
                    return VmResult::success_with_diagnostics_no_output(
                        Some(Value::Bool(false)),
                        diagnostics,
                    );
                }
                if let Some(message) = session_save_path_open_basedir_ini_error(
                    &option, &value, &state.cwd, &state.ini,
                ) {
                    let diagnostic = RuntimeDiagnostic::new(
                        "E_PHP_VM_SESSION_SAVE_PATH_OPEN_BASEDIR",
                        RuntimeSeverity::Warning,
                        message,
                        builtin_source_span(compiled, call_span),
                        stack_trace(compiled, stack),
                        None,
                    );
                    let handled = match self.dispatch_error_handler(
                        compiled,
                        output,
                        stack,
                        state,
                        php_runtime::api::PHP_E_WARNING,
                        &diagnostic,
                    ) {
                        Ok(handled) => handled,
                        Err(result) => return result,
                    };
                    if !handled && error_reporting_allows(state, php_runtime::api::PHP_E_WARNING) {
                        emit_vm_diagnostic(
                            output,
                            state,
                            &diagnostic,
                            php_runtime::api::PhpDiagnosticChannel::Warning,
                            php_runtime::api::PHP_E_WARNING,
                        );
                    }
                    diagnostics.push(diagnostic);
                    return VmResult::success_with_diagnostics_no_output(
                        Some(Value::Bool(false)),
                        diagnostics,
                    );
                }
                if let Some(message) = session_serialize_handler_ini_error(&option, &value) {
                    let diagnostic = RuntimeDiagnostic::new(
                        "E_PHP_VM_SESSION_SERIALIZER_INI_UNKNOWN",
                        RuntimeSeverity::Warning,
                        message,
                        builtin_source_span(compiled, call_span),
                        stack_trace(compiled, stack),
                        None,
                    );
                    let handled = match self.dispatch_error_handler(
                        compiled,
                        output,
                        stack,
                        state,
                        php_runtime::api::PHP_E_WARNING,
                        &diagnostic,
                    ) {
                        Ok(handled) => handled,
                        Err(result) => return result,
                    };
                    if !handled && error_reporting_allows(state, php_runtime::api::PHP_E_WARNING) {
                        emit_vm_diagnostic(
                            output,
                            state,
                            &diagnostic,
                            php_runtime::api::PhpDiagnosticChannel::Warning,
                            php_runtime::api::PHP_E_WARNING,
                        );
                    }
                    diagnostics.push(diagnostic);
                    return VmResult::success_with_diagnostics_no_output(
                        Some(Value::Bool(false)),
                        diagnostics,
                    );
                }
                if let Some(message) = session_sid_ini_deprecation(&option, &value) {
                    let diagnostic = RuntimeDiagnostic::new(
                        "E_PHP_VM_SESSION_SID_INI_DEPRECATED",
                        RuntimeSeverity::Deprecation,
                        message,
                        RuntimeSourceSpan::default(),
                        stack_trace(compiled, stack),
                        None,
                    );
                    let handled = match self.dispatch_error_handler(
                        compiled,
                        output,
                        stack,
                        state,
                        php_runtime::api::PHP_E_DEPRECATED,
                        &diagnostic,
                    ) {
                        Ok(handled) => handled,
                        Err(result) => return result,
                    };
                    if !handled && error_reporting_allows(state, php_runtime::api::PHP_E_DEPRECATED)
                    {
                        emit_vm_diagnostic(
                            output,
                            state,
                            &diagnostic,
                            php_runtime::api::PhpDiagnosticChannel::Deprecated,
                            php_runtime::api::PHP_E_DEPRECATED,
                        );
                    }
                    diagnostics.push(diagnostic);
                }
                let effective_value = ini_set_effective_value(&option, value, &state.cwd);
                let previous = state
                    .ini
                    .set(&option, effective_value)
                    .map(Value::string)
                    .unwrap_or(Value::Bool(false));
                if option.eq_ignore_ascii_case("include_path") {
                    state.bump_include_config_epoch();
                }
                if option.eq_ignore_ascii_case("precision") {
                    apply_float_string_precision(&state.ini);
                }
                if diagnostics.is_empty() {
                    VmResult::success_no_output(Some(previous))
                } else {
                    VmResult::success_with_diagnostics_no_output(Some(previous), diagnostics)
                }
            }
            "ini_get_all" => {
                if values.len() > 2 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_CONFIG_ARITY: ini_get_all expects zero to two arguments",
                    );
                }
                let extension_filter = if let Some(extension) = values.first()
                    && !matches!(extension, Value::Null)
                {
                    let extension = match ini_option_name(extension) {
                        Ok(extension) => extension,
                        Err(message) => {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    };
                    if !extension.eq_ignore_ascii_case("standard")
                        && !extension.eq_ignore_ascii_case("core")
                        && !extension.eq_ignore_ascii_case("ffi")
                        && !extension.eq_ignore_ascii_case("session")
                    {
                        return VmResult::success_no_output(Some(Value::Bool(false)));
                    }
                    if extension.eq_ignore_ascii_case("ffi")
                        || extension.eq_ignore_ascii_case("session")
                    {
                        Some(extension)
                    } else {
                        None
                    }
                } else {
                    None
                };
                let details = values
                    .get(1)
                    .is_none_or(|value| to_bool(value).unwrap_or(true));
                VmResult::success_no_output(Some(Value::Array(ini_get_all_array(
                    &state.ini,
                    details,
                    extension_filter.as_deref(),
                ))))
            }
            "get_cfg_var" => {
                if values.len() != 1 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_CONFIG_ARITY: get_cfg_var expects one argument",
                    );
                }
                let option = match ini_option_name(&values[0]) {
                    Ok(option) => option,
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                let value = state
                    .ini
                    .cfg_var(&option)
                    .map(Value::string)
                    .unwrap_or(Value::Bool(false));
                VmResult::success_no_output(Some(value))
            }
            _ => self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNKNOWN_CONFIG_BUILTIN: {name}"),
            ),
        }
    }

    pub(super) fn call_environment_builtin(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let values = match call_args_to_positional(name, args) {
            Ok(args) => args,
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        match name {
            "getenv" => {
                if values.len() > 2 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_ENV_ARITY: getenv expects zero to two arguments",
                    );
                }
                if values
                    .first()
                    .is_none_or(|value| matches!(value, Value::Null))
                {
                    return VmResult::success_no_output(Some(Value::Array(env_entries_array(
                        &state.env,
                    ))));
                }
                let key = match to_string(&values[0]) {
                    Ok(value) => value.to_string_lossy(),
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                let value = state
                    .env
                    .iter()
                    .find(|(entry_key, _)| entry_key == &key)
                    .map(|(_, value)| Value::string(value.clone()))
                    .unwrap_or(Value::Bool(false));
                VmResult::success_no_output(Some(value))
            }
            "putenv" => {
                if values.len() != 1 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_ENV_ARITY: putenv expects one argument",
                    );
                }
                let assignment = match to_string(&values[0]) {
                    Ok(value) => value.to_string_lossy(),
                    Err(message) => return self.runtime_error(output, compiled, stack, message),
                };
                if let Some((key, value)) = assignment.split_once('=') {
                    if key.is_empty() {
                        return self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_RUNTIME_BUILTIN_VALUE: putenv(): Argument #1 ($assignment) must have a valid syntax",
                        );
                    }
                    set_env_entry(
                        Arc::make_mut(&mut state.env),
                        key.to_string(),
                        Some(value.to_string()),
                    );
                } else {
                    if assignment.is_empty() {
                        return self.runtime_error(
                            output,
                            compiled,
                            stack,
                            "E_PHP_RUNTIME_BUILTIN_VALUE: putenv(): Argument #1 ($assignment) must have a valid syntax",
                        );
                    }
                    set_env_entry(Arc::make_mut(&mut state.env), assignment, None);
                }
                VmResult::success_no_output(Some(Value::Bool(true)))
            }
            "php_sapi_name" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_ENV_ARITY: php_sapi_name expects no arguments",
                    );
                }
                VmResult::success_no_output(Some(Value::string(
                    self.options.runtime_context.sapi_name.clone(),
                )))
            }
            "php_uname" => {
                if values.len() > 1 {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_ENV_ARITY: php_uname expects zero or one argument",
                    );
                }
                let mode = match values.first() {
                    Some(value) => match to_string(value) {
                        Ok(value) => value.to_string_lossy(),
                        Err(message) => {
                            return self.runtime_error(output, compiled, stack, message);
                        }
                    },
                    None => "a".to_string(),
                };
                VmResult::success_no_output(Some(Value::string(php_uname_value(&mode))))
            }
            "get_current_user" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_ENV_ARITY: get_current_user expects no arguments",
                    );
                }
                VmResult::success_no_output(Some(Value::string("phrust")))
            }
            "getmyuid" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_ENV_ARITY: getmyuid expects no arguments",
                    );
                }
                VmResult::success_no_output(Some(Value::Int(script_owner_uid(
                    self.options
                        .runtime_context
                        .argv
                        .first()
                        .map(String::as_str),
                ))))
            }
            "getmygid" => {
                if !values.is_empty() {
                    return self.runtime_error(
                        output,
                        compiled,
                        stack,
                        "E_PHP_VM_ENV_ARITY: getmygid expects no arguments",
                    );
                }
                VmResult::success_no_output(Some(Value::Int(script_owner_gid(
                    self.options
                        .runtime_context
                        .argv
                        .first()
                        .map(String::as_str),
                ))))
            }
            _ => self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNKNOWN_ENV_BUILTIN: {name}"),
            ),
        }
    }

    pub(super) fn call_process_builtin(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        args: Vec<CallArgument>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
    ) -> VmResult {
        if let Some(message) = validate_process_arity(name, args.len()) {
            return self.runtime_error(output, compiled, stack, message);
        }
        if let Some(arg) = args.iter().find(|arg| arg.name.is_some()) {
            return self.runtime_error(
                output,
                compiled,
                stack,
                format!(
                    "E_PHP_VM_UNKNOWN_NAMED_ARG: function {name} has no builtin parameter ${}",
                    arg.name.as_deref().unwrap_or_default()
                ),
            );
        }

        match &self.options.runtime_context.process {
            ProcessCapability::Disabled => {
                process_disabled_result(output, name, stack_trace(compiled, stack))
            }
            ProcessCapability::Mock {
                output: mock_output,
                exit_status,
            } => self.call_mocked_process_builtin(
                compiled,
                name,
                args,
                mock_output,
                *exit_status,
                output,
                stack,
            ),
        }
    }

    pub(super) fn call_mocked_process_builtin(
        &self,
        compiled: &CompiledUnit,
        name: &str,
        args: Vec<CallArgument>,
        mock_output: &str,
        exit_status: i64,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
    ) -> VmResult {
        match name {
            "shell_exec" => VmResult::success_no_output(Some(Value::string(mock_output))),
            "exec" => {
                if let Some(arg) = args.get(1)
                    && let Err(message) =
                        assign_process_ref_arg(stack, arg, process_output_lines_array(mock_output))
                {
                    return self.runtime_error(output, compiled, stack, message);
                }
                if let Some(arg) = args.get(2)
                    && let Err(message) =
                        assign_process_ref_arg(stack, arg, Value::Int(exit_status))
                {
                    return self.runtime_error(output, compiled, stack, message);
                }
                VmResult::success_no_output(Some(Value::string(process_last_output_line(
                    mock_output,
                ))))
            }
            "system" => {
                output.write_bytes(mock_output.as_bytes());
                VmResult::success_no_output(Some(Value::string(process_last_output_line(
                    mock_output,
                ))))
            }
            "passthru" => {
                output.write_bytes(mock_output.as_bytes());
                if let Some(arg) = args.get(1)
                    && let Err(message) =
                        assign_process_ref_arg(stack, arg, Value::Int(exit_status))
                {
                    return self.runtime_error(output, compiled, stack, message);
                }
                VmResult::success_no_output(Some(Value::Null))
            }
            "proc_open" | "proc_close" | "proc_get_status" | "popen" | "pclose" => {
                process_unsupported_mock_result(output, name, stack_trace(compiled, stack))
            }
            _ => self.runtime_error(
                output,
                compiled,
                stack,
                format!("E_PHP_VM_UNKNOWN_PROCESS_BUILTIN: {name}"),
            ),
        }
    }
}
