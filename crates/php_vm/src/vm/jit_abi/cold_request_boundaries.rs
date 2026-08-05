//! Cold outer-request and lifecycle boundaries.
//!
//! Userland callback bodies enter one generated Generic continuation; this
//! module only owns request orchestration and result/control materialization.

use super::*;
use php_runtime::api::PhpString;
use php_runtime::api::Value;

impl<'a> NativeRequestColdState<'a> {
    pub(in crate::vm) fn new(
        compiled: &'a crate::compiled_unit::CompiledUnit,
        unit_identity: u64,
        options: &'a crate::vm::VmOptions,
        worker_state: &'a crate::vm::VmWorkerState,
        output: php_runtime::api::OutputBuffer,
        native_entries: std::sync::Arc<
            std::collections::BTreeMap<php_ir::FunctionId, php_jit::JitFunctionHandle>,
        >,
    ) -> Self {
        let unit = compiled.unit();
        let inherited_globals =
            BASELINE_INCLUDE_GLOBALS.with(|globals| globals.borrow_mut().take());
        let inherited_constants =
            BASELINE_INCLUDE_CONSTANTS.with(|constants| constants.borrow_mut().take());
        let inherited_ini = BASELINE_INCLUDE_INI.with(|ini| ini.borrow_mut().take());
        let inherited_default_timezone =
            BASELINE_INCLUDE_DEFAULT_TIMEZONE.with(|timezone| timezone.borrow_mut().take());
        let inherited_http_response =
            BASELINE_INCLUDE_HTTP_RESPONSE.with(|response| response.borrow_mut().take());
        let inherited_files = BASELINE_INCLUDE_FILES.with(|files| files.borrow_mut().take());
        let inherited_mysql = BASELINE_INCLUDE_MYSQL.with(|mysql| mysql.borrow_mut().take());
        let inherited_filter_input_arrays =
            BASELINE_INCLUDE_FILTER_INPUT_ARRAYS.with(|arrays| arrays.borrow_mut().take());
        let inherited_function_names = BASELINE_INCLUDE_FUNCTION_NAMES.with(|names| {
            names
                .borrow_mut()
                .take()
                .unwrap_or_else(|| Rc::new(NativeFunctionNameScope::default()))
        });
        let visible_function_names = NativeFunctionNameScope::child(
            inherited_function_names,
            unit.function_table
                .iter()
                .map(|entry| entry.name.to_ascii_lowercase()),
        );
        let mut inherited_symbols = BASELINE_INCLUDE_SYMBOLS
            .with(|symbols| symbols.borrow_mut().take().unwrap_or_default());
        let inherited_error_reporting = inherited_symbols.error_reporting;
        let inherited_display_errors = inherited_symbols.display_errors;
        let include_child = inherited_globals.is_some();
        if include_child {
            for package in &mut inherited_symbols.dynamic_units {
                package.reset_runtime_publication();
            }
        }
        let mut inherited_globals = inherited_globals.unwrap_or_default();
        let session = options.runtime_context.session.clone();
        let session_global = inherited_globals
            .get("_SESSION")
            .and_then(|value| match value {
                Value::Reference(reference) => Some(reference.clone()),
                _ => None,
            })
            .unwrap_or_else(|| {
                php_runtime::api::ReferenceCell::new(
                    if session.status() == php_runtime::api::PHP_SESSION_ACTIVE || session.started()
                    {
                        session.data_value()
                    } else {
                        Value::Uninitialized
                    },
                )
            });
        inherited_globals.insert(
            "_SESSION".to_owned(),
            Value::Reference(session_global.clone()),
        );
        let filter_input_arrays = inherited_filter_input_arrays.unwrap_or_else(|| {
            Rc::new(
                [0_i64, 1, 2, 4, 5]
                    .into_iter()
                    .filter_map(|source| {
                        options
                            .runtime_context
                            .filter_input_array(source)
                            .map(|array| (source, array))
                    })
                    .collect(),
            )
        });
        let mut resources = php_runtime::api::ResourceTable::new();
        let stdin = resources.register_stdin(options.runtime_context.stdin.to_vec());
        let stdout = resources.register_stdout();
        let stderr = resources.register_stderr();
        let mut cold_dynamic_constants = inherited_constants.unwrap_or_default();
        cold_dynamic_constants
            .entry("STDIN".to_owned())
            .or_insert(Value::Resource(stdin));
        cold_dynamic_constants
            .entry("STDOUT".to_owned())
            .or_insert(Value::Resource(stdout));
        cold_dynamic_constants
            .entry("STDERR".to_owned())
            .or_insert(Value::Resource(stderr));
        let (trusted_property_function_offsets, continuation_capacity) =
            trusted_continuation_storage(compiled.unit());
        let trusted_property_slots =
            php_runtime::api::StableNativeArena::new(continuation_capacity);
        let trusted_closure_plans = php_runtime::api::StableNativeArena::new(continuation_capacity);
        let trusted_exception_plans =
            php_runtime::api::StableNativeArena::new(continuation_capacity);
        let trusted_exception_plan_owners = std::collections::BTreeMap::new();
        let (trusted_request_local_function_offsets, trusted_request_local_slots) =
            trusted_request_local_storage(compiled.unit());
        let trusted_constant_slots =
            php_runtime::api::StableNativeArena::new(continuation_capacity);
        let trusted_global_reference_slots =
            php_runtime::api::StableNativeArena::new(continuation_capacity);
        let trusted_global_reference_names = std::collections::BTreeMap::new();
        let trusted_static_local_slots =
            php_runtime::api::StableNativeArena::new(continuation_capacity);
        let trusted_static_property_slots =
            php_runtime::api::StableNativeArena::new(continuation_capacity);
        let trusted_instanceof_plans =
            php_runtime::api::StableNativeArena::new(continuation_capacity);
        let trusted_exception_route_plans =
            php_runtime::api::StableNativeArena::new(continuation_capacity);
        let native_call_argument_capacity = compiled
            .prepared_deployment_image()
            .native_call_argument_capacity;
        let mut environment = std::sync::Arc::clone(&options.runtime_context.env);
        if !environment.windows(2).all(|pair| {
            pair[0].0 <= pair[1].0 && !(pair[0].0 == pair[1].0 && pair[0].1 > pair[1].1)
        }) {
            let mut sorted = environment.as_ref().clone();
            sorted.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
            environment = std::sync::Arc::new(sorted);
        }
        let NativeRequestBuffers {
            direct_value_slots,
            direct_value_next,
            direct_object_owners,
            direct_array_states,
            direct_array_entries,
            direct_array_next,
            direct_value_free_head,
            direct_value_reused_bytes,
            direct_array_free_heads,
            direct_array_reused_bytes,
            direct_string_bytes,
            direct_string_next,
            direct_string_free_heads,
            direct_string_reused_bytes,
            fiber_suspension_states,
            fiber_suspension_next,
            native_trace_frames,
            native_trace_depth,
            static_property_slots,
            static_property_next,
            native_call_encoded_scratch,
            native_frame_arena,
            direct_resource_handles,
            direct_closure_handles,
            class_constant_cache,
            diagnostic_telemetry,
        } = worker_state.checkout_native_request_buffers(native_call_argument_capacity);
        let baseline_values = BaselineValueState::new(
            cold_dynamic_constants,
            inherited_symbols.autoload_callbacks,
            inherited_symbols.shutdown_callbacks,
            inherited_symbols.error_handlers,
            inherited_symbols.exception_handlers,
            inherited_globals,
            session_global,
            filter_input_arrays,
            inherited_symbols.static_property_transfer,
            inherited_symbols.static_locals,
            inherited_symbols.enum_cases,
        );
        Self {
            compiled: compiled.clone(),
            unit: ActiveNativeUnit::new(compiled),
            unit_identity,
            options,
            worker_state,
            fast_state: std::ptr::null_mut(),
            native_entries,
            native_call_encoded_scratch,
            native_frame_arena,
            baseline_transition_store_owner_pending: false,
            fiber_suspension_states,
            fiber_suspension_next,
            native_trace_frames,
            native_trace_depth,
            native_execution_scopes: vec![Box::new(NativeExecutionScope {
                unit: None,
                called_class: None,
                scope_class: None,
            })],
            current_native_execution_scope: 1,
            output,
            direct_value_slots,
            direct_value_next,
            direct_object_owners,
            direct_array_states,
            direct_array_entries,
            direct_array_next,
            direct_value_free_head,
            direct_value_reused_bytes,
            direct_array_free_heads,
            direct_array_reused_bytes,
            direct_string_bytes,
            direct_string_next,
            direct_string_free_heads,
            direct_string_reused_bytes,
            static_property_slots,
            static_property_next,
            static_property_indices: std::collections::BTreeMap::new(),
            native_global_reference_handles: std::collections::BTreeMap::new(),
            direct_resource_handles,
            direct_closure_handles,
            direct_string_interned_slots: std::collections::HashMap::new(),
            cross_unit_stable_values: std::collections::HashSet::new(),
            // Wrapping 4095 + 1 makes the first loop-header visit poll. Native
            // code then checks the deadline once per 4096 loop-header visits.
            native_poll_counter: Box::new(4095),
            native_root_mutation_pending: Box::new(0),
            baseline_values,
            registered_callbacks: NativeRegisteredCallbackState::default(),
            runtime_class_cache: RefCell::new(std::collections::HashMap::new()),
            runtime_class_layout_cache: RefCell::new(std::collections::HashMap::new()),
            runtime_class_ancestry_cache: RefCell::new(std::collections::HashMap::new()),
            runtime_declared_property_slot_cache: RefCell::new(std::collections::HashMap::new()),
            trusted_class_plans: Vec::new(),
            root_index: RequestRootIndex::new_dirty(),
            resources,
            builtin_request_state: php_runtime::api::BuiltinRequestState::new(),
            registered_extensions: NativeRegisteredExtensionRequestState::default(),
            native_stream_context: NativeStreamContextState::default(),
            http_response: inherited_http_response.unwrap_or_default(),
            upload_registry: options.runtime_context.upload_registry(),
            session,
            ini_registry: inherited_ini.unwrap_or_else(|| options.runtime_context.ini_registry()),
            default_timezone: inherited_default_timezone
                .unwrap_or_else(|| php_runtime::api::datetime::DEFAULT_TIMEZONE.to_owned()),
            mysql_state: inherited_mysql
                .unwrap_or_else(|| std::rc::Rc::new(RefCell::new(Default::default()))),
            native_dynamic_constants: std::collections::BTreeMap::new(),
            trusted_dynamic_constant_sites: std::collections::BTreeMap::new(),
            visible_function_names,
            dynamic_functions: std::collections::BTreeMap::new(),
            deployment_functions: inherited_symbols.deployment_functions,
            deployment_classes: inherited_symbols.deployment_classes,
            external_functions: inherited_symbols.external_functions,
            external_class_units: inherited_symbols.external_class_units,
            external_signature_epoch: inherited_symbols.external_signature_epoch,
            dynamic_units: inherited_symbols.dynamic_units,
            current_dynamic_unit: None,
            typed_static_reference_constraints: inherited_symbols
                .typed_static_reference_constraints,
            class_constant_cache,
            active_fiber: None,
            pending_fiber_suspension_value: None,
            completed_nested_fiber_call: None,
            called_classes: Vec::new(),
            lexical_scope_classes: Vec::new(),
            call_frames: Vec::new(),
            dynamic_classes: inherited_symbols.dynamic_classes,
            class_aliases: inherited_symbols.class_aliases,
            shutdown_destructor_queue: None,
            destroyed_objects: inherited_symbols.destroyed_objects,
            error_reporting: inherited_error_reporting
                .unwrap_or(options.runtime_context.ini.error_reporting.mask),
            display_errors: inherited_display_errors
                .unwrap_or(options.runtime_context.ini.display_errors),
            last_error: inherited_symbols.last_error,
            explicit_reference_ids: std::collections::BTreeSet::new(),
            environment,
            included_files: inherited_files.unwrap_or_default(),
            include_path: Arc::new(options.runtime_context.include_path.clone()),
            cwd: options.runtime_context.cwd.clone(),
            trusted_globals_proxy: php_jit::jit_encode_constant(php_jit::JIT_VALUE_UNINITIALIZED),
            trusted_empty_string_key: php_jit::jit_encode_constant(
                php_jit::JIT_VALUE_UNINITIALIZED,
            ),
            trusted_request_local_function_offsets,
            trusted_request_local_slots,
            trusted_property_function_offsets,
            trusted_property_slots,
            trusted_closure_plans,
            trusted_exception_plans,
            trusted_exception_plan_owners,
            trusted_constant_slots,
            trusted_literal_slots: std::collections::BTreeMap::new(),
            trusted_global_reference_slots,
            trusted_global_reference_names,
            trusted_static_local_slots,
            trusted_static_property_slots,
            trusted_instanceof_plans,
            trusted_instanceof_entries: Vec::new(),
            prepared_internal_class_layouts: Vec::new(),
            trusted_exception_route_plans,
            trusted_exception_route_entries: Vec::new(),
            trusted_exception_route_symbol_epoch: 0,
            native_metadata_preparation_scope: None,
            prepared_native_metadata_functions: std::collections::BTreeSet::new(),
            include_child,
            execution_deadline_at: options
                .runtime_context
                .execution_time_limit
                .and_then(|limit| std::time::Instant::now().checked_add(limit)),
            execution_deadline_mutable: options.runtime_context.execution_time_limit.is_some(),
            runtime_telemetry: Rc::new(RefCell::new(diagnostic_telemetry)),
            diagnostic: None,
        }
    }

