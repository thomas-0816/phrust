use super::prelude::*;

pub(super) struct IncludeExecutionRequest<'a> {
    pub(super) site: UnitInlineCacheSite,
    pub(super) instruction_span: php_ir::IrSpan,
    pub(super) kind: IncludeKind,
    pub(super) path: &'a Value,
}

impl Vm {
    pub(super) fn record_include_trace_event(&self, event: impl AsRef<str>) {
        if !(self.options.trace_includes || self.options.trace_runtime) {
            return;
        }
        let mut trace = self.trace.borrow_mut();
        let step = trace.len() + 1;
        trace.push(format!("step={step} include {}", event.as_ref()));
    }

    pub(super) fn record_counter_dense_include_entry_attempt(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_include_entry_attempt();
        }
    }

    pub(super) fn record_counter_dense_include_entry_success(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_include_entry_success();
        }
    }

    pub(super) fn record_counter_dense_include_entry_fallback(&self, reason: &str, path: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_dense_include_entry_fallback(reason, path);
        }
    }

    pub(super) fn record_counter_include_compile_miss(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_include_compile_miss();
        }
    }

    pub(super) fn record_counter_include_once_skip(&self) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_include_once_skip();
        }
    }

    pub(super) fn record_counter_include_stale_invalidation_by_reason(&self, reason: &str) {
        if !self.options.collect_counters {
            return;
        }
        if let Some(counters) = self.counters.borrow_mut().as_mut() {
            counters.record_include_stale_invalidation_by_reason(reason);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_linked_include_entries(
        &self,
        caller: &CompiledUnit,
        included: &CompiledUnit,
        instruction_span: IrSpan,
        caller_is_top_level: bool,
        shared: &mut HashMap<String, Slot>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let linked_entries = if included.unit().linked_file_entries.is_empty() {
            vec![included.unit().entry]
        } else {
            included.unit().linked_file_entries.clone()
        };
        let mut linked_diagnostics = Vec::new();
        for (linked_index, linked_entry) in linked_entries.into_iter().enumerate() {
            // Explicit resolver metadata can link the source needed for class
            // composition before execution. Runtime activation still follows
            // PHP's autoload protocol: an already-declared or callback-provided
            // declaration wins, while an unresolved declaration fails.
            if let Some(normalized_name) = included
                .unit()
                .linked_entry_autoload_declarations
                .get(linked_index)
                .and_then(|declaration| declaration.as_deref())
            {
                if dynamic_class_entry_by_normalized_name(state, normalized_name).is_some() {
                    continue;
                }
                // Autoloaders see the declared spelling (PSR closures do
                // case-sensitive prefix checks); the injected unit carries it
                // on the trait's own class entry.
                let display_name = included
                    .unit()
                    .classes
                    .iter()
                    .find(|class| normalize_class_name(&class.name) == normalized_name)
                    .map_or_else(
                        || normalized_name.to_owned(),
                        |class| class.display_name.clone(),
                    );
                if let Err(result) = self.autoload_runtime_missing_declaration(
                    caller,
                    &display_name,
                    output,
                    stack,
                    state,
                ) {
                    return *result;
                }
                if dynamic_class_entry_by_normalized_name(state, normalized_name).is_some() {
                    continue;
                }
                let mut result = self.runtime_error(
                    output,
                    included,
                    stack,
                    format!("E_PHP_VM_TRAIT_NOT_FOUND: Trait \"{display_name}\" not found"),
                );
                result.diagnostics.splice(0..0, linked_diagnostics);
                return result;
            }
            self.register_linked_include_path(included, linked_entry, state);
            let call = FunctionCall {
                positional_values: None,
                args: Vec::new(),
                captures: Vec::new(),
                call_span: Some(instruction_span),
                call_site_strict_types: Some(caller.unit().strict_types),
                error_context_compiled: None,
                allow_by_ref_value_warnings: false,
                by_ref_warning_callable_name: None,
                this_value: None,
                scope_class: None,
                called_class: None,
                declaring_class: None,
                shared_top_level_locals: Some(shared),
                shared_top_level_bind_missing_globals: caller_is_top_level,
                running_generator: None,
                resume_continuation: None,
                resume_input: None,
                running_fiber: None,
                resume_fiber_continuation: None,
                resume_fiber_input: None,
            };
            let mut result =
                self.execute_include_entry(included, linked_entry, call, output, stack, state);
            if linked_entry != included.unit().entry
                && result.status.is_success()
                && !result.process_exit_terminates_process
            {
                linked_diagnostics.append(&mut result.diagnostics);
                continue;
            }
            result.diagnostics.splice(0..0, linked_diagnostics);
            return result;
        }

        VmResult::compile_error(output.clone(), "linked include entry is missing")
    }

    fn register_linked_include_path(
        &self,
        included: &CompiledUnit,
        linked_entry: FunctionId,
        state: &mut ExecutionState,
    ) {
        if linked_entry == included.unit().entry {
            return;
        }
        let linked_path = included
            .unit()
            .functions
            .get(linked_entry.index())
            .and_then(|function| included.unit().files.get(function.span.file.index()))
            .map(|file| PathBuf::from(&file.path));
        if let Some(linked_path) = linked_path
            && !state.included_once.contains(&linked_path)
        {
            state.included_once.push(linked_path);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_include_entry(
        &self,
        included: &CompiledUnit,
        function_id: FunctionId,
        call: FunctionCall<'_>,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        let included_path_label = included
            .unit()
            .functions
            .get(function_id.index())
            .and_then(|function| included.unit().files.get(function.span.file.index()))
            .map(|file| file.path.clone())
            .unwrap_or_default();
        if self.options.dense_include_execution.is_enabled()
            && self.options.execution_format.attempts_bytecode()
        {
            self.record_counter_dense_include_entry_attempt();
            match self.try_execute_dense_function_entry(
                included,
                function_id,
                call,
                output,
                stack,
                state,
            ) {
                BytecodeFunctionAttempt::Executed(result, BytecodeFunctionTier::Dense) => {
                    self.record_counter_dense_include_entry_success();
                    return *result;
                }
                BytecodeFunctionAttempt::Executed(
                    result,
                    BytecodeFunctionTier::RichFallback(reason),
                ) => {
                    self.record_counter_dense_include_entry_fallback(&reason, &included_path_label);
                    return *result;
                }
                BytecodeFunctionAttempt::Unsupported(message, call) => {
                    let reason = dense_bytecode_unsupported_reason(&message);
                    self.record_counter_bytecode_unsupported_reason(reason);
                    self.record_counter_dense_include_entry_fallback(reason, &included_path_label);
                    if self.options.execution_format.is_strict_bytecode() {
                        return VmResult::unsupported(output.clone(), message);
                    }
                    self.record_counter_bytecode_unsupported_fallback();
                    self.record_counter_bytecode_auto_fallback_reason(reason);
                    return self.execute_function(
                        included,
                        function_id,
                        call,
                        output,
                        stack,
                        state,
                    );
                }
            }
        }
        self.execute_function(included, function_id, call, output, stack, state)
    }

    pub(super) fn execute_include(
        &self,
        cursor: ExecutionCursor<'_>,
        request: IncludeExecutionRequest<'_>,
    ) -> VmResult {
        let ExecutionCursor {
            compiled,
            output,
            stack,
            state,
        } = cursor;
        let IncludeExecutionRequest {
            site:
                UnitInlineCacheSite {
                    cache_id,
                    unit_key,
                    function: function_id,
                    block: block_id,
                    instruction: instruction_id,
                },
            instruction_span,
            kind,
            path,
        } = request;
        let path = match to_string(path) {
            Ok(path) => path.to_string_lossy(),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        let including_file = current_source_path(compiled, stack);
        let include_path = state_include_path(state);
        let cwd = state.cwd.clone();
        self.record_include_trace_event(format!(
            "request kind={} path={} including_file={} stack_depth={}",
            include_kind_function_name(kind),
            path,
            including_file
                .as_deref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<entry>".to_owned()),
            state.include_stack.len(),
        ));
        let request = IncludePathCacheKey {
            path: path.clone(),
            include_path: include_path.as_ref().clone(),
            cwd: cwd.clone(),
            calling_file_directory: including_file
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf),
        };
        let mut compiled_include = None;
        let mut include_path_recorded = false;
        let loaded = if php_runtime::api::phar::is_phar_uri(&path) {
            self.record_counter_fallback_by_path_semantics("phar_stream");
            match load_phar_include(&path, &cwd, &self.options.runtime_context.filesystem) {
                Ok(loaded) => loaded,
                Err(message) => {
                    return include_failure(
                        output,
                        compiled,
                        instruction_span,
                        kind,
                        message,
                        state,
                        stack_trace(compiled, stack),
                    );
                }
            }
        } else {
            let Some(loader) = &self.options.include_loader else {
                self.record_counter_fallback_by_path_semantics("loader_disabled");
                return include_failure(
                    output,
                    compiled,
                    instruction_span,
                    kind,
                    include_vm_error(
                        "E_PHP_VM_INCLUDE_DISABLED",
                        "include/require loader is not configured",
                    ),
                    state,
                    stack_trace(compiled, stack),
                );
            };
            if let Some(cache) = &self.options.include_cache {
                let before_resolve = cache.cache_stats();
                let resolved = match cache.resolve_with_include_path(
                    loader,
                    including_file.as_deref(),
                    &path,
                    &include_path,
                    Some(&cwd),
                ) {
                    Ok(resolved) => resolved,
                    Err(message) => {
                        self.record_include_cache_stats_delta(before_resolve, cache.cache_stats());
                        self.record_include_graph_resolution_fallback(
                            &path,
                            &message.render_message(),
                        );
                        return include_failure(
                            output,
                            compiled,
                            instruction_span,
                            kind,
                            message,
                            state,
                            stack_trace(compiled, stack),
                        );
                    }
                };
                self.record_include_cache_stats_delta(before_resolve, cache.cache_stats());
                let after_resolve = cache.cache_stats();
                self.record_include_trace_event(format!(
                    "resolved kind={} path={} canonical={} resolution_cache={} stack_depth={}",
                    include_kind_function_name(kind),
                    path,
                    resolved.canonical_path.display(),
                    include_cache_resolution_outcome(before_resolve, after_resolve),
                    state.include_stack.len(),
                ));
                if matches!(kind, IncludeKind::IncludeOnce | IncludeKind::RequireOnce) {
                    if state.has_included(&resolved.canonical_path) {
                        self.record_counter_include_once_skip();
                        self.record_include_trace_event(format!(
                            "once kind={} canonical={} decision=skip stack_depth={}",
                            include_kind_function_name(kind),
                            resolved.canonical_path.display(),
                            state.include_stack.len(),
                        ));
                        return VmResult::success_no_output(Some(Value::Bool(true)));
                    }
                    state.record_included(resolved.canonical_path.clone());
                    include_path_recorded = true;
                    self.record_include_trace_event(format!(
                        "once kind={} canonical={} decision=record stack_depth={}",
                        include_kind_function_name(kind),
                        resolved.canonical_path.display(),
                        state.include_stack.len(),
                    ));
                } else {
                    state.record_included(resolved.canonical_path.clone());
                    include_path_recorded = true;
                    self.record_include_trace_event(format!(
                        "once kind={} canonical={} decision=not_once stack_depth={}",
                        include_kind_function_name(kind),
                        resolved.canonical_path.display(),
                        state.include_stack.len(),
                    ));
                }
                if self.options.trace_includes
                    && state.include_stack.contains(&resolved.canonical_path)
                {
                    return include_failure(
                        output,
                        compiled,
                        instruction_span,
                        kind,
                        include_vm_error(
                            "E_PHP_VM_INCLUDE_CYCLE",
                            format!(
                                "recursive include {} stack=[{}]",
                                resolved.canonical_path.display(),
                                format_include_stack(&state.include_stack),
                            ),
                        )
                        .with_context("canonical_path", resolved.canonical_path.display()),
                        state,
                        stack_trace(compiled, stack),
                    );
                }
                let before_compile = cache.cache_stats();
                let Some(include_compiler) = self.options.include_compiler.as_deref() else {
                    return include_failure(
                        output,
                        compiled,
                        instruction_span,
                        kind,
                        include_vm_error(
                            "E_PHP_VM_INCLUDE_COMPILER_UNAVAILABLE",
                            "include compiler is not configured",
                        ),
                        state,
                        stack_trace(compiled, stack),
                    );
                };
                let included =
                    match cache.get_or_compile_include(loader, &resolved, include_compiler) {
                        Ok(included) => included,
                        Err(message) => {
                            self.record_include_cache_stats_delta(
                                before_compile,
                                cache.cache_stats(),
                            );
                            return include_failure(
                                output,
                                compiled,
                                instruction_span,
                                kind,
                                message,
                                state,
                                stack_trace(compiled, stack),
                            );
                        }
                    };
                self.record_include_cache_stats_delta(before_compile, cache.cache_stats());
                let after_compile = cache.cache_stats();
                self.record_include_trace_event(format!(
                    "compiled kind={} canonical={} compile_cache={} functions={} classes={} constants={} entry_instructions={}",
                    include_kind_function_name(kind),
                    resolved.canonical_path.display(),
                    include_cache_compile_outcome(before_compile, after_compile),
                    included.unit().functions.len(),
                    included.unit().classes.len(),
                    included.unit().constant_table.len(),
                    entry_instruction_count(included.unit()),
                ));
                compiled_include = Some(included.as_ref().clone());
                LoadedInclude {
                    canonical_path: resolved.canonical_path,
                    source: String::new(),
                }
            } else {
                let cached = self.lookup_include_path_inline_cache(
                    UnitInlineCacheSite::new(
                        cache_id,
                        unit_key,
                        function_id,
                        block_id,
                        instruction_id,
                    ),
                    &request,
                    InvalidationEpoch::default(),
                );
                if let Some(target) = cached {
                    if target.is_current() {
                        self.record_include_path_inline_cache_hit(
                            cache_id,
                            unit_key,
                            function_id,
                            block_id,
                            instruction_id,
                        );
                        self.record_counter_directory_version_observation(&target);
                        match loader.load_resolved(target.canonical_path) {
                            Ok(loaded) => loaded,
                            Err(message) => {
                                return include_failure(
                                    output,
                                    compiled,
                                    instruction_span,
                                    kind,
                                    message,
                                    state,
                                    stack_trace(compiled, stack),
                                );
                            }
                        }
                    } else {
                        self.record_include_path_inline_cache_invalidation(
                            cache_id,
                            unit_key,
                            function_id,
                            block_id,
                            instruction_id,
                        );
                        self.record_counter_invalidation_by_reason("file_fingerprint_changed");
                        self.record_counter_include_stale_invalidation_by_reason(
                            "file_fingerprint_changed",
                        );
                        match loader.resolve_with_include_path(
                            including_file.as_deref(),
                            &path,
                            &include_path,
                            Some(&cwd),
                        ) {
                            Ok(resolved) => {
                                let target = IncludePathCacheTarget::from_resolved(&resolved);
                                self.install_include_path_inline_cache(
                                    UnitInlineCacheSite::new(
                                        cache_id,
                                        unit_key,
                                        function_id,
                                        block_id,
                                        instruction_id,
                                    ),
                                    request.clone(),
                                    InvalidationEpoch::default(),
                                    target,
                                );
                                match loader.load_resolved(resolved.canonical_path) {
                                    Ok(loaded) => loaded,
                                    Err(message) => {
                                        return include_failure(
                                            output,
                                            compiled,
                                            instruction_span,
                                            kind,
                                            message,
                                            state,
                                            stack_trace(compiled, stack),
                                        );
                                    }
                                }
                            }
                            Err(message) => {
                                self.record_include_graph_resolution_fallback(
                                    &path,
                                    &message.render_message(),
                                );
                                return include_failure(
                                    output,
                                    compiled,
                                    instruction_span,
                                    kind,
                                    message,
                                    state,
                                    stack_trace(compiled, stack),
                                );
                            }
                        }
                    }
                } else {
                    match loader.resolve_with_include_path(
                        including_file.as_deref(),
                        &path,
                        &include_path,
                        Some(&cwd),
                    ) {
                        Ok(resolved) => {
                            let target = IncludePathCacheTarget::from_resolved(&resolved);
                            self.install_include_path_inline_cache(
                                UnitInlineCacheSite::new(
                                    cache_id,
                                    unit_key,
                                    function_id,
                                    block_id,
                                    instruction_id,
                                ),
                                request,
                                InvalidationEpoch::default(),
                                target,
                            );
                            match loader.load_resolved(resolved.canonical_path) {
                                Ok(loaded) => loaded,
                                Err(message) => {
                                    return include_failure(
                                        output,
                                        compiled,
                                        instruction_span,
                                        kind,
                                        message,
                                        state,
                                        stack_trace(compiled, stack),
                                    );
                                }
                            }
                        }
                        Err(message) => {
                            self.record_include_graph_resolution_fallback(
                                &path,
                                &message.render_message(),
                            );
                            return include_failure(
                                output,
                                compiled,
                                instruction_span,
                                kind,
                                message,
                                state,
                                stack_trace(compiled, stack),
                            );
                        }
                    }
                }
            }
        };
        if !include_path_recorded {
            if matches!(kind, IncludeKind::IncludeOnce | IncludeKind::RequireOnce) {
                if state.has_included(&loaded.canonical_path) {
                    self.record_counter_include_once_skip();
                    self.record_include_trace_event(format!(
                        "once kind={} canonical={} decision=skip stack_depth={}",
                        include_kind_function_name(kind),
                        loaded.canonical_path.display(),
                        state.include_stack.len(),
                    ));
                    return VmResult::success_no_output(Some(Value::Bool(true)));
                }
                state.record_included(loaded.canonical_path.clone());
                self.record_include_trace_event(format!(
                    "once kind={} canonical={} decision=record stack_depth={}",
                    include_kind_function_name(kind),
                    loaded.canonical_path.display(),
                    state.include_stack.len(),
                ));
            } else {
                state.record_included(loaded.canonical_path.clone());
                self.record_include_trace_event(format!(
                    "once kind={} canonical={} decision=not_once stack_depth={}",
                    include_kind_function_name(kind),
                    loaded.canonical_path.display(),
                    state.include_stack.len(),
                ));
            }
        }

        if self.options.trace_includes && state.include_stack.contains(&loaded.canonical_path) {
            return include_failure(
                output,
                compiled,
                instruction_span,
                kind,
                include_vm_error(
                    "E_PHP_VM_INCLUDE_CYCLE",
                    format!(
                        "recursive include {} stack=[{}]",
                        loaded.canonical_path.display(),
                        format_include_stack(&state.include_stack),
                    ),
                )
                .with_context("canonical_path", loaded.canonical_path.display()),
                state,
                stack_trace(compiled, stack),
            );
        }

        let compiled_from_shared_cache = compiled_include.is_some();
        let included = match compiled_include {
            Some(included) => included,
            None => match self
                .options
                .include_compiler
                .as_deref()
                .ok_or_else(|| {
                    include_vm_error(
                        "E_PHP_VM_INCLUDE_COMPILER_UNAVAILABLE",
                        "include compiler is not configured",
                    )
                })
                .and_then(|compiler| {
                    let loader = self.options.include_loader.as_ref().ok_or_else(|| {
                        include_vm_error(
                            "E_PHP_VM_INCLUDE_DISABLED",
                            "include loader is not configured",
                        )
                    })?;
                    let resolved = loader.resolve_with_include_path(
                        None,
                        &loaded.canonical_path.to_string_lossy(),
                        &[],
                        loader.allowed_roots().first().map(PathBuf::as_path),
                    )?;
                    let validated = loader.load_validated_resolved(&resolved)?;
                    compiler
                        .compile_include(validated, loader)
                        .map(|compilation| compilation.unit)
                }) {
                Ok(included) => {
                    self.record_counter_include_compile_miss();
                    included
                }
                Err(message) => {
                    return include_failure(
                        output,
                        compiled,
                        instruction_span,
                        kind,
                        message,
                        state,
                        stack_trace(compiled, stack),
                    );
                }
            },
        };
        if !compiled_from_shared_cache {
            self.record_include_trace_event(format!(
                "compiled kind={} cache=off functions={} classes={} constants={} entry_instructions={}",
                include_kind_function_name(kind),
                included.unit().functions.len(),
                included.unit().classes.len(),
                included.unit().constant_table.len(),
                entry_instruction_count(included.unit()),
            ));
        }
        if let Err(message) = validate_dynamic_declarations(compiled, state, &included) {
            return self.runtime_error(output, compiled, stack, message);
        }
        let mut declaration_diagnostics = Vec::new();
        if let Err(result) = self.emit_duplicate_dynamic_constant_warnings(
            ExecutionCursor::new(compiled, output, stack, state),
            &included,
            DeclarationLoadKind::Include,
            None,
            &mut declaration_diagnostics,
        ) {
            return *result;
        }
        register_dynamic_unit(
            state,
            compiled,
            included.clone(),
            DeclarationLoadKind::Include,
        );
        let mut shared = shared_locals_from_current_frame(compiled, stack);
        let caller_is_top_level = compiled
            .unit()
            .functions
            .get(function_id.index())
            .is_some_and(|function| function.flags.is_top_level);
        let included_path = included
            .unit()
            .files
            .first()
            .map(|file| PathBuf::from(&file.path))
            .unwrap_or_default();
        state.include_stack.push(included_path.clone());
        self.record_include_trace_event(format!(
            "execute-start kind={} canonical={} stack_depth={}",
            include_kind_function_name(kind),
            included_path.display(),
            state.include_stack.len(),
        ));
        let include_instructions_before = self.current_instructions_executed();
        let include_bytecode_before = self.current_bytecode_instructions_executed();
        let prior_include_depth = self.include_execution_depth.get();
        self.include_execution_depth
            .set(prior_include_depth.saturating_add(1));
        let include_profile_boundary = self.request_profile_boundary_start();
        let mut result = self.execute_linked_include_entries(
            compiled,
            &included,
            instruction_span,
            caller_is_top_level,
            &mut shared,
            output,
            stack,
            state,
        );
        self.include_execution_depth.set(prior_include_depth);
        self.record_counter_include_profile(
            &included_path.display().to_string(),
            include_profile_boundary,
        );
        let include_instructions_after = self.current_instructions_executed();
        let include_bytecode_after = self.current_bytecode_instructions_executed();
        let include_instructions_executed = include_instructions_before
            .zip(include_instructions_after)
            .map(|(before, after)| after.saturating_sub(before))
            .map(|count| count.to_string())
            .unwrap_or_else(|| "unavailable".to_owned());
        let include_bytecode_executed = include_bytecode_before
            .zip(include_bytecode_after)
            .map(|(before, after)| after.saturating_sub(before))
            .map(|count| count.to_string())
            .unwrap_or_else(|| "unavailable".to_owned());
        self.record_include_trace_event(format!(
            "execute-end kind={} canonical={} status={:?} stack_depth={} instructions_executed={} bytecode_instructions_executed={}",
            include_kind_function_name(kind),
            included_path.display(),
            result.status.exit_status(),
            state.include_stack.len(),
            include_instructions_executed,
            include_bytecode_executed,
        ));
        if result.status.is_success()
            && let Some(error) =
                self.validate_runtime_class_dependencies(compiled, &included, output, stack, state)
        {
            result = error;
        }
        state.include_stack.pop();
        if result.status.is_success() {
            result.return_value =
                include_return_value(result.return_value.take(), result.returned_explicitly);
            write_shared_locals_to_current_frame(compiled, stack, &shared);
        }
        result.diagnostics.splice(0..0, declaration_diagnostics);
        result
    }

    pub(super) fn get_included_files_value(
        compiled: &CompiledUnit,
        stack: &CallStack,
        state: &ExecutionState,
    ) -> Value {
        let mut paths = Vec::new();
        if let Some(path) = current_source_path(compiled, stack) {
            paths.push(path);
        }
        for path in &state.included_once {
            if !paths.iter().any(|existing| existing == path) {
                paths.push(path.clone());
            }
        }
        Value::Array(PhpArray::from_packed(
            paths
                .into_iter()
                .map(|path| Value::string(path.to_string_lossy().into_owned()))
                .collect(),
        ))
    }

    pub(super) fn execute_eval(
        &self,
        compiled: &CompiledUnit,
        code: &Value,
        eval_span: IrSpan,
        output: &mut OutputBuffer,
        stack: &mut CallStack,
        state: &mut ExecutionState,
    ) -> VmResult {
        if state.eval_depth >= MAX_EVAL_DEPTH {
            return eval_failure(
                output,
                "E_PHP_VM_EVAL_RECURSION_LIMIT: maximum nested eval depth exceeded",
                stack_trace(compiled, stack),
            );
        }

        let code = match to_string(code) {
            Ok(code) => code.to_string_lossy(),
            Err(message) => return self.runtime_error(output, compiled, stack, message),
        };
        state.eval_counter += 1;
        state.bump_lookup_epoch();
        let source_path = format!("eval://{}", state.eval_counter);
        let source = format!("<?php {code}");
        let Some(include_compiler) = self.options.include_compiler.as_deref() else {
            return eval_failure(
                output,
                "E_PHP_VM_EVAL_COMPILER_UNAVAILABLE: eval compiler is not configured",
                stack_trace(compiled, stack),
            );
        };
        let evaluated = match include_compiler.compile_eval(&source_path, &source) {
            Ok(evaluated) => evaluated,
            Err(error) => {
                return eval_failure(output, error.render_message(), stack_trace(compiled, stack));
            }
        };
        let has_named_function_declarations = evaluated
            .unit()
            .function_table
            .iter()
            .any(|entry| entry.function != evaluated.unit().entry);
        let has_source_class_declarations = evaluated
            .class_table()
            .any(|class| !is_lowered_internal_interface_skeleton(class));
        let has_constant_declarations = !evaluated.unit().constant_table.is_empty();
        let has_declarations = has_named_function_declarations
            || has_source_class_declarations
            || has_constant_declarations;
        let mut declaration_diagnostics = Vec::new();
        if has_declarations {
            if let Err(message) = validate_dynamic_declarations(compiled, state, &evaluated) {
                return eval_failure(output, message, stack_trace(compiled, stack));
            }
            if let Err(result) = self.emit_duplicate_dynamic_constant_warnings(
                ExecutionCursor::new(compiled, output, stack, state),
                &evaluated,
                DeclarationLoadKind::Eval,
                Some(eval_span),
                &mut declaration_diagnostics,
            ) {
                return *result;
            }
            register_dynamic_unit(
                state,
                compiled,
                evaluated.clone(),
                DeclarationLoadKind::Eval,
            );
        }
        let has_closures = evaluated
            .unit()
            .functions
            .iter()
            .any(|function| function.flags.is_closure);
        if has_closures && !has_declarations {
            retain_dynamic_closure_unit(state, evaluated.clone());
        }

        let mut shared = shared_locals_from_current_frame(compiled, stack);
        let call = FunctionCall {
            positional_values: None,
            args: Vec::new(),
            captures: Vec::new(),
            call_span: Some(eval_span),
            call_site_strict_types: Some(compiled.unit().strict_types),
            error_context_compiled: None,
            allow_by_ref_value_warnings: false,
            by_ref_warning_callable_name: None,
            this_value: None,
            scope_class: None,
            called_class: None,
            declaring_class: None,
            shared_top_level_locals: Some(&mut shared),
            shared_top_level_bind_missing_globals: current_frame_is_top_level(compiled, stack),
            running_generator: None,
            resume_continuation: None,
            resume_input: None,
            running_fiber: None,
            resume_fiber_continuation: None,
            resume_fiber_input: None,
        };
        state.eval_depth += 1;
        state
            .eval_diagnostic_spans
            .push(eval_diagnostic_source_span(compiled, eval_span));
        let mut result = self.execute_function(
            &evaluated,
            evaluated.unit().entry,
            call,
            output,
            stack,
            state,
        );
        state.eval_diagnostic_spans.pop();
        state.eval_depth -= 1;
        if result.status.is_success()
            && has_source_class_declarations
            && let Some(error) =
                self.validate_runtime_class_dependencies(compiled, &evaluated, output, stack, state)
        {
            return error;
        }
        if result.status.is_success() {
            write_shared_locals_to_current_frame(compiled, stack, &shared);
        }
        result.diagnostics.splice(0..0, declaration_diagnostics);
        result
    }
}

pub(super) fn include_failure(
    output: &mut OutputBuffer,
    compiled: &CompiledUnit,
    span: IrSpan,
    kind: IncludeKind,
    error: VmError,
    state: &ExecutionState,
    stack_trace: Vec<RuntimeStackFrame>,
) -> VmResult {
    let message = error.render_message();
    let severity = if matches!(kind, IncludeKind::Include | IncludeKind::IncludeOnce) {
        RuntimeSeverity::Warning
    } else {
        RuntimeSeverity::FatalError
    };
    let source_span = runtime_source_span(compiled, span);
    let mut diagnostic = RuntimeDiagnostic::new(
        error.code(),
        severity,
        message.clone(),
        source_span,
        stack_trace,
        None,
    );
    if matches!(
        error.code(),
        "E_PHP_VM_INCLUDE_MISSING" | "E_PHP_VM_INCLUDE_READ"
    ) && let Some(target) = error
        .context()
        .get("path")
        .or_else(|| error.context().get("canonical_path"))
        .or_else(|| error.context().get("candidate"))
    {
        diagnostic = diagnostic.with_diagnostic_payload(RuntimeDiagnosticPayload::IncludeFailure(
            php_runtime::api::IncludeFailureDiagnosticContext::new(
                target,
                include_failure_reason(
                    error
                        .context()
                        .get("reason")
                        .map_or(error.message(), String::as_str),
                ),
            ),
        ));
    }
    emit_include_failure_output(output, compiled, span, kind, state, &diagnostic);
    VmResult::runtime_error_with_diagnostic(output.clone(), message.clone(), diagnostic)
}

pub(super) fn load_phar_include(
    uri: &str,
    cwd: &Path,
    filesystem: &php_runtime::api::FilesystemCapabilities,
) -> Result<LoadedInclude, VmError> {
    let parsed = php_runtime::api::phar::parse_uri(uri, cwd, filesystem).map_err(|error| {
        include_vm_error(error.diagnostic_id(), error.message())
            .with_context("path", uri)
            .with_context("reason", error.message())
    })?;
    let bytes = php_runtime::api::phar::read_entry(&parsed.archive_path, &parsed.entry_path)
        .map_err(|error| {
            include_vm_error(error.diagnostic_id(), error.message())
                .with_context("path", uri)
                .with_context("reason", error.message())
        })?;
    let source = String::from_utf8(bytes).map_err(|_| {
        include_vm_error(
            "E_PHP_VM_INCLUDE_READ",
            format!("{uri}: PHAR entry is not valid UTF-8 PHP source"),
        )
        .with_context("path", uri)
        .with_context("reason", "PHAR entry is not valid UTF-8 PHP source")
    })?;
    Ok(LoadedInclude {
        canonical_path: parsed.synthetic_path,
        source,
    })
}

#[cold]
#[inline(never)]
pub(super) fn include_vm_error(code: &'static str, message: impl Into<String>) -> VmError {
    VmError::fatal(code, "include", message)
}

pub(super) fn include_failure_allows_continuation(kind: IncludeKind, result: &VmResult) -> bool {
    matches!(kind, IncludeKind::Include | IncludeKind::IncludeOnce)
        && result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity() == RuntimeSeverity::Warning)
        && result
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity() == RuntimeSeverity::Warning)
}

#[cold]
pub(super) fn emit_include_failure_output(
    output: &mut OutputBuffer,
    compiled: &CompiledUnit,
    span: IrSpan,
    kind: IncludeKind,
    state: &ExecutionState,
    diagnostic: &RuntimeDiagnostic,
) {
    let Some(RuntimeDiagnosticPayload::IncludeFailure(payload)) = diagnostic.payload() else {
        let channel =
            php_runtime::api::PhpDiagnosticChannel::from_runtime_severity(diagnostic.severity());
        let level = match diagnostic.severity() {
            RuntimeSeverity::FatalError => php_runtime::api::PHP_E_ERROR,
            _ => php_runtime::api::PHP_E_WARNING,
        };
        emit_vm_diagnostic(output, state, diagnostic, channel, level);
        return;
    };
    let target = payload.target();
    let reason = payload.reason();
    let (file, line) = source_span_file_line(compiled, span)
        .unwrap_or_else(|| ("<unknown>".to_owned(), i64::from(span.start)));
    if display_errors_enabled(state)
        && error_reporting_allows(state, php_runtime::api::PHP_E_WARNING)
    {
        let function_name = include_kind_function_name(kind);
        output.write_bytes(
            format!(
                "\nWarning: {function_name}({target}): Failed to open stream: {reason} in {file} on line {line}\n",
            )
            .as_bytes(),
        );
        if matches!(kind, IncludeKind::Include | IncludeKind::IncludeOnce) {
            let include_path = include_path_warning_display(state);
            output.write_bytes(
                format!(
                    "\nWarning: {function_name}(): Failed opening '{target}' for inclusion (include_path='{include_path}') in {file} on line {line}\n"
                )
                .as_bytes(),
            );
        }
    }
    if matches!(kind, IncludeKind::Require | IncludeKind::RequireOnce)
        && display_errors_enabled(state)
        && error_reporting_allows(state, php_runtime::api::PHP_E_ERROR)
    {
        let include_path = include_path_warning_display(state);
        output.write_bytes(
            format!(
                "\nFatal error: Uncaught Error: Failed opening required '{target}' (include_path='{include_path}') in {file}:{line}\nStack trace:\n#0 {{main}}\n  thrown in {file} on line {line}\n"
            )
            .as_bytes(),
        );
    }
}

#[cold]
fn include_failure_reason(reason: &str) -> String {
    if reason.contains("No such file or directory") || reason.ends_with(": not found") {
        "No such file or directory".to_owned()
    } else if reason.contains("Is a directory") {
        "Is a directory".to_owned()
    } else {
        reason
            .split_once(" (os error")
            .map(|(reason, _)| reason)
            .unwrap_or(reason)
            .to_owned()
    }
}

pub(super) fn include_path_warning_display(state: &ExecutionState) -> &str {
    match state.ini.get("include_path") {
        Some(".") | None => ".:",
        Some(value) => value,
    }
}

pub(super) fn include_kind_function_name(kind: IncludeKind) -> &'static str {
    match kind {
        IncludeKind::Include => "include",
        IncludeKind::IncludeOnce => "include_once",
        IncludeKind::Require => "require",
        IncludeKind::RequireOnce => "require_once",
    }
}

pub(super) fn include_cache_resolution_outcome(
    before: IncludeCacheStats,
    after: IncludeCacheStats,
) -> &'static str {
    if after.resolution_hits > before.resolution_hits {
        "hit"
    } else if after.resolution_misses > before.resolution_misses {
        "miss"
    } else {
        "unchanged"
    }
}

pub(super) fn include_cache_compile_outcome(
    before: IncludeCacheStats,
    after: IncludeCacheStats,
) -> &'static str {
    if after.compile_hits > before.compile_hits {
        "hit"
    } else if after.compile_misses > before.compile_misses {
        "miss"
    } else if after.compile_errors > before.compile_errors {
        "error"
    } else {
        "unchanged"
    }
}

pub(super) fn entry_instruction_count(unit: &IrUnit) -> usize {
    unit.functions
        .get(unit.entry.index())
        .map(|function| {
            function
                .blocks
                .iter()
                .map(|block| block.instructions.len() + usize::from(block.terminator.is_some()))
                .sum()
        })
        .unwrap_or(0)
}

pub(super) fn format_include_stack(stack: &[PathBuf]) -> String {
    stack
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(" -> ")
}
