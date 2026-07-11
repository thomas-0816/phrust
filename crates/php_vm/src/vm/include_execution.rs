use super::prelude::*;

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
            // A linked file that compile-time inference injected stands in
            // for what the autoload protocol would load at class-link time in
            // reference PHP. Mirror that: an already-declared or
            // autoloader-provided declaration wins and the injected copy is
            // skipped; a declaration nobody provides is the reference
            // "Trait not found" failure.
            if let Some(normalized_name) = included
                .unit()
                .linked_entry_inferred_declarations
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
                    return result;
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
                positional_values: Vec::new(),
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
}