    pub(super) fn lookup_constant(&self, name: &str) -> Result<Value, String> {
        if let Some(encoded) = self.native_dynamic_constants.get(name).copied() {
            return self.native_dynamic_constant_value(encoded);
        }
        if let Some(value) = self.baseline_values.cold_dynamic_constants.get(name) {
            return Ok(value.clone());
        }
        if let Some(constant) = self
            .unit
            .constant_table
            .iter()
            .find(|constant| constant.name == name)
            .and_then(|constant| self.unit.constants.get(constant.value.index()))
        {
            return ir_constant_value(constant);
        }
        php_std::ExtensionRegistry::standard_library()
            .enabled_constant(name)
            .and_then(php_std::ConstantDescriptor::value)
            .map(php_std::constants::constant_to_value)
            .ok_or_else(|| format!("Undefined constant \"{name}\""))
    }

    pub(super) fn native_dynamic_constant_value(&self, encoded: i64) -> Result<Value, String> {
        match self.native_encoded_value_kind(encoded) {
            Some(NativeEncodedValueKind::Null) => Ok(Value::Null),
            Some(NativeEncodedValueKind::Bool(value)) => Ok(Value::Bool(value)),
            Some(NativeEncodedValueKind::Int) => self
                .native_encoded_int(encoded)
                .map(Value::Int)
                .ok_or_else(|| "native dynamic integer constant lost its payload".to_owned()),
            Some(NativeEncodedValueKind::Float) => self
                .native_encoded_float(encoded)
                .map(Value::float)
                .ok_or_else(|| "native dynamic float constant lost its payload".to_owned()),
            Some(NativeEncodedValueKind::String) => self
                .native_string_name_bytes(encoded)
                .map(|bytes| Value::String(PhpString::from_bytes(bytes)))
                .ok_or_else(|| "native dynamic string constant lost its bytes".to_owned()),
            Some(NativeEncodedValueKind::Array) => {
                let entries = self
                    .direct_array_entries_for(encoded)
                    .ok_or_else(|| "native dynamic array constant lost its entries".to_owned())?;
                let mut value = php_runtime::api::PhpArray::with_capacity(entries.len());
                for entry in entries {
                    let key = self
                        .native_encoded_plain_array_key(entry.key)
                        .ok_or_else(|| {
                            "native dynamic array constant has an invalid key".to_owned()
                        })?;
                    value.insert(key, self.native_dynamic_constant_value(entry.value)?);
                }
                if let Some(index) = Self::direct_value_index(encoded)
                    && let Some(state) = self.direct_array_states.get(index)
                {
                    value.set_native_next_append_key(
                        (state.has_next_append_key != 0).then_some(state.next_append_key),
                    );
                    value.set_native_pointer_position(
                        self.direct_value_slots
                            .get(index)
                            .and_then(|slot| php_jit::jit_native_direct_array_cursor(slot.flags))
                            .and_then(|cursor| usize::try_from(cursor).ok()),
                    );
                }
                Ok(Value::Array(value))
            }
            Some(NativeEncodedValueKind::Resource) => Self::direct_value_index(encoded)
                .and_then(|index| self.direct_resource(index))
                .map(Value::Resource)
                .ok_or_else(|| "native dynamic resource constant lost its owner".to_owned()),
            _ => Err("native dynamic constant left the admitted constant plane".to_owned()),
        }
    }

