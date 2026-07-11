//! Request-local builtin adapter state and internal builtin dispatch caches.

use super::prelude::*;

pub(super) struct BuiltinTypeError<'a> {
    pub(super) output: &'a OutputBuffer,
    pub(super) compiled: &'a CompiledUnit,
    pub(super) stack: &'a CallStack,
    pub(super) state: &'a mut ExecutionState,
    pub(super) function: &'a str,
    pub(super) values: &'a [Value],
    pub(super) call_span: Option<php_ir::IrSpan>,
}

impl BuiltinTypeError<'_> {
    pub(super) fn result(self, message: String) -> VmResult {
        let diagnostic = RuntimeDiagnostic::new(
            "E_PHP_RUNTIME_BUILTIN_TYPE",
            RuntimeSeverity::FatalError,
            message.clone(),
            builtin_source_span(self.compiled, self.call_span),
            stack_trace(self.compiled, self.stack),
            Some(php_runtime::PhpReferenceClassification::TypeError),
        );
        let result =
            VmResult::runtime_error_with_diagnostic(self.output.clone(), message, diagnostic);
        if let Some(call_span) = self.call_span
            && let Some(throwable) = runtime_error_throwable(&result)
        {
            tag_throwable_location(&throwable, self.compiled, call_span);
            self.state.pending_trace = Some(capture_backtrace_string_with_builtin_failed_call(
                self.compiled,
                self.stack,
                self.function,
                self.values,
                call_span,
            ));
            self.state.pending_throw = Some(throwable);
        }
        result
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InternalFunctionDispatchCacheOutcome {
    Hit,
    Miss,
    Uncached,
}

#[derive(Clone, Debug, Default)]
pub(super) struct InternalFunctionDispatchCache {
    entries: HashMap<String, BuiltinEntry>,
}

impl InternalFunctionDispatchCache {
    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }

    pub(super) fn lookup(
        &mut self,
        name: &str,
    ) -> (Option<BuiltinEntry>, InternalFunctionDispatchCacheOutcome) {
        if !internal_function_dispatch_cacheable(name) {
            return (
                BuiltinRegistry::new().get(name),
                InternalFunctionDispatchCacheOutcome::Uncached,
            );
        }
        if let Some(entry) = self.entries.get(name).copied() {
            return (Some(entry), InternalFunctionDispatchCacheOutcome::Hit);
        }
        let entry = BuiltinRegistry::new().get(name);
        if let Some(entry) = entry {
            self.entries.insert(name.to_owned(), entry);
        }
        (entry, InternalFunctionDispatchCacheOutcome::Miss)
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct UserStreamWrapperRegistry {
    wrappers: BTreeMap<String, UserStreamWrapperClass>,
    open_streams: BTreeMap<php_runtime::ResourceId, UserStreamWrapperInstance>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct UserStreamWrapperClass {
    pub(super) protocol: String,
    pub(super) class_name: String,
    pub(super) display_class_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct UserStreamWrapperInstance {
    pub(super) object: ObjectRef,
    pub(super) close_called: bool,
}

impl UserStreamWrapperRegistry {
    pub(super) fn register(
        &mut self,
        protocol: &str,
        class_name: &str,
        display_class_name: &str,
    ) -> bool {
        let normalized = normalize_stream_wrapper_protocol(protocol);
        if normalized.is_empty() || self.wrappers.contains_key(&normalized) {
            return false;
        }
        self.wrappers.insert(
            normalized.clone(),
            UserStreamWrapperClass {
                protocol: normalized,
                class_name: class_name.to_owned(),
                display_class_name: display_class_name.to_owned(),
            },
        );
        true
    }

    pub(super) fn wrapper_for_uri(&self, uri: &str) -> Option<UserStreamWrapperClass> {
        let protocol = stream_uri_protocol(uri)?;
        self.wrappers.get(&protocol).cloned()
    }

    pub(super) fn protocols(&self) -> Vec<String> {
        self.wrappers
            .values()
            .map(|wrapper| wrapper.protocol.clone())
            .collect()
    }

    pub(super) fn register_open_stream(
        &mut self,
        resource: &php_runtime::ResourceRef,
        object: ObjectRef,
    ) {
        self.open_streams.insert(
            resource.id(),
            UserStreamWrapperInstance {
                object,
                close_called: false,
            },
        );
    }

    pub(super) fn pending_close_object(
        &mut self,
        id: php_runtime::ResourceId,
    ) -> Option<ObjectRef> {
        let instance = self.open_streams.get_mut(&id)?;
        if instance.close_called {
            return None;
        }
        instance.close_called = true;
        Some(instance.object.clone())
    }

    pub(super) fn pending_close_ids(&self) -> Vec<php_runtime::ResourceId> {
        self.open_streams
            .iter()
            .filter_map(|(id, instance)| (!instance.close_called).then_some(*id))
            .collect()
    }
}

#[derive(Debug, Default)]
pub(super) struct BuiltinAdapterState {
    pub(super) bcmath_scale: usize,
    pub(super) strtok_state: php_runtime::StrtokState,
    pub(super) iconv_state: php_runtime::IconvEncodingState,
    pub(super) apcu_state: php_runtime::ApcuState,
    pub(super) opcache_state: php_runtime::OpcacheState,
    pub(super) soap_state: php_runtime::SoapState,
    pub(super) openssl_error_state: php_runtime::OpenSslErrorState,
    pub(super) gettext_state: php_runtime::GettextState,
    pub(super) shmop_state: php_runtime::ShmopState,
    pub(super) readline_state: php_runtime::ReadlineState,
    pub(super) sysvmsg_state: php_runtime::SysvMessageQueueState,
    pub(super) sysvsem_state: php_runtime::SysvSemaphoreState,
    pub(super) sysvshm_state: php_runtime::SysvSharedMemoryState,
    pub(super) pcntl_state: php_runtime::PcntlState,
    pub(super) ftp_state: php_runtime::FtpState,
    pub(super) imap_state: php_runtime::ImapState,
    pub(super) ldap_state: php_runtime::LdapState,
    pub(super) ssh2_state: php_runtime::Ssh2State,
    pub(super) socket_state: php_runtime::SocketState,
    pub(super) filesystem_state: php_runtime::FilesystemRuntimeState,
    pub(super) stream_context_state: php_runtime::StreamContextState,
    pub(super) user_stream_wrappers: UserStreamWrapperRegistry,
    pub(super) mb_internal_encoding: String,
    pub(super) mb_substitute_character: php_runtime::MbSubstituteCharacter,
    pub(super) builtin_request_state: php_runtime::BuiltinRequestState,
    pub(super) json_serializable_active_objects: Vec<u64>,
    pub(super) posix_last_error: i32,
    pub(super) sqlite: php_runtime::SqliteState,
    pub(super) mysql: php_runtime::MysqlState,
    pub(super) postgres: php_runtime::PostgresState,
    pub(super) redis_clients: RedisClientState,
    pub(super) memcached_clients: MemcachedClientState,
}

impl BuiltinAdapterState {
    pub(super) fn pcre_state_mut(&mut self) -> &mut php_runtime::PcreRequestState {
        self.builtin_request_state.pcre_mut()
    }

    pub(super) fn set_json_last_error(&mut self, code: i64) {
        self.builtin_request_state.json_mut().set(code);
    }
}

pub(super) fn execute_builtin_entry(
    entry: BuiltinEntry,
    args: Vec<Value>,
    output: &mut OutputBuffer,
    runtime_context: &RuntimeContext,
    state: &mut ExecutionState,
    source_span: RuntimeSourceSpan,
) -> VmResult {
    let include_path = state_include_path(state);
    let diagnostic_display = diagnostic_display_options(state);
    if state.default_timezone.is_empty() {
        state.default_timezone = php_runtime::datetime::DEFAULT_TIMEZONE.to_owned();
    }
    if state.builtins.mb_internal_encoding.is_empty() {
        state.builtins.mb_internal_encoding = "UTF-8".to_owned();
    }
    let mut context = BuiltinContext::with_runtime_request_state(
        output,
        PathBuf::new(),
        runtime_context.filesystem.clone(),
        Some(&mut state.resources),
        &mut state.builtins.builtin_request_state,
    );
    context.set_cwd_state(&mut state.cwd);
    context.set_include_path_shared(include_path);
    context.set_ini_registry_state(&mut state.ini);
    context.set_network_requests_enabled(state.network_requests_enabled);
    context.set_env_entries(Arc::clone(&state.env));
    if let php_runtime::RuntimeRequestMode::Http(request) = &runtime_context.request_mode {
        context.set_php_input(Arc::clone(&request.raw_body));
    }
    context.set_default_timezone_state(&mut state.default_timezone);
    context.set_diagnostic_display(diagnostic_display);
    context.set_filter_input_arrays_shared(Rc::clone(&state.filter_input_arrays));
    context.set_bcmath_scale(state.builtins.bcmath_scale);
    context.set_strtok_state(&mut state.builtins.strtok_state);
    context.set_iconv_state(&mut state.builtins.iconv_state);
    context.set_apcu_state(&mut state.builtins.apcu_state);
    context.set_opcache_state(&mut state.builtins.opcache_state);
    context.set_soap_state(&mut state.builtins.soap_state);
    context.set_openssl_error_state(&mut state.builtins.openssl_error_state);
    context.set_gettext_state(&mut state.builtins.gettext_state);
    context.set_shmop_state(&mut state.builtins.shmop_state);
    context.set_readline_state(&mut state.builtins.readline_state);
    context.set_sysvmsg_state(&mut state.builtins.sysvmsg_state);
    context.set_sysvsem_state(&mut state.builtins.sysvsem_state);
    context.set_sysvshm_state(&mut state.builtins.sysvshm_state);
    context.set_pcntl_state(&mut state.builtins.pcntl_state);
    context.set_ftp_state(&mut state.builtins.ftp_state);
    context.set_imap_state(&mut state.builtins.imap_state);
    context.set_ldap_state(&mut state.builtins.ldap_state);
    context.set_ssh2_state(&mut state.builtins.ssh2_state);
    context.set_socket_state(&mut state.builtins.socket_state);
    context.set_posix_last_error(state.builtins.posix_last_error);
    context.set_filesystem_state(&mut state.builtins.filesystem_state);
    context.set_stream_context_state(&mut state.builtins.stream_context_state);
    context.set_http_response_state(&mut state.request.http_response);
    context.set_upload_registry(&mut state.request.upload_registry);
    context.set_mysql_state(&mut state.builtins.mysql);
    context.set_postgres_state(&mut state.builtins.postgres);
    context.set_mb_internal_encoding_state(&mut state.builtins.mb_internal_encoding);
    context.set_mb_substitute_character_state(&mut state.builtins.mb_substitute_character);
    let initial_session_global = if state.request.session.status()
        == php_runtime::PHP_SESSION_ACTIVE
        || state.request.session.started()
    {
        state.request.session.data_value()
    } else {
        Value::Uninitialized
    };
    let session_global = state
        .globals
        .ensure_slot("_SESSION", initial_session_global);
    context.set_session_state(&mut state.request.session, session_global);
    context.set_session_loader(state.request.session_loader.as_ref());
    context.sync_session_state_from_global();
    let time_limit_arg = (entry.name() == "set_time_limit").then(|| args.first().cloned());
    let result = (entry.function())(&mut context, args, source_span.clone());
    context.sync_session_state_from_global();
    state.builtins.posix_last_error = context.posix_last_error();
    state.builtins.bcmath_scale = context.bcmath_scale();
    let mut diagnostics = context.take_diagnostics();
    let error_output = result.as_ref().err().map(|_| context.output().clone());
    drop(context);
    match result {
        Ok(value) => {
            if let Some(Some(seconds_value)) = time_limit_arg
                && let Ok(seconds) = to_int(&seconds_value)
                && seconds >= 0
            {
                state.reset_execution_deadline_seconds(seconds as u64);
            }
            VmResult::success_with_diagnostics_no_output(Some(value), diagnostics)
        }
        Err(error) => {
            let output = error_output.unwrap_or_else(|| output.clone());
            let mut error_diagnostic = RuntimeDiagnostic::new(
                error.diagnostic_id(),
                RuntimeSeverity::FatalError,
                error.message().to_owned(),
                source_span,
                Vec::new(),
                None,
            );
            if let Some(json_error_code) =
                error.context().and_then(|context| context.json_error_code)
            {
                error_diagnostic = error_diagnostic.with_diagnostic_payload(
                    RuntimeDiagnosticPayload::JsonBuiltin(JsonDiagnosticContext::new(
                        json_error_code,
                    )),
                );
            }
            if let Some(line) = error
                .context()
                .and_then(|context| context.tokenizer_parse_line)
            {
                error_diagnostic = error_diagnostic.with_diagnostic_payload(
                    RuntimeDiagnosticPayload::TokenizerParse(
                        php_runtime::TokenizerParseDiagnosticContext::new(line),
                    ),
                );
            }
            diagnostics.push(error_diagnostic.clone());
            let mut result = VmResult::runtime_error_with_diagnostic(
                output,
                error.display_message(),
                error_diagnostic,
            );
            result.diagnostics = diagnostics;
            result
        }
    }
}

pub(super) fn request_filter_input_arrays(
    runtime_context: &RuntimeContext,
) -> Rc<BTreeMap<i64, PhpArray>> {
    let mut arrays = BTreeMap::new();
    for source in [0, 1, 2, 4, 5] {
        if let Some(array) = runtime_context.filter_input_array(source) {
            arrays.insert(source, array);
        }
    }
    Rc::new(arrays)
}

pub(super) fn sorted_request_env(env: &Arc<Vec<(String, String)>>) -> Arc<Vec<(String, String)>> {
    let mut sorted = env.as_ref().clone();
    sorted.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    Arc::new(sorted)
}

pub(super) fn unknown_builtin_result(name: &str, output: &OutputBuffer) -> VmResult {
    let message = format!("E_PHP_VM_UNKNOWN_BUILTIN: builtin {name} is not implemented");
    let payload = RuntimeBringupDiagnosticContext::new("stdlib_builtin")
        .with_field("requested_name", name)
        .with_field("normalized_name", normalize_function_name(name))
        .with_field("lookup_kind", "function")
        .with_optional_field(
            "builtin_owner",
            infer_builtin_owner_for_name(name).map(str::to_owned),
        );
    VmResult::runtime_error_with_diagnostic(
        output.clone(),
        message.clone(),
        RuntimeDiagnostic::new(
            "E_PHP_VM_UNKNOWN_BUILTIN",
            RuntimeSeverity::FatalError,
            message,
            RuntimeSourceSpan::default(),
            Vec::new(),
            None,
        )
        .with_diagnostic_payload(RuntimeDiagnosticPayload::Bringup(payload)),
    )
}

pub(super) fn builtin_source_span(
    compiled: &CompiledUnit,
    call_span: Option<php_ir::IrSpan>,
) -> RuntimeSourceSpan {
    match call_span {
        Some(span) => runtime_source_span(compiled, span),
        None => RuntimeSourceSpan {
            file: compiled.unit().files.first().map(|file| file.path.clone()),
            start: 0,
            end: 0,
        },
    }
}

pub(super) fn validate_internal_builtin_args(
    name: &str,
    values: Vec<Value>,
    compiled: &CompiledUnit,
    call_span: Option<php_ir::IrSpan>,
    output: &mut OutputBuffer,
    state: &ExecutionState,
) -> Result<Vec<Value>, VmResult> {
    if builtin_uses_custom_argument_validation(name) {
        return Ok(values);
    }
    let Some(metadata) = php_std::arginfo::function_metadata_indexed(name) else {
        return Ok(values);
    };
    if metadata.params.iter().any(|param| param.by_ref) {
        return Ok(values);
    }
    let Some(info) = php_std::arginfo::function_arginfo_indexed(name) else {
        return Ok(values);
    };
    let mode = if compiled.unit().strict_types {
        php_std::arginfo::CoercionMode::Strict
    } else {
        php_std::arginfo::CoercionMode::Weak
    };
    let span = builtin_source_span(compiled, call_span);
    match php_std::arginfo::ArgumentValidator::new(mode).validate_owned(info, values, span) {
        Ok(validated) => {
            let (values, diagnostics) = validated.into_parts();
            for diagnostic in &diagnostics {
                emit_vm_diagnostic(
                    output,
                    state,
                    diagnostic,
                    php_runtime::PhpDiagnosticChannel::Deprecated,
                    php_runtime::PHP_E_DEPRECATED,
                );
            }
            Ok(values)
        }
        Err(error) => Err(arginfo_error_result(error, output)),
    }
}

fn builtin_uses_custom_argument_validation(name: &str) -> bool {
    matches!(
        name,
        "decbin"
            | "dechex"
            | "decoct"
            | "filter_var_array"
            | "hash_equals"
            | "number_format"
            | "range"
            | "readline_callback_handler_install"
            | "readline_completion_function"
            | "round"
            | "strip_tags"
            | "stristr"
            | "strpos"
            | "strripos"
            | "strrpos"
            | "strstr"
            | "strtr"
            | "vfprintf"
            | "vprintf"
    )
}

fn arginfo_error_result(error: php_std::arginfo::ArginfoError, output: &OutputBuffer) -> VmResult {
    let diagnostic = error.diagnostic().clone();
    VmResult::runtime_error_with_diagnostic(
        output.clone(),
        diagnostic.message().to_owned(),
        diagnostic,
    )
}