    pub(super) fn visible_include_constants(
        &mut self,
    ) -> Result<std::collections::BTreeMap<String, Value>, String> {
        self.materialize_native_dynamic_constants()?;
        let mut constants = self.baseline_values.cold_dynamic_constants.clone();
        for entry in &self.unit.constant_table {
            if let Some(value) = self.unit.constants.get(entry.value.index())
                && let Ok(value) = ir_constant_value(value)
            {
                constants.entry(entry.name.clone()).or_insert(value);
            }
        }
        Ok(constants)
    }

    pub(super) fn record_last_error(
        &mut self,
        error_type: i64,
        message: &str,
        file: &str,
        line: usize,
    ) {
        self.last_error = Some(NativeLastError {
            error_type,
            message: message.to_owned(),
            file: file.to_owned(),
            line,
        });
    }

    pub(super) fn last_error_value(&self) -> Value {
        let Some(error) = &self.last_error else {
            return Value::Null;
        };
        let mut value = php_runtime::api::PhpArray::new();
        for (name, field) in [
            ("type", Value::Int(error.error_type)),
            (
                "message",
                Value::String(PhpString::from_bytes(error.message.as_bytes().to_vec())),
            ),
            (
                "file",
                Value::String(PhpString::from_bytes(error.file.as_bytes().to_vec())),
            ),
            (
                "line",
                Value::Int(i64::try_from(error.line).unwrap_or(i64::MAX)),
            ),
        ] {
            value.insert(
                php_runtime::api::ArrayKey::String(PhpString::from_bytes(name.as_bytes().to_vec())),
                field,
            );
        }
        Value::Array(value)
    }

    pub(in crate::vm) fn take_pending_throwable(&mut self) -> Option<Value> {
        let throwable = self.baseline_values.pending_throwable.take();
        if throwable.is_some() {
            self.mark_roots_dirty(RootMutationReason::PendingThrowable);
        }
        throwable
    }

    /// Issues one cold callback action through the shared generated call
    /// spine. The outer lifecycle loop remains Rust, but it never resolves or
    /// invokes the userland target itself.
    pub(super) fn enter_generated_callback_continuation(
        &mut self,
        callable: i64,
        arguments: &[i64],
    ) -> NativeCallResult {
        let handle = self
            .worker_state
            .generated_callback_continuation(self.options)
            .map_err(NativeCallControl::RuntimeError)?;
        let mut entries: Vec<php_jit::JitNativeDirectArrayEntry> =
            Vec::with_capacity(arguments.len());
        for (index, argument) in arguments.iter().copied().enumerate() {
            let key = self
                .encode_native_int(i64::try_from(index).unwrap_or(i64::MAX))
                .map_err(NativeCallControl::RuntimeError)?;
            if let Err(error) = self.retain(argument) {
                self.release_if_live(key)
                    .map_err(NativeCallControl::RuntimeError)?;
                for entry in entries {
                    self.release_if_live(entry.key)
                        .map_err(NativeCallControl::RuntimeError)?;
                    self.release_if_live(entry.value)
                        .map_err(NativeCallControl::RuntimeError)?;
                }
                return Err(NativeCallControl::RuntimeError(error));
            }
            entries.push(php_jit::JitNativeDirectArrayEntry {
                key,
                value: argument,
            });
        }
        let argument_array = self
            .publish_owned_direct_array_entries(entries)
            .map_err(NativeCallControl::RuntimeError)?;
        let packed = [callable, argument_array];
        self.record_native_direct_calls(&handle);
        let _runtime_view = activate_native_context(self);
        let runtime = self.native_runtime_ptr();
        let outcome = handle.invoke_i64_with_native_unwind_runtime(
            &packed,
            php_jit::JIT_RUNTIME_ABI_HASH,
            runtime,
            |types, value| {
                types
                    .iter()
                    .any(|type_| self.direct_object_is_a(value, type_))
            },
        );
        let outcome = resume_native_optimizing_exit(self, handle, outcome);
        let release_arguments = self.release_if_live(argument_array);
        let result = match outcome {
            Ok(php_jit::JitI64InvokeOutcome::Returned(value)) => Ok(value),
            Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
                if status == php_jit::JitCallStatus::RETURN_REFERENCE.0 as i32 =>
            {
                Ok(value)
            }
            Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
                if status == php_jit::JitCallStatus::THROW.0 as i32 =>
            {
                let throwable = self
                    .decode_baseline_value(value)
                    .map_err(NativeCallControl::RuntimeError)?;
                self.baseline_values.pending_throwable = Some(throwable);
                self.mark_roots_dirty(RootMutationReason::PendingThrowable);
                Err(NativeCallControl::Rethrow)
            }
            Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
                if status == php_jit::JitCallStatus::EXIT.0 as i32 =>
            {
                Err(NativeCallControl::Exit(value))
            }
            Ok(php_jit::JitI64InvokeOutcome::SideExit { status, state, .. })
                if status == php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32 =>
            {
                if self.diagnostic.is_some() {
                    Err(NativeCallControl::PublishedRuntimeError)
                } else {
                    let operation = self.instruction_kind_debug_for_state(&state);
                    Err(NativeCallControl::RuntimeError(format!(
                        "generated callback continuation returned a runtime error at {}:{} ({operation}; reserved={:#x}, value={}, arguments={})",
                        state.function_id,
                        state.continuation_id,
                        state.control_reserved,
                        state.control_value,
                        arguments.len(),
                    )))
                }
            }
            Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. }) => {
                Err(NativeCallControl::RuntimeError(format!(
                    "generated callback continuation returned status {status} with value {value}"
                )))
            }
            Err(error) => Err(NativeCallControl::RuntimeError(format!(
                "generated callback continuation failed: {error:?}"
            ))),
        };
        release_arguments.map_err(NativeCallControl::RuntimeError)?;
        result
    }

    pub(in crate::vm) fn run_shutdown_callbacks(&mut self) -> Result<(), String> {
        if self.include_child {
            return Ok(());
        }
        while !self.registered_callbacks.shutdown_callbacks.is_empty() {
            let NativeRegisteredShutdownCallback {
                callable,
                arguments,
                source,
                transient_export: _,
            } = self.registered_callbacks.shutdown_callbacks.remove(0);
            self.mark_roots_dirty(RootMutationReason::CallbackOrHandler);
            let mut encoded = Vec::with_capacity(arguments.len() + 1);
            encoded.push(callable);
            encoded.extend_from_slice(&arguments);
            let result = self
                .enter_generated_callback_continuation(encoded[0], &encoded[1..])
                .map_err(NativeCallControl::into_baseline_error)
                .and_then(|returned| {
                    self.release_if_live(returned)?;
                    Ok(())
                });
            if matches!(&result, Err(error) if error == "E_PHP_RETHROW")
                && let Some(throwable) = self.take_pending_throwable()
            {
                self.baseline_values.pending_throwable = Some(
                    native_throwable_with_internal_frame(self, throwable, &source),
                );
                self.mark_roots_dirty(RootMutationReason::PendingThrowable);
            }
            let mut release_error = self.release_if_live(callable).err();
            for argument in arguments {
                if let Err(error) = self.release_if_live(argument) {
                    release_error.get_or_insert(error);
                }
            }
            result?;
            if let Some(error) = release_error {
                return Err(error);
            }
        }
        let mut seen = std::collections::HashSet::new();
        let used = usize::try_from(*self.direct_value_next).unwrap_or(0);
        let objects = (0..used)
            .filter_map(|index| self.direct_object(index))
            .filter(|object| {
                !self
                    .destroyed_objects
                    .get(&object.id())
                    .is_some_and(WeakObjectHandle::is_alive)
                    && seen.insert(object.id())
            })
            .map(|object| object.weak_handle())
            .collect();
        self.shutdown_destructor_queue = Some(objects);

        let result = loop {
            let Some(handle) = self.shutdown_destructor_queue.as_mut().and_then(Vec::pop) else {
                break Ok(());
            };
            let Some(object) = handle.upgrade() else {
                continue;
            };
            if self
                .destroyed_objects
                .get(&object.id())
                .is_some_and(WeakObjectHandle::is_alive)
            {
                continue;
            }
            if let Err(error) = self.enter_generated_shutdown_destructor(object) {
                break Err(error);
            }
        };
        self.shutdown_destructor_queue = None;
        result
    }

    pub(in crate::vm) fn handle_uncaught_throwable(
        &mut self,
        encoded: i64,
    ) -> Result<bool, String> {
        let Some(handler) = self.registered_callbacks.exception_handlers.last().copied() else {
            return Ok(false);
        };
        self.retain(handler)?;
        let result = self
            .enter_generated_callback_continuation(handler, &[encoded])
            .map_err(NativeCallControl::into_baseline_error)
            .and_then(|returned| self.release_if_live(returned));
        let release_result = self.release_if_live(handler);
        result?;
        release_result?;
        Ok(true)
    }

    pub(in crate::vm) fn publish_include_globals(&mut self) -> Result<(), String> {
        if self.include_child {
            self.materialize_native_request_globals()?;
            self.materialize_native_dynamic_constants()?;
            let entry_file = self
                .unit
                .functions
                .get(self.unit.entry.index())
                .map(|function| function.span.file);
            BASELINE_INCLUDE_GLOBALS.with(|globals| {
                globals.replace(Some(std::mem::take(
                    &mut self.baseline_values.inherited_globals,
                )));
            });
            BASELINE_INCLUDE_INI.with(|ini| {
                ini.replace(Some(std::mem::take(&mut self.ini_registry)));
            });
            BASELINE_INCLUDE_DEFAULT_TIMEZONE.with(|timezone| {
                timezone.replace(Some(std::mem::take(&mut self.default_timezone)));
            });
            BASELINE_INCLUDE_HTTP_RESPONSE.with(|response| {
                response.replace(Some(std::mem::take(&mut self.http_response)));
            });
            BASELINE_INCLUDE_FILES.with(|files| {
                files.replace(Some(std::mem::take(&mut self.included_files)));
            });
            BASELINE_INCLUDE_MYSQL.with(|mysql| {
                mysql.replace(Some(self.mysql_state.clone()));
            });
            let mut functions = self
                .unit
                .function_table
                .iter()
                .map(|entry| (entry.name.clone(), entry.function))
                .collect::<Vec<_>>();
            functions.extend(
                self.dynamic_functions
                    .iter()
                    .map(|(name, function)| (name.clone(), *function)),
            );
            let classes = self
                .unit
                .classes
                .iter()
                .filter(|class| {
                    (!class.flags.is_conditional
                        || self.class_is_visible(&normalize_class_name(&class.name)))
                        && (class.span.start != 0 || class.span.end != 0)
                        && entry_file.is_none_or(|file| class.span.file == file)
                })
                .map(|class| class.name.clone())
                .collect::<Vec<_>>();
            let mut constants = std::collections::BTreeMap::new();
            for entry in &self.unit.constant_table {
                if entry_file.is_none_or(|file| entry.span.file == file)
                    && let Some(value) = self.unit.constants.get(entry.value.index())
                    && let Ok(value) = ir_constant_value(value)
                {
                    constants.insert(entry.name.clone(), value);
                }
            }
            BASELINE_INCLUDE_CONSTANTS.with(|constants| {
                constants.replace(Some(std::mem::take(
                    &mut self.baseline_values.cold_dynamic_constants,
                )));
            });
            let (autoload_callbacks, shutdown_callbacks) =
                self.take_registered_include_exports()?;
            let native_entry_signature_hashes = self
                .native_entries
                .keys()
                .copied()
                .map(|function| {
                    let signatures =
                        visible_external_function_signatures(self, &self.compiled, function);
                    (
                        function,
                        crate::vm::external_function_signatures_hash(&signatures),
                    )
                })
                .collect();
            self.detach_transient_include_unit()?;
            let mut symbols = self.take_include_symbols()?;
            for class in &classes {
                let class = normalize_class_name(class);
                // Only discard declarations owned by the transient child
                // unit that was just detached. An inherited class with the
                // same normalized name must survive the transfer so the
                // parent publication boundary rejects this unit as a PHP
                // redeclaration instead of silently replacing its owner.
                let transient_owner = symbols
                    .external_class_units
                    .get(&class)
                    .is_some_and(|unit| *unit >= symbols.dynamic_units.len());
                if transient_owner {
                    symbols.dynamic_classes.remove(&class);
                    symbols.external_class_units.remove(&class);
                }
            }
            BASELINE_INCLUDE_SYMBOLS.with(|slot| {
                slot.replace(Some(symbols));
            });
            BASELINE_INCLUDE_EXPORTS.with(|exports| {
                exports.replace(Some(NativeIncludeExports {
                    functions,
                    native_entries: std::mem::take(&mut self.native_entries),
                    native_entry_signature_hashes,
                    classes,
                    constants,
                    autoload_callbacks,
                    shutdown_callbacks,
                }));
            });
        }
        Ok(())
    }
}
