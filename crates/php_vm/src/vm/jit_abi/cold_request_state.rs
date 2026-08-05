//! Cold request coordinator.
//!
//! Generated code receives only [`NativeRequestFastState`]; cold modules own
//! request-wide compatibility state.

use super::*;

/// Performs the single cold identity transition for a generated top-level
/// `$GLOBALS` unset. The caller supplies the already-published authoritative
/// native reference; no operation selector or compatibility value crosses the
/// boundary.
pub(crate) fn unset_native_global_binding(
    context: *mut std::ffi::c_void,
    reference: i64,
) -> Result<bool, String> {
    if context.is_null() {
        return Err("native global-binding context is unavailable".to_owned());
    }
    // SAFETY: the request owner publishes its stable boxed cold-state address
    // for the complete synchronous activation.
    #[allow(unsafe_code)]
    let state = unsafe { &mut *context.cast::<NativeRequestColdState<'static>>() };
    state.unset_native_global_reference(reference)
}

/// Performs the single cold identity transition for generated
/// `$GLOBALS[$name] =& $source`.
pub(crate) fn rebind_native_global_binding(
    context: *mut std::ffi::c_void,
    destination: i64,
    source: i64,
) -> Result<bool, String> {
    if context.is_null() {
        return Err("native global-binding context is unavailable".to_owned());
    }
    // SAFETY: the request owner publishes its stable boxed cold-state address
    // for the complete synchronous activation.
    #[allow(unsafe_code)]
    let state = unsafe { &mut *context.cast::<NativeRequestColdState<'static>>() };
    state.rebind_native_global_reference(destination, source)
}

/// Cold request-lifetime owner. Publication wires the separately boxed native
/// fast state exactly once before any optimizing entry becomes reachable.
pub(in crate::vm) struct NativeRequestOwner<'a> {
    cold: Box<NativeRequestColdState<'a>>,
    _fast: Box<NativeRequestFastState>,
}

/// Executes the one cold transport continuation required by `session_start`.
/// Generated code reaches this only through the fixed session-start handler;
/// ordinary session payload reads and writes remain in the native data plane.
pub(crate) fn prepare_native_session_start_transport(
    context: *mut std::ffi::c_void,
    generate_id: bool,
) -> Result<(), String> {
    if context.is_null() {
        return Err("native session transport context is unavailable".to_owned());
    }
    // SAFETY: the request owner publishes its stable boxed cold-state address
    // for the complete synchronous activation.
    #[allow(unsafe_code)]
    let state = unsafe { &mut *context.cast::<NativeRequestColdState<'static>>() };

    if state.session.native_control().needs_lazy_load() {
        let loader = state
            .options
            .runtime_context
            .session_loader
            .clone()
            .ok_or_else(|| "session loader is unavailable".to_owned())?;
        let id = state.session.id().to_owned();
        let data = loader.load(&id)?;
        let encoded = state.encode_native_array_owner(data)?;
        // SAFETY: activation initializes the separately boxed fast state before
        // any generated session call can reach this continuation.
        #[allow(unsafe_code)]
        let fast = unsafe { state.fast_state.as_mut() }
            .ok_or_else(|| "native session fast state is unavailable".to_owned())?;
        if !fast.replace_native_session_payload_owned(encoded)
            || !fast.commit_native_session_payload()
        {
            return Err("loaded session payload could not be published".to_owned());
        }
        state.session.native_control_mut().mark_payload_loaded();
    }

    if generate_id {
        let generator = state
            .options
            .runtime_context
            .session_id_generator
            .clone()
            .ok_or_else(|| "session id generator is unavailable".to_owned())?;
        state
            .session
            .native_control_mut()
            .set_pending_generated_id(generator.generate()?);
    }
    Ok(())
}

impl<'a> NativeRequestOwner<'a> {
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
        let mut cold = Box::new(NativeRequestColdState::new(
            compiled,
            unit_identity,
            options,
            worker_state,
            output,
            native_entries,
        ));
        cold.promote_cold_dynamic_constants()
            .expect("request constants must fit the authoritative native arena");
        cold.promote_pending_registered_callbacks()
            .expect("registered callbacks must fit the authoritative native arena");
        let mut fast = Box::<NativeRequestFastState>::default();
        fast.prepare_constructorless_stdclass_plan();
        fast.prepare_datetime_class_plan();
        fast.prepare_datetimezone_class_plan();
        fast.prepare_finfo_class_plan();
        fast.prepare_mysqli_class_plan();
        fast.prepare_mysqli_result_class_plan();
        fast.prepare_reflection_class_plan();
        cold.prepared_internal_class_layouts = fast.prepared_internal_class_layouts();
        let fast_ptr = std::ptr::from_mut(fast.as_mut());
        cold.fast_state = fast_ptr;
        fast.output = std::ptr::from_mut(&mut cold.output);
        fast.json_state = std::ptr::from_mut(cold.builtin_request_state.json_mut());
        fast.pcre_state = std::ptr::from_mut(cold.builtin_request_state.pcre_mut());
        fast.gc_state = std::ptr::from_mut(cold.builtin_request_state.gc_mut());
        fast.cwd = std::ptr::from_mut(&mut cold.cwd);
        fast.filesystem_capabilities = std::ptr::from_ref(&cold.options.runtime_context.filesystem);
        fast.filesystem_state = cold.registered_extensions.filesystem_ptr();
        let default_stream_context = cold
            .publish_owned_direct_array_entries(Vec::new())
            .expect("default stream context must fit the native array arena");
        cold.native_stream_context.default_options = default_stream_context;
        fast.stream_context = std::ptr::from_mut(&mut cold.native_stream_context);
        fast.stdin = std::ptr::from_ref(&cold.options.runtime_context.stdin);
        fast.resources = std::ptr::from_mut(&mut cold.resources);
        fast.upload_registry = std::ptr::from_mut(&mut cold.upload_registry);
        fast.last_error = std::ptr::from_mut(&mut cold.last_error);
        fast.direct_resource_handles = std::ptr::from_mut(&mut cold.direct_resource_handles);
        fast.direct_closure_handles = std::ptr::from_mut(&mut cold.direct_closure_handles);
        fast.callback_handlers = std::ptr::from_mut(&mut cold.registered_callbacks);
        fast.callback_transient_export = u8::from(cold.include_child);
        fast.symbol_query = publish_native_symbol_query(cold.as_ref());
        fast.configuration = publish_native_configuration(cold.as_ref());
        fast.http_response = publish_native_http_response(cold.as_ref());
        fast.request_query = publish_native_request_query(cold.as_ref());
        fast.mbstring = NativeMbstringCapability {
            internal_encoding: cold.registered_extensions.mb_internal_encoding_ptr(),
            substitute_character: cold.registered_extensions.mb_substitute_character_ptr(),
        };
        fast.bcmath = NativeBcmathCapability {
            scale: cold.registered_extensions.bcmath_scale_ptr(),
        };
        fast.mysql_state = Rc::as_ptr(&cold.mysql_state);
        fast.random = NativeRandomCapability {
            fill: Some(php_runtime::api::native_random_fill),
        };
        let (filter_roots, filter_present) = cold
            .publish_native_filter_input_roots()
            .expect("request filter inputs must fit the native value arena");
        fast.filter = NativeFilterCapability {
            roots: filter_roots,
            present: filter_present,
        };
        fast.runtime_diagnostic = publish_native_runtime_diagnostic(cold.as_mut());
        fast.frame_arena = publish_native_frame_arena(cold.as_mut());
        cold.trusted_globals_proxy = cold
            .encode_globals_proxy()
            .expect("request globals proxy must fit the native value arena");
        cold.trusted_empty_string_key = cold
            .encode_direct_string_bytes(b"")
            .expect("canonical empty array key must fit the native value arena");
        cold.prepare_trusted_literal_slots();
        cold.prepare_trusted_closure_plans();
        cold.prepare_trusted_exception_plans();
        cold.prepare_trusted_constant_fetches();
        cold.prepare_trusted_request_locals();
        cold.prepare_trusted_global_references()
            .expect("trusted global references must publish before native entry");
        let session_reference = cold
            .native_global_reference_handle("_SESSION")
            .expect("session global must publish in the native plane")
            .expect("session global must have one canonical reference");
        let committed = cold
            .encode_native_array_owner(cold.session.committed_data())
            .expect("committed session payload must fit the native arena");
        fast.session = NativeSessionCapability {
            control: std::ptr::from_mut(cold.session.native_control_mut()),
            transport_context: std::ptr::from_mut(cold.as_mut()).cast(),
            global_reference: session_reference,
            committed,
            has_loader: u8::from(cold.options.runtime_context.session_loader.is_some()),
            has_id_generator: u8::from(cold.options.runtime_context.session_id_generator.is_some()),
        };
        fast.global_binding = NativeGlobalBindingCapability {
            cold_context: std::ptr::from_mut(cold.as_mut()).cast(),
        };
        cold.prepare_trusted_static_locals();
        cold.prepare_trusted_static_properties();
        cold.prepare_trusted_class_plans();
        cold.prepare_trusted_declared_properties();
        cold.prepare_trusted_instanceof_plans();
        cold.prepare_trusted_exception_routes();
        if cold.include_child {
            cold.republish_transferred_dynamic_units()
                .expect("transferred native units must publish before include execution");
        }
        Self { cold, _fast: fast }
    }
}

impl<'a> std::ops::Deref for NativeRequestOwner<'a> {
    type Target = NativeRequestColdState<'a>;

    fn deref(&self) -> &Self::Target {
        self.cold.as_ref()
    }
}

impl<'a> std::ops::DerefMut for NativeRequestOwner<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.cold.as_mut()
    }
}

pub(in crate::vm) struct NativeRequestColdState<'a> {
    pub(super) compiled: crate::compiled_unit::CompiledUnit,
    pub(super) unit: ActiveNativeUnit,
    pub(super) unit_identity: u64,
    pub(super) options: &'a crate::vm::VmOptions,
    pub(super) worker_state: &'a crate::vm::VmWorkerState,
    pub(super) fast_state: *mut NativeRequestFastState,
    pub(super) native_entries:
        Arc<std::collections::BTreeMap<php_ir::FunctionId, php_jit::JitFunctionHandle>>,
    pub(super) native_call_encoded_scratch: Vec<i64>,
    pub(super) native_frame_arena: NativeFrameArena,
    pub(super) baseline_transition_store_owner_pending: bool,
    pub(super) fiber_suspension_states: php_runtime::api::StableNativeArena<php_jit::JitDeoptState>,
    pub(super) fiber_suspension_next: Box<u32>,
    pub(super) native_trace_frames:
        php_runtime::api::StableNativeArena<php_jit::JitNativeTraceFrame>,
    pub(super) native_trace_depth: Box<u32>,
    #[allow(clippy::vec_box)]
    pub(super) native_execution_scopes: Vec<Box<NativeExecutionScope>>,
    pub(super) current_native_execution_scope: u32,
    pub(in crate::vm) output: php_runtime::api::OutputBuffer,
    pub(super) direct_value_slots: php_runtime::api::StableNativeArena<php_jit::JitNativeValueSlot>,
    pub(super) direct_value_next: Box<u32>,
    pub(super) direct_object_owners: php_runtime::api::StableNativeArena<u64>,
    pub(super) direct_array_states:
        php_runtime::api::StableNativeArena<php_jit::JitNativeDirectArrayState>,
    pub(super) direct_array_entries:
        php_runtime::api::StableNativeArena<php_jit::JitNativeDirectArrayEntry>,
    pub(super) direct_array_next: Box<u32>,
    pub(super) direct_value_free_head: Box<u32>,
    pub(super) direct_value_reused_bytes: Box<u64>,
    pub(super) direct_array_free_heads: Box<[u32; php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS]>,
    pub(super) direct_array_reused_bytes: Box<u64>,
    pub(super) direct_string_bytes: php_runtime::api::StableNativeArena<u8>,
    pub(super) direct_string_next: Box<u32>,
    pub(super) direct_string_free_heads: Box<[u32; php_jit::JIT_NATIVE_DIRECT_STRING_FREE_BUCKETS]>,
    pub(super) direct_string_reused_bytes: Box<u64>,
    pub(super) static_property_slots:
        php_runtime::api::StableNativeArena<php_jit::JitNativeStaticPropertySlot>,
    pub(super) static_property_next: Box<u32>,
    pub(super) static_property_indices: std::collections::BTreeMap<(String, String), u32>,
    pub(super) native_global_reference_handles: std::collections::BTreeMap<String, i64>,
    pub(super) direct_resource_handles: std::collections::HashMap<u64, u32>,
    pub(super) direct_closure_handles: std::collections::HashMap<u64, u32>,
    pub(super) direct_string_interned_slots: std::collections::HashMap<u64, Vec<u32>>,
    pub(super) cross_unit_stable_values: std::collections::HashSet<usize>,
    pub(super) native_poll_counter: Box<u32>,
    pub(super) native_root_mutation_pending: Box<u32>,
    pub(super) baseline_values: BaselineValueState,
    pub(super) registered_callbacks: NativeRegisteredCallbackState,
    pub(super) runtime_class_cache: RuntimeClassCache,
    pub(super) runtime_class_layout_cache:
        RefCell<std::collections::HashMap<(Option<usize>, String), u64>>,
    pub(super) runtime_class_ancestry_cache:
        RefCell<std::collections::HashMap<(u64, String), std::collections::BTreeSet<String>>>,
    pub(super) runtime_declared_property_slot_cache:
        RefCell<std::collections::HashMap<(u64, Arc<str>), Option<u32>>>,
    pub(super) trusted_class_plans: Vec<php_jit::JitNativePreparedClassPlan>,
    pub(super) root_index: RequestRootIndex,
    pub(super) resources: php_runtime::api::ResourceTable,
    pub(super) builtin_request_state: php_runtime::api::BuiltinRequestState,
    pub(super) registered_extensions: NativeRegisteredExtensionRequestState,
    pub(super) native_stream_context: NativeStreamContextState,
    pub(in crate::vm) http_response: php_runtime::api::RuntimeHttpResponseState,
    pub(in crate::vm) upload_registry: php_runtime::api::UploadRegistry,
    pub(in crate::vm) session: php_runtime::api::SessionState,
    pub(super) ini_registry: php_runtime::api::IniRegistry,
    pub(super) default_timezone: String,
    pub(super) mysql_state: Rc<RefCell<php_runtime::api::MysqlState>>,
    pub(super) native_dynamic_constants: std::collections::BTreeMap<String, i64>,
    pub(super) trusted_dynamic_constant_sites: std::collections::BTreeMap<String, Vec<usize>>,
    pub(super) visible_function_names: Rc<NativeFunctionNameScope>,
    pub(super) dynamic_functions: std::collections::BTreeMap<String, php_ir::FunctionId>,
    pub(super) deployment_functions: Arc<std::collections::HashMap<Arc<str>, php_ir::FunctionId>>,
    pub(super) deployment_classes: Arc<std::collections::HashSet<Arc<str>>>,
    pub(super) external_functions: std::collections::HashMap<String, NativeDynamicFunction>,
    pub(super) external_class_units: std::collections::HashMap<String, usize>,
    pub(super) external_signature_epoch: u64,
    pub(super) dynamic_units: Vec<NativeDynamicUnit>,
    pub(super) current_dynamic_unit: Option<usize>,
    pub(super) typed_static_reference_constraints:
        std::collections::BTreeMap<u64, Vec<NativeTypedStaticReferenceConstraint>>,
    pub(super) class_constant_cache: NativeClassConstantCache,
    pub(super) active_fiber: Option<u64>,
    pub(super) pending_fiber_suspension_value: Option<i64>,
    pub(super) completed_nested_fiber_call: Option<(u32, u32, php_jit::JitCallStatus, i64)>,
    pub(super) called_classes: Vec<Arc<str>>,
    pub(super) lexical_scope_classes: Vec<String>,
    pub(super) call_frames: Vec<NativeBacktraceFrame>,
    pub(super) dynamic_classes: std::collections::BTreeSet<String>,
    pub(super) class_aliases: std::collections::BTreeMap<String, String>,
    pub(super) shutdown_destructor_queue: Option<Vec<WeakObjectHandle>>,
    pub(super) destroyed_objects: std::collections::BTreeMap<u64, WeakObjectHandle>,
    pub(super) error_reporting: i64,
    pub(super) display_errors: bool,
    pub(super) last_error: Option<NativeLastError>,
    pub(super) explicit_reference_ids: std::collections::BTreeSet<u64>,
    pub(super) environment: Arc<Vec<(String, String)>>,
    pub(super) included_files: std::collections::BTreeSet<std::path::PathBuf>,
    pub(super) include_path: Arc<Vec<std::path::PathBuf>>,
    pub(super) cwd: std::path::PathBuf,
    pub(super) trusted_globals_proxy: i64,
    pub(super) trusted_empty_string_key: i64,
    pub(super) trusted_request_local_function_offsets: Vec<u32>,
    pub(super) trusted_request_local_slots:
        php_runtime::api::StableNativeArena<php_jit::JitNativeRequestLocalSlot>,
    pub(super) trusted_property_function_offsets: Vec<u32>,
    pub(super) trusted_property_slots:
        php_runtime::api::StableNativeArena<php_jit::JitNativeTrustedPropertySlot>,
    pub(super) trusted_closure_plans: php_runtime::api::StableNativeArena<u64>,
    pub(super) trusted_exception_plans: php_runtime::api::StableNativeArena<u64>,
    pub(super) trusted_exception_plan_owners:
        std::collections::BTreeMap<usize, PreparedNativeThrowableOwner>,
    pub(super) trusted_constant_slots:
        php_runtime::api::StableNativeArena<php_jit::JitNativeTrustedConstantSlot>,
    pub(super) trusted_literal_slots:
        std::collections::BTreeMap<u64, Box<[php_jit::JitNativeTrustedLiteralSlot]>>,
    pub(super) trusted_global_reference_slots:
        php_runtime::api::StableNativeArena<php_jit::JitNativeTrustedGlobalReferenceSlot>,
    pub(super) trusted_global_reference_names: std::collections::BTreeMap<usize, Box<str>>,
    pub(super) trusted_static_local_slots:
        php_runtime::api::StableNativeArena<php_jit::JitNativeTrustedStaticLocalSlot>,
    pub(super) trusted_static_property_slots:
        php_runtime::api::StableNativeArena<php_jit::JitNativeTrustedStaticPropertySlot>,
    pub(super) trusted_instanceof_plans:
        php_runtime::api::StableNativeArena<php_jit::JitNativeInstanceOfPlan>,
    pub(super) trusted_instanceof_entries: Vec<php_jit::JitNativeInstanceOfEntry>,
    pub(super) prepared_internal_class_layouts: Vec<(String, u64)>,
    pub(super) trusted_exception_route_plans:
        php_runtime::api::StableNativeArena<php_jit::JitNativeExceptionRoutePlan>,
    pub(super) trusted_exception_route_entries: Vec<php_jit::JitNativeExceptionRouteEntry>,
    pub(super) trusted_exception_route_symbol_epoch: u64,
    pub(super) native_metadata_preparation_scope: Option<Vec<php_ir::FunctionId>>,
    pub(super) prepared_native_metadata_functions: std::collections::BTreeSet<php_ir::FunctionId>,
    pub(super) include_child: bool,
    pub(super) execution_deadline_at: Option<std::time::Instant>,
    pub(super) execution_deadline_mutable: bool,
    pub(super) runtime_telemetry: Rc<RefCell<NativeRuntimeTelemetry>>,
    pub(in crate::vm) diagnostic: Option<php_runtime::api::RuntimeDiagnostic>,
}

impl<'a> NativeRequestColdState<'a> {
    pub(super) fn all_published_native_functions(&self) -> Vec<php_ir::FunctionId> {
        let mut functions = self
            .native_entries
            .keys()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        functions.extend(
            self.compiled
                .prepared_deployment_image()
                .preferred_function_entries
                .iter()
                .enumerate()
                .filter_map(|(function, entry)| {
                    (entry.load(std::sync::atomic::Ordering::Acquire) != 0)
                        .then(|| u32::try_from(function).ok())
                        .flatten()
                        .map(php_ir::FunctionId::new)
                }),
        );
        functions.into_iter().collect()
    }

    pub(super) fn published_native_functions(&self) -> Vec<php_ir::FunctionId> {
        self.native_metadata_preparation_scope
            .clone()
            .unwrap_or_else(|| self.all_published_native_functions())
    }

    pub(super) fn prepared_continuation_instructions(
        &self,
        function: php_ir::FunctionId,
    ) -> Option<std::sync::Arc<[Option<std::sync::Arc<php_ir::Instruction>>]>> {
        self.compiled.prepared_continuation_instructions(function)
    }

    pub(super) fn published_continuation_ranges(&self) -> Vec<std::ops::Range<usize>> {
        self.published_native_functions()
            .into_iter()
            .filter_map(|function| {
                let instructions = self.prepared_continuation_instructions(function)?;
                let base = self
                    .trusted_property_function_offsets
                    .get(function.index())
                    .copied()
                    .and_then(|base| usize::try_from(base).ok())?;
                base.checked_add(instructions.len()).map(|end| base..end)
            })
            .collect()
    }

    pub(super) fn published_request_local_ranges(&self) -> Vec<std::ops::Range<usize>> {
        self.published_native_functions()
            .into_iter()
            .filter_map(|function| {
                let local_count = self.unit.functions.get(function.index())?.locals.len();
                let base = self
                    .trusted_request_local_function_offsets
                    .get(function.index())
                    .copied()
                    .and_then(|base| usize::try_from(base).ok())?;
                base.checked_add(local_count).map(|end| base..end)
            })
            .collect()
    }

    /// Materialize immutable metadata and request-owned plans only for entries
    /// already published in the active unit. Calling this after an on-demand
    /// compilation is the single publication boundary for the newly reached
    /// function; dormant declarations are never traversed.
    pub(super) fn prepare_published_native_metadata(&mut self) -> Result<(), String> {
        let pending = self
            .all_published_native_functions()
            .into_iter()
            .filter(|function| !self.prepared_native_metadata_functions.contains(function))
            .collect::<Vec<_>>();
        if pending.is_empty() {
            if self.trusted_exception_route_symbol_epoch != self.external_signature_epoch {
                self.prepare_trusted_exception_routes();
            }
            return Ok(());
        }
        let rebuild_exception_routes = self.trusted_exception_route_symbol_epoch
            != self.external_signature_epoch
            || pending.iter().any(|function| {
                self.compiled
                    .preferred_function_metadata(*function)
                    .is_some_and(|metadata| {
                        metadata
                            .exception_handlers
                            .iter()
                            .any(|handler| handler.function == *function)
                    })
            });
        let rebuild_instanceof = pending.iter().any(|function| {
            self.prepared_continuation_instructions(*function)
                .is_some_and(|instructions| {
                    instructions.iter().flatten().any(|instruction| {
                        matches!(
                            &instruction.kind,
                            php_ir::InstructionKind::InstanceOf { class_name, .. }
                                if !class_name.eq_ignore_ascii_case("static")
                        )
                    })
                })
        });
        self.native_metadata_preparation_scope = Some(pending.clone());
        self.prepare_trusted_closure_plans();
        self.prepare_trusted_exception_plans();
        self.prepare_trusted_constant_fetches();
        self.prepare_trusted_request_locals();
        if let Err(error) = self.prepare_trusted_global_references() {
            self.native_metadata_preparation_scope = None;
            return Err(error);
        }
        self.prepare_trusted_static_locals();
        self.prepare_trusted_static_properties();
        self.prepare_trusted_class_plans();
        self.prepare_trusted_declared_properties();
        self.native_metadata_preparation_scope = None;
        if rebuild_instanceof {
            // The entry table is shared by every plan in this unit. Rebuild
            // it unit-wide only when a newly published function adds an
            // instanceof site; ordinary function publication remains
            // strictly incremental.
            self.prepare_trusted_instanceof_plans();
        }
        if rebuild_exception_routes {
            self.prepare_trusted_exception_routes();
        }
        self.prepared_native_metadata_functions.extend(pending);
        Ok(())
    }

    pub(in crate::vm) fn native_runtime_ptr(&mut self) -> *mut std::ffi::c_void {
        self.fast_state.cast()
    }

    pub(super) fn publish_active_call_argument_view(&mut self) {
        let (arguments, count, fixed_count) = self.call_frames.last().map_or((0, 0, 0), |frame| {
            (
                frame.arguments.as_ptr() as usize as u64,
                u32::try_from(frame.arguments.len()).unwrap_or(u32::MAX),
                frame.fixed_argument_count,
            )
        });
        // SAFETY: the separately allocated fast state is request-stable. A
        // selected linked runtime view is likewise request-owned and mutable
        // for the duration of this synchronous activation.
        #[allow(unsafe_code)]
        unsafe {
            let fast = &mut *self.fast_state;
            let view = if fast.header.runtime_view_pointer == 0 {
                &mut fast.header.runtime_view
            } else {
                &mut *(fast.header.runtime_view_pointer as usize
                    as *mut php_jit::JitNativeRuntimeView)
            };
            view.active_call_arguments = arguments;
            view.active_call_argument_count = count;
            view.active_call_fixed_argument_count = fixed_count;
            view.active_call_fixed_arguments = 0;
            view.active_call_tail_arguments = 0;
        }
    }

    pub(super) fn discard_native_fiber_suspension_states(&mut self) {
        // Stack entries are snapshots of owners already carried by generated
        // activation state; the arena itself owns no encoded values. Native
        // code updates only the current stack depth, so a fully popped stack
        // does not retain a separate high-water mark. Discarding the reserved
        // range decommits every page touched by this request without moving
        // the worker-stable mapping.
        self.fiber_suspension_states
            .discard_prefix(self.fiber_suspension_states.capacity());
        *self.fiber_suspension_next = 0;
    }

    pub(super) fn release_native_suspension_owners(
        &mut self,
        handle: &php_jit::JitFunctionHandle,
        state: &php_jit::JitDeoptState,
    ) -> Result<(), String> {
        let metadata = handle
            .region_state_metadata()
            .ok_or_else(|| "suspended native activation has no state metadata".to_owned())?;
        let (owned_locals, owned_registers) = metadata
            .suspensions
            .iter()
            .find(|entry| {
                entry.function.raw() == state.function_id
                    && entry.continuation_id == state.continuation_id
            })
            .map(|entry| (&entry.owned_locals, &entry.owned_registers))
            .or_else(|| {
                metadata
                    .native_transitions
                    .iter()
                    .find(|entry| {
                        entry.function.raw() == state.function_id
                            && entry.continuation_id == state.continuation_id
                    })
                    .map(|entry| (&entry.owned_locals, &entry.owned_registers))
            })
            .ok_or_else(|| {
                format!(
                    "suspended native activation state {}:{} has no ownership metadata",
                    state.function_id, state.continuation_id
                )
            })?;

        let mut owners = owned_locals
            .iter()
            .filter(|local| state.local_initialized(**local))
            .map(|local| state.slots[local.index()])
            .collect::<Vec<_>>();
        for snapshot in 0..php_jit::JIT_DEOPT_MAX_REGISTERS {
            let initialized = state.initialized_register_mask
                & 1_u64
                    .checked_shl(u32::try_from(snapshot).unwrap_or(u32::MAX))
                    .unwrap_or(0)
                != 0;
            if initialized
                && owned_registers
                    .iter()
                    .any(|register| register.raw() == state.register_ids[snapshot])
            {
                owners.push(state.registers[snapshot]);
            }
        }
        if self.completed_nested_fiber_call.as_ref().is_some_and(
            |(function, continuation, _, _)| {
                *function == state.function_id && *continuation == state.continuation_id
            },
        ) && let Some((_, _, _, value)) = self.completed_nested_fiber_call.take()
        {
            owners.push(value);
        }
        for owner in owners {
            self.release_if_live(owner)?;
        }
        Ok(())
    }

    pub(in crate::vm) const fn process_exit_terminates_process(&self) -> bool {
        self.registered_extensions.is_fork_child()
    }

    /// Publish every immutable source literal into the request-wide native
    /// value plane once per compiled unit. Generated storage operations borrow
    /// these slots and retain only when the value actually acquires an owner.
    ///
    /// Named and class constants are deliberately excluded: their PHP-visible
    /// resolution remains a cold/exact operation and cannot be frozen as a
    /// source literal.
    pub(super) fn prepare_trusted_literal_slots(&mut self) {
        let identity = self.unit_identity;
        if self.trusted_literal_slots.contains_key(&identity) {
            return;
        }
        let constants = self.unit.constants.clone();
        // Slot zero is an unreadable-state sentinel for branchless generated
        // lookup when a dynamic value is not a unit literal. It must exist
        // even for a constant-free unit.
        let mut slots =
            vec![php_jit::JitNativeTrustedLiteralSlot::default(); constants.len().max(1)];
        for (index, constant) in constants.iter().enumerate() {
            if matches!(
                constant,
                php_ir::IrConstant::NamedConstant(_) | php_ir::IrConstant::ClassConstant { .. }
            ) {
                continue;
            }
            let Ok(value) = self.encode_native_ir_constant_owned(constant) else {
                continue;
            };
            slots[index] = php_jit::JitNativeTrustedLiteralSlot {
                value,
                state: php_jit::JIT_NATIVE_TRUSTED_LITERAL_PUBLISHED,
                reserved: 0,
            };
        }
        self.trusted_literal_slots
            .insert(identity, slots.into_boxed_slice());
    }

    pub(super) fn clear_trusted_literal_slots(&mut self) {
        let values = std::mem::take(&mut self.trusted_literal_slots)
            .into_values()
            .flat_map(|slots| {
                slots
                    .into_vec()
                    .into_iter()
                    .filter_map(|slot| {
                        (slot.state == php_jit::JIT_NATIVE_TRUSTED_LITERAL_PUBLISHED)
                            .then_some(slot.value)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for value in values {
            let _ = self.release_if_live(value);
        }
    }

    /// Cold symbol-mutation hook for one newly visible constant. Resolution
    /// and encoding occur once here; every exact callsite receives an owned
    /// handle and generated code subsequently performs only a numeric load.
    pub(super) fn publish_trusted_constant_encoding(&mut self, name: &str, encoded: i64) {
        for function in self.published_native_functions() {
            let Some(instructions) = self.prepared_continuation_instructions(function) else {
                continue;
            };
            let function = function.raw();
            for (continuation, instruction) in instructions.iter().enumerate() {
                let Some(instruction) = instruction.as_ref() else {
                    continue;
                };
                if !matches!(
                    &instruction.kind,
                    php_ir::InstructionKind::FetchConst { name: candidate, .. }
                        if candidate == name
                ) {
                    continue;
                }
                let Ok(continuation) = u32::try_from(continuation) else {
                    continue;
                };
                let _ = self.publish_trusted_constant_fetch(function, continuation, encoded);
            }
        }
    }

    /// Publish immutable closure allocation descriptors only for functions
    /// whose native entries are callable in this unit. Dormant declarations
    /// retain neither a RegionGraph-derived index nor resident plan pages.
    pub(super) fn prepare_trusted_closure_plans(&mut self) {
        for function in self.published_native_functions() {
            let Some(sites) = self.compiled.prepared_native_closure_sites(function) else {
                continue;
            };
            let Some(base) = self
                .trusted_property_function_offsets
                .get(function.index())
                .copied()
                .and_then(|base| usize::try_from(base).ok())
            else {
                continue;
            };
            for (continuation, site) in sites.iter().enumerate() {
                let Some(site) = site.as_ref() else {
                    continue;
                };
                let Some(plan) = self
                    .trusted_closure_plans
                    .get_mut(base.saturating_add(continuation))
                else {
                    continue;
                };
                *plan = Arc::as_ptr(site) as usize as u64;
            }
        }
    }

    /// Resolves internal throwable class and source metadata once per
    /// published `MakeException` continuation. The optimizing allocator
    /// consumes only these stable opaque plans and the native message value.
    pub(super) fn prepare_trusted_exception_plans(&mut self) {
        let mut sites = Vec::new();
        for function in self.published_native_functions() {
            let Some((function_name, include_function_frame)) = self
                .unit
                .functions
                .get(function.index())
                .map(|function| (function.name.clone(), !function.flags.is_top_level))
            else {
                continue;
            };
            let Some(instructions) = self.prepared_continuation_instructions(function) else {
                continue;
            };
            let Some(base) = self
                .trusted_property_function_offsets
                .get(function.index())
                .copied()
                .and_then(|base| usize::try_from(base).ok())
            else {
                continue;
            };
            for (continuation, instruction) in instructions.iter().enumerate() {
                let Some(instruction) = instruction.as_ref() else {
                    continue;
                };
                let count_family = matches!(
                    &instruction.kind,
                    php_ir::InstructionKind::CallFunction { name, .. }
                        if matches!(
                            name.trim_start_matches('\\').to_ascii_lowercase().as_str(),
                            "count" | "sizeof"
                        )
                );
                let json_decode_family = matches!(
                    &instruction.kind,
                    php_ir::InstructionKind::CallFunction { name, .. }
                        if name
                            .trim_start_matches('\\')
                            .eq_ignore_ascii_case("json_decode")
                );
                let normalizer_family = matches!(
                    &instruction.kind,
                    php_ir::InstructionKind::CallFunction { name, .. }
                        if matches!(
                            name.trim_start_matches('\\').to_ascii_lowercase().as_str(),
                            "normalizer_normalize" | "normalizer_is_normalized"
                        )
                );
                let date_create_family = matches!(
                    &instruction.kind,
                    php_ir::InstructionKind::CallFunction { name, .. }
                        if name
                            .trim_start_matches('\\')
                            .eq_ignore_ascii_case("date_create")
                );
                let unpack_family = match &instruction.kind {
                    php_ir::InstructionKind::CallFunction { args, .. }
                    | php_ir::InstructionKind::CallMethod { args, .. }
                    | php_ir::InstructionKind::CallStaticMethod { args, .. }
                    | php_ir::InstructionKind::CallClosure { args, .. }
                    | php_ir::InstructionKind::CallCallable { args, .. }
                    | php_ir::InstructionKind::NewObject { args, .. }
                    | php_ir::InstructionKind::DynamicNewObject { args, .. }
                    | php_ir::InstructionKind::BindReferenceFromCall { args, .. }
                    | php_ir::InstructionKind::BindReferenceFromMethodCall { args, .. } => {
                        args.iter().any(|argument| argument.unpack)
                    }
                    _ => false,
                };
                let class_name = match &instruction.kind {
                    php_ir::InstructionKind::MakeException { class_name, .. } => class_name.clone(),
                    php_ir::InstructionKind::NewObject { class_name, .. }
                        if matches!(
                            php_ir::module::normalize_class_name(class_name).as_str(),
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
                        ) =>
                    {
                        class_name.clone()
                    }
                    php_ir::InstructionKind::CallFunction { name, .. }
                        if matches!(
                            name.trim_start_matches('\\').to_ascii_lowercase().as_str(),
                            "count"
                                | "sizeof"
                                | "json_decode"
                                | "call_user_func"
                                | "normalizer_normalize"
                                | "normalizer_is_normalized"
                                | "date_create"
                        ) =>
                    {
                        "TypeError".to_owned()
                    }
                    php_ir::InstructionKind::Binary { .. } => "TypeError".to_owned(),
                    _ if unpack_family => "TypeError".to_owned(),
                    _ => continue,
                };
                let binary_family =
                    matches!(instruction.kind, php_ir::InstructionKind::Binary { .. });
                sites.push((
                    base.saturating_add(continuation),
                    class_name,
                    function_name.clone(),
                    include_function_frame,
                    instruction.span,
                    count_family || json_decode_family || normalizer_family || date_create_family,
                    binary_family,
                ));
            }
        }
        for (
            index,
            class_name,
            function_name,
            include_function_frame,
            span,
            count_family,
            binary_family,
        ) in sites
        {
            if self
                .trusted_exception_plans
                .get(index)
                .is_some_and(|plan| *plan != 0)
            {
                continue;
            }
            let owner = if count_family {
                let prepared = Box::new(PreparedNativeCountThrowableSites {
                    type_error: prepare_native_throwable_site(
                        self,
                        &class_name,
                        &function_name,
                        include_function_frame,
                        span,
                    ),
                    value_error: prepare_native_throwable_site(
                        self,
                        "ValueError",
                        &function_name,
                        include_function_frame,
                        span,
                    ),
                    json_exception: prepare_native_throwable_site(
                        self,
                        "JsonException",
                        &function_name,
                        include_function_frame,
                        span,
                    ),
                });
                PreparedNativeThrowableOwner::Count(prepared)
            } else if binary_family {
                let prepared = Box::new(PreparedNativeBinaryThrowableSites {
                    type_error: prepare_native_throwable_site(
                        self,
                        &class_name,
                        &function_name,
                        include_function_frame,
                        span,
                    ),
                    division_by_zero: prepare_native_throwable_site(
                        self,
                        "DivisionByZeroError",
                        &function_name,
                        include_function_frame,
                        span,
                    ),
                    arithmetic_error: prepare_native_throwable_site(
                        self,
                        "ArithmeticError",
                        &function_name,
                        include_function_frame,
                        span,
                    ),
                });
                PreparedNativeThrowableOwner::Binary(prepared)
            } else {
                let prepared = Box::new(prepare_native_throwable_site(
                    self,
                    &class_name,
                    &function_name,
                    include_function_frame,
                    span,
                ));
                PreparedNativeThrowableOwner::Single(prepared)
            };
            let pointer = owner.type_error_pointer();
            self.trusted_exception_plan_owners.insert(index, owner);
            if let Some(plan) = self.trusted_exception_plans.get_mut(index) {
                *plan = pointer;
            }
        }
    }

    /// Publish exact declared-property slots for statically proven object
    /// classes. Visibility, hooks, readonly/type constraints, layout identity,
    /// and numeric storage location are resolved once before native entry.
    pub(super) fn prepare_trusted_declared_properties(&mut self) {
        let owner = self.current_dynamic_unit;
        for function in self.published_native_functions() {
            let Some(instructions) = self.compiled.prepared_native_property_sites(function) else {
                continue;
            };
            let Some(base) = self
                .trusted_property_function_offsets
                .get(function.index())
                .copied()
                .and_then(|base| usize::try_from(base).ok())
            else {
                continue;
            };
            for (continuation, site) in instructions.iter().enumerate() {
                let Some(site) = site.as_ref() else {
                    continue;
                };
                if site.dynamic_stdclass {
                    let Some(plan) = self
                        .trusted_property_slots
                        .get_mut(base.saturating_add(continuation))
                    else {
                        continue;
                    };
                    *plan = php_jit::JitNativeTrustedPropertySlot {
                        state: site.required_state,
                        slot_index: u32::MAX,
                        layout_id: 0,
                        property_name_bytes: site.property.as_ptr() as usize as u64,
                        property_name_length: site.property.len() as u64,
                    };
                    continue;
                }
                let Some(class_index) = site.class_index else {
                    continue;
                };
                let Some(class) = self.unit.classes.get(class_index as usize) else {
                    continue;
                };
                let prepared = {
                    self.runtime_class_cache
                        .borrow()
                        .get(&(owner, class.name.clone()))
                        .cloned()
                };
                let Some(prepared) = prepared else {
                    continue;
                };
                let Some(declaration) = native_instance_property_declaration(
                    self,
                    &class.name,
                    &site.property,
                    function.raw(),
                ) else {
                    continue;
                };
                let property = &declaration.entry;
                // The statically prepared receiver class and the compiling
                // method are resolved to the declaration owner once. This
                // preserves inherited protected access without repeating
                // hierarchy or visibility checks in generated code.
                let readable =
                    native_instance_property_readable(self, &declaration, function.raw())
                        && property.hooks.get.is_none();
                let setter_visible = (!property.flags.set_is_private
                    && !property.flags.set_is_protected)
                    || native_instance_property_writable(self, &declaration, function.raw());
                let writable = readable
                    && !prepared.entry.flags.is_readonly
                    && !property.flags.is_readonly
                    && setter_visible
                    && ((!property.flags.is_typed && property.type_.is_none())
                        || site.direct_typed_assignment)
                    && property.hooks.set.is_none();
                let referenceable = writable && property.hooks.get.is_none();
                let dimension_writable = readable
                    && !prepared.entry.flags.is_readonly
                    && !property.flags.is_readonly
                    && setter_visible
                    && property.hooks.set.is_none();
                let admitted = match site.required_state {
                    php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_PUBLISHED => readable,
                    php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_WRITABLE => writable,
                    php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_REFERENCEABLE => referenceable,
                    php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_DIMENSION_WRITABLE => {
                        dimension_writable
                    }
                    _ => false,
                };
                if !admitted {
                    continue;
                }
                let slot_key = (prepared.layout_id, site.property.clone());
                let cached_slot = self
                    .runtime_declared_property_slot_cache
                    .borrow()
                    .get(&slot_key)
                    .copied();
                let slot_index = cached_slot.unwrap_or_else(|| {
                    let slot = php_runtime::api::ObjectRef::prepared_declared_slot_index(
                        &prepared.entry,
                        &prepared.display_name,
                        &site.property,
                    );
                    self.runtime_declared_property_slot_cache
                        .borrow_mut()
                        .insert(slot_key, slot);
                    slot
                });
                let Some(slot_index) = slot_index else {
                    continue;
                };
                let Some(plan) = self
                    .trusted_property_slots
                    .get_mut(base.saturating_add(continuation))
                else {
                    continue;
                };
                *plan = php_jit::JitNativeTrustedPropertySlot {
                    state: site.required_state,
                    slot_index,
                    // A non-final instance method can receive any subclass.
                    // Runtime class assembly appends inherited backed slots in
                    // lineage order, so the declaring slot is a stable prefix.
                    // Zero publishes that class-family contract; exact/final
                    // layouts retain the ordinary identity guard.
                    layout_id: if prepared.entry.flags.is_final {
                        prepared.layout_id
                    } else {
                        0
                    },
                    property_name_bytes: 0,
                    property_name_length: 0,
                };
            }
        }
    }

    /// Resolve fixed `instanceof C` sites into immutable layout-id hash
    /// tables. Every class whose object layout is currently visible receives
    /// an exact boolean result. A class loaded later has a new unknown layout
    /// and therefore takes the site's single baseline continuation.
    pub(super) fn prepare_trusted_instanceof_plans(&mut self) {
        fn published_target(
            constants: &[php_ir::IrConstant],
            constant_registers: &std::collections::BTreeMap<php_ir::RegId, php_ir::ConstId>,
            instruction: &php_ir::Instruction,
        ) -> Option<String> {
            match &instruction.kind {
                php_ir::InstructionKind::InstanceOf { class_name, .. }
                    if !class_name.eq_ignore_ascii_case("static") =>
                {
                    Some(class_name.clone())
                }
                php_ir::InstructionKind::DynamicInstanceOf { target, .. } => {
                    let target = match target {
                        php_ir::Operand::Constant(target) => *target,
                        php_ir::Operand::Register(target) => *constant_registers.get(target)?,
                        php_ir::Operand::Local(_) => return None,
                    };
                    match constants.get(target.index()) {
                        Some(php_ir::IrConstant::String(target)) if !target.is_empty() => {
                            Some(target.clone())
                        }
                        Some(php_ir::IrConstant::StringBytes(target)) if !target.is_empty() => {
                            std::str::from_utf8(target).ok().map(str::to_owned)
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        let published_functions = self.published_native_functions();
        let has_instanceof_site = published_functions.iter().any(|function| {
            self.prepared_continuation_instructions(*function)
                .is_some_and(|instructions| {
                    instructions.iter().flatten().any(|instruction| {
                        matches!(
                            instruction.kind,
                            php_ir::InstructionKind::InstanceOf { .. }
                                | php_ir::InstructionKind::DynamicInstanceOf { .. }
                        )
                    })
                })
        });
        if !has_instanceof_site {
            return;
        }
        for function in &published_functions {
            let Some(instructions) = self.prepared_continuation_instructions(*function) else {
                continue;
            };
            let Some(base) = self
                .trusted_property_function_offsets
                .get(function.index())
                .copied()
                .and_then(|base| usize::try_from(base).ok())
            else {
                continue;
            };
            let end = base.saturating_add(instructions.len());
            if let Some(plans) = self.trusted_instanceof_plans.get_mut(base..end) {
                plans.fill(Default::default());
            }
        }
        self.trusted_instanceof_entries.clear();

        let (known_names, layouts) = {
            let mut seen = std::collections::BTreeSet::new();
            let mut declarations = Vec::new();
            for class in &self.unit.classes {
                if class.flags.is_conditional && !self.class_is_visible(&class.name) {
                    continue;
                }
                if seen.insert(class.name.clone()) {
                    declarations.push((self.current_dynamic_unit, class));
                }
            }
            for (name, unit) in &self.external_class_units {
                if self.current_dynamic_unit == Some(*unit) || !seen.insert(name.clone()) {
                    continue;
                }
                let Some(class) = self
                    .dynamic_units
                    .get(*unit)
                    .and_then(|package| package.compiled.lookup_unit_class(name))
                else {
                    continue;
                };
                declarations.push((Some(*unit), class));
            }

            let mut known_names = declarations
                .iter()
                .map(|(_, class)| class.name.clone())
                .collect::<std::collections::BTreeSet<_>>();
            let mut layouts = declarations
                .iter()
                .filter(|(_, class)| {
                    !class.flags.is_abstract && !class.flags.is_interface && !class.flags.is_trait
                })
                .filter_map(|(owner, class)| {
                    self.prepared_runtime_class_layout_id(*owner, class)
                        .map(|layout_id| (class.name.clone(), layout_id))
                })
                .collect::<Vec<_>>();
            let mut seen_layouts = layouts
                .iter()
                .map(|(_, layout_id)| *layout_id)
                .collect::<std::collections::BTreeSet<_>>();
            for (name, layout_id) in &self.prepared_internal_class_layouts {
                known_names.insert(name.clone());
                if seen_layouts.insert(*layout_id) {
                    layouts.push((name.clone(), *layout_id));
                }
            }
            (known_names, layouts)
        };

        // The table contents depend only on the resolved target and the
        // request's frozen layout set, not on the instruction that consumes
        // them. WordPress contains many repeated `instanceof` targets; build
        // each immutable open-addressed table once and let every matching
        // site publish the same offset/mask pair.
        let mut published_tables = std::collections::HashMap::<String, (u32, u32)>::new();
        for function in published_functions {
            let Some(instructions) = self.prepared_continuation_instructions(function) else {
                continue;
            };
            let Some(base) = self
                .trusted_property_function_offsets
                .get(function.index())
                .copied()
                .and_then(|base| usize::try_from(base).ok())
            else {
                continue;
            };
            let caller_function = function.raw();
            let constant_registers = instructions
                .iter()
                .flatten()
                .filter_map(|instruction| match instruction.kind {
                    php_ir::InstructionKind::LoadConst { dst, constant } => Some((dst, constant)),
                    _ => None,
                })
                .collect::<std::collections::BTreeMap<_, _>>();
            for (continuation, instruction) in instructions.iter().enumerate() {
                let Some(instruction) = instruction.as_ref() else {
                    continue;
                };
                let Some(class_name) =
                    published_target(&self.unit.constants, &constant_registers, instruction)
                else {
                    continue;
                };
                let Ok(target) =
                    native_resolve_scoped_class_name(self, &class_name, caller_function)
                else {
                    continue;
                };
                let target = normalize_class_name(&target);
                if self.class_aliases.contains_key(&target)
                    || (!known_names.contains(&target)
                        && !native_internal_class_is_available(&target))
                {
                    continue;
                }

                if let Some(&(entry_offset, mask)) = published_tables.get(&target) {
                    let Some(plan) = self
                        .trusted_instanceof_plans
                        .get_mut(base.saturating_add(continuation))
                    else {
                        continue;
                    };
                    *plan = php_jit::JitNativeInstanceOfPlan {
                        entry_offset,
                        mask,
                        state: php_jit::JIT_NATIVE_INSTANCEOF_PLAN_PUBLISHED,
                        reserved: 0,
                    };
                    continue;
                }

                let capacity = layouts.len().saturating_mul(2).max(2).next_power_of_two();
                let Ok(mask) = u32::try_from(capacity - 1) else {
                    continue;
                };
                let Ok(entry_offset) = u32::try_from(self.trusted_instanceof_entries.len()) else {
                    continue;
                };
                self.trusted_instanceof_entries.resize(
                    self.trusted_instanceof_entries
                        .len()
                        .saturating_add(capacity),
                    php_jit::JitNativeInstanceOfEntry::default(),
                );
                for (candidate, layout_id) in &layouts {
                    let result = native_internal_instanceof(candidate, &target)
                        .unwrap_or_else(|| native_class_is_a(self, candidate, &target));
                    let mut bucket = php_jit::jit_native_instanceof_index(*layout_id, mask);
                    loop {
                        let index = entry_offset as usize + bucket as usize;
                        let entry = &mut self.trusted_instanceof_entries[index];
                        if entry.layout_id == 0 || entry.layout_id == *layout_id {
                            *entry = php_jit::JitNativeInstanceOfEntry {
                                layout_id: *layout_id,
                                result: u32::from(result),
                                reserved: 0,
                            };
                            break;
                        }
                        bucket = bucket.wrapping_add(1) & mask;
                    }
                }
                let Some(plan) = self
                    .trusted_instanceof_plans
                    .get_mut(base.saturating_add(continuation))
                else {
                    continue;
                };
                *plan = php_jit::JitNativeInstanceOfPlan {
                    entry_offset,
                    mask,
                    state: php_jit::JIT_NATIVE_INSTANCEOF_PLAN_PUBLISHED,
                    reserved: 0,
                };
                published_tables.insert(target, (entry_offset, mask));
            }
        }
    }

    /// Resolve one immutable class layout while keeping every `RefCell`
    /// borrow confined to its cache probe. Chaining `borrow().get().or_else`
    /// with a later `borrow_mut()` keeps the temporary immutable guard alive
    /// for the complete expression and panics precisely on a cache miss.
    pub(super) fn prepared_runtime_class_layout_id(
        &self,
        owner: Option<usize>,
        class: &php_ir::module::ClassEntry,
    ) -> Option<u64> {
        let key = (owner, class.name.clone());
        if let Some(layout_id) = { self.runtime_class_layout_cache.borrow().get(&key).copied() } {
            return Some(layout_id);
        }
        if let Some(layout_id) = {
            self.runtime_class_cache
                .borrow()
                .get(&key)
                .map(|prepared| prepared.layout_id)
        } {
            return Some(layout_id);
        }
        let runtime = native_runtime_class_with_owner(self, owner, class).ok()?;
        let layout_id =
            php_runtime::api::ObjectRef::prepared_layout_id(&runtime, &class.display_name);
        self.runtime_class_layout_cache
            .borrow_mut()
            .insert(key, layout_id);
        Some(layout_id)
    }

    /// Resolve every currently published compiled catch/finally edge into an
    /// immutable throwable-layout table. A direct compiled caller consumes
    /// this table while its own machine activation is still live, then
    /// re-enters the same fixed callee entry at the selected handler block.
    pub(super) fn prepare_trusted_exception_routes(&mut self) {
        let published_functions = self.all_published_native_functions();
        for function in &published_functions {
            let Some(instructions) = self.prepared_continuation_instructions(*function) else {
                continue;
            };
            let Some(base) = self
                .trusted_property_function_offsets
                .get(function.index())
                .copied()
                .and_then(|base| usize::try_from(base).ok())
            else {
                continue;
            };
            let end = base.saturating_add(instructions.len());
            if let Some(plans) = self.trusted_exception_route_plans.get_mut(base..end) {
                plans.fill(Default::default());
            }
        }
        self.trusted_exception_route_entries.clear();

        let mut layouts = Vec::<(String, u64)>::new();
        let mut seen_layouts = std::collections::BTreeSet::new();
        let mut seen_classes = std::collections::BTreeSet::new();
        for class in &self.unit.classes {
            if class.flags.is_conditional && !self.class_is_visible(&class.name) {
                continue;
            }
            if class.flags.is_abstract || class.flags.is_interface || class.flags.is_trait {
                continue;
            }
            let layout_id = self.prepared_runtime_class_layout_id(self.current_dynamic_unit, class);
            if let Some(layout_id) = layout_id
                && seen_layouts.insert(layout_id)
            {
                seen_classes.insert(normalize_class_name(&class.name));
                layouts.push((class.name.clone(), layout_id));
            }
        }
        for (name, unit) in &self.external_class_units {
            if self.current_dynamic_unit == Some(*unit)
                || seen_classes.contains(&normalize_class_name(name))
            {
                continue;
            }
            let Some(class) = self
                .dynamic_units
                .get(*unit)
                .and_then(|package| package.compiled.lookup_unit_class(name))
            else {
                continue;
            };
            if class.flags.is_abstract || class.flags.is_interface || class.flags.is_trait {
                continue;
            }
            let layout_id = self.prepared_runtime_class_layout_id(Some(*unit), class);
            if let Some(layout_id) = layout_id
                && seen_layouts.insert(layout_id)
            {
                seen_classes.insert(normalize_class_name(&class.name));
                layouts.push((class.name.clone(), layout_id));
            }
        }
        let registry = php_std::ExtensionRegistry::standard_library();
        for class in registry
            .extensions()
            .filter(|extension| registry.is_extension_enabled(extension.name()))
            .flat_map(|extension| extension.classes())
        {
            let candidate = normalize_class_name(class.name());
            if seen_classes.contains(&candidate)
                || !native_class_is_a(self, &candidate, "throwable")
            {
                continue;
            }
            let (runtime, display_name) = cold_diagnostics::native_throwable_class(class.name());
            let layout_id =
                php_runtime::api::ObjectRef::prepared_layout_id(&runtime, &display_name);
            if seen_layouts.insert(layout_id) {
                seen_classes.insert(candidate);
                layouts.push((class.name().to_owned(), layout_id));
            }
        }
        for class in [
            "Exception",
            "ErrorException",
            "Error",
            "TypeError",
            "ValueError",
            "ArgumentCountError",
            "ArithmeticError",
            "DivisionByZeroError",
            "CompileError",
            "ParseError",
            "FiberError",
            "UnhandledMatchError",
            "RuntimeException",
            "LogicException",
        ] {
            let candidate = normalize_class_name(class);
            if seen_classes.contains(&candidate) {
                continue;
            }
            let (runtime, display_name) = cold_diagnostics::native_throwable_class(class);
            let layout_id =
                php_runtime::api::ObjectRef::prepared_layout_id(&runtime, &display_name);
            if seen_layouts.insert(layout_id) {
                seen_classes.insert(candidate);
                layouts.push((class.to_owned(), layout_id));
            }
        }

        for function in published_functions {
            let Some(metadata) = self.compiled.preferred_function_metadata(function) else {
                continue;
            };
            let exception_handlers = metadata
                .exception_handlers
                .iter()
                .filter(|handler| handler.function == function)
                .collect::<Vec<_>>();
            if exception_handlers.is_empty() {
                continue;
            }
            let continuations = metadata
                .continuations
                .iter()
                .filter(|continuation| continuation.function == function);
            let Some(base) = self
                .trusted_property_function_offsets
                .get(function.index())
                .copied()
                .and_then(|base| usize::try_from(base).ok())
            else {
                continue;
            };
            for continuation in continuations {
                let handlers = exception_handlers
                    .iter()
                    .copied()
                    .filter(|handler| handler.protected_blocks.contains(&continuation.block))
                    .collect::<Vec<_>>();
                if handlers.is_empty() {
                    continue;
                }
                let decisions = layouts
                    .iter()
                    .filter_map(|(candidate, layout_id)| {
                        let decision = handlers.iter().rev().find_map(|handler| {
                            if let Some(catch) = handler.catch {
                                let matches = handler.catch_types.iter().any(|target| {
                                    let target = native_resolve_scoped_class_name(
                                        self,
                                        target,
                                        function.raw(),
                                    )
                                    .unwrap_or_else(|_| target.clone());
                                    native_internal_instanceof(candidate, &target).unwrap_or_else(
                                        || native_class_is_a(self, candidate, &target),
                                    )
                                });
                                if matches {
                                    return Some((
                                        php_jit::jit_native_handler_resume_id(catch),
                                        php_jit::JitCallStatus::CONTINUE.0,
                                    ));
                                }
                            }
                            handler.finally.map(|finally| {
                                (
                                    php_jit::jit_native_handler_resume_id(finally),
                                    php_jit::JitCallStatus::THROW.0,
                                )
                            })
                        })?;
                        Some((*layout_id, decision.0, decision.1))
                    })
                    .collect::<Vec<_>>();
                if decisions.is_empty() {
                    continue;
                }
                let capacity = decisions.len().saturating_mul(2).max(2).next_power_of_two();
                let Ok(mask) = u32::try_from(capacity - 1) else {
                    continue;
                };
                let Ok(entry_offset) = u32::try_from(self.trusted_exception_route_entries.len())
                else {
                    continue;
                };
                self.trusted_exception_route_entries.resize(
                    self.trusted_exception_route_entries
                        .len()
                        .saturating_add(capacity),
                    php_jit::JitNativeExceptionRouteEntry::default(),
                );
                for (layout_id, resume_id, pending_status) in decisions {
                    let mut bucket = php_jit::jit_native_instanceof_index(layout_id, mask);
                    loop {
                        let index = entry_offset as usize + bucket as usize;
                        let entry = &mut self.trusted_exception_route_entries[index];
                        if entry.layout_id == 0 || entry.layout_id == layout_id {
                            *entry = php_jit::JitNativeExceptionRouteEntry {
                                layout_id,
                                resume_id,
                                pending_status,
                            };
                            break;
                        }
                        bucket = bucket.wrapping_add(1) & mask;
                    }
                }
                let Some(plan) = self
                    .trusted_exception_route_plans
                    .get_mut(base.saturating_add(continuation.id as usize))
                else {
                    continue;
                };
                *plan = php_jit::JitNativeExceptionRoutePlan {
                    entry_offset,
                    mask,
                    state: php_jit::JIT_NATIVE_EXCEPTION_ROUTE_PUBLISHED,
                    reserved: 0,
                };
            }
        }
        self.trusted_exception_route_symbol_epoch = self.external_signature_epoch;
    }

    pub(super) fn direct_static_property_encoded(&self, key: &(String, String)) -> Option<i64> {
        let index = usize::try_from(*self.static_property_indices.get(key)?).ok()?;
        let slot = self.static_property_slots.get(index)?;
        (slot.initialized != 0).then_some(slot.value)
    }

    /// Publish the immutable result of one exact constant continuation.
    /// The plan retains its own owner; the caller keeps the owner returned by
    /// the baseline operation for the current SSA result.
    pub(super) fn publish_trusted_constant_fetch(
        &mut self,
        function: u32,
        continuation: u32,
        encoded: i64,
    ) -> Result<(), String> {
        let base = self
            .trusted_property_function_offsets
            .get(function as usize)
            .copied()
            .and_then(|base| usize::try_from(base).ok())
            .ok_or_else(|| "trusted constant function index is missing".to_owned())?;
        let index = base
            .checked_add(continuation as usize)
            .ok_or_else(|| "trusted constant continuation index overflow".to_owned())?;
        let plan = self
            .trusted_constant_slots
            .get(index)
            .copied()
            .ok_or_else(|| "trusted constant continuation is missing".to_owned())?;
        if plan.state == php_jit::JIT_NATIVE_TRUSTED_CONSTANT_PUBLISHED {
            return Ok(());
        }
        self.retain(encoded)?;
        self.trusted_constant_slots[index] = php_jit::JitNativeTrustedConstantSlot {
            value: encoded,
            state: php_jit::JIT_NATIVE_TRUSTED_CONSTANT_PUBLISHED,
            reserved: 0,
        };
        Ok(())
    }

    pub(super) fn clear_trusted_constant_fetches(&mut self) {
        let mut values = Vec::new();
        for range in self.published_continuation_ranges() {
            values.extend(
                self.trusted_constant_slots[range]
                    .iter_mut()
                    .filter_map(|slot| {
                        (slot.state == php_jit::JIT_NATIVE_TRUSTED_CONSTANT_PUBLISHED).then(|| {
                            let value = slot.value;
                            *slot = php_jit::JitNativeTrustedConstantSlot::default();
                            value
                        })
                    }),
            );
        }
        for value in values {
            let _ = self.release_if_live(value);
        }
    }

    pub(in crate::vm) fn recycle_native_request_buffers(&mut self) {
        cold_dynamic_units::schedule_hot_native_functions(self);
        self.clear_trusted_constant_fetches();
        self.clear_trusted_literal_slots();
        self.clear_trusted_request_locals();
        self.clear_trusted_global_references();
        self.clear_trusted_static_locals();
        if let Some(value) = self.pending_fiber_suspension_value.take() {
            let _ = self.release_if_live(value);
        }
        if let Some((_, _, _, value)) = self.completed_nested_fiber_call.take() {
            let _ = self.release_if_live(value);
        }
        self.discard_native_fiber_suspension_states();
        self.native_execution_scopes.truncate(1);
        self.current_native_execution_scope = 1;
        self.active_fiber = None;
        let registered_callbacks = std::mem::take(&mut self.registered_callbacks);
        let _ = self.release_registered_callback_state(registered_callbacks);
        // ObjectRef identities may escape an include/nested VM through
        // globals or returned symbols. Their native property cells point into
        // this request arena, so restore every such object before the arena is
        // force-recycled. Doing this after individual slots were reclaimed
        // made graph order observable and could leave an escaped empty shell.
        let _ = self.demote_all_direct_objects();
        let stream_context = std::mem::take(&mut self.native_stream_context);
        for options in stream_context.resource_options.into_values() {
            let _ = self.release_if_live(options);
        }
        let _ = self.release_if_live(stream_context.default_options);
        let direct_value_used = usize::try_from(*self.direct_value_next).unwrap_or(0);
        let direct_array_used = usize::try_from(*self.direct_array_next).unwrap_or(0);
        let direct_string_used = usize::try_from(*self.direct_string_next).unwrap_or(0);
        let static_property_used = usize::try_from(*self.static_property_next).unwrap_or(0);
        let static_values = self
            .static_property_slots
            .get(..static_property_used)
            .unwrap_or_default()
            .iter()
            .filter(|slot| slot.initialized != 0)
            .map(|slot| slot.value)
            .collect::<Vec<_>>();
        self.static_property_slots
            .discard_prefix(static_property_used);
        *self.static_property_next = 0;
        self.static_property_indices.clear();
        for value in static_values {
            let _ = self.release_if_live(value);
        }
        for index in (0..direct_value_used).rev() {
            while self.direct_value_slots[index].refcount != 0 {
                if self.release_direct_value_index(index).is_err() {
                    break;
                }
            }
        }
        self.direct_value_slots.discard_prefix(direct_value_used);
        self.direct_object_owners.discard_prefix(direct_value_used);
        self.direct_array_states.discard_prefix(direct_value_used);
        self.direct_array_entries.discard_prefix(direct_array_used);
        *self.direct_value_next = 0;
        *self.direct_array_next = 0;
        *self.direct_value_free_head = php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
        *self.direct_value_reused_bytes = 0;
        self.direct_array_free_heads
            .fill(php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE);
        *self.direct_array_reused_bytes = 0;
        self.direct_string_free_heads
            .fill(php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE);
        *self.direct_string_reused_bytes = 0;
        self.direct_string_bytes.discard_prefix(direct_string_used);
        *self.direct_string_next = 0;
        self.baseline_values.direct_reference_cells.clear();
        self.baseline_values.materialized_direct_references.clear();
        self.native_global_reference_handles.clear();
        self.baseline_values.direct_object_handles.clear();
        debug_assert!(self.direct_resource_handles.is_empty());
        self.direct_resource_handles.clear();
        debug_assert!(self.direct_closure_handles.is_empty());
        self.direct_closure_handles.clear();
        self.baseline_values.direct_fiber_handles.clear();
        self.baseline_values.direct_fiber_cells.clear();
        self.baseline_values.direct_generator_handles.clear();
        self.baseline_values.direct_generator_cells.clear();
        // This content index owns neither slots nor bytes. Generated native
        // releases can retire a string without crossing the cold HashMap, so
        // dead indices are expected and are filtered by lookup. Request reset
        // discards the complete non-owning index with the recycled arenas.
        self.direct_string_interned_slots.clear();
        self.baseline_values.direct_array_handles.clear();
        self.baseline_values.direct_array_storage_ids.clear();
        self.baseline_values.direct_array_encode_depth = 0;
        self.class_constant_cache.clear();
        let diagnostic_telemetry = std::mem::replace(
            &mut self.runtime_telemetry,
            Rc::new(RefCell::new(NativeRuntimeTelemetry::default())),
        );
        let mut diagnostic_telemetry = Rc::try_unwrap(diagnostic_telemetry)
            .map(RefCell::into_inner)
            .unwrap_or_default();
        diagnostic_telemetry.reset_for_pool();
        self.worker_state
            .recycle_native_request_buffers(NativeRequestBuffers {
                direct_value_slots: std::mem::take(&mut self.direct_value_slots),
                direct_value_next: std::mem::take(&mut self.direct_value_next),
                direct_object_owners: std::mem::take(&mut self.direct_object_owners),
                direct_array_states: std::mem::take(&mut self.direct_array_states),
                direct_array_entries: std::mem::take(&mut self.direct_array_entries),
                direct_array_next: std::mem::take(&mut self.direct_array_next),
                direct_value_free_head: std::mem::take(&mut self.direct_value_free_head),
                direct_value_reused_bytes: std::mem::take(&mut self.direct_value_reused_bytes),
                direct_array_free_heads: std::mem::take(&mut self.direct_array_free_heads),
                direct_array_reused_bytes: std::mem::take(&mut self.direct_array_reused_bytes),
                direct_string_bytes: std::mem::take(&mut self.direct_string_bytes),
                direct_string_next: std::mem::take(&mut self.direct_string_next),
                direct_string_free_heads: std::mem::take(&mut self.direct_string_free_heads),
                direct_string_reused_bytes: std::mem::take(&mut self.direct_string_reused_bytes),
                fiber_suspension_states: std::mem::take(&mut self.fiber_suspension_states),
                fiber_suspension_next: std::mem::take(&mut self.fiber_suspension_next),
                native_trace_frames: std::mem::take(&mut self.native_trace_frames),
                native_trace_depth: std::mem::take(&mut self.native_trace_depth),
                static_property_slots: std::mem::take(&mut self.static_property_slots),
                static_property_next: std::mem::take(&mut self.static_property_next),
                native_call_encoded_scratch: std::mem::take(&mut self.native_call_encoded_scratch),
                native_frame_arena: std::mem::take(&mut self.native_frame_arena),
                direct_resource_handles: std::mem::take(&mut self.direct_resource_handles),
                direct_closure_handles: std::mem::take(&mut self.direct_closure_handles),
                class_constant_cache: std::mem::take(&mut self.class_constant_cache),
                diagnostic_telemetry,
            });
    }

    pub(super) fn reset_execution_deadline_seconds(&mut self, seconds: u64) {
        if !self.execution_deadline_mutable {
            return;
        }
        self.execution_deadline_at = if seconds == 0 {
            None
        } else {
            std::time::Instant::now().checked_add(std::time::Duration::from_secs(seconds))
        };
    }

    pub(super) fn publish_native_entry_address(
        &self,
        function: php_ir::FunctionId,
        address: usize,
    ) {
        let deployment = self.compiled.prepared_deployment_image();
        if let Some(cell) = deployment.generic_function_entries.get(function.index()) {
            cell.store(address, std::sync::atomic::Ordering::Release);
        }
        if let Some(cell) = deployment.preferred_function_entries.get(function.index()) {
            let _ = cell.compare_exchange(
                0,
                address,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            );
        }
    }

    pub(in crate::vm) fn attach_root_deployment_image(
        &mut self,
        compiled: crate::compiled_unit::CompiledUnit,
    ) {
        if self.current_dynamic_unit.is_some() {
            return;
        }
        let unit = self.dynamic_units.len();
        let deployment = compiled.prepared_deployment_image();
        for (function, handle) in self.native_entries.iter() {
            if !handle.region_state_metadata().is_some_and(|metadata| {
                metadata.compiler_tier == php_jit::region_ir::NativeCompilerTier::Generic
            }) {
                continue;
            }
            if let (Some(cell), Some(preferred), Some(address)) = (
                deployment.generic_function_entries.get(function.index()),
                deployment.preferred_function_entries.get(function.index()),
                handle.native_entry_address(),
            ) {
                cell.store(address, std::sync::atomic::Ordering::Release);
                let _ = preferred.compare_exchange(
                    0,
                    address,
                    std::sync::atomic::Ordering::AcqRel,
                    std::sync::atomic::Ordering::Acquire,
                );
            }
        }
        let native_entry_signature_hashes = if self.include_child {
            self.native_entries
                .keys()
                .copied()
                .map(|function| {
                    let signatures = cold_dynamic_units::visible_external_function_signatures(
                        self, &compiled, function,
                    );
                    (
                        function,
                        crate::vm::external_function_signatures_hash(&signatures),
                    )
                })
                .collect()
        } else {
            // Before the root image is attached there are no runtime
            // declaration overlays. Each compiled entry therefore hashes its
            // immutable late-link placeholder set.
            self.native_entries
                .keys()
                .copied()
                .map(|function| {
                    let signatures =
                        crate::vm::linked_external_function_signatures(&compiled, function, &[]);
                    (
                        function,
                        crate::vm::external_function_signatures_hash(&signatures),
                    )
                })
                .collect()
        };
        if !self.include_child && !deployment.function_exports.is_empty() {
            self.external_signature_epoch = self.external_signature_epoch.saturating_add(1);
        }
        let native_entry_signature_epochs = self
            .native_entries
            .keys()
            .copied()
            .map(|function| (function, self.external_signature_epoch))
            .collect();
        let runtime_state = NativeUnitRuntimeState::for_compiled(&compiled);
        let linked_functions = vec![
            php_jit::JitNativeLinkedFunction::default();
            compiled.prepared_linked_function_count()
        ]
        .into_boxed_slice();
        self.dynamic_units.push(NativeDynamicUnit {
            compiled: compiled.clone(),
            cross_unit_global_names: cold_dynamic_units::dynamic_unit_cross_unit_global_names(
                &compiled,
                self.native_entries.keys().copied(),
            ),
            native_entries: self.native_entries.clone(),
            native_entry_signature_hashes,
            native_entry_signature_epochs,
            runtime_state,
            linked_functions,
            published_runtime_view: Box::default(),
        });
        self.prepare_trusted_literal_slots();
        self.current_dynamic_unit = Some(unit);
        debug_assert_eq!(
            self.current_native_execution_scope, 1,
            "root deployment attachment must precede nested native execution"
        );
        self.native_execution_scopes
            .first_mut()
            .expect("every native request publishes its root execution scope")
            .unit = Some(unit);
        if self.include_child {
            cold_dynamic_units::refresh_linked_function_records(self);
            return;
        }
        debug_assert_eq!(unit, 0, "immutable deployment must be the root native unit");
        self.deployment_functions = std::sync::Arc::clone(&deployment.function_exports);
        self.deployment_classes = std::sync::Arc::clone(&deployment.exported_classes);
    }

    pub(super) fn class_is_visible(&self, normalized: &str) -> bool {
        self.deployment_classes.contains(normalized) || self.dynamic_classes.contains(normalized)
    }

    pub(super) fn rebind_native_request_local_reference(
        &mut self,
        name: &str,
        encoded: i64,
    ) -> Result<(), String> {
        if self.native_reference_identity(encoded).is_none() {
            return Err(format!(
                "native request local ${name} was rebound to a non-reference value"
            ));
        }
        let published_functions = self.published_native_functions();
        let slot_indices = published_functions
            .into_iter()
            .filter_map(|function| {
                self.unit
                    .functions
                    .get(function.index())
                    .map(|definition| (function.index(), definition))
            })
            .flat_map(|(function, definition)| {
                definition
                    .locals
                    .iter()
                    .enumerate()
                    .filter_map(move |(local, _)| {
                        (native_request_local_name(definition, local) == Some(name))
                            .then_some((function, local))
                    })
            })
            .filter_map(|(function, local)| {
                self.trusted_request_local_function_offsets
                    .get(function)
                    .copied()
                    .and_then(|base| usize::try_from(base).ok())
                    .and_then(|base| base.checked_add(local))
                    .filter(|index| {
                        self.trusted_request_local_slots
                            .get(*index)
                            .is_some_and(|slot| slot.encoded != encoded)
                    })
            })
            .collect::<Vec<_>>();
        let map_changed = self.native_global_reference_handles.get(name).copied() != Some(encoded);
        let owner_count = slot_indices.len().saturating_add(usize::from(map_changed));
        let mut retained = 0_usize;
        for _ in 0..owner_count {
            if let Err(error) = self.retain(encoded) {
                for _ in 0..retained {
                    let _ = self.release(encoded);
                }
                return Err(error);
            }
            retained = retained.saturating_add(1);
        }

        let mut replaced = Vec::with_capacity(owner_count);
        if map_changed
            && let Some(previous) = self
                .native_global_reference_handles
                .insert(name.to_owned(), encoded)
        {
            replaced.push(previous);
        }
        for index in slot_indices {
            let previous = self.trusted_request_local_slots[index];
            self.trusted_request_local_slots[index] = php_jit::JitNativeRequestLocalSlot {
                encoded,
                state: php_jit::JIT_NATIVE_REQUEST_LOCAL_PUBLISHED,
                reserved: 0,
            };
            if previous.state == php_jit::JIT_NATIVE_REQUEST_LOCAL_PUBLISHED {
                replaced.push(previous.encoded);
            }
        }
        for previous in replaced {
            self.release(previous)?;
        }
        self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
        Ok(())
    }

    pub(super) fn prepare_trusted_request_locals(&mut self) {
        self.ensure_native_global_references();
        let mut sites = Vec::new();
        for function in self.published_native_functions() {
            let Some(definition) = self.unit.functions.get(function.index()) else {
                continue;
            };
            sites.extend(
                definition
                    .locals
                    .iter()
                    .enumerate()
                    .filter_map(|(local, _)| {
                        native_request_local_name(definition, local)
                            .map(|name| (function.index(), local, name.to_owned()))
                    }),
            );
        }
        for (function, local, name) in sites {
            let Ok(encoded) = self.native_request_local_handle(&name) else {
                continue;
            };
            let Some(index) = self
                .trusted_request_local_function_offsets
                .get(function)
                .copied()
                .and_then(|base| usize::try_from(base).ok())
                .and_then(|base| base.checked_add(local))
            else {
                continue;
            };
            let Some(previous) = self.trusted_request_local_slots.get(index).copied() else {
                continue;
            };
            if previous.state == php_jit::JIT_NATIVE_REQUEST_LOCAL_PUBLISHED
                && previous.encoded == encoded
            {
                continue;
            }
            if self.retain(encoded).is_err() {
                continue;
            }
            self.trusted_request_local_slots[index] = php_jit::JitNativeRequestLocalSlot {
                encoded,
                state: php_jit::JIT_NATIVE_REQUEST_LOCAL_PUBLISHED,
                reserved: 0,
            };
            if previous.state == php_jit::JIT_NATIVE_REQUEST_LOCAL_PUBLISHED {
                let _ = self.release(previous.encoded);
            }
        }
    }

    pub(super) fn clear_trusted_request_locals(&mut self) {
        let mut values = Vec::new();
        for range in self.published_request_local_ranges() {
            values.extend(
                self.trusted_request_local_slots[range]
                    .iter_mut()
                    .filter_map(|slot| {
                        (slot.state == php_jit::JIT_NATIVE_REQUEST_LOCAL_PUBLISHED).then(|| {
                            let encoded = slot.encoded;
                            *slot = php_jit::JitNativeRequestLocalSlot::default();
                            encoded
                        })
                    }),
            );
        }
        for encoded in values {
            let _ = self.release_if_live(encoded);
        }
    }

    pub(super) fn publish_trusted_static_local_reference(
        &mut self,
        function: u32,
        local: u32,
        encoded: i64,
    ) -> Result<(), String> {
        if encoded as u64 & php_jit::JIT_VALUE_RUNTIME_KIND_MASK
            != php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG
            || Self::direct_value_index(encoded).is_none()
        {
            return Err("native static local did not produce a direct reference".to_owned());
        }
        let Some(base) = self
            .trusted_property_function_offsets
            .get(function as usize)
            .copied()
            .and_then(|base| usize::try_from(base).ok())
        else {
            return Err("native static-local function index is missing".to_owned());
        };
        let instructions = self
            .prepared_continuation_instructions(php_ir::FunctionId::new(function))
            .ok_or_else(|| "native static-local function metadata is missing".to_owned())?;
        let sites = instructions
            .iter()
            .enumerate()
            .filter_map(|(continuation, instruction)| {
                matches!(
                    instruction.as_ref().map(|instruction| &instruction.kind),
                    Some(php_ir::InstructionKind::InitStaticLocal { local: candidate, .. })
                        if candidate.raw() == local
                )
                .then_some(base.saturating_add(continuation))
            })
            .collect::<Vec<_>>();
        for index in sites {
            let previous = self
                .trusted_static_local_slots
                .get(index)
                .copied()
                .ok_or_else(|| "native static-local continuation is missing".to_owned())?;
            if previous.state == php_jit::JIT_NATIVE_TRUSTED_STATIC_LOCAL_PUBLISHED
                && previous.encoded == encoded
            {
                continue;
            }
            self.retain(encoded)?;
            self.trusted_static_local_slots[index] = php_jit::JitNativeTrustedStaticLocalSlot {
                encoded,
                state: php_jit::JIT_NATIVE_TRUSTED_STATIC_LOCAL_PUBLISHED,
                reserved: 0,
            };
            if previous.state == php_jit::JIT_NATIVE_TRUSTED_STATIC_LOCAL_PUBLISHED {
                self.release(previous.encoded)?;
            }
        }
        Ok(())
    }

    pub(super) fn direct_value_index(encoded: i64) -> Option<usize> {
        let index = php_jit::jit_decode_runtime_value(encoded)?;
        let index = index.checked_sub(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)? as usize;
        (index < php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY).then_some(index)
    }

    /// Clones the stable backing owner of one live direct object. The
    /// slot-parallel pointer arena is authoritative; no object-value HashMap
    /// participates in ordinary lookup.
    #[allow(unsafe_code)]
    pub(super) fn direct_object(&self, index: usize) -> Option<php_runtime::api::ObjectRef> {
        let slot = self.direct_value_slots.get(index)?;
        if slot.refcount == 0 || slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT {
            return None;
        }
        self.direct_object_owner(index)
    }

    /// Clone the backing owner while a direct object is being retired.  At
    /// that point its native refcount is already zero, but the parallel owner
    /// remains valid until the descriptor and owner are reclaimed together.
    #[allow(unsafe_code)]
    pub(super) fn direct_object_owner(&self, index: usize) -> Option<php_runtime::api::ObjectRef> {
        let slot = self.direct_value_slots.get(index)?;
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT {
            return None;
        }
        let owner =
            *self.direct_object_owners.get(index)? as usize as *const php_runtime::api::ObjectRef;
        // SAFETY: encode/publish stores a Box<ObjectRef> before exposing the
        // descriptor, and release clears the pointer only after refcount zero.
        unsafe { owner.as_ref().cloned() }
    }

    /// Reads a direct object's published class without demoting its
    /// authoritative native property plane. Exception routing uses this at a
    /// cold control boundary; decoding the whole object here used to strand
    /// live catch bindings with an empty native descriptor.
    pub(super) fn direct_object_class_name(&self, encoded: i64) -> Option<String> {
        let runtime_index = php_jit::jit_decode_runtime_value(encoded)?;
        let index =
            runtime_index.checked_sub(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)? as usize;
        let slot = self.direct_value_slots.get(index)?;
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT {
            return None;
        }
        self.direct_object_owner(index)
            .map(|object| object.class_name())
    }

    pub(in crate::vm) fn direct_object_is_a(&self, encoded: i64, target: &str) -> bool {
        self.direct_object_class_name(encoded)
            .is_some_and(|class| native_class_is_a(self, &class, target))
    }

    /// Clones the stable resource capability owned directly by one live
    /// native slot. No `Value` or cold handle lookup participates.
    #[allow(unsafe_code)]
    pub(super) fn direct_resource(&self, index: usize) -> Option<php_runtime::api::ResourceRef> {
        let slot = *self.direct_value_slots.get(index)?;
        if slot.refcount == 0
            || slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_RESOURCE
            || slot.flags != php_jit::JIT_NATIVE_DIRECT_RESOURCE_ABI_VERSION
            || slot.aux == 0
        {
            return None;
        }
        let owner = slot.aux as usize as *const php_runtime::api::ResourceRef;
        // SAFETY: publication installs exactly one boxed ResourceRef before
        // exposing the slot, and final release reclaims it exactly once.
        unsafe { owner.as_ref().cloned() }
    }

    /// Resolves a resource operand without crossing the Rust `Value` plane.
    /// Direct references are transparent to by-value builtin parameters.
    pub(super) fn native_resource(&self, encoded: i64) -> Option<php_runtime::api::ResourceRef> {
        let encoded = self.dereference_direct_encoding(encoded);
        if let Some(index) = Self::direct_value_index(encoded) {
            return self.direct_resource(index);
        }
        None
    }

    /// Borrows the authoritative native callable view. The pointer is stable
    /// until the final encoded owner is released; no Rust dispatch enum or
    /// parallel runtime-value mirror participates.
    #[allow(unsafe_code)]
    pub(super) fn direct_prepared_callable_view(
        &self,
        index: usize,
    ) -> Option<&php_jit::JitNativePreparedCallableView> {
        let slot = *self.direct_value_slots.get(index)?;
        if slot.refcount == 0
            || slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE
            || slot.flags != php_jit::JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION
        {
            return None;
        }
        let owner = slot.aux as usize as *const NativePreparedCallableOwner;
        // SAFETY: publication installs exactly one boxed record before the
        // descriptor becomes visible, and final release reclaims both.
        unsafe { owner.as_ref().map(|owner| &owner.native_view) }
    }

    /// Baseline-only Closure metadata. Generated and exact code consumes the
    /// native view and never reaches this compatibility payload.
    #[allow(unsafe_code)]
    pub(super) fn direct_prepared_closure(&self, index: usize) -> Option<&NativePreparedClosure> {
        let slot = *self.direct_value_slots.get(index)?;
        if slot.refcount == 0
            || slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE
            || slot.flags != php_jit::JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION
        {
            return None;
        }
        let owner = slot.aux as usize as *const NativePreparedCallableOwner;
        // SAFETY: the validated owner remains live for this shared request
        // borrow. Only Closure-kind records populate the cold payload.
        unsafe { owner.as_ref()?.cold_closure.as_ref() }
    }

    #[allow(unsafe_code)]
    pub(super) fn direct_prepared_closure_mut(
        &mut self,
        index: usize,
    ) -> Option<&mut NativePreparedClosure> {
        let slot = *self.direct_value_slots.get(index)?;
        if slot.refcount == 0
            || slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE
            || slot.flags != php_jit::JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION
        {
            return None;
        }
        let owner = slot.aux as usize as *mut NativePreparedCallableOwner;
        // SAFETY: mutation requires `&mut self`, so no competing owner borrow
        // can exist on this request thread.
        unsafe { owner.as_mut()?.cold_closure.as_mut() }
    }

    #[allow(unsafe_code)]
    pub(super) fn native_callable_string(&self, bytes: u64, length: u32) -> Option<String> {
        if length == 0 {
            return Some(String::new());
        }
        let bytes = usize::try_from(bytes).ok()? as *const u8;
        // SAFETY: every non-empty range is backed by one immutable boxed byte
        // owner adjacent to the validated native view.
        let bytes = unsafe { std::slice::from_raw_parts(bytes, length as usize) };
        std::str::from_utf8(bytes).ok().map(str::to_owned)
    }

    #[allow(unsafe_code)]
    pub(super) fn fiber_record(&self, index: usize) -> Option<&NativeDirectFiber> {
        let slot = self.direct_value_slots.get(index)?;
        if slot.refcount == 0
            || !matches!(
                slot.kind,
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER
                    | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_FIBER
            )
            || slot.flags != php_jit::JIT_NATIVE_DIRECT_FIBER_ABI_VERSION
        {
            return None;
        }
        let owner = slot.aux as usize as *const NativeDirectFiber;
        // SAFETY: direct Fiber publication owns one boxed record until the
        // slot's final encoded owner is released.
        unsafe { owner.as_ref() }
    }

    #[allow(unsafe_code)]
    pub(super) fn direct_generator(&self, index: usize) -> Option<&NativeDirectGenerator> {
        let slot = self.direct_value_slots.get(index)?;
        if slot.refcount == 0
            || !matches!(
                slot.kind,
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_GENERATOR
                    | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_GENERATOR
            )
            || slot.flags != php_jit::JIT_NATIVE_DIRECT_GENERATOR_ABI_VERSION
        {
            return None;
        }
        let owner = slot.aux as usize as *const NativeDirectGenerator;
        // SAFETY: publication installs one boxed activation before exposing
        // the slot, and final release reclaims both on the request thread.
        unsafe { owner.as_ref() }
    }

    #[allow(unsafe_code)]
    pub(super) fn direct_generator_mut(
        &mut self,
        index: usize,
    ) -> Option<&mut NativeDirectGenerator> {
        let slot = self.direct_value_slots.get(index)?;
        if slot.refcount == 0
            || !matches!(
                slot.kind,
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_GENERATOR
                    | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_GENERATOR
            )
            || slot.flags != php_jit::JIT_NATIVE_DIRECT_GENERATOR_ABI_VERSION
        {
            return None;
        }
        let owner = slot.aux as usize as *mut NativeDirectGenerator;
        // SAFETY: `&mut self` excludes a competing activation borrow.
        unsafe { owner.as_mut() }
    }

    pub(super) fn direct_generator_index(&self, encoded: i64) -> Option<usize> {
        let index = Self::direct_value_index(encoded)?;
        self.direct_generator(index).map(|_| index)
    }

    pub(super) fn reserve_direct_value_slot(&mut self) -> Result<usize, String> {
        if *self.direct_value_free_head != php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE {
            let index = usize::try_from(*self.direct_value_free_head)
                .map_err(|_| "direct native free-list index overflow".to_owned())?;
            let slot = self
                .direct_value_slots
                .get(index)
                .ok_or_else(|| format!("direct native free-list slot {index} is missing"))?;
            *self.direct_value_free_head = u32::try_from(slot.payload)
                .map_err(|_| format!("direct native free-list link {} is invalid", slot.payload))?;
            *self.direct_value_reused_bytes = self
                .direct_value_reused_bytes
                .saturating_add(std::mem::size_of::<php_jit::JitNativeValueSlot>() as u64);
            self.cross_unit_stable_values.remove(&index);
            return Ok(index);
        }
        let index = usize::try_from(*self.direct_value_next)
            .map_err(|_| "direct native value index overflow".to_owned())?;
        if index >= self.direct_value_slots.len() {
            let mut live_by_kind = std::collections::BTreeMap::<u32, (usize, u64, u32)>::new();
            let mut dead = 0usize;
            for slot in self.direct_value_slots.get(..index).unwrap_or_default() {
                if slot.refcount == 0 {
                    dead = dead.saturating_add(1);
                    continue;
                }
                let entry = live_by_kind.entry(slot.kind).or_default();
                entry.0 = entry.0.saturating_add(1);
                entry.1 = entry.1.saturating_add(u64::from(slot.refcount));
                entry.2 = entry.2.max(slot.refcount);
            }
            return Err(format!(
                "direct native value arena exhausted at {} slots (dead={dead}, live_by_kind={live_by_kind:?})",
                index.saturating_add(1),
            ));
        }
        *self.direct_value_next = u32::try_from(index + 1)
            .map_err(|_| "direct native value index overflow".to_owned())?;
        self.cross_unit_stable_values.remove(&index);
        Ok(index)
    }

    pub(super) fn encode_direct_slot_index(index: usize, tag: u64) -> Result<i64, String> {
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .ok_or_else(|| "direct native value handle overflow".to_owned())?;
        Ok(php_jit::jit_encode_typed_runtime_value(runtime_index, tag))
    }

    pub(super) fn publish_cold_generator(
        &mut self,
        generator: php_runtime::api::GeneratorRef,
    ) -> Result<i64, String> {
        let index = self.reserve_direct_value_slot()?;
        let id = generator.id();
        let owner = Box::into_raw(Box::new(generator));
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_COLD_GENERATOR,
            flags: php_jit::JIT_NATIVE_COLD_GENERATOR_ABI_VERSION,
            payload: id,
            aux: owner as usize as u64,
            ..php_jit::JitNativeValueSlot::default()
        };
        self.baseline_values
            .direct_generator_handles
            .insert(id, index as u32);
        Self::encode_direct_slot_index(index, php_jit::JIT_VALUE_RUNTIME_GENERATOR_TAG)
    }

    pub(super) fn encode_native_generator_owner(
        &mut self,
        generator: php_runtime::api::GeneratorRef,
    ) -> Result<i64, String> {
        if let Some(index) = self
            .baseline_values
            .direct_generator_handles
            .get(&generator.id())
            .copied()
        {
            let slot = self
                .direct_value_slots
                .get_mut(index as usize)
                .filter(|slot| {
                    slot.refcount != 0
                        && matches!(
                            slot.kind,
                            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_GENERATOR
                                | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_GENERATOR
                                | php_jit::JIT_NATIVE_VALUE_VIEW_COLD_GENERATOR
                        )
                })
                .ok_or_else(|| {
                    "native Generator identity points at a dead activation".to_owned()
                })?;
            slot.refcount = slot
                .refcount
                .checked_add(1)
                .ok_or_else(|| "native Generator refcount overflow".to_owned())?;
            let runtime_index = index
                .checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
                .ok_or_else(|| "native Generator handle overflow".to_owned())?;
            return Ok(php_jit::jit_encode_typed_runtime_value(
                runtime_index,
                php_jit::JIT_VALUE_RUNTIME_GENERATOR_TAG,
            ));
        }
        self.publish_cold_generator(generator)
    }

    pub(super) fn cold_generator(&self, index: usize) -> Option<&php_runtime::api::GeneratorRef> {
        let slot = self.direct_value_slots.get(index)?;
        if slot.refcount == 0
            || slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_COLD_GENERATOR
            || slot.flags != php_jit::JIT_NATIVE_COLD_GENERATOR_ABI_VERSION
            || slot.aux == 0
        {
            return None;
        }
        // SAFETY: the direct slot owns this boxed GeneratorRef until final
        // release and request execution is synchronous.
        #[allow(unsafe_code)]
        unsafe {
            (slot.aux as usize as *const php_runtime::api::GeneratorRef).as_ref()
        }
    }

    /// Publishes a PHP string directly into the authoritative request-owned
    /// native byte/value plane. The Rust `PhpString` is consumed at this
    /// boundary and is not mirrored or retained in a second identity table.
    ///
    /// PHP strings have value semantics, so a cold `PhpString` owner does not
    /// need request-wide identity preservation. Equal immutable bytes may
    /// Publishes borrowed bytes directly as one native PHP string owner.
    /// Metadata/introspection producers already own stable byte slices and
    /// must not construct an intermediate `PhpString` merely to enter the
    /// authoritative string arena.
    #[track_caller]
    pub(super) fn encode_native_string_bytes_owner(&mut self, bytes: &[u8]) -> Result<i64, String> {
        let hash = native_direct_string_hash(bytes);
        let existing = self
            .direct_string_interned_slots
            .get(&hash)
            .and_then(|indices| {
                indices.iter().copied().find(|index| {
                    let index = *index as usize;
                    self.direct_value_slots.get(index).is_some_and(|slot| {
                        slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_STRING
                    }) && self
                        .native_string_bytes(
                            (php_jit::JIT_VALUE_RUNTIME_STRING_TAG
                                | u64::from(
                                    index as u32 + php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
                                )) as i64,
                        )
                        .is_some_and(|candidate| candidate == bytes)
                })
            });
        if let Some(index) = existing {
            let runtime_index = index
                .checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
                .ok_or_else(|| "direct native string handle overflow".to_owned())?;
            let encoded = (php_jit::JIT_VALUE_RUNTIME_STRING_TAG | u64::from(runtime_index)) as i64;
            self.retain(encoded)?;
            return Ok(encoded);
        }
        let encoded = self.encode_direct_string_bytes(bytes)?;
        let index = Self::direct_value_index(encoded)
            .and_then(|index| u32::try_from(index).ok())
            .ok_or_else(|| "direct native string index is invalid".to_owned())?;
        self.direct_string_interned_slots
            .entry(hash)
            .or_default()
            .push(index);
        Ok(encoded)
    }

    pub(super) fn direct_string_capacity(length: usize) -> Result<usize, String> {
        length
            .max(php_jit::JIT_NATIVE_DIRECT_STRING_MIN_CAPACITY as usize)
            .checked_next_power_of_two()
            .ok_or_else(|| "direct native string capacity overflow".to_owned())
    }

    pub(super) fn reserve_direct_string_bytes(
        &mut self,
        length: usize,
    ) -> Result<(usize, usize), String> {
        let capacity = Self::direct_string_capacity(length)?;
        let bucket = capacity.trailing_zeros() as usize;
        let head = self.direct_string_free_heads[bucket];
        if head != php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE {
            let start = head as usize;
            let next_bytes: [u8; 4] = self
                .direct_string_bytes
                .get(start..start + 4)
                .ok_or_else(|| "direct native string free-list entry is missing".to_owned())?
                .try_into()
                .expect("four-byte string free-list header");
            self.direct_string_free_heads[bucket] = u32::from_ne_bytes(next_bytes);
            *self.direct_string_reused_bytes = self
                .direct_string_reused_bytes
                .saturating_add(capacity as u64);
            return Ok((start, capacity));
        }
        let start = usize::try_from(*self.direct_string_next)
            .map_err(|_| "direct native string offset overflow".to_owned())?;
        let end = start
            .checked_add(capacity)
            .ok_or_else(|| "direct native string range overflow".to_owned())?;
        if end > self.direct_string_bytes.len() {
            return Err(format!(
                "direct native string arena exhausted at {end} bytes (next={start}, requested={capacity})"
            ));
        }
        *self.direct_string_next =
            u32::try_from(end).map_err(|_| "direct native string offset overflow".to_owned())?;
        Ok((start, capacity))
    }

    pub(super) fn free_direct_string_bytes(&mut self, start: usize, capacity: usize) {
        if capacity < php_jit::JIT_NATIVE_DIRECT_STRING_MIN_CAPACITY as usize
            || !capacity.is_power_of_two()
        {
            return;
        }
        let bucket = capacity.trailing_zeros() as usize;
        let Some(head) = self.direct_string_free_heads.get_mut(bucket) else {
            return;
        };
        let Some(bytes) = self.direct_string_bytes.get_mut(start..start + 4) else {
            return;
        };
        bytes.copy_from_slice(&head.to_ne_bytes());
        *head = u32::try_from(start).unwrap_or(php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE);
    }

    pub(super) fn encode_direct_string_bytes(&mut self, bytes: &[u8]) -> Result<i64, String> {
        let (start, capacity) = self.reserve_direct_string_bytes(bytes.len())?;
        let end = start + bytes.len();
        let index = match self.reserve_direct_value_slot() {
            Ok(index) => index,
            Err(error) => {
                self.free_direct_string_bytes(start, capacity);
                return Err(error);
            }
        };
        let runtime_index = match u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
        {
            Some(runtime_index) => runtime_index,
            None => {
                self.direct_value_slots[index] = php_jit::JitNativeValueSlot::default();
                self.free_direct_string_bytes(start, capacity);
                return Err("direct native value handle overflow".to_owned());
            }
        };
        self.direct_string_bytes[start..end].copy_from_slice(bytes);
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
            flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
            reserved: php_jit::jit_native_direct_string_reserved(
                u32::try_from(capacity).unwrap_or(u32::MAX),
                bytes == b"0",
            ),
            payload: bytes.len() as u64,
            aux: self.direct_string_bytes[start..].as_ptr() as usize as u64,
        };
        Ok((php_jit::JIT_VALUE_RUNTIME_STRING_TAG | u64::from(runtime_index)) as i64)
    }

    /// Convert a unit-scoped literal to its request-wide native encoding at a
    /// cross-unit call boundary without reconstructing a Rust `Value`.
    pub(super) fn stabilize_active_unit_constant(&mut self, index: u32) -> Result<i64, String> {
        let constant = self
            .unit
            .constants
            .get(index as usize)
            .cloned()
            .ok_or_else(|| format!("native constant {index} is missing from the active unit"))?;
        match constant {
            php_ir::IrConstant::Null => Ok(php_jit::jit_encode_constant(u32::MAX)),
            php_ir::IrConstant::Bool(false) => {
                Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE))
            }
            php_ir::IrConstant::Bool(true) => {
                Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE))
            }
            php_ir::IrConstant::Int(value) => self.encode_native_int(value),
            php_ir::IrConstant::Float(value) => {
                self.encode_native_float_owner(php_runtime::api::FloatValue::from_f64(value))
            }
            php_ir::IrConstant::String(value) => self.encode_direct_string_bytes(value.as_bytes()),
            php_ir::IrConstant::StringBytes(value) => self.encode_direct_string_bytes(&value),
            constant @ php_ir::IrConstant::Array(_) => {
                self.encode_native_ir_constant_owned(&constant)
            }
            constant @ (php_ir::IrConstant::NamedConstant(_)
            | php_ir::IrConstant::ClassConstant { .. }) => {
                self.encode_native_ir_constant_owned(&constant)
            }
        }
    }

    /// Publishes a parameter/default constant directly into the native value
    /// plane.  Scalar and array defaults are common call-frame data and must
    /// not be constructed as a temporary Rust `Value` merely because the
    /// caller omitted an argument.
    pub(super) fn encode_native_ir_constant_owned(
        &mut self,
        constant: &php_ir::IrConstant,
    ) -> Result<i64, String> {
        self.encode_native_ir_constant_owned_at_depth(constant, 0)
    }

    pub(super) fn encode_native_ir_constant_owned_at_depth(
        &mut self,
        constant: &php_ir::IrConstant,
        depth: usize,
    ) -> Result<i64, String> {
        if depth > 32 {
            return Err("native constant resolution exceeded its recursion limit".to_owned());
        }
        match constant {
            php_ir::IrConstant::Null => Ok(php_jit::jit_encode_constant(u32::MAX)),
            php_ir::IrConstant::Bool(false) => {
                Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE))
            }
            php_ir::IrConstant::Bool(true) => {
                Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE))
            }
            php_ir::IrConstant::Int(value) => self.encode_native_int(*value),
            php_ir::IrConstant::Float(value) => {
                self.encode_native_float_owner(php_runtime::api::FloatValue::from_f64(*value))
            }
            php_ir::IrConstant::String(value) => self.encode_direct_string_bytes(value.as_bytes()),
            php_ir::IrConstant::StringBytes(value) => self.encode_direct_string_bytes(value),
            php_ir::IrConstant::Array(source) => {
                let mut entries =
                    Vec::<php_jit::JitNativeDirectArrayEntry>::with_capacity(source.len());
                let mut next_index = None;
                for source_entry in source {
                    let value = match self
                        .encode_native_ir_constant_owned_at_depth(&source_entry.value, depth + 1)
                    {
                        Ok(value) => value,
                        Err(error) => {
                            for entry in entries {
                                let _ = self.release(entry.key);
                                let _ = self.release(entry.value);
                            }
                            return Err(error);
                        }
                    };
                    let key = match source_entry.key.as_ref() {
                        Some(key) => {
                            self.encode_native_constant_array_key_owned_at_depth(key, depth + 1)
                        }
                        None => {
                            let next = next_index.unwrap_or(0);
                            if next == i64::MAX
                                && entries.iter().any(|entry| {
                                    self.native_encoded_int(entry.key) == Some(i64::MAX)
                                })
                            {
                                Err(php_runtime::api::PHP_ARRAY_APPEND_OVERFLOW_MESSAGE.to_owned())
                            } else {
                                self.encode_native_int(next)
                            }
                        }
                    };
                    let key = match key {
                        Ok(key) => key,
                        Err(error) => {
                            let _ = self.release(value);
                            for entry in entries {
                                let _ = self.release(entry.key);
                                let _ = self.release(entry.value);
                            }
                            return Err(error);
                        }
                    };
                    if let Some(key_value) = self.native_encoded_int(key) {
                        let next = key_value.saturating_add(1);
                        if next_index.is_none_or(|current| next > current) {
                            next_index = Some(next);
                        }
                    }
                    if let Some(existing) = entries
                        .iter_mut()
                        .find(|entry| self.native_encoded_array_keys_equal(entry.key, key))
                    {
                        let _ = self.release(key);
                        let previous = std::mem::replace(&mut existing.value, value);
                        self.release(previous)?;
                    } else {
                        entries.push(php_jit::JitNativeDirectArrayEntry { key, value });
                    }
                }
                self.publish_owned_direct_array_entries(entries)
            }
            php_ir::IrConstant::NamedConstant(name) => {
                self.encode_named_runtime_constant_owned(name, depth + 1)
            }
            php_ir::IrConstant::ClassConstant {
                class_name,
                constant_name,
                ..
            } => self.encode_class_runtime_constant_owned(class_name, constant_name, depth + 1),
        }
    }

    /// Follows local and linked class declarations while retaining their
    /// encoded native representation. Visibility and autoload diagnostics
    /// remain on the explicit `FetchClassConstant` continuation.
    pub(super) fn encode_class_runtime_constant_owned(
        &mut self,
        class_name: &str,
        constant_name: &str,
        depth: usize,
    ) -> Result<i64, String> {
        if depth > 32 {
            return Err("native constant resolution exceeded its recursion limit".to_owned());
        }
        let normalized = normalize_class_name(class_name);
        let local = self
            .unit
            .classes
            .iter()
            .find(|class| class.name == normalized)
            .and_then(|class| {
                class
                    .constants
                    .iter()
                    .find(|entry| entry.name.eq_ignore_ascii_case(constant_name))
            })
            .cloned();
        if let Some(entry) = local {
            if let Some(constant) = entry
                .value
                .and_then(|id| self.unit.constants.get(id.index()))
                .cloned()
            {
                return self.encode_native_ir_constant_owned_at_depth(&constant, depth + 1);
            }
            if let Some(reference) = entry.value_named_constant {
                for name in reference.names {
                    if let Ok(value) = self.encode_named_runtime_constant_owned(&name, depth + 1) {
                        return Ok(value);
                    }
                }
            }
            if let Some(reference) = entry.value_class_constant {
                return self.encode_class_runtime_constant_owned(
                    &reference.class_name,
                    &reference.constant_name,
                    depth + 1,
                );
            }
        }

        if let Some((unit, class)) = native_external_class_handle(self, &normalized) {
            let entry = class
                .constants
                .iter()
                .find(|entry| entry.name.eq_ignore_ascii_case(constant_name))
                .cloned();
            if let Some(entry) = entry {
                if let Some(constant) = entry
                    .value
                    .and_then(|id| {
                        self.dynamic_units
                            .get(unit)
                            .and_then(|package| package.compiled.unit().constants.get(id.index()))
                    })
                    .cloned()
                {
                    return self.encode_native_ir_constant_owned_at_depth(&constant, depth + 1);
                }
                if let Some(reference) = entry.value_named_constant {
                    for name in reference.names {
                        if let Ok(value) =
                            self.encode_named_runtime_constant_owned(&name, depth + 1)
                        {
                            return Ok(value);
                        }
                    }
                }
                if let Some(reference) = entry.value_class_constant {
                    return self.encode_class_runtime_constant_owned(
                        &reference.class_name,
                        &reference.constant_name,
                        depth + 1,
                    );
                }
            }
        }
        Err(format!("Undefined constant {class_name}::{constant_name}"))
    }

    pub(super) fn encode_native_constant_array_key_owned_at_depth(
        &mut self,
        constant: &php_ir::IrConstant,
        depth: usize,
    ) -> Result<i64, String> {
        match constant {
            php_ir::IrConstant::Null => self.encode_direct_string_bytes(&[]),
            php_ir::IrConstant::Bool(value) => Ok(i64::from(*value)),
            php_ir::IrConstant::Int(value) => self.encode_native_int(*value),
            php_ir::IrConstant::Float(value) => Ok(*value as i64),
            php_ir::IrConstant::String(value) => {
                if let Some(key) = php_runtime::api::array_key_integer_bytes(value.as_bytes()) {
                    self.encode_native_int(key)
                } else {
                    self.encode_direct_string_bytes(value.as_bytes())
                }
            }
            php_ir::IrConstant::StringBytes(value) => {
                if let Some(key) = php_runtime::api::array_key_integer_bytes(value) {
                    self.encode_native_int(key)
                } else {
                    self.encode_direct_string_bytes(value)
                }
            }
            php_ir::IrConstant::Array(_) => Err("native constant array key is invalid".to_owned()),
            php_ir::IrConstant::NamedConstant(_) | php_ir::IrConstant::ClassConstant { .. } => {
                let encoded = self.encode_native_ir_constant_owned_at_depth(constant, depth + 1)?;
                match self.native_encoded_value_kind(encoded) {
                    Some(NativeEncodedValueKind::Null) => {
                        self.release(encoded)?;
                        self.encode_direct_string_bytes(&[])
                    }
                    Some(NativeEncodedValueKind::Bool(value)) => {
                        self.release(encoded)?;
                        self.encode_native_int(i64::from(value))
                    }
                    Some(NativeEncodedValueKind::Int) => Ok(encoded),
                    Some(NativeEncodedValueKind::Float) => {
                        let value = self.native_encoded_float(encoded).ok_or_else(|| {
                            "native constant float key lost its payload".to_owned()
                        })?;
                        self.release(encoded)?;
                        self.encode_native_int(php_runtime::api::php_float_to_int(value))
                    }
                    Some(NativeEncodedValueKind::String) => {
                        let bytes = self.native_string_name_bytes(encoded).ok_or_else(|| {
                            "native constant string key lost its bytes".to_owned()
                        })?;
                        let integer_key = php_runtime::api::array_key_integer_bytes(&bytes);
                        self.release(encoded)?;
                        if let Some(key) = integer_key {
                            self.encode_native_int(key)
                        } else {
                            self.encode_direct_string_bytes(&bytes)
                        }
                    }
                    _ => {
                        self.release(encoded)?;
                        Err("native constant array key is invalid".to_owned())
                    }
                }
            }
        }
    }

    pub(super) fn native_encoded_array_keys_equal(&self, left: i64, right: i64) -> bool {
        let left_int = self.native_encoded_int(left).or_else(|| {
            self.native_string_bytes(left)
                .and_then(php_runtime::api::array_key_integer_bytes)
        });
        let right_int = self.native_encoded_int(right).or_else(|| {
            self.native_string_bytes(right)
                .and_then(php_runtime::api::array_key_integer_bytes)
        });
        match (left_int, right_int) {
            (Some(left), Some(right)) => left == right,
            (None, None) => self.native_string_bytes(left) == self.native_string_bytes(right),
            _ => false,
        }
    }

    pub(super) fn native_encoded_matches_array_key(
        &self,
        encoded: i64,
        key: &php_runtime::api::ArrayKey,
    ) -> bool {
        match key {
            php_runtime::api::ArrayKey::Int(key) => {
                self.native_encoded_int(encoded).or_else(|| {
                    self.native_string_bytes(encoded)
                        .and_then(php_runtime::api::array_key_integer_bytes)
                }) == Some(*key)
            }
            php_runtime::api::ArrayKey::String(key) => {
                if let Some(key) = php_runtime::api::array_key_integer_bytes(key.as_bytes()) {
                    self.native_encoded_int(encoded) == Some(key)
                } else {
                    self.native_string_bytes(encoded)
                        .is_some_and(|bytes| bytes == key.as_bytes())
                }
            }
        }
    }

    pub(super) fn encode_native_array_key_owned(
        &mut self,
        key: &php_runtime::api::ArrayKey,
    ) -> Result<i64, String> {
        match key {
            php_runtime::api::ArrayKey::Int(key) => self.encode_native_int(*key),
            php_runtime::api::ArrayKey::String(key) => {
                if let Some(key) = php_runtime::api::array_key_integer_bytes(key.as_bytes()) {
                    self.encode_native_int(key)
                } else {
                    self.encode_native_string_bytes_owner(key.as_bytes())
                }
            }
        }
    }

    /// Converts the two diagnostic-free PHP array-key families directly.
    /// Float/bool/null/object conversions remain at the semantic boundary
    /// because they may emit PHP-visible diagnostics.
    pub(super) fn native_encoded_plain_array_key(
        &self,
        encoded: i64,
    ) -> Option<php_runtime::api::ArrayKey> {
        let encoded = self.dereference_direct_encoding(encoded);
        match self.native_encoded_value_kind(encoded)? {
            NativeEncodedValueKind::Int => self
                .native_encoded_int(encoded)
                .map(php_runtime::api::ArrayKey::Int),
            NativeEncodedValueKind::String => self
                .native_string_name_bytes(encoded)
                .map(php_runtime::api::ArrayKey::from_bytes),
            _ => None,
        }
    }

    /// Publishes one IEEE-754 scalar directly. The payload is authoritative
    /// and no cold value mirror is retained.
    pub(super) fn encode_native_float_owner(
        &mut self,
        value: php_runtime::api::FloatValue,
    ) -> Result<i64, String> {
        let index = self.reserve_direct_value_slot()?;
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT,
            payload: value.to_f64().to_bits(),
            ..php_jit::JitNativeValueSlot::default()
        };
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .ok_or_else(|| "direct native value handle overflow".to_owned())?;
        Ok((php_jit::JIT_VALUE_RUNTIME_FLOAT_TAG | u64::from(runtime_index)) as i64)
    }

    /// Keeps the full PHP integer domain on the authoritative native plane.
    /// Most integers remain immediate; only bit patterns overlapping a native
    /// handle namespace consume a direct slot.
    pub(super) fn encode_native_int(&mut self, value: i64) -> Result<i64, String> {
        if php_jit::jit_decode_runtime_value(value).is_none()
            && php_jit::jit_decode_constant(value).is_none()
        {
            return Ok(value);
        }
        let index = self.reserve_direct_value_slot()?;
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT,
            flags: php_jit::JIT_NATIVE_DIRECT_INT_ABI_VERSION,
            payload: value as u64,
            ..php_jit::JitNativeValueSlot::default()
        };
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .ok_or_else(|| "direct native integer handle overflow".to_owned())?;
        Ok(php_jit::jit_encode_runtime_value(runtime_index))
    }

    /// Publishes one opaque PHP resource capability directly. The slot owns
    /// ResourceRef identity and lifetime; ordinary calls never wrap it in a
    /// Rust `Value` or allocate a compatibility handle.
    pub(super) fn encode_native_resource_owner(
        &mut self,
        resource: php_runtime::api::ResourceRef,
    ) -> Result<i64, String> {
        let resource_id = resource.id().get();
        let resource_type_length = resource.resource_type().len().max("Unknown".len());
        let resource_type_length = u32::try_from(resource_type_length)
            .map_err(|_| "direct native resource type name exceeds the descriptor".to_owned())?;
        if let Some(index) = self.direct_resource_handles.get(&resource_id).copied() {
            let slot = self
                .direct_value_slots
                .get_mut(index as usize)
                .filter(|slot| {
                    slot.refcount != 0
                        && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_RESOURCE
                        && slot.flags == php_jit::JIT_NATIVE_DIRECT_RESOURCE_ABI_VERSION
                        && slot.payload == resource_id
                })
                .ok_or_else(|| {
                    "direct native resource identity points at a dead slot".to_owned()
                })?;
            slot.reserved = slot.reserved.max(resource_type_length);
            slot.refcount = slot
                .refcount
                .checked_add(1)
                .ok_or_else(|| "direct native resource refcount overflow".to_owned())?;
            let runtime_index = index
                .checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
                .ok_or_else(|| "direct native resource handle overflow".to_owned())?;
            return Ok(php_jit::jit_encode_typed_runtime_value(
                runtime_index,
                php_jit::JIT_VALUE_RUNTIME_RESOURCE_TAG,
            ));
        }

        let index = self.reserve_direct_value_slot()?;
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .ok_or_else(|| "direct native resource handle overflow".to_owned())?;
        let owner = Box::into_raw(Box::new(resource));
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            // The identity table owns one request-lifetime native reference
            // in addition to the encoded owner returned to the caller.
            refcount: 2,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_RESOURCE,
            flags: php_jit::JIT_NATIVE_DIRECT_RESOURCE_ABI_VERSION,
            reserved: resource_type_length,
            payload: resource_id,
            aux: owner as usize as u64,
        };
        self.direct_resource_handles.insert(
            resource_id,
            u32::try_from(index).map_err(|_| "direct native resource index overflow".to_owned())?,
        );
        Ok(php_jit::jit_encode_typed_runtime_value(
            runtime_index,
            php_jit::JIT_VALUE_RUNTIME_RESOURCE_TAG,
        ))
    }

    /// Publishes object identity and PHP ownership in the direct plane. The
    /// slot-parallel stable owner supplies the backing identity needed at a
    /// cold boundary; declared values move into native slots immediately.
    #[track_caller]
    pub(super) fn encode_native_object_owner(
        &mut self,
        object: php_runtime::api::ObjectRef,
    ) -> Result<i64, String> {
        let object_id = object.id();
        let existing = self
            .baseline_values
            .direct_object_handles
            .get(&object_id)
            .copied()
            .or_else(|| {
                let used = usize::try_from(*self.direct_value_next).ok()?;
                (0..used)
                    .find(|index| {
                        self.direct_object(*index)
                            .is_some_and(|candidate| candidate.id() == object_id)
                    })
                    .and_then(|index| u32::try_from(index).ok())
            });
        if let Some(index) = existing {
            let slot = self
                .direct_value_slots
                .get_mut(index as usize)
                .filter(|slot| {
                    slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
                })
                .ok_or_else(|| "direct native object identity points at a dead slot".to_owned())?;
            slot.refcount = slot
                .refcount
                .checked_add(1)
                .ok_or_else(|| "direct native object refcount overflow".to_owned())?;
            self.baseline_values
                .direct_object_handles
                .insert(object_id, index);
            let runtime_index = index
                .checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
                .ok_or_else(|| "direct native object handle overflow".to_owned())?;
            if !php_jit::jit_native_object_property_view_is_published(
                self.direct_value_slots[index as usize].flags,
            ) && let Err(error) = self.promote_direct_object_property_slots(index as usize)
            {
                let _ = self.release_direct_value_index(index as usize);
                return Err(error);
            }
            return Ok((php_jit::JIT_VALUE_RUNTIME_OBJECT_TAG | u64::from(runtime_index)) as i64);
        }
        let index = self.reserve_direct_value_slot()?;
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .ok_or_else(|| "direct native value handle overflow".to_owned())?;
        let shutdown_handle = self
            .shutdown_destructor_queue
            .is_some()
            .then(|| object.weak_handle());
        let owner = Box::into_raw(Box::new(object));
        self.direct_object_owners[index] = owner as usize as u64;
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT,
            payload: object_id,
            ..php_jit::JitNativeValueSlot::default()
        };
        self.baseline_values.direct_object_handles.insert(
            object_id,
            u32::try_from(index).map_err(|_| "direct native object index overflow".to_owned())?,
        );
        if let Err(error) = self.promote_direct_object_property_slots(index) {
            let _ = self.release_direct_value_index(index);
            return Err(error);
        }
        if let Some(handle) = shutdown_handle {
            self.shutdown_destructor_queue
                .as_mut()
                .expect("shutdown destructor queue disappeared during object publication")
                .push(handle);
        }
        Ok((php_jit::JIT_VALUE_RUNTIME_OBJECT_TAG | u64::from(runtime_index)) as i64)
    }

    /// Removes authoritative native properties from an object that is about
    /// to die without running user code. The encoded children are returned to
    /// the central direct release walk; no Rust `Value` is reconstructed.
    pub(super) fn take_direct_object_children(&mut self, index: usize) -> Result<Vec<i64>, String> {
        let object = self
            .direct_object_owner(index)
            .ok_or_else(|| format!("direct native object {index} has no stable owner"))?;
        let descriptor = *self
            .direct_value_slots
            .get(index)
            .ok_or_else(|| format!("direct native object {index} slot is missing"))?;
        if !php_jit::jit_native_object_property_view_is_published(descriptor.flags) {
            return Ok(Vec::new());
        }
        let (slots, dynamic) = object
            .take_native_property_slots(descriptor.payload)
            .ok_or_else(|| format!("direct native object {index} lost its property slots"))?;
        let mut children: Vec<i64> = slots
            .iter()
            .filter(|slot| slot.initialized != 0)
            .map(|slot| slot.value)
            .collect();
        children.extend(
            dynamic
                .values()
                .filter(|cell| cell.slot.initialized != 0)
                .map(|cell| cell.slot.value),
        );
        self.direct_value_slots[index].flags = 0;
        self.direct_value_slots[index].reserved = 0;
        self.direct_value_slots[index].payload = object.id();
        self.direct_value_slots[index].aux = 0;
        Ok(children)
    }

    pub(super) fn reserve_direct_array_entries(
        &mut self,
        length: usize,
    ) -> Result<(usize, usize), String> {
        // Rust-side publication normally installs a completed immutable/COW
        // snapshot. Reserving the CLIF construction headroom for every such
        // array made hundreds of thousands of one- and two-element values each
        // pin eight entries. Keep one cell so a freed empty range can carry
        // its intrusive free-list link; mutation grows the range on demand.
        // Newly constructed CLIF arrays still use
        // `JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY` directly in generated
        // code and therefore retain their append headroom.
        let capacity = length.max(1).next_power_of_two();
        let bucket = capacity.trailing_zeros() as usize;
        let head = self.direct_array_free_heads[bucket];
        if head != php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE {
            let start = head as usize;
            let next = self
                .direct_array_entries
                .get(start)
                .map(|entry| entry.key as u32)
                .ok_or_else(|| "direct native array free-list entry is missing".to_owned())?;
            self.direct_array_free_heads[bucket] = next;
            *self.direct_array_reused_bytes = self.direct_array_reused_bytes.saturating_add(
                capacity.saturating_mul(std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>())
                    as u64,
            );
            return Ok((start, capacity));
        }
        let start = usize::try_from(*self.direct_array_next)
            .map_err(|_| "direct native array entry index overflow".to_owned())?;
        let end = start
            .checked_add(capacity)
            .ok_or_else(|| "direct native array entry range overflow".to_owned())?;
        if end > self.direct_array_entries.len() {
            let reusable = self
                .direct_array_free_heads
                .iter()
                .filter(|head| **head != php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE)
                .count();
            let (live_arrays, live_entries, live_capacity, live_refs) = self
                .direct_value_slots
                .get(..usize::try_from(*self.direct_value_next).unwrap_or(0))
                .unwrap_or_default()
                .iter()
                .filter(|slot| {
                    slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
                })
                .fold((0usize, 0u64, 0u64, 0u64), |totals, slot| {
                    (
                        totals.0.saturating_add(1),
                        totals.1.saturating_add(slot.payload),
                        totals.2.saturating_add(u64::from(slot.reserved)),
                        totals.3.saturating_add(u64::from(slot.refcount)),
                    )
                });
            let direct_used = usize::try_from(*self.direct_value_next).unwrap_or(0);
            let mut referenced = vec![false; direct_used];
            let direct_base = self.direct_array_entries.as_ptr() as usize;
            let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
            for slot in self
                .direct_value_slots
                .get(..direct_used)
                .unwrap_or_default()
                .iter()
                .filter(|slot| {
                    slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
                })
            {
                let start = usize::try_from(slot.aux)
                    .unwrap_or(direct_base)
                    .saturating_sub(direct_base)
                    / entry_size;
                let length = usize::try_from(slot.payload).unwrap_or(0);
                for entry in self
                    .direct_array_entries
                    .get(start..start.saturating_add(length))
                    .unwrap_or_default()
                {
                    for encoded in [entry.key, entry.value] {
                        if let Some(index) = Self::direct_value_index(encoded)
                            && index < referenced.len()
                        {
                            referenced[index] = true;
                        }
                    }
                }
            }
            let unreferenced_arrays = self
                .direct_value_slots
                .get(..direct_used)
                .unwrap_or_default()
                .iter()
                .enumerate()
                .filter(|(index, slot)| {
                    slot.refcount != 0
                        && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
                        && !referenced[*index]
                })
                .count();
            return Err(format!(
                "direct native array arena exhausted at {end} entries (next={start}, requested={capacity}, reusable_buckets={reusable}, live_arrays={live_arrays}, live_entries={live_entries}, live_capacity={live_capacity}, live_refs={live_refs}, unreferenced_arrays={unreferenced_arrays})"
            ));
        }
        *self.direct_array_next = u32::try_from(end)
            .map_err(|_| "direct native array entry index overflow".to_owned())?;
        Ok((start, capacity))
    }

    pub(super) fn free_direct_array_entries(&mut self, start: usize, capacity: usize) {
        if capacity == 0 {
            return;
        }
        if !capacity.is_power_of_two() {
            return;
        }
        let Ok(start_u32) = u32::try_from(start) else {
            return;
        };
        let bucket = capacity.trailing_zeros() as usize;
        if bucket >= self.direct_array_free_heads.len() || start >= self.direct_array_entries.len()
        {
            return;
        }
        let previous = self.direct_array_free_heads[bucket];
        self.direct_array_entries[start].key = i64::from(previous);
        self.direct_array_entries[start].value = 0;
        self.direct_array_free_heads[bucket] = start_u32;
    }

    pub(super) fn encode_prepared_callable(
        &mut self,
        callable: Box<php_runtime::api::CallableValue>,
    ) -> Result<i64, String> {
        if matches!(
            callable.as_ref(),
            php_runtime::api::CallableValue::Closure(_)
        ) {
            return self.encode_prepared_closure(*callable);
        }
        let owner = match *callable {
            php_runtime::api::CallableValue::UserFunction { name } => {
                let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
                let resolved_function = self.compiled.lookup_function(&normalized).or_else(|| {
                    normalized
                        .rsplit_once('\\')
                        .and_then(|(_, basename)| self.compiled.lookup_function(basename))
                });
                let resolved_function = resolved_function.and_then(|function| {
                    native_fixed_callable_plan(&self.compiled, function, false)
                });
                NativePreparedCallableOwner::user_function(
                    name.into_bytes().into_boxed_slice(),
                    resolved_function,
                )
            }
            php_runtime::api::CallableValue::InternalBuiltin { name } => {
                NativePreparedCallableOwner::internal_builtin(name.into_bytes().into_boxed_slice())
            }
            php_runtime::api::CallableValue::BoundMethod {
                target,
                method,
                scope,
            } => {
                let method = method.into_bytes().into_boxed_slice();
                let scope = scope.map(|scope| scope.into_bytes().into_boxed_slice());
                match target {
                    php_runtime::api::CallableMethodTarget::Object(object) => {
                        NativePreparedCallableOwner::bound_object(
                            self.encode_native_object_owner(object)?,
                            method,
                            scope,
                            None,
                        )
                    }
                    php_runtime::api::CallableMethodTarget::Class(class) => {
                        NativePreparedCallableOwner::bound_class(
                            class.into_bytes().into_boxed_slice(),
                            method,
                            scope,
                            None,
                        )
                    }
                }
            }
            php_runtime::api::CallableValue::MethodPlaceholder { target } => {
                NativePreparedCallableOwner::method_placeholder(
                    target.into_bytes().into_boxed_slice(),
                )
            }
            php_runtime::api::CallableValue::UnresolvedDynamic { target } => {
                NativePreparedCallableOwner::unresolved_dynamic(
                    target.into_bytes().into_boxed_slice(),
                )
            }
            php_runtime::api::CallableValue::Closure(_) => unreachable!(),
        };
        let index = match self.reserve_direct_value_slot() {
            Ok(index) => index,
            Err(error) => {
                if owner.native_view.kind == php_jit::JIT_NATIVE_CALLABLE_KIND_BOUND_OBJECT_METHOD {
                    let _ = self.release(owner.native_view.receiver);
                }
                return Err(error);
            }
        };
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .expect("direct callable index is bounded by the native value arena");
        let owner = Box::into_raw(Box::new(owner));
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE,
            flags: php_jit::JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION,
            aux: owner as usize as u64,
            ..php_jit::JitNativeValueSlot::default()
        };
        Ok(php_jit::jit_encode_typed_runtime_value(
            runtime_index,
            php_jit::JIT_VALUE_RUNTIME_CALLABLE_TAG,
        ))
    }

    /// Gives an encoded value one additional request-arena owner without
    /// decoding or reconstructing it. Direct values are authoritative;
    /// `None` is reserved for proxies/iterators whose cold semantics require
    /// an explicit operation.
    pub(super) fn duplicate_authoritative_native_value(
        &mut self,
        encoded: i64,
    ) -> Result<Option<i64>, String> {
        if self.is_globals_proxy(encoded) {
            return Ok(None);
        }
        if let Some(index) = Self::direct_value_index(encoded) {
            if self.direct_value_slots.get(index).is_some_and(|slot| {
                matches!(
                    slot.kind,
                    php_jit::JIT_NATIVE_VALUE_VIEW_FOREACH_DIRECT
                        | php_jit::JIT_NATIVE_VALUE_VIEW_COLD_ITERATOR
                )
            }) {
                return Ok(None);
            }
            self.retain(encoded)?;
            return Ok(Some(encoded));
        }
        if let Some(index) = php_jit::jit_decode_runtime_value(encoded) {
            return Err(format!(
                "native runtime value {index} is outside the authoritative direct slot plane"
            ));
        }
        if let Some(constant) = php_jit::jit_decode_constant(encoded)
            && !matches!(
                constant,
                u32::MAX
                    | php_jit::JIT_VALUE_UNINITIALIZED
                    | php_jit::JIT_VALUE_FALSE
                    | php_jit::JIT_VALUE_TRUE
            )
        {
            return self.stabilize_active_unit_constant(constant).map(Some);
        }
        Ok(Some(encoded))
    }

    pub(super) fn transfer_external_return(
        &mut self,
        encoded: i64,
        owner_unit: usize,
    ) -> Result<i64, String> {
        if let Some(index) = Self::direct_value_index(encoded) {
            if let Some(prepared) = self.direct_prepared_closure_mut(index)
                && prepared.closure.context.owner_unit.is_none()
            {
                prepared.closure.context.owner_unit = Some(owner_unit);
                return Ok(encoded);
            }
            // Direct arrays may still contain constants indexed by the
            // callee's IrUnit. Rewrite only those embedded constants while
            // the callee unit is active; otherwise the caller can interpret
            // the same numeric index as an unrelated value. The native
            // array slots remain authoritative and no Rust `PhpArray` is
            // reconstructed at this boundary.
            self.stabilize_direct_array_for_cross_unit(encoded)?;
            return Ok(encoded);
        }
        if php_jit::jit_decode_runtime_value(encoded).is_some() {
            return Ok(encoded);
        }
        if let Some(constant) = php_jit::jit_decode_constant(encoded)
            && !matches!(
                constant,
                u32::MAX
                    | php_jit::JIT_VALUE_UNINITIALIZED
                    | php_jit::JIT_VALUE_FALSE
                    | php_jit::JIT_VALUE_TRUE
            )
        {
            return self.stabilize_active_unit_constant(constant);
        }
        Ok(encoded)
    }

    pub(super) fn retain(&mut self, encoded: i64) -> Result<(), String> {
        if let Some(index) = Self::direct_value_index(encoded) {
            let refcount = &mut self
                .direct_value_slots
                .get_mut(index)
                .ok_or_else(|| format!("direct native value {index} is missing"))?
                .refcount;
            *refcount = refcount
                .checked_add(1)
                .ok_or_else(|| format!("direct native value {index} refcount overflow"))?;
            return Ok(());
        }
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Ok(());
        };
        Err(format!(
            "native runtime value {index} is outside the authoritative direct slot plane"
        ))
    }

    /// Classify an encoded PHP value without cloning it out of the request
    /// arena. Immediates and direct records are authoritative; cold iterator
    /// and generator records are never references.
    pub(super) fn php_handle_is_reference(&self, encoded: i64) -> Option<bool> {
        if let Some(index) = Self::direct_value_index(encoded) {
            return self.direct_value_slots.get(index).and_then(|slot| {
                (slot.refcount != 0).then_some(matches!(
                    slot.kind,
                    php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
                        | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                ))
            });
        }
        php_jit::jit_decode_runtime_value(encoded)
            .is_none()
            .then_some(false)
    }

    /// Borrows one authoritative native string without materializing or
    /// copying it. Direct string slots and immutable unit literals share this
    /// read plane; consumers that must outlive the borrow explicitly copy at
    /// their cold capability boundary.
    pub(super) fn native_string_bytes(&self, encoded: i64) -> Option<&[u8]> {
        if let Some(index) = Self::direct_value_index(encoded) {
            let slot = self.direct_value_slots.get(index)?;
            if slot.refcount == 0 || slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_STRING {
                return None;
            }
            let length = usize::try_from(slot.payload).ok()?;
            let base = self.direct_string_bytes.as_ptr() as usize;
            let address = usize::try_from(slot.aux).ok()?;
            let start = address.checked_sub(base)?;
            return self
                .direct_string_bytes
                .get(start..start.checked_add(length)?);
        }
        if php_jit::jit_decode_runtime_value(encoded).is_some() {
            return None;
        }
        let constant = php_jit::jit_decode_constant(encoded)?;
        match self.unit.constants.get(constant as usize)? {
            php_ir::IrConstant::String(value) => Some(value.as_bytes()),
            php_ir::IrConstant::StringBytes(value) => Some(value.as_slice()),
            _ => None,
        }
    }

    /// Copy one native string name for a cold capability lookup without
    /// materializing a PHP `Value`. Symbol tables own Rust strings, so this
    /// allocation is the exact query payload rather than a value-plane
    /// conversion.
    pub(super) fn native_string_name_bytes(&self, encoded: i64) -> Option<Vec<u8>> {
        self.native_string_bytes(encoded).map(<[u8]>::to_vec)
    }

    /// Borrows the stable owner of a direct native object without demoting its
    /// authoritative property storage or constructing a Rust `Value`.
    pub(super) fn native_query_object(&self, encoded: i64) -> Option<php_runtime::api::ObjectRef> {
        let encoded = self.dereference_direct_encoding(encoded);
        if let Some(index) = Self::direct_value_index(encoded) {
            return self.direct_object(index);
        }
        None
    }

    /// Returns the immutable, inheritance-complete class record already
    /// published for a native object layout.  Conditional definitions from a
    /// different unit are deliberately not searched by name: the active or
    /// deployment owner must identify the exact class allocation.
    pub(super) fn prepared_native_runtime_class(
        &self,
        name: &str,
    ) -> Option<Rc<PreparedNativeRuntimeClass>> {
        let normalized = normalize_class_name(name);
        let cache = self.runtime_class_cache.borrow();
        if let Some(prepared) = cache.get(&(self.current_dynamic_unit, normalized.clone())) {
            return Some(Rc::clone(prepared));
        }
        if let Some(unit) = self.external_class_units.get(&normalized).copied()
            && let Some(prepared) = cache.get(&(Some(unit), normalized.clone()))
        {
            return Some(Rc::clone(prepared));
        }
        cache.get(&(None, normalized)).map(Rc::clone)
    }

    /// Reads one declared property cell from the authoritative native object
    /// representation without materializing the remaining object slots.
    #[allow(unsafe_code)]
    pub(super) fn native_declared_property_slot(
        &mut self,
        encoded: i64,
        property: &str,
    ) -> Option<php_runtime::api::NativeDeclaredPropertySlot> {
        let location = self.native_declared_property_slot_location(encoded, property)?;
        // SAFETY: the native slot box is the authoritative immovable object
        // storage while the live direct descriptor publishes this layout.
        Some(unsafe { *location })
    }

    #[allow(unsafe_code)]
    pub(super) fn native_declared_property_slot_location(
        &mut self,
        encoded: i64,
        property: &str,
    ) -> Option<*mut php_runtime::api::NativeDeclaredPropertySlot> {
        let encoded = self.dereference_direct_encoding(encoded);
        let index = Self::direct_value_index(encoded)?;
        let descriptor = *self.direct_value_slots.get(index)?;
        if descriptor.refcount == 0
            || descriptor.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
        {
            return None;
        }
        if !php_jit::jit_native_object_property_view_is_published(descriptor.flags)
            && !self.promote_direct_object_property_slots(index).ok()?
        {
            return None;
        }
        let descriptor = *self.direct_value_slots.get(index)?;
        let object = self.direct_object(index)?;
        let slot = object.declared_slot_index(property)?;
        let (base, count) = object.native_declared_slots_view(descriptor.payload)?;
        let slot = usize::try_from(slot).ok()?;
        if slot >= count {
            return None;
        }
        // SAFETY: the native slot box is the authoritative immovable object
        // storage while the live direct descriptor publishes this layout.
        Some(unsafe { base.add(slot) })
    }

    /// Reads one dynamic property from the same authoritative native value
    /// plane as declared slots. The outer option denotes a valid direct
    /// object representation; the inner option denotes property existence.
    pub(super) fn native_dynamic_property_slot(
        &mut self,
        encoded: i64,
        property: &str,
    ) -> Option<Option<php_runtime::api::NativeDeclaredPropertySlot>> {
        let encoded = self.dereference_direct_encoding(encoded);
        let index = Self::direct_value_index(encoded)?;
        let descriptor = *self.direct_value_slots.get(index)?;
        if descriptor.refcount == 0
            || descriptor.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
        {
            return None;
        }
        if !php_jit::jit_native_object_property_view_is_published(descriptor.flags)
            && !self.promote_direct_object_property_slots(index).ok()?
        {
            return None;
        }
        let descriptor = *self.direct_value_slots.get(index)?;
        self.direct_object(index)?
            .native_dynamic_property_slot(descriptor.payload, property)
    }

    /// Borrows one existing object-property value from the authoritative
    /// native slot plane. Internal native classes use this for their private
    /// state instead of consulting the now-empty cold `ObjectRef` property
    /// map after promotion.
    pub(super) fn native_object_property_value(
        &mut self,
        encoded: i64,
        property: &str,
    ) -> Option<i64> {
        if let Some(slot) = self.native_declared_property_slot(encoded, property) {
            return (slot.initialized != 0).then_some(slot.value);
        }
        self.native_dynamic_property_slot(encoded, property)?
            .and_then(|slot| (slot.initialized != 0).then_some(slot.value))
    }

    /// Moves a fresh encoded owner into one existing authoritative object
    /// property. This is the mutation counterpart of
    /// `native_object_property_value`; it neither reconstructs a Rust `Value`
    /// nor demotes sibling properties.
    #[allow(unsafe_code)]
    pub(super) fn replace_native_object_property_owned(
        &mut self,
        object: i64,
        property: &str,
        value: i64,
    ) -> Result<bool, String> {
        if self.php_handle_is_reference(value) != Some(false) {
            self.release(value)?;
            return Ok(false);
        }
        if let Some(location) = self.native_declared_property_slot_location(object, property) {
            // SAFETY: `location` belongs to the request-stable authoritative
            // declared-slot box resolved above.
            let previous = unsafe { *location };
            if previous.initialized != 0
                && self.php_handle_is_reference(previous.value) != Some(false)
            {
                self.release(value)?;
                return Ok(false);
            }
            // SAFETY: the fresh owner moves into the stable property cell.
            unsafe {
                *location = php_runtime::api::NativeDeclaredPropertySlot {
                    initialized: 1,
                    reserved: 0,
                    value,
                };
            }
            self.mark_roots_dirty(RootMutationReason::RootedContainer);
            if previous.initialized != 0 {
                self.release(previous.value)?;
            }
            return Ok(true);
        }

        let object = self.dereference_direct_encoding(object);
        let Some(index) = Self::direct_value_index(object) else {
            self.release(value)?;
            return Ok(false);
        };
        let Some(descriptor) = self.direct_value_slots.get(index).copied() else {
            self.release(value)?;
            return Ok(false);
        };
        if descriptor.refcount == 0
            || descriptor.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
            || (!php_jit::jit_native_object_property_view_is_published(descriptor.flags)
                && !self.promote_direct_object_property_slots(index)?)
        {
            self.release(value)?;
            return Ok(false);
        }
        let descriptor = self.direct_value_slots[index];
        let Some(owner) = self.direct_object(index) else {
            self.release(value)?;
            return Ok(false);
        };
        let Some(Some(previous)) = owner.native_dynamic_property_slot(descriptor.payload, property)
        else {
            self.release(value)?;
            return Ok(false);
        };
        if self.php_handle_is_reference(previous.value) != Some(false) {
            self.release(value)?;
            return Ok(false);
        }
        let replacement = php_runtime::api::NativeDeclaredPropertySlot {
            initialized: 1,
            reserved: 0,
            value,
        };
        let previous = match owner.set_native_dynamic_property(
            descriptor.payload,
            property.to_owned(),
            replacement,
        ) {
            Ok(Some(previous)) => previous,
            Ok(None) => {
                return Err(format!(
                    "native internal property {property} disappeared during replacement"
                ));
            }
            Err(replacement) => {
                self.release(replacement.value)?;
                return Ok(false);
            }
        };
        self.mark_roots_dirty(RootMutationReason::RootedContainer);
        self.release(previous.value)?;
        Ok(true)
    }

    /// Replaces one ordinary declared-property owner without materializing
    /// either the object or the assigned value into the cold Rust plane.
    ///
    /// The property and assignment expression each need an independent
    /// owner unless executable ownership moves the input owner into the
    /// expression result. Reference-backed cells deliberately remain a cold
    /// semantic shape until their write-through path is native as well.
    #[allow(unsafe_code)]
    pub(super) fn assign_plain_native_declared_property(
        &mut self,
        object: i64,
        value: i64,
        property: &str,
        move_result: bool,
    ) -> Result<Option<i64>, String> {
        let Some(location) = self.native_declared_property_slot_location(object, property) else {
            return Ok(None);
        };
        // SAFETY: `location` belongs to the request-stable authoritative
        // declared-slot box resolved above.
        let previous = unsafe { *location };
        if previous.initialized != 0 && self.php_handle_is_reference(previous.value) != Some(false)
        {
            return Ok(None);
        }
        if self.php_handle_is_reference(value) != Some(false) {
            return Ok(None);
        }
        let Some(property_owner) = self.duplicate_authoritative_native_value(value)? else {
            return Ok(None);
        };
        let result = if move_result {
            value
        } else {
            let Some(result) = self.duplicate_authoritative_native_value(value)? else {
                self.release(property_owner)?;
                return Ok(None);
            };
            result
        };
        // SAFETY: the old owner remains live until the replacement record has
        // been installed. The new record consumes `property_owner`.
        unsafe {
            *location = php_runtime::api::NativeDeclaredPropertySlot {
                initialized: 1,
                reserved: 0,
                value: property_owner,
            };
        }
        self.mark_roots_dirty(RootMutationReason::RootedContainer);
        if previous.initialized != 0
            && let Err(error) = self.release(previous.value)
        {
            if !move_result {
                let _ = self.release(result);
            }
            return Err(error);
        }
        Ok(Some(result))
    }

    /// Replaces one ordinary dynamic-property owner without decoding the
    /// receiver or assigned value. Magic access, declared-name visibility,
    /// references, and creation diagnostics are admitted by the caller.
    pub(super) fn encode_direct_reference_payload_owned(
        &mut self,
        payload: i64,
    ) -> Result<i64, String> {
        let index = self.reserve_direct_value_slot()?;
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
            flags: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
            reserved: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED,
            payload: payload as u64,
            ..php_jit::JitNativeValueSlot::default()
        };
        let Some(runtime_index) = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
        else {
            self.direct_value_slots[index] = php_jit::JitNativeValueSlot::default();
            return Err("direct native reference handle overflow".to_owned());
        };
        Ok((php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG | u64::from(runtime_index)) as i64)
    }

    /// Turns one authoritative declared-property cell into a direct reference
    /// without materializing the object or any sibling property. The property
    /// owns one reference handle and the returned handle is an independent
    /// owner for the callee frame.
    #[allow(unsafe_code)]
    pub(super) fn bind_native_declared_property_reference(
        &mut self,
        object: i64,
        property: &str,
    ) -> Result<Option<i64>, String> {
        let Some(location) = self.native_declared_property_slot_location(object, property) else {
            return Ok(None);
        };
        // SAFETY: location belongs to the request-stable native declared slot
        // vector resolved above and remains live for this synchronous bind.
        let previous = unsafe { *location };
        if previous.initialized != 0 && self.php_handle_is_reference(previous.value) == Some(true) {
            self.retain(previous.value)?;
            return Ok(Some(previous.value));
        }
        let payload = if previous.initialized == 0 {
            php_jit::jit_encode_constant(u32::MAX)
        } else {
            previous.value
        };
        // Keep the existing property owner intact until both reference owners
        // have been established. This makes every error path recover without
        // reviving a released payload.
        self.retain(payload)?;
        let reference = match self.encode_direct_reference_payload_owned(payload) {
            Ok(reference) => reference,
            Err(error) => {
                self.release(payload)?;
                return Err(error);
            }
        };
        if let Err(error) = self.retain(reference) {
            self.release(reference)?;
            return Err(error);
        }
        let callee_owner = reference;
        // SAFETY: same stable slot location as above. Ownership of one
        // reference handle moves into the property cell.
        unsafe {
            *location = php_runtime::api::NativeDeclaredPropertySlot {
                initialized: 1,
                reserved: 0,
                value: reference,
            };
        }
        if previous.initialized != 0 {
            self.release(previous.value)?;
        }
        Ok(Some(callee_owner))
    }

    /// Gives an exact native call an independently owned dereferenced value
    /// without entering `ReferenceCell` or the Rust `Value` plane. `None`
    /// means the caller must take its one baseline continuation before any
    /// PHP-visible call binding effect.
    pub(super) fn duplicate_authoritative_dereferenced_native_value(
        &mut self,
        mut encoded: i64,
    ) -> Result<Option<i64>, String> {
        for _ in 0..16 {
            let Some(index) = Self::direct_value_index(encoded) else {
                break;
            };
            let Some(slot) = self
                .direct_value_slots
                .get(index)
                .copied()
                .filter(|slot| slot.refcount != 0)
            else {
                return Ok(None);
            };
            match slot.kind {
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                    if slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
                        && native_reference_state(slot.reserved)
                            != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY =>
                {
                    encoded = slot.payload as i64;
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR => return Ok(None),
                _ => return self.duplicate_authoritative_native_value(encoded),
            }
        }
        if self.php_handle_is_reference(encoded) == Some(true) {
            return Ok(None);
        }
        if php_jit::jit_decode_runtime_value(encoded).is_some() {
            return Ok(None);
        }
        self.duplicate_authoritative_native_value(encoded)
    }

    pub(super) fn direct_reference_payload(&self, encoded: i64) -> Option<i64> {
        let index = Self::direct_value_index(encoded)?;
        let slot = *self.direct_value_slots.get(index)?;
        (slot.refcount != 0
            && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            && native_reference_state(slot.reserved)
                != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY)
            .then_some(slot.payload as i64)
    }

    pub(super) fn dereference_direct_encoding(&self, mut encoded: i64) -> i64 {
        for _ in 0..16 {
            let Some(payload) = self.direct_reference_payload(encoded) else {
                break;
            };
            encoded = payload;
        }
        encoded
    }

    pub(super) fn native_encoded_value_kind(&self, encoded: i64) -> Option<NativeEncodedValueKind> {
        let encoded = self.dereference_direct_encoding(encoded);
        if let Some(constant) = php_jit::jit_decode_constant(encoded) {
            return match constant {
                u32::MAX => Some(NativeEncodedValueKind::Null),
                php_jit::JIT_VALUE_UNINITIALIZED => Some(NativeEncodedValueKind::Uninitialized),
                php_jit::JIT_VALUE_FALSE => Some(NativeEncodedValueKind::Bool(false)),
                php_jit::JIT_VALUE_TRUE => Some(NativeEncodedValueKind::Bool(true)),
                constant => match self.unit.constants.get(constant as usize)? {
                    php_ir::IrConstant::Null => Some(NativeEncodedValueKind::Null),
                    php_ir::IrConstant::Bool(value) => Some(NativeEncodedValueKind::Bool(*value)),
                    php_ir::IrConstant::Int(_) => Some(NativeEncodedValueKind::Int),
                    php_ir::IrConstant::Float(_) => Some(NativeEncodedValueKind::Float),
                    php_ir::IrConstant::String(_) | php_ir::IrConstant::StringBytes(_) => {
                        Some(NativeEncodedValueKind::String)
                    }
                    php_ir::IrConstant::Array(_) => Some(NativeEncodedValueKind::Array),
                    php_ir::IrConstant::NamedConstant(_)
                    | php_ir::IrConstant::ClassConstant { .. } => None,
                },
            };
        }
        if php_jit::jit_decode_runtime_value(encoded).is_none() {
            return Some(NativeEncodedValueKind::Int);
        }
        if let Some(index) = Self::direct_value_index(encoded) {
            let slot = self.direct_value_slots.get(index)?;
            if slot.refcount == 0 {
                return None;
            }
            return match slot.kind {
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT
                    if slot.flags == php_jit::JIT_NATIVE_DIRECT_INT_ABI_VERSION =>
                {
                    Some(NativeEncodedValueKind::Int)
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_STRING => Some(NativeEncodedValueKind::String),
                php_jit::JIT_NATIVE_VALUE_VIEW_ARRAY
                | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
                | php_jit::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY
                | php_jit::JIT_NATIVE_VALUE_VIEW_BORROWED_REFERENCE_ARRAY
                | php_jit::JIT_NATIVE_VALUE_VIEW_GLOBALS_PROXY => {
                    Some(NativeEncodedValueKind::Array)
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT => Some(NativeEncodedValueKind::Float),
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT => {
                    Some(NativeEncodedValueKind::Object)
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_RESOURCE => {
                    Some(NativeEncodedValueKind::Resource)
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE => {
                    Some(NativeEncodedValueKind::Callable)
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER
                | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_FIBER => {
                    Some(NativeEncodedValueKind::Fiber)
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_GENERATOR
                | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_GENERATOR
                | php_jit::JIT_NATIVE_VALUE_VIEW_COLD_GENERATOR => {
                    Some(NativeEncodedValueKind::Generator)
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
                | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR => {
                    Some(NativeEncodedValueKind::Reference)
                }
                _ => None,
            };
        }
        let _ = php_jit::jit_decode_runtime_value(encoded)?;
        None
    }

    pub(super) fn native_encoded_int(&self, encoded: i64) -> Option<i64> {
        let encoded = self.dereference_direct_encoding(encoded);
        if let Some(index) = Self::direct_value_index(encoded) {
            let slot = self.direct_value_slots.get(index)?;
            return (slot.refcount != 0
                && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT
                && slot.flags == php_jit::JIT_NATIVE_DIRECT_INT_ABI_VERSION)
                .then_some(slot.payload as i64);
        }
        if php_jit::jit_decode_runtime_value(encoded).is_none()
            && php_jit::jit_decode_constant(encoded).is_none()
        {
            return Some(encoded);
        }
        if let Some(constant) = php_jit::jit_decode_constant(encoded) {
            return match self.unit.constants.get(constant as usize)? {
                php_ir::IrConstant::Int(value) => Some(*value),
                _ => None,
            };
        }
        None
    }

    pub(super) fn native_encoded_float(&self, encoded: i64) -> Option<f64> {
        let encoded = self.dereference_direct_encoding(encoded);
        if let Some(index) = Self::direct_value_index(encoded) {
            let slot = self.direct_value_slots.get(index)?;
            return (slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT)
                .then(|| f64::from_bits(slot.payload));
        }
        if let Some(constant) = php_jit::jit_decode_constant(encoded) {
            return match self.unit.constants.get(constant as usize)? {
                php_ir::IrConstant::Float(value) => Some(*value),
                _ => None,
            };
        }
        None
    }

    pub(super) fn native_encoded_bool(&self, encoded: i64) -> Option<bool> {
        match self.native_encoded_value_kind(encoded)? {
            NativeEncodedValueKind::Bool(value) => Some(value),
            _ => None,
        }
    }

    pub(super) fn native_encoded_resource_id(&self, encoded: i64) -> Option<u64> {
        let encoded = self.dereference_direct_encoding(encoded);
        let index = Self::direct_value_index(encoded)?;
        let slot = self.direct_value_slots.get(index)?;
        (slot.refcount != 0
            && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_RESOURCE
            && slot.flags == php_jit::JIT_NATIVE_DIRECT_RESOURCE_ABI_VERSION)
            .then_some(slot.payload)
    }

    pub(super) fn native_encoded_type_name(&self, encoded: i64) -> &'static str {
        match self.native_encoded_value_kind(encoded) {
            Some(NativeEncodedValueKind::Null) => "null",
            Some(NativeEncodedValueKind::Uninitialized) => "uninitialized",
            Some(NativeEncodedValueKind::Bool(_)) => "bool",
            Some(NativeEncodedValueKind::Int) => "int",
            Some(NativeEncodedValueKind::Float) => "float",
            Some(NativeEncodedValueKind::String) => "string",
            Some(NativeEncodedValueKind::Array) => "array",
            Some(NativeEncodedValueKind::Object | NativeEncodedValueKind::Callable) => "object",
            Some(NativeEncodedValueKind::Resource) => "resource",
            Some(NativeEncodedValueKind::Generator) => "Generator",
            Some(NativeEncodedValueKind::Fiber) => "Fiber",
            Some(NativeEncodedValueKind::Reference) => "reference",
            None => "unknown",
        }
    }

    pub(super) fn native_encoded_is_callable(&self, encoded: i64) -> Option<bool> {
        let encoded = self.dereference_direct_encoding(encoded);
        match self.native_encoded_value_kind(encoded)? {
            NativeEncodedValueKind::Callable => Some(true),
            NativeEncodedValueKind::Object => {
                let object = self.native_query_object(encoded)?;
                let class = object.class_name();
                Some(
                    native_method_in_hierarchy(self, &class, "__invoke").is_some()
                        || native_external_method(self, &class, "__invoke").is_some(),
                )
            }
            NativeEncodedValueKind::String => {
                let name = String::from_utf8_lossy(self.native_string_bytes(encoded)?);
                Some(if let Some((class, method)) = name.split_once("::") {
                    native_method_in_hierarchy(self, class, method).is_some()
                        || native_external_method(self, class, method).is_some()
                } else {
                    self.function_id(&name).is_some()
                        || self.external_function(&name).is_some()
                        || php_extensions::BuiltinRegistry::new()
                            .contains(&name.to_ascii_lowercase())
                })
            }
            NativeEncodedValueKind::Array => {
                let entries = self.direct_array_entries_for(encoded)?;
                if entries.len() != 2 {
                    return Some(false);
                }
                let mut target = None;
                let mut method = None;
                for entry in entries {
                    match self.native_encoded_int(entry.key) {
                        Some(0) => target = Some(entry.value),
                        Some(1) => method = Some(entry.value),
                        _ => {}
                    }
                }
                let target = self.dereference_direct_encoding(target?);
                let method = self.dereference_direct_encoding(method?);
                let method = String::from_utf8_lossy(self.native_string_bytes(method)?);
                if let Some(object) = self.native_query_object(target) {
                    let class = object.class_name();
                    Some(
                        native_method_in_hierarchy(self, &class, &method).is_some()
                            || native_external_method(self, &class, &method).is_some(),
                    )
                } else {
                    let class = String::from_utf8_lossy(self.native_string_bytes(target)?);
                    Some(
                        native_method_in_hierarchy(self, &class, &method).is_some()
                            || native_external_method(self, &class, &method).is_some(),
                    )
                }
            }
            _ => Some(false),
        }
    }

    pub(super) fn native_encoded_matches_ir_type(
        &self,
        encoded: i64,
        type_: &php_ir::IrReturnType,
    ) -> Option<bool> {
        use php_ir::IrReturnType as Ir;
        let encoded = self.dereference_direct_encoding(encoded);
        let kind = self.native_encoded_value_kind(encoded)?;
        match type_ {
            Ir::Int => Some(kind == NativeEncodedValueKind::Int),
            Ir::Float => Some(matches!(
                kind,
                NativeEncodedValueKind::Float | NativeEncodedValueKind::Int
            )),
            Ir::String => Some(kind == NativeEncodedValueKind::String),
            Ir::Array => Some(kind == NativeEncodedValueKind::Array),
            Ir::Callable => self.native_encoded_is_callable(encoded),
            Ir::Iterable => Some(match kind {
                NativeEncodedValueKind::Array | NativeEncodedValueKind::Generator => true,
                NativeEncodedValueKind::Object => {
                    self.native_query_object(encoded).is_some_and(|object| {
                        native_class_is_a(self, &object.class_name(), "traversable")
                    })
                }
                _ => false,
            }),
            Ir::Object => Some(matches!(
                kind,
                NativeEncodedValueKind::Object
                    | NativeEncodedValueKind::Callable
                    | NativeEncodedValueKind::Generator
                    | NativeEncodedValueKind::Fiber
            )),
            Ir::Bool => Some(matches!(kind, NativeEncodedValueKind::Bool(_))),
            Ir::Null | Ir::Void => Some(kind == NativeEncodedValueKind::Null),
            Ir::Mixed => Some(true),
            Ir::Never => Some(false),
            Ir::False => Some(kind == NativeEncodedValueKind::Bool(false)),
            Ir::True => Some(kind == NativeEncodedValueKind::Bool(true)),
            Ir::Class { name, .. } => Some(
                native_special_value_class_is_a(kind, name).unwrap_or_else(|| {
                    self.native_query_object(encoded)
                        .is_some_and(|object| native_class_is_a(self, &object.class_name(), name))
                }),
            ),
            Ir::Nullable { inner } => {
                if kind == NativeEncodedValueKind::Null {
                    Some(true)
                } else {
                    self.native_encoded_matches_ir_type(encoded, inner)
                }
            }
            Ir::Union { members } | Ir::Dnf { members } => {
                let mut unknown = false;
                for member in members {
                    match self.native_encoded_matches_ir_type(encoded, member) {
                        Some(true) => return Some(true),
                        Some(false) => {}
                        None => unknown = true,
                    }
                }
                (!unknown).then_some(false)
            }
            Ir::Intersection { members } => {
                let mut unknown = false;
                for member in members {
                    match self.native_encoded_matches_ir_type(encoded, member) {
                        Some(true) => {}
                        Some(false) => return Some(false),
                        None => unknown = true,
                    }
                }
                (!unknown).then_some(true)
            }
        }
    }

    /// Checks whether a native value already has a representation accepted by
    /// typed storage. Unlike call-argument admission, this must not treat an
    /// integer as an already-coerced float.
    pub(super) fn native_encoded_exactly_matches_ir_type(
        &self,
        encoded: i64,
        type_: &php_ir::IrReturnType,
    ) -> Option<bool> {
        use php_ir::IrReturnType as Ir;
        let encoded = self.dereference_direct_encoding(encoded);
        let kind = self.native_encoded_value_kind(encoded)?;
        match type_ {
            Ir::Float => Some(kind == NativeEncodedValueKind::Float),
            Ir::Nullable { inner } => {
                if kind == NativeEncodedValueKind::Null {
                    Some(true)
                } else {
                    self.native_encoded_exactly_matches_ir_type(encoded, inner)
                }
            }
            Ir::Union { members } | Ir::Dnf { members } => {
                let mut unknown = false;
                for member in members {
                    match self.native_encoded_exactly_matches_ir_type(encoded, member) {
                        Some(true) => return Some(true),
                        Some(false) => {}
                        None => unknown = true,
                    }
                }
                (!unknown).then_some(false)
            }
            Ir::Intersection { members } => {
                let mut unknown = false;
                for member in members {
                    match self.native_encoded_exactly_matches_ir_type(encoded, member) {
                        Some(true) => {}
                        Some(false) => return Some(false),
                        None => unknown = true,
                    }
                }
                (!unknown).then_some(true)
            }
            _ => self.native_encoded_matches_ir_type(encoded, type_),
        }
    }

    /// Produces one owned native value for a typed by-value call parameter.
    /// `None` denotes a compatibility-only shape which has already crossed a
    /// cold call boundary and still requires the baseline `Value` coercer.
    pub(super) fn coerce_native_call_argument_encoded(
        &mut self,
        encoded: i64,
        type_: &php_ir::IrReturnType,
        strict: bool,
    ) -> Result<Option<i64>, String> {
        use php_ir::IrReturnType as Type;
        let encoded = self.dereference_direct_encoding(encoded);
        let Some(kind) = self.native_encoded_value_kind(encoded) else {
            return Ok(None);
        };

        if let Type::Nullable { inner } = type_ {
            if kind == NativeEncodedValueKind::Null {
                return self.duplicate_authoritative_native_value(encoded);
            }
            return self.coerce_native_call_argument_encoded(encoded, inner, strict);
        }

        // PHP admits int for a float declaration even under strict_types and
        // the callee observes a float value.
        if matches!(type_, Type::Float) && kind == NativeEncodedValueKind::Int {
            let value = self
                .native_encoded_int(encoded)
                .expect("classified native int has an integer payload");
            return self
                .encode_native_float_owner(php_runtime::api::FloatValue::from_f64(value as f64))
                .map(Some);
        }
        if self.native_encoded_matches_ir_type(encoded, type_) == Some(true) || strict {
            return self.duplicate_authoritative_native_value(encoded);
        }

        let converted = match (type_, kind) {
            (Type::Int, NativeEncodedValueKind::String) => {
                let bytes = self
                    .native_string_bytes(encoded)
                    .expect("classified native string has bytes");
                String::from_utf8_lossy(bytes).trim().parse::<i64>().ok()
            }
            (Type::Int, NativeEncodedValueKind::Float) => {
                self.native_encoded_float(encoded).map(|value| value as i64)
            }
            (Type::Int, NativeEncodedValueKind::Bool(_)) => {
                self.native_encoded_bool(encoded).map(i64::from)
            }
            _ => None,
        };
        if let Some(value) = converted {
            return Ok(Some(value));
        }

        match (type_, kind) {
            (Type::Float, NativeEncodedValueKind::String) => {
                let bytes = self
                    .native_string_bytes(encoded)
                    .expect("classified native string has bytes");
                if let Ok(value) = String::from_utf8_lossy(bytes).trim().parse::<f64>() {
                    return self
                        .encode_native_float_owner(php_runtime::api::FloatValue::from_f64(value))
                        .map(Some);
                }
            }
            (Type::Float, NativeEncodedValueKind::Bool(_)) => {
                let value = if self.native_encoded_bool(encoded).unwrap_or(false) {
                    1.0
                } else {
                    0.0
                };
                return self
                    .encode_native_float_owner(php_runtime::api::FloatValue::from_f64(value))
                    .map(Some);
            }
            (Type::String, NativeEncodedValueKind::Int) => {
                let value = self
                    .native_encoded_int(encoded)
                    .expect("classified native int has an integer payload");
                return self
                    .encode_direct_string_bytes(value.to_string().as_bytes())
                    .map(Some);
            }
            (Type::String, NativeEncodedValueKind::Float) => {
                let value = self
                    .native_encoded_float(encoded)
                    .expect("classified native float has a float payload");
                return self
                    .encode_direct_string_bytes(value.to_string().as_bytes())
                    .map(Some);
            }
            (Type::String, NativeEncodedValueKind::Bool(value)) => {
                return self
                    .encode_direct_string_bytes(if value { b"1" } else { b"" })
                    .map(Some);
            }
            (
                Type::Bool,
                NativeEncodedValueKind::Int
                | NativeEncodedValueKind::Float
                | NativeEncodedValueKind::String,
            ) => {
                if let Some(value) = self.native_encoded_truthy(encoded) {
                    return Ok(Some(php_jit::jit_encode_constant(if value {
                        php_jit::JIT_VALUE_TRUE
                    } else {
                        php_jit::JIT_VALUE_FALSE
                    })));
                }
            }
            (Type::Nullable { inner }, _) => {
                return self.coerce_native_call_argument_encoded(encoded, inner, strict);
            }
            (Type::Union { members } | Type::Dnf { members }, _) => {
                for member in members {
                    let Some(candidate) =
                        self.coerce_native_call_argument_encoded(encoded, member, strict)?
                    else {
                        continue;
                    };
                    if self.native_encoded_matches_ir_type(candidate, type_) == Some(true) {
                        return Ok(Some(candidate));
                    }
                    self.release(candidate)?;
                }
            }
            _ => {}
        }
        self.duplicate_authoritative_native_value(encoded)
    }

    /// Returns `None` for a shape that needs baseline semantics, otherwise an
    /// exact PHP isset classification without constructing a Rust `Value`.
    pub(super) fn native_encoded_is_set(&self, encoded: i64) -> Option<bool> {
        let encoded = self.dereference_direct_encoding(encoded);
        if php_jit::jit_decode_runtime_value(encoded).is_none()
            && php_jit::jit_decode_constant(encoded).is_none()
        {
            return Some(true);
        }
        if let Some(constant) = php_jit::jit_decode_constant(encoded) {
            return Some(!matches!(
                constant,
                u32::MAX | php_jit::JIT_VALUE_UNINITIALIZED
            ));
        }
        if let Some(index) = Self::direct_value_index(encoded) {
            let slot = self.direct_value_slots.get(index)?;
            if matches!(
                slot.kind,
                php_jit::JIT_NATIVE_VALUE_VIEW_FOREACH_DIRECT
                    | php_jit::JIT_NATIVE_VALUE_VIEW_COLD_ITERATOR
            ) {
                return None;
            }
            return (slot.refcount != 0
                && !matches!(
                    slot.kind,
                    php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
                        | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                ))
            .then_some(true);
        }
        let _ = php_jit::jit_decode_runtime_value(encoded)?;
        None
    }

    /// Exact native truthiness for scalar/string/array common shapes. Objects
    /// and materialized compatibility references remain baseline because
    /// SimpleXML and user-visible reference state require cold semantics.
    pub(super) fn native_encoded_truthy(&self, encoded: i64) -> Option<bool> {
        let encoded = self.dereference_direct_encoding(encoded);
        if php_jit::jit_decode_runtime_value(encoded).is_none()
            && php_jit::jit_decode_constant(encoded).is_none()
        {
            return Some(encoded != 0);
        }
        if let Some(constant) = php_jit::jit_decode_constant(encoded) {
            return match constant {
                u32::MAX | php_jit::JIT_VALUE_UNINITIALIZED | php_jit::JIT_VALUE_FALSE => {
                    Some(false)
                }
                php_jit::JIT_VALUE_TRUE => Some(true),
                _ => None,
            };
        }
        if let Some(index) = Self::direct_value_index(encoded) {
            let slot = *self.direct_value_slots.get(index)?;
            if slot.refcount == 0 {
                return None;
            }
            return match slot.kind {
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT
                    if slot.flags == php_jit::JIT_NATIVE_DIRECT_INT_ABI_VERSION =>
                {
                    Some(slot.payload != 0)
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT => Some(f64::from_bits(slot.payload) != 0.0),
                php_jit::JIT_NATIVE_VALUE_VIEW_STRING => Some(
                    slot.payload != 0 && slot.reserved & php_jit::JIT_NATIVE_STRING_VALUE_ZERO == 0,
                ),
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY => Some(slot.payload != 0),
                php_jit::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY
                | php_jit::JIT_NATIVE_VALUE_VIEW_BORROWED_REFERENCE_ARRAY => {
                    baseline_shared_array_storage_is_empty(slot.payload as usize)
                        .map(|is_empty| !is_empty)
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
                | php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
                | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR => None,
                php_jit::JIT_NATIVE_VALUE_VIEW_FOREACH_DIRECT
                | php_jit::JIT_NATIVE_VALUE_VIEW_COLD_ITERATOR => None,
                _ => Some(true),
            };
        }
        let _ = php_jit::jit_decode_runtime_value(encoded)?;
        None
    }

    /// Outer `None` means a non-direct shape; inner `None` means a valid
    /// direct traversal whose key is absent.
    pub(super) fn direct_dimension_path_encoded(
        &mut self,
        mut encoded: i64,
        keys: &[i64],
    ) -> Result<Option<Option<i64>>, String> {
        for key in keys {
            encoded = self.dereference_direct_encoding(encoded);
            if self.direct_array_slot(encoded).is_none() {
                return Ok(None);
            }
            let Some(key) = self.native_encoded_plain_array_key(*key) else {
                return Ok(None);
            };
            let Some(value) = self.direct_array_find_encoded(encoded, &key)? else {
                return Ok(Some(None));
            };
            encoded = value;
        }
        Ok(Some(Some(encoded)))
    }

    pub(super) fn php_handle_is_uninitialized(&self, encoded: i64) -> bool {
        if php_jit::jit_decode_constant(encoded) == Some(php_jit::JIT_VALUE_UNINITIALIZED) {
            return true;
        }
        false
    }

    pub(super) fn release(&mut self, encoded: i64) -> Result<(), String> {
        if let Some(index) = Self::direct_value_index(encoded) {
            return self.release_direct_value_index(index);
        }
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Ok(());
        };
        Err(format!(
            "native runtime value {index} is outside the authoritative direct slot plane"
        ))
    }

    pub(super) fn release_direct_value_index(&mut self, index: usize) -> Result<(), String> {
        if let Some(slot) = self.direct_value_slots.get(index)
            && slot.refcount == 1
            && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
        {
            let object = self
                .direct_object_owner(index)
                .ok_or_else(|| format!("direct native object {index} has no stable owner"))?;
            if self.object_has_native_destructor(&object.class_name())
                && !self
                    .destroyed_objects
                    .get(&object.id())
                    .is_some_and(WeakObjectHandle::is_alive)
            {
                return Err(format!(
                    "final owner of direct object {index} must enter the generated release spine"
                ));
            }
        }
        let reached_zero = {
            let slot = self
                .direct_value_slots
                .get_mut(index)
                .ok_or_else(|| format!("direct native value {index} is missing"))?;
            if slot.refcount == 0 {
                return Err(format!(
                    "direct native value {index} was already released (retired kind {})",
                    slot.flags
                ));
            }
            slot.refcount -= 1;
            slot.refcount == 0
        };
        if !reached_zero {
            return Ok(());
        }
        self.cross_unit_stable_values.remove(&index);
        let mut direct_object_children = Vec::new();
        if self.direct_value_slots[index].kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT {
            let object = self
                .direct_object_owner(index)
                .ok_or_else(|| format!("direct native object {index} has no stable owner"))?;
            let has_cold_alias = object.gc_refcount_estimate() > 2;
            if self.object_has_native_destructor(&object.class_name()) || has_cold_alias {
                // The direct descriptor is losing its final encoded owner, but
                // an ObjectRef may still live in a PHP reference/root. Restore
                // Rust slots before dropping the native owner so that alias
                // remains a complete object rather than an empty shell.
                self.demote_direct_object_property_slots(index)?;
            } else {
                direct_object_children = self.take_direct_object_children(index)?;
            }
        }
        let slot = self.direct_value_slots[index];
        let released_object = if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT {
            let owner = std::mem::replace(&mut self.direct_object_owners[index], 0);
            if owner == 0 {
                return Err(format!(
                    "direct native object {index} lost its stable owner"
                ));
            }
            // SAFETY: object publication created exactly one Box<ObjectRef>
            // for this slot and release clears/reclaims it exactly once when
            // the authoritative direct refcount reaches zero.
            #[allow(unsafe_code)]
            let object =
                unsafe { *Box::from_raw(owner as usize as *mut php_runtime::api::ObjectRef) };
            if self.baseline_values.direct_object_handles.get(&object.id()) == Some(&(index as u32))
            {
                self.baseline_values
                    .direct_object_handles
                    .remove(&object.id());
            }
            Some(object)
        } else {
            None
        };
        let released_resource = if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_RESOURCE {
            if slot.aux == 0 {
                return Err(format!(
                    "direct native resource {index} lost its stable owner"
                ));
            }
            // SAFETY: resource publication created exactly one boxed
            // ResourceRef and final direct-slot release reclaims it once.
            #[allow(unsafe_code)]
            let resource =
                unsafe { Box::from_raw(slot.aux as usize as *mut php_runtime::api::ResourceRef) };
            if self.direct_resource_handles.get(&resource.id().get()) == Some(&(index as u32)) {
                self.direct_resource_handles.remove(&resource.id().get());
            }
            Some(resource)
        } else {
            None
        };
        let released_callable = if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE {
            if slot.aux == 0 {
                return Err(format!(
                    "direct native callable {index} lost its stable record"
                ));
            }
            // SAFETY: callable publication created exactly one boxed record
            // for this slot and final release reclaims it exactly once.
            #[allow(unsafe_code)]
            let callable =
                unsafe { Box::from_raw(slot.aux as usize as *mut NativePreparedCallableOwner) };
            if callable.native_view.kind == php_jit::JIT_NATIVE_CALLABLE_KIND_CLOSURE
                && self.direct_closure_handles.get(&slot.payload) == Some(&(index as u32))
            {
                self.direct_closure_handles.remove(&slot.payload);
            }
            Some(callable)
        } else {
            None
        };
        let released_fiber = if matches!(
            slot.kind,
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER
                | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_FIBER
        ) {
            if slot.aux == 0 {
                return Err(format!(
                    "direct native Fiber {index} lost its stable record"
                ));
            }
            // SAFETY: Fiber publication created exactly one boxed record and
            // final direct-slot release reclaims it exactly once.
            #[allow(unsafe_code)]
            let fiber = unsafe { Box::from_raw(slot.aux as usize as *mut NativeDirectFiber) };
            self.baseline_values
                .direct_fiber_handles
                .retain(|_, mapped| *mapped as usize != index);
            self.baseline_values.direct_fiber_cells.remove(&index);
            Some(fiber)
        } else {
            None
        };
        let released_generator = if matches!(
            slot.kind,
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_GENERATOR
                | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_GENERATOR
        ) {
            if slot.aux == 0 {
                return Err(format!(
                    "direct native Generator {index} lost its stable activation"
                ));
            }
            // SAFETY: Generator publication created exactly one boxed
            // activation and final direct-slot release reclaims it once.
            #[allow(unsafe_code)]
            let generator =
                unsafe { Box::from_raw(slot.aux as usize as *mut NativeDirectGenerator) };
            self.baseline_values
                .direct_generator_handles
                .retain(|_, mapped| *mapped as usize != index);
            self.baseline_values.direct_generator_cells.remove(&index);
            Some(generator)
        } else {
            None
        };
        let released_cold_generator = if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_COLD_GENERATOR
        {
            if slot.aux == 0 {
                return Err(format!(
                    "cold native Generator {index} lost its stable identity"
                ));
            }
            // SAFETY: cold Generator publication created exactly one
            // boxed identity and final direct-slot release reclaims it.
            #[allow(unsafe_code)]
            let generator =
                unsafe { Box::from_raw(slot.aux as usize as *mut php_runtime::api::GeneratorRef) };
            self.baseline_values
                .direct_generator_handles
                .retain(|_, mapped| *mapped as usize != index);
            Some(generator)
        } else {
            None
        };
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY
            && !release_baseline_shared_array_storage(slot.payload as usize)
        {
            return Err(format!(
                "shared native array {index} storage was already released"
            ));
        }
        let freed_string_range = if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_STRING {
            let base = self.direct_string_bytes.as_ptr() as usize;
            let address = usize::try_from(slot.aux).unwrap_or(base);
            let start = address.saturating_sub(base);
            let length = usize::try_from(slot.payload).unwrap_or(0);
            if let Some(bytes) = self
                .direct_string_bytes
                .get(start..start.saturating_add(length))
            {
                let hash = native_direct_string_hash(bytes);
                let remove_bucket = self
                    .direct_string_interned_slots
                    .get_mut(&hash)
                    .is_some_and(|indices| {
                        indices.retain(|candidate| *candidate as usize != index);
                        indices.is_empty()
                    });
                if remove_bucket {
                    self.direct_string_interned_slots.remove(&hash);
                }
            }
            let capacity = php_jit::jit_native_direct_string_capacity(slot.reserved) as usize;
            (capacity != 0).then_some((start, capacity))
        } else {
            None
        };
        let (mut children, freed_array_range) =
            if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FOREACH {
                (vec![slot.payload as i64], None)
            } else if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY {
                if let Some(storage_version) =
                    self.baseline_values.direct_array_storage_ids.remove(&index)
                    && self
                        .baseline_values
                        .direct_array_handles
                        .get(&storage_version)
                        == Some(&(index as u32))
                {
                    self.baseline_values
                        .direct_array_handles
                        .remove(&storage_version);
                }
                let length = usize::try_from(slot.payload).unwrap_or(0);
                let base = self.direct_array_entries.as_ptr() as usize;
                let address = usize::try_from(slot.aux).unwrap_or(base);
                let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
                let start = address.saturating_sub(base) / entry_size;
                (
                    self.direct_array_entries
                        .get(start..start.saturating_add(length))
                        .unwrap_or_default()
                        .iter()
                        .flat_map(|entry| [entry.key, entry.value])
                        .collect::<Vec<_>>(),
                    Some((start, slot.reserved as usize)),
                )
            } else if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
                && native_reference_state(slot.reserved)
                    != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
            {
                (vec![slot.payload as i64], None)
            } else {
                (Vec::new(), None)
            };
        if let Some(callable) = released_callable {
            let view = callable.native_view;
            if view.kind == php_jit::JIT_NATIVE_CALLABLE_KIND_CLOSURE {
                if view.flags & php_jit::JIT_NATIVE_PREPARED_CLOSURE_HAS_IMPLICIT_THIS != 0 {
                    children.push(view.implicit_this);
                }
                if view.capture_count != 0 && view.captures != 0 {
                    // SAFETY: the callable owner still owns the immutable
                    // boxed capture slice addressed by its native view.
                    #[allow(unsafe_code)]
                    let captures = unsafe {
                        std::slice::from_raw_parts(
                            view.captures as usize as *const i64,
                            view.capture_count as usize,
                        )
                    };
                    children.extend_from_slice(captures);
                }
            } else if view.kind == php_jit::JIT_NATIVE_CALLABLE_KIND_BOUND_OBJECT_METHOD {
                children.push(view.receiver);
            }
        }
        if let Some(fiber) = released_fiber {
            children.push(fiber.callable);
            children.extend(fiber.return_value);
        }
        let released_generator_state = released_generator
            .as_ref()
            .and_then(|generator| generator.handle.clone().zip(generator.state));
        if let Some(generator) = released_generator {
            if generator.lifecycle == php_runtime::api::GeneratorState::Created {
                children.extend(generator.arguments);
            }
            children.extend(generator.current_key);
            children.extend(generator.current_value);
            children.extend(generator.return_value);
            if let Some(delegation) = generator.delegation {
                children.push(match delegation {
                    NativeGeneratorDelegation::Array { source, .. } => source,
                    NativeGeneratorDelegation::Generator { generator } => generator,
                });
            }
        }
        children.extend(direct_object_children);
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            flags: slot.kind,
            ..php_jit::JitNativeValueSlot::default()
        };
        self.direct_array_states[index] = php_jit::JitNativeDirectArrayState::default();
        self.baseline_values.direct_reference_cells.remove(&index);
        self.baseline_values
            .materialized_direct_references
            .retain(|candidate| *candidate != index);
        if let Some((start, capacity)) = freed_array_range {
            self.free_direct_array_entries(start, capacity);
        }
        if let Some((start, capacity)) = freed_string_range {
            self.free_direct_string_bytes(start, capacity);
        }
        for child in children {
            self.release(child)?;
        }
        if let Some((handle, state)) = released_generator_state {
            self.release_native_suspension_owners(&handle, &state)?;
        }
        drop(released_object);
        drop(released_resource);
        drop(released_cold_generator);
        let index = u32::try_from(index)
            .map_err(|_| "direct native free-list index overflow".to_owned())?;
        self.direct_value_slots[index as usize].payload = u64::from(*self.direct_value_free_head);
        *self.direct_value_free_head = index;
        Ok(())
    }

    /// Retires one final direct object owner after its generated destructor
    /// has returned. Native property owners are transferred to the generated
    /// recursive release walk; this exact finalizer never invokes PHP and
    /// never recursively releases an encoded child in Rust.
    pub(super) fn finalize_generated_object_release(
        &mut self,
        encoded: i64,
    ) -> Result<Vec<i64>, String> {
        let index = Self::direct_value_index(encoded)
            .ok_or_else(|| "generated object finalizer received a non-direct value".to_owned())?;
        let slot = *self
            .direct_value_slots
            .get(index)
            .ok_or_else(|| format!("direct native object {index} is missing"))?;
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT || slot.refcount == 0 {
            return Err(format!(
                "generated object finalizer received dead/non-object slot {index}"
            ));
        }
        if slot.refcount > 1 {
            self.direct_value_slots[index].refcount -= 1;
            return Ok(Vec::new());
        }

        self.cross_unit_stable_values.remove(&index);
        let object = self
            .direct_object_owner(index)
            .ok_or_else(|| format!("direct native object {index} has no stable owner"))?;
        let has_cold_alias = object.gc_refcount_estimate() > 2;
        let children = if has_cold_alias {
            // A genuine cold/outer alias retains the Rust object identity.
            // Restore its property representation before retiring the direct
            // descriptor; no userland body is executed on this boundary.
            self.demote_direct_object_property_slots(index)?;
            Vec::new()
        } else {
            self.take_direct_object_children(index)?
        };

        let owner = std::mem::replace(&mut self.direct_object_owners[index], 0);
        if owner == 0 {
            return Err(format!(
                "direct native object {index} lost its stable owner"
            ));
        }
        // SAFETY: publication created exactly one boxed ObjectRef for this
        // direct slot and this finalizer retires it exactly once.
        #[allow(unsafe_code)]
        let released =
            unsafe { *Box::from_raw(owner as usize as *mut php_runtime::api::ObjectRef) };
        if self
            .baseline_values
            .direct_object_handles
            .get(&released.id())
            == Some(&(index as u32))
        {
            self.baseline_values
                .direct_object_handles
                .remove(&released.id());
        }
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            flags: slot.kind,
            ..php_jit::JitNativeValueSlot::default()
        };
        self.direct_array_states[index] = php_jit::JitNativeDirectArrayState::default();
        self.baseline_values.direct_reference_cells.remove(&index);
        self.baseline_values
            .materialized_direct_references
            .retain(|candidate| *candidate != index);
        drop(released);
        let free_index = u32::try_from(index)
            .map_err(|_| "direct native free-list index overflow".to_owned())?;
        self.direct_value_slots[index].payload = u64::from(*self.direct_value_free_head);
        *self.direct_value_free_head = free_index;
        Ok(children)
    }

    pub(super) fn release_if_live(&mut self, encoded: i64) -> Result<(), String> {
        if let Some(index) = Self::direct_value_index(encoded) {
            if self.direct_value_slots[index].refcount == 0 {
                return Ok(());
            }
            return self.release_direct_value_index(index);
        }
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Ok(());
        };
        Err(format!(
            "native runtime value {index} is outside the authoritative direct slot plane"
        ))
    }

    pub(super) fn object_is_request_rooted(&mut self, object_id: u64) -> bool {
        self.consume_native_root_mutation();
        if self.root_index.is_dirty() {
            let reason = self.root_index.last_reason().as_str();
            let roots = self.request_root_values();
            self.root_index.synchronize(&roots);
            self.record_object_release_root_check(false);
            self.record_root_rebuild_reason(reason);
        } else {
            self.record_object_release_root_check(true);
        }
        if self.root_index.contains(object_id) {
            return true;
        }
        self.live_native_values_contain_object(object_id)
    }

    pub(super) fn live_native_values_contain_object(&self, object_id: u64) -> bool {
        let mut visited = std::collections::HashSet::new();
        let used = usize::try_from(*self.direct_value_next).unwrap_or(0);
        (0..used).any(|index| {
            self.direct_value_slots
                .get(index)
                .is_some_and(|slot| slot.refcount != 0)
                && self.direct_slot_contains_object(index, object_id, &mut visited)
        })
    }

    pub(super) fn direct_slot_contains_object(
        &self,
        index: usize,
        object_id: u64,
        visited: &mut std::collections::HashSet<usize>,
    ) -> bool {
        if !visited.insert(index) {
            return false;
        }
        let Some(slot) = self
            .direct_value_slots
            .get(index)
            .copied()
            .filter(|slot| slot.refcount != 0)
        else {
            return false;
        };
        match slot.kind {
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT => {
                let Some(object) = self.direct_object(index) else {
                    return false;
                };
                if object.id() == object_id {
                    return true;
                }
                let mut cold_property_contains = false;
                object.visit_property_values(|value| {
                    cold_property_contains |= values_contain_object([value], object_id);
                });
                if cold_property_contains {
                    return true;
                }
                if !php_jit::jit_native_object_property_view_is_published(slot.flags) {
                    return false;
                }
                let Some((base, count)) = object.native_declared_slots_view(slot.payload) else {
                    return false;
                };
                // SAFETY: publication installs one boxed slot slice and keeps
                // it immovable until the descriptor is demoted. This scan is
                // synchronous on the owning request thread and performs no
                // mutation or cold conversion while the slice is borrowed.
                #[allow(unsafe_code)]
                let properties = unsafe { std::slice::from_raw_parts(base, count) };
                properties.iter().any(|property| {
                    property.initialized != 0
                        && self.encoded_value_contains_object(property.value, object_id, visited)
                })
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY => {
                let length = usize::try_from(slot.payload).unwrap_or(0);
                let base = self.direct_array_entries.as_ptr() as usize;
                let address = usize::try_from(slot.aux).unwrap_or(base);
                let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
                if address < base || !(address - base).is_multiple_of(entry_size) {
                    return false;
                }
                let start = (address - base) / entry_size;
                self.direct_array_entries
                    .get(start..start.saturating_add(length))
                    .is_some_and(|entries| {
                        entries.iter().any(|entry| {
                            self.encoded_value_contains_object(entry.key, object_id, visited)
                                || self.encoded_value_contains_object(
                                    entry.value,
                                    object_id,
                                    visited,
                                )
                        })
                    })
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FOREACH => {
                self.encoded_value_contains_object(slot.payload as i64, object_id, visited)
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE => {
                self.direct_prepared_callable_view(index)
                    .is_some_and(|view| {
                        if view.kind == php_jit::JIT_NATIVE_CALLABLE_KIND_BOUND_OBJECT_METHOD {
                            return self.encoded_value_contains_object(
                                view.receiver,
                                object_id,
                                visited,
                            );
                        }
                        if view.kind != php_jit::JIT_NATIVE_CALLABLE_KIND_CLOSURE {
                            return false;
                        }
                        if view.flags & php_jit::JIT_NATIVE_PREPARED_CLOSURE_HAS_IMPLICIT_THIS != 0
                            && self.encoded_value_contains_object(
                                view.implicit_this,
                                object_id,
                                visited,
                            )
                        {
                            return true;
                        }
                        if view.capture_count == 0 || view.captures == 0 {
                            return false;
                        }
                        // SAFETY: the live callable owner holds the immutable
                        // capture allocation for the lifetime of this view.
                        #[allow(unsafe_code)]
                        let captures = unsafe {
                            std::slice::from_raw_parts(
                                view.captures as usize as *const i64,
                                view.capture_count as usize,
                            )
                        };
                        captures.iter().copied().any(|value| {
                            self.encoded_value_contains_object(value, object_id, visited)
                        })
                    })
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER
            | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_FIBER => {
                let native_contains = self.fiber_record(index).is_some_and(|fiber| {
                    self.encoded_value_contains_object(fiber.callable, object_id, visited)
                        || fiber.return_value.is_some_and(|value| {
                            self.encoded_value_contains_object(value, object_id, visited)
                        })
                });
                native_contains
                    || self
                        .baseline_values
                        .direct_fiber_cells
                        .get(&index)
                        .is_some_and(|fiber| {
                            let callable = fiber.callable();
                            values_contain_object([&callable], object_id)
                                || fiber
                                    .return_value()
                                    .is_some_and(|value| values_contain_object([&value], object_id))
                        })
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_GENERATOR
            | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_GENERATOR => {
                self.direct_generator(index).is_some_and(|generator| {
                    generator
                        .arguments
                        .iter()
                        .copied()
                        .any(|value| self.encoded_value_contains_object(value, object_id, visited))
                        || generator.current_key.is_some_and(|value| {
                            self.encoded_value_contains_object(value, object_id, visited)
                        })
                        || generator.current_value.is_some_and(|value| {
                            self.encoded_value_contains_object(value, object_id, visited)
                        })
                        || generator.return_value.is_some_and(|value| {
                            self.encoded_value_contains_object(value, object_id, visited)
                        })
                        || generator.delegation.as_ref().is_some_and(|delegation| {
                            let value = match delegation {
                                NativeGeneratorDelegation::Array { source, .. } => *source,
                                NativeGeneratorDelegation::Generator { generator } => *generator,
                            };
                            self.encoded_value_contains_object(value, object_id, visited)
                        })
                        || generator.state.as_ref().is_some_and(|state| {
                            state
                                .slots
                                .iter()
                                .take(state.slot_count as usize)
                                .enumerate()
                                .any(|(index, value)| {
                                    state.local_initialized(php_ir::LocalId::new(
                                        u32::try_from(index).unwrap_or(u32::MAX),
                                    )) && self
                                        .encoded_value_contains_object(*value, object_id, visited)
                                })
                                || state.registers.iter().enumerate().any(|(index, value)| {
                                    state.initialized_register_mask & (1_u64 << index) != 0
                                        && self.encoded_value_contains_object(
                                            *value, object_id, visited,
                                        )
                                })
                        })
                })
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_FOREACH_DIRECT
            | php_jit::JIT_NATIVE_VALUE_VIEW_COLD_ITERATOR => false,
            php_jit::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY
            | php_jit::JIT_NATIVE_VALUE_VIEW_BORROWED_REFERENCE_ARRAY => {
                baseline_shared_array_storage_contains_object(slot.payload as usize, object_id)
            }
            _ => false,
        }
    }

    pub(super) fn encoded_value_contains_object(
        &self,
        encoded: i64,
        object_id: u64,
        visited: &mut std::collections::HashSet<usize>,
    ) -> bool {
        if let Some(index) = Self::direct_value_index(encoded) {
            return self.direct_slot_contains_object(index, object_id, visited);
        }
        false
    }

    pub(super) fn enter_generated_shutdown_destructor(
        &mut self,
        object: php_runtime::api::ObjectRef,
    ) -> Result<(), String> {
        if self
            .destroyed_objects
            .get(&object.id())
            .is_some_and(WeakObjectHandle::is_alive)
        {
            return Ok(());
        }
        let class_name = object.class_name();
        let has_destructor = self
            .unit
            .classes
            .iter()
            .find(|class| class.name == normalize_class_name(&class_name))
            .and_then(|class| {
                class
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("__destruct"))
            })
            .is_some()
            || native_external_method(self, &class_name, "__destruct").is_some();
        if !has_destructor {
            return Ok(());
        }
        self.destroyed_objects
            .insert(object.id(), object.weak_handle());
        let receiver = self.encode_native_object_owner(object)?;
        let runtime = self.native_runtime_ptr();
        // SAFETY: the request owns the fast state for this synchronous cold
        // action preparation. Resolution publishes a stable callable but
        // never invokes the destructor body.
        #[allow(unsafe_code)]
        let callable = match unsafe { &mut *runtime.cast::<NativeRequestFastState>() }
            .acquire_direct_method_callable(receiver, b"__destruct", self.unit.entry.raw(), true)
        {
            Ok(NativeMethodCallableResolution::Ready(callable)) => Ok(callable),
            Ok(NativeMethodCallableResolution::InvokeUserCallback(_)) => {
                Err("destructor resolution unexpectedly requested autoload".to_owned())
            }
            Ok(NativeMethodCallableResolution::NotFound) => {
                Err(format!("{class_name}::__destruct() is unavailable"))
            }
            Err(error) => Err(error.to_owned()),
        };
        let invoked = callable.and_then(|callable| {
            let result = self
                .enter_generated_callback_continuation(callable, &[])
                .map_err(NativeCallControl::into_baseline_error)
                .and_then(|returned| self.release_if_live(returned));
            let released = self.release_if_live(callable);
            result.and(released)
        });
        let receiver_released = self.release_if_live(receiver);
        invoked.and(receiver_released)
    }

    pub(super) fn object_has_native_destructor(&self, class_name: &str) -> bool {
        self.unit
            .classes
            .iter()
            .find(|class| class.name == normalize_class_name(class_name))
            .is_some_and(|class| {
                class
                    .methods
                    .iter()
                    .any(|method| method.name.eq_ignore_ascii_case("__destruct"))
            })
            || native_external_method(self, class_name, "__destruct").is_some()
    }

    pub(super) fn function_id(&self, name: &str) -> Option<php_ir::FunctionId> {
        self.unit
            .function_table
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
            .map(|entry| entry.function)
            .or_else(|| {
                self.dynamic_functions.get(name).copied().or_else(|| {
                    name.bytes()
                        .any(|byte| byte.is_ascii_uppercase())
                        .then(|| name.to_ascii_lowercase())
                        .and_then(|normalized| self.dynamic_functions.get(&normalized).copied())
                })
            })
    }

    pub(super) fn publish_function_names(&mut self, names: impl IntoIterator<Item = String>) {
        self.visible_function_names =
            NativeFunctionNameScope::child(self.visible_function_names.clone(), names);
    }

    pub(super) fn demote_all_direct_objects(&mut self) -> Result<(), String> {
        let native_objects = (0..usize::try_from(*self.direct_value_next).unwrap_or(0))
            .filter_map(|index| {
                self.direct_value_slots
                    .get(index)
                    .is_some_and(|slot| {
                        slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
                            && php_jit::jit_native_object_property_view_is_published(slot.flags)
                    })
                    .then(|| self.direct_object_owner(index))
                    .flatten()
                    .filter(|object| {
                        object
                            .native_declared_slots_view(object.class_layout_epoch())
                            .is_some()
                    })
            })
            .collect::<Vec<_>>();
        for object in native_objects {
            self.materialize_direct_object_alias(&object)?;
        }
        Ok(())
    }

    pub(super) fn take_include_symbols(&mut self) -> Result<NativeIncludeSymbols, String> {
        self.demote_trusted_static_properties();
        self.materialize_trusted_static_locals()?;
        // Include/eval hands Rust-owned request state to a separately owned
        // native arena. No ObjectRef crossing that ownership boundary may
        // retain declared-property slots encoded against this arena.
        self.demote_all_direct_objects()?;
        self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
        let NativeRegisteredCallbackTransfer {
            autoload_callbacks,
            shutdown_callbacks,
            error_handlers,
            exception_handlers,
        } = self.take_registered_callback_transfer()?;
        Ok(NativeIncludeSymbols {
            deployment_functions: std::sync::Arc::clone(&self.deployment_functions),
            deployment_classes: std::sync::Arc::clone(&self.deployment_classes),
            external_functions: std::mem::take(&mut self.external_functions),
            external_class_units: std::mem::take(&mut self.external_class_units),
            external_signature_epoch: self.external_signature_epoch,
            dynamic_units: std::mem::take(&mut self.dynamic_units),
            dynamic_classes: std::mem::take(&mut self.dynamic_classes),
            class_aliases: std::mem::take(&mut self.class_aliases),
            autoload_callbacks,
            shutdown_callbacks,
            static_property_transfer: std::mem::take(
                &mut self.baseline_values.static_property_transfer,
            ),
            typed_static_reference_constraints: std::mem::take(
                &mut self.typed_static_reference_constraints,
            ),
            static_locals: std::mem::take(&mut self.baseline_values.static_locals),
            enum_cases: std::mem::take(&mut self.baseline_values.enum_cases),
            destroyed_objects: std::mem::take(&mut self.destroyed_objects),
            error_reporting: Some(self.error_reporting),
            display_errors: Some(self.display_errors),
            error_handlers,
            exception_handlers,
            last_error: self.last_error.take(),
        })
    }

    pub(super) fn detach_transient_include_unit(&mut self) -> Result<(), String> {
        if !self.include_child {
            return Ok(());
        }
        let unit = self
            .current_dynamic_unit
            .take()
            .ok_or_else(|| "include execution unit was not attached".to_owned())?;
        if unit + 1 != self.dynamic_units.len()
            || self.dynamic_units.get(unit).is_none_or(|package| {
                package.compiled.artifact_identity() != self.compiled.artifact_identity()
            })
        {
            self.current_dynamic_unit = Some(unit);
            return Err("include execution unit publication is inconsistent".to_owned());
        }
        self.dynamic_units
            .pop()
            .ok_or_else(|| "include execution unit disappeared".to_owned())?;
        cold_dynamic_units::refresh_linked_function_records(self);
        Ok(())
    }

    pub(super) fn external_function(&self, name: &str) -> Option<NativeDynamicFunction> {
        self.external_functions.get(name).copied().or_else(|| {
            let normalized = name
                .bytes()
                .any(|byte| byte.is_ascii_uppercase())
                .then(|| name.to_ascii_lowercase());
            normalized
                .as_deref()
                .and_then(|normalized| self.external_functions.get(normalized).copied())
                .or_else(|| {
                    let normalized = normalized.as_deref().unwrap_or(name);
                    self.deployment_functions
                        .get(normalized)
                        .copied()
                        .map(|function| NativeDynamicFunction { unit: 0, function })
                })
        })
    }

    pub(super) fn stabilize_active_dynamic_global_roots(
        &mut self,
        unit: usize,
    ) -> Result<(), String> {
        let names = self
            .dynamic_units
            .get(unit)
            .map(|package| package.cross_unit_global_names.clone())
            .ok_or_else(|| "dynamic native unit is missing".to_owned())?;
        let mut roots = names
            .iter()
            .filter_map(|name| self.native_global_reference_handles.get(name).copied())
            .collect::<Vec<_>>();
        roots.sort_unstable();
        roots.dedup();
        self.stabilize_owned_native_values_for_cross_unit(&mut roots)
    }

    pub(super) fn replace_active_unit_runtime_state(
        &mut self,
        replacement: NativeUnitRuntimeState,
    ) -> NativeUnitRuntimeState {
        let NativeUnitRuntimeState {
            prepared_native_metadata_functions,
            trusted_request_local_function_offsets,
            trusted_request_local_slots,
            trusted_property_function_offsets,
            trusted_property_slots,
            trusted_closure_plans,
            trusted_exception_plans,
            trusted_exception_plan_owners,
            trusted_constant_slots,
            trusted_dynamic_constant_sites,
            trusted_global_reference_slots,
            trusted_global_reference_names,
            trusted_static_local_slots,
            trusted_static_property_slots,
            trusted_instanceof_plans,
            trusted_instanceof_entries,
            trusted_exception_route_plans,
            trusted_exception_route_entries,
            trusted_exception_route_symbol_epoch,
            trusted_class_plans,
        } = replacement;
        NativeUnitRuntimeState {
            prepared_native_metadata_functions: std::mem::replace(
                &mut self.prepared_native_metadata_functions,
                prepared_native_metadata_functions,
            ),
            trusted_request_local_function_offsets: std::mem::replace(
                &mut self.trusted_request_local_function_offsets,
                trusted_request_local_function_offsets,
            ),
            trusted_request_local_slots: std::mem::replace(
                &mut self.trusted_request_local_slots,
                trusted_request_local_slots,
            ),
            trusted_property_function_offsets: std::mem::replace(
                &mut self.trusted_property_function_offsets,
                trusted_property_function_offsets,
            ),
            trusted_property_slots: std::mem::replace(
                &mut self.trusted_property_slots,
                trusted_property_slots,
            ),
            trusted_closure_plans: std::mem::replace(
                &mut self.trusted_closure_plans,
                trusted_closure_plans,
            ),
            trusted_exception_plans: std::mem::replace(
                &mut self.trusted_exception_plans,
                trusted_exception_plans,
            ),
            trusted_exception_plan_owners: std::mem::replace(
                &mut self.trusted_exception_plan_owners,
                trusted_exception_plan_owners,
            ),
            trusted_constant_slots: std::mem::replace(
                &mut self.trusted_constant_slots,
                trusted_constant_slots,
            ),
            trusted_dynamic_constant_sites: std::mem::replace(
                &mut self.trusted_dynamic_constant_sites,
                trusted_dynamic_constant_sites,
            ),
            trusted_global_reference_slots: std::mem::replace(
                &mut self.trusted_global_reference_slots,
                trusted_global_reference_slots,
            ),
            trusted_global_reference_names: std::mem::replace(
                &mut self.trusted_global_reference_names,
                trusted_global_reference_names,
            ),
            trusted_static_local_slots: std::mem::replace(
                &mut self.trusted_static_local_slots,
                trusted_static_local_slots,
            ),
            trusted_static_property_slots: std::mem::replace(
                &mut self.trusted_static_property_slots,
                trusted_static_property_slots,
            ),
            trusted_instanceof_plans: std::mem::replace(
                &mut self.trusted_instanceof_plans,
                trusted_instanceof_plans,
            ),
            trusted_instanceof_entries: std::mem::replace(
                &mut self.trusted_instanceof_entries,
                trusted_instanceof_entries,
            ),
            trusted_exception_route_plans: std::mem::replace(
                &mut self.trusted_exception_route_plans,
                trusted_exception_route_plans,
            ),
            trusted_exception_route_entries: std::mem::replace(
                &mut self.trusted_exception_route_entries,
                trusted_exception_route_entries,
            ),
            trusted_exception_route_symbol_epoch: std::mem::replace(
                &mut self.trusted_exception_route_symbol_epoch,
                trusted_exception_route_symbol_epoch,
            ),
            trusted_class_plans: std::mem::replace(
                &mut self.trusted_class_plans,
                trusted_class_plans,
            ),
        }
    }

    pub(super) fn republish_transferred_dynamic_units(&mut self) -> Result<(), String> {
        let dependencies = cold_dynamic_units::transferred_link_dependencies(self);
        for (unit, functions) in dependencies {
            self.native_metadata_preparation_scope = Some(functions.into_iter().collect());
            let published = self.with_active_dynamic_unit(unit, None, |_| ());
            self.native_metadata_preparation_scope = None;
            published?;
        }
        cold_dynamic_units::refresh_linked_function_records(self);
        Ok(())
    }

    pub(super) fn with_active_dynamic_unit<R>(
        &mut self,
        unit: usize,
        request_local_bindings: Option<&[(String, i64)]>,
        operation: impl FnOnce(&mut Self) -> R,
    ) -> Result<R, String> {
        if self.current_dynamic_unit == Some(unit)
            && self.dynamic_units.get(unit).is_some_and(|package| {
                package.compiled.artifact_identity() == self.compiled.artifact_identity()
            })
        {
            let previous_execution_scope = self.current_native_execution_scope;
            let active_scope_matches = usize::try_from(previous_execution_scope)
                .ok()
                .and_then(|identity| identity.checked_sub(1))
                .and_then(|index| self.native_execution_scopes.get(index))
                .is_some_and(|scope| scope.unit == Some(unit));
            if !active_scope_matches {
                let mut scope = usize::try_from(previous_execution_scope)
                    .ok()
                    .and_then(|identity| identity.checked_sub(1))
                    .and_then(|index| self.native_execution_scopes.get(index))
                    .map_or(
                        NativeExecutionScope {
                            unit: Some(unit),
                            called_class: None,
                            scope_class: None,
                        },
                        |scope| scope.as_ref().clone(),
                    );
                scope.unit = Some(unit);
                self.current_native_execution_scope =
                    self.register_native_execution_scope(scope)?;
            }
            let binding_result = request_local_bindings.map_or(Ok(()), |bindings| {
                self.publish_active_entry_request_local_bindings(bindings)
            });
            let _runtime_view = activate_native_context(self);
            let result = binding_result.map(|()| operation(self));
            self.current_native_execution_scope = previous_execution_scope;
            return result;
        }
        let (compiled, active_entries, active_runtime_state) = {
            let package = self
                .dynamic_units
                .get_mut(unit)
                .ok_or_else(|| "dynamic native unit is missing".to_owned())?;
            (
                package.compiled.clone(),
                std::mem::take(&mut package.native_entries),
                std::mem::take(&mut package.runtime_state),
            )
        };
        let previous_dynamic_unit = self.current_dynamic_unit;
        let previous_execution_scope = self.current_native_execution_scope;
        let previous_compiled = std::mem::replace(&mut self.compiled, compiled.clone());
        let previous_unit = std::mem::replace(&mut self.unit, ActiveNativeUnit::new(&compiled));
        let previous_identity =
            std::mem::replace(&mut self.unit_identity, compiled.artifact_identity());
        let previous_entries = std::mem::replace(&mut self.native_entries, active_entries);
        let previous_runtime_state = self.replace_active_unit_runtime_state(active_runtime_state);
        let mut detached_previous = Some((previous_entries, previous_runtime_state));
        if let Some(previous) = previous_dynamic_unit {
            let (previous_entries, previous_runtime_state) = detached_previous
                .take()
                .expect("previous active native unit state was already stored");
            let package = self
                .dynamic_units
                .get_mut(previous)
                .ok_or_else(|| "active native unit package is missing".to_owned())?;
            package.native_entries = previous_entries;
            package.runtime_state = previous_runtime_state;
        }
        self.current_dynamic_unit = Some(unit);
        let active_scope_matches = usize::try_from(self.current_native_execution_scope)
            .ok()
            .and_then(|identity| identity.checked_sub(1))
            .and_then(|index| self.native_execution_scopes.get(index))
            .is_some_and(|scope| scope.unit == Some(unit));
        if !active_scope_matches {
            let mut scope = usize::try_from(self.current_native_execution_scope)
                .ok()
                .and_then(|identity| identity.checked_sub(1))
                .and_then(|index| self.native_execution_scopes.get(index))
                .map_or(
                    NativeExecutionScope {
                        unit: Some(unit),
                        called_class: None,
                        scope_class: None,
                    },
                    |scope| scope.as_ref().clone(),
                );
            scope.unit = Some(unit);
            self.current_native_execution_scope = self.register_native_execution_scope(scope)?;
        }
        let metadata_current = self.native_metadata_preparation_scope.is_none()
            && self
                .all_published_native_functions()
                .iter()
                .all(|function| self.prepared_native_metadata_functions.contains(function))
            && self.trusted_exception_route_symbol_epoch == self.external_signature_epoch;
        if !metadata_current {
            self.prepare_trusted_literal_slots();
            self.prepare_trusted_closure_plans();
            self.prepare_trusted_exception_plans();
            self.prepare_trusted_static_properties();
            self.prepare_trusted_constant_fetches();
            self.prepare_trusted_request_locals();
        }
        let binding_result = request_local_bindings.map_or(Ok(()), |bindings| {
            self.publish_active_entry_request_local_bindings(bindings)
        });
        let global_binding_result = if metadata_current {
            Ok(())
        } else {
            let result = self.prepare_trusted_global_references();
            self.prepare_trusted_static_locals();
            self.prepare_trusted_class_plans();
            self.prepare_trusted_declared_properties();
            self.prepare_trusted_instanceof_plans();
            self.prepare_trusted_exception_routes();
            let prepared_functions = self.published_native_functions();
            self.prepared_native_metadata_functions
                .extend(prepared_functions);
            result
        };

        // Native code in an included/eval unit uses that unit's dense trusted
        // function-cell table. The outer request activation describes the
        // root deployment; refresh the by-value runtime view for the scoped
        // unit before constructing any nested JitDeoptState. Without this,
        // FunctionId N from an include indexed root FunctionId N and could
        // indirect-call arbitrary data as an address.
        let _runtime_view = activate_native_context(self);
        let result = binding_result
            .and(global_binding_result)
            .map(|()| operation(self));
        let root_stabilization = if result.is_ok() {
            self.stabilize_active_dynamic_global_roots(unit)
        } else {
            Ok(())
        };

        let active_runtime_state =
            self.replace_active_unit_runtime_state(NativeUnitRuntimeState::default());
        let active_entries = std::mem::take(&mut self.native_entries);
        {
            let package = self
                .dynamic_units
                .get_mut(unit)
                .expect("active dynamic native unit disappeared");
            package.native_entries = active_entries;
            package.runtime_state = active_runtime_state;
        }
        match previous_dynamic_unit {
            Some(previous) => {
                let (previous_entries, previous_runtime_state) = {
                    let package = self
                        .dynamic_units
                        .get_mut(previous)
                        .expect("previous native unit package disappeared");
                    (
                        std::mem::take(&mut package.native_entries),
                        std::mem::take(&mut package.runtime_state),
                    )
                };
                self.native_entries = previous_entries;
                let empty = self.replace_active_unit_runtime_state(previous_runtime_state);
                debug_assert!(
                    empty.trusted_property_function_offsets.is_empty(),
                    "inactive native unit left an unexpected runtime state installed"
                );
            }
            None => {
                let (previous_entries, previous_runtime_state) = detached_previous
                    .take()
                    .expect("detached native unit state is missing");
                self.native_entries = previous_entries;
                let empty = self.replace_active_unit_runtime_state(previous_runtime_state);
                debug_assert!(
                    empty.trusted_property_function_offsets.is_empty(),
                    "inactive native unit left an unexpected runtime state installed"
                );
            }
        }
        self.current_dynamic_unit = previous_dynamic_unit;
        self.current_native_execution_scope = previous_execution_scope;
        self.unit_identity = previous_identity;
        self.unit = previous_unit;
        self.compiled = previous_compiled;
        if self.trusted_exception_route_symbol_epoch != self.external_signature_epoch {
            self.prepare_trusted_exception_routes();
        }
        root_stabilization?;
        result
    }

    pub(super) fn publish_active_entry_request_local_bindings(
        &mut self,
        bindings: &[(String, i64)],
    ) -> Result<(), String> {
        let entry = self.unit.entry;
        let locals = self
            .unit
            .functions
            .get(entry.index())
            .map(|function| function.locals.clone())
            .ok_or_else(|| "dynamic unit entry function is missing".to_owned())?;
        let base = self
            .trusted_request_local_function_offsets
            .get(entry.index())
            .copied()
            .and_then(|base| usize::try_from(base).ok())
            .ok_or_else(|| "dynamic unit entry local slots are missing".to_owned())?;
        for (name, encoded) in bindings {
            let Some(local) = locals.iter().position(|candidate| candidate == name) else {
                continue;
            };
            if self.php_handle_is_reference(*encoded) != Some(true) {
                return Err(format!(
                    "dynamic unit entry local ${name} has no native reference identity"
                ));
            }
            let index = base
                .checked_add(local)
                .ok_or_else(|| "dynamic unit entry local slot overflow".to_owned())?;
            let previous = self
                .trusted_request_local_slots
                .get(index)
                .copied()
                .ok_or_else(|| format!("dynamic unit entry local ${name} slot is missing"))?;
            self.retain(*encoded)?;
            self.trusted_request_local_slots[index] = php_jit::JitNativeRequestLocalSlot {
                encoded: *encoded,
                state: php_jit::JIT_NATIVE_REQUEST_LOCAL_PUBLISHED,
                reserved: 0,
            };
            if previous.state == php_jit::JIT_NATIVE_REQUEST_LOCAL_PUBLISHED
                && let Err(error) = self.release(previous.encoded)
            {
                return Err(error);
            }
        }
        Ok(())
    }

    pub(super) fn direct_array_slot(
        &self,
        encoded: i64,
    ) -> Option<(usize, php_jit::JitNativeValueSlot)> {
        let index = Self::direct_value_index(encoded)?;
        let slot = *self.direct_value_slots.get(index)?;
        (slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY)
            .then_some((index, slot))
    }

    /// Resolves a direct-reference chain to its authoritative direct array.
    /// The returned encoding owns no additional reference; callers either
    /// borrow entries or retain the selected child explicitly.
    pub(super) fn direct_array_encoding(&self, encoded: i64) -> Option<i64> {
        let encoded = self.dereference_direct_encoding(encoded);
        self.direct_array_slot(encoded).map(|_| encoded)
    }

    pub(super) fn direct_array_entry_range(&self, encoded: i64) -> Option<(usize, usize)> {
        let encoded = self.dereference_direct_encoding(encoded);
        let (_, slot) = self.direct_array_slot(encoded)?;
        let length = usize::try_from(slot.payload).ok()?;
        let base = self.direct_array_entries.as_ptr() as usize;
        let address = usize::try_from(slot.aux).ok()?;
        let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
        let offset = address.checked_sub(base)?;
        (offset % entry_size == 0).then_some(())?;
        let start = offset / entry_size;
        (start.checked_add(length)? <= self.direct_array_entries.len()).then_some((start, length))
    }

    pub(super) fn direct_array_entries_for(
        &self,
        encoded: i64,
    ) -> Option<&[php_jit::JitNativeDirectArrayEntry]> {
        let (start, length) = self.direct_array_entry_range(encoded)?;
        self.direct_array_entries
            .get(start..start.checked_add(length)?)
    }

    /// Reads one authoritative direct-array entry without copying the whole
    /// array into a temporary compatibility vector.
    ///
    /// Call binding may need mutable access to the request while it walks an
    /// argument array (warnings, reference preparation, or target dispatch).
    /// Returning the plain ABI record by value keeps no slice borrow alive
    /// across those operations and preserves the stable native array as the
    /// only argument representation.
    pub(super) fn direct_array_entry_at(
        &self,
        start: usize,
        index: usize,
    ) -> php_jit::JitNativeDirectArrayEntry {
        self.direct_array_entries[start + index]
    }

    /// Rewrites unit-indexed constants embedded in an authoritative native
    /// ownership graph before that graph crosses an IR-unit boundary.
    ///
    /// Arrays are not the only possible carrier: references, declared object
    /// slots, and prepared closure captures can all own an array (or a literal)
    /// that is later read in another unit. Walk those native owners in place;
    /// no Rust `Value`, `PhpArray`, or compatibility facade participates.
    pub(super) fn stabilize_direct_array_for_cross_unit(
        &mut self,
        encoded: i64,
    ) -> Result<(), String> {
        let mut visited = std::collections::BTreeSet::new();
        self.stabilize_cross_unit_graph_value(encoded, &mut visited)?;
        Ok(())
    }

    pub(super) fn stabilize_cross_unit_graph_value(
        &mut self,
        encoded: i64,
        visited: &mut std::collections::BTreeSet<usize>,
    ) -> Result<i64, String> {
        self.consume_native_root_mutation();
        let encoded = self.stabilize_cross_unit_value(encoded)?;
        let Some(index) = Self::direct_value_index(encoded) else {
            return Ok(encoded);
        };
        if self.cross_unit_stable_values.contains(&index) {
            return Ok(encoded);
        }
        if !visited.insert(index) {
            return Ok(encoded);
        }
        let slot = self
            .direct_value_slots
            .get(index)
            .copied()
            .filter(|slot| slot.refcount != 0)
            .ok_or_else(|| format!("direct native value {index} is missing"))?;
        match slot.kind {
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY => {
                let length = usize::try_from(slot.payload)
                    .map_err(|_| format!("direct native array {index} length overflow"))?;
                let base = self.direct_array_entries.as_ptr() as usize;
                let address = usize::try_from(slot.aux)
                    .map_err(|_| format!("direct native array {index} address overflow"))?;
                let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
                let offset = address
                    .checked_sub(base)
                    .ok_or_else(|| format!("direct native array {index} is outside its arena"))?;
                if offset % entry_size != 0 {
                    return Err(format!("direct native array {index} address is unaligned"));
                }
                let start = offset / entry_size;
                let end = start
                    .checked_add(length)
                    .ok_or_else(|| format!("direct native array {index} range overflow"))?;
                if end > self.direct_array_entries.len() {
                    return Err(format!(
                        "direct native array {index} entries are outside its arena"
                    ));
                }
                for entry_index in start..end {
                    let entry = self.direct_array_entries[entry_index];
                    let key = self.stabilize_cross_unit_graph_value(entry.key, visited)?;
                    let value = self.stabilize_cross_unit_graph_value(entry.value, visited)?;
                    self.direct_array_entries[entry_index] =
                        php_jit::JitNativeDirectArrayEntry { key, value };
                }
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                if slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
                    && native_reference_state(slot.reserved)
                        != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY =>
            {
                let payload =
                    self.stabilize_cross_unit_graph_value(slot.payload as i64, visited)?;
                self.direct_value_slots[index].payload = payload as u64;
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
                if php_jit::jit_native_object_property_view_is_published(slot.flags) =>
            {
                let object = self
                    .direct_object(index)
                    .ok_or_else(|| format!("direct native object {index} has no stable owner"))?;
                let (base, count) =
                    object
                        .native_declared_slots_view(slot.payload)
                        .ok_or_else(|| {
                            format!("direct native object {index} lost its declared slots")
                        })?;
                for property_index in 0..count {
                    // SAFETY: the object owns one immovable native slot slice
                    // for this layout. This request-thread walk neither
                    // demotes the object nor changes the slice allocation.
                    #[allow(unsafe_code)]
                    let property = unsafe { *base.add(property_index) };
                    if property.initialized == 0 {
                        continue;
                    }
                    let value = self.stabilize_cross_unit_graph_value(property.value, visited)?;
                    #[allow(unsafe_code)]
                    unsafe {
                        *base.add(property_index) =
                            php_runtime::api::NativeDeclaredPropertySlot { value, ..property };
                    }
                }
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE => {
                let view = self.direct_prepared_callable_view(index).copied();
                let children = view.map_or_else(Vec::new, |view| {
                    if view.kind == php_jit::JIT_NATIVE_CALLABLE_KIND_BOUND_OBJECT_METHOD {
                        return vec![view.receiver];
                    }
                    if view.kind != php_jit::JIT_NATIVE_CALLABLE_KIND_CLOSURE {
                        return Vec::new();
                    }
                    let mut children = Vec::with_capacity(
                        view.capture_count as usize
                            + usize::from(
                                view.flags & php_jit::JIT_NATIVE_PREPARED_CLOSURE_HAS_IMPLICIT_THIS
                                    != 0,
                            ),
                    );
                    if view.flags & php_jit::JIT_NATIVE_PREPARED_CLOSURE_HAS_IMPLICIT_THIS != 0 {
                        children.push(view.implicit_this);
                    }
                    if view.capture_count != 0 && view.captures != 0 {
                        // SAFETY: the live callable owner holds this immutable
                        // capture allocation until final slot release.
                        #[allow(unsafe_code)]
                        let captures = unsafe {
                            std::slice::from_raw_parts(
                                view.captures as usize as *const i64,
                                view.capture_count as usize,
                            )
                        };
                        children.extend_from_slice(captures);
                    }
                    children
                });
                let stabilized = children
                    .into_iter()
                    .map(|value| self.stabilize_cross_unit_graph_value(value, visited))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut published_implicit_this = None;
                let published_receiver = view
                    .filter(|view| {
                        view.kind == php_jit::JIT_NATIVE_CALLABLE_KIND_BOUND_OBJECT_METHOD
                    })
                    .and_then(|_| stabilized.first().copied());
                if view.is_some_and(|view| view.kind == php_jit::JIT_NATIVE_CALLABLE_KIND_CLOSURE)
                    && let Some(closure) = self.direct_prepared_closure_mut(index)
                {
                    let mut values = stabilized.into_iter();
                    if closure.implicit_this.is_some() {
                        closure.implicit_this = values.next();
                    }
                    for capture in &mut closure.captures {
                        *capture = values
                            .next()
                            .expect("closure capture stabilization kept its arity");
                    }
                    closure.native_view.implicit_this =
                        closure.implicit_this.unwrap_or_else(|| {
                            php_jit::jit_encode_constant(php_jit::JIT_VALUE_UNINITIALIZED)
                        });
                    published_implicit_this = Some(closure.native_view.implicit_this);
                }
                if published_implicit_this.is_some() || published_receiver.is_some() {
                    let owner = slot.aux as usize as *mut NativePreparedCallableOwner;
                    // SAFETY: this is the same request-owned record validated
                    // by direct_prepared_callable_mut above. The authoritative
                    // C view must be refreshed together with its explicitly
                    // cold Closure metadata.
                    #[allow(unsafe_code)]
                    unsafe {
                        if let Some(implicit_this) = published_implicit_this {
                            (*owner).native_view.implicit_this = implicit_this;
                        }
                        if let Some(receiver) = published_receiver {
                            (*owner).native_view.receiver = receiver;
                        }
                    }
                }
            }
            _ => {}
        }
        self.cross_unit_stable_values.insert(index);
        Ok(encoded)
    }

    /// Rehomes only unit-indexed immediates before already-owned native frame
    /// values cross into another IR unit. Direct values keep the same owner;
    /// direct arrays keep the same COW identity while their embedded constant
    /// indexes are stabilized in place.
    pub(super) fn stabilize_owned_native_values_for_cross_unit(
        &mut self,
        values: &mut [i64],
    ) -> Result<(), String> {
        for encoded in values {
            let unit_local_constant = php_jit::jit_decode_constant(*encoded).is_some_and(|index| {
                index != u32::MAX
                    && index != php_jit::JIT_VALUE_UNINITIALIZED
                    && index != php_jit::JIT_VALUE_FALSE
                    && index != php_jit::JIT_VALUE_TRUE
            });
            if unit_local_constant || Self::direct_value_index(*encoded).is_some() {
                let mut visited = std::collections::BTreeSet::new();
                *encoded = self.stabilize_cross_unit_graph_value(*encoded, &mut visited)?;
            }
        }
        Ok(())
    }

    pub(super) fn stabilize_cross_unit_value(&mut self, encoded: i64) -> Result<i64, String> {
        let Some(constant) = php_jit::jit_decode_constant(encoded) else {
            return Ok(encoded);
        };
        if matches!(
            constant,
            u32::MAX
                | php_jit::JIT_VALUE_UNINITIALIZED
                | php_jit::JIT_VALUE_FALSE
                | php_jit::JIT_VALUE_TRUE
        ) {
            return Ok(encoded);
        }
        self.stabilize_active_unit_constant(constant)
    }

    pub(super) fn direct_array_length(&self, encoded: i64) -> Option<usize> {
        self.direct_array_entries_for(encoded).map(<[_]>::len)
    }

    pub(super) fn direct_array_is_unique(&self, encoded: i64) -> Option<bool> {
        self.direct_array_slot(encoded)
            .map(|(_, slot)| slot.refcount == 1)
    }

    pub(super) fn direct_array_can_append(&self, encoded: i64) -> Option<bool> {
        let (index, _) = self.direct_array_slot(encoded)?;
        let state = self.direct_array_states.get(index)?;
        let next = if state.has_next_append_key != 0 {
            state.next_append_key
        } else {
            0
        };
        if next != i64::MAX {
            return Some(true);
        }
        Some(
            !self
                .direct_array_entries_for(encoded)?
                .iter()
                .any(|entry| self.native_encoded_int(entry.key) == Some(i64::MAX)),
        )
    }

    pub(super) fn fresh_direct_array_next_append_key(
        &self,
        entries: &[php_jit::JitNativeDirectArrayEntry],
    ) -> Option<i64> {
        entries
            .iter()
            .filter_map(|entry| self.native_encoded_int(entry.key))
            .map(|key| key.saturating_add(1))
            .max()
    }

    pub(super) fn direct_array_find_encoded(
        &self,
        encoded: i64,
        key: &php_runtime::api::ArrayKey,
    ) -> Result<Option<i64>, String> {
        let Some(entries) = self.direct_array_entries_for(encoded) else {
            return Err("native value is not a direct array".to_owned());
        };
        Ok(entries
            .iter()
            .find(|entry| self.native_encoded_matches_array_key(entry.key, key))
            .map(|entry| entry.value))
    }

    /// Binds one entry of an authoritative direct array as a PHP reference.
    ///
    /// The direct array remains the only array representation: its entry owns
    /// one reference handle and the returned handle is an independent owner
    /// for the callee. A shared array is deliberately rejected here because
    /// its COW replacement must also update the containing lvalue.
    pub(super) fn bind_native_direct_array_element_reference(
        &mut self,
        encoded: i64,
        key: &php_runtime::api::ArrayKey,
    ) -> Result<Option<i64>, String> {
        let Some(array) = self.direct_array_encoding(encoded) else {
            return Ok(None);
        };
        if self.direct_array_is_unique(array) != Some(true) {
            return Ok(None);
        }
        if let Some(current) = self.direct_array_find_encoded(array, key)?
            && self.php_handle_is_reference(current) == Some(true)
        {
            self.retain(current)?;
            return Ok(Some(current));
        }

        let payload = self
            .direct_array_find_encoded(array, key)?
            .unwrap_or_else(|| php_jit::jit_encode_constant(u32::MAX));
        // Preserve the entry's current owner until direct_array_insert_encoded
        // has installed and retained the reference. The retained payload then
        // moves into the new reference descriptor.
        self.retain(payload)?;
        let reference = match self.encode_direct_reference_payload_owned(payload) {
            Ok(reference) => reference,
            Err(error) => {
                self.release(payload)?;
                return Err(error);
            }
        };
        if let Err(error) = self.direct_array_insert_encoded(array, Some(key), reference) {
            self.release(reference)?;
            return Err(error);
        }
        Ok(Some(reference))
    }

    /// Collapses a reference created solely for one array-walk callback.
    ///
    /// The array entry is restored only when it remains the reference's sole
    /// owner. A callback-exported alias raises the reference count and keeps
    /// the shared PHP identity intact.
    pub(super) fn collapse_native_direct_array_element_reference(
        &mut self,
        encoded: i64,
        key: &php_runtime::api::ArrayKey,
        reference: i64,
    ) -> Result<bool, String> {
        let Some(array) = self.direct_array_encoding(encoded) else {
            return Ok(false);
        };
        let Some((array_index, array_slot)) = self.direct_array_slot(array) else {
            return Ok(false);
        };
        if array_slot.refcount != 1 {
            return Ok(false);
        }
        let Some(index) = Self::direct_value_index(reference) else {
            return Ok(false);
        };
        let Some(slot) = self.direct_value_slots.get(index).copied() else {
            return Ok(false);
        };
        if slot.refcount != 1
            || slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            || slot.flags != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            || native_reference_state(slot.reserved)
                == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
        {
            return Ok(false);
        }
        let payload = slot.payload as i64;
        let (start, length) = self
            .direct_array_entry_range(array)
            .ok_or_else(|| "native array-walk cleanup lost its array entries".to_owned())?;
        let Some(entry_index) = (start..start.saturating_add(length)).find(|entry_index| {
            let entry = self.direct_array_entries[*entry_index];
            entry.value == reference && self.native_encoded_matches_array_key(entry.key, key)
        }) else {
            return Ok(false);
        };

        // Ordinary assignment to a reference-valued array entry must preserve
        // its PHP identity, so `direct_array_insert_encoded` deliberately
        // replaces the reference payload instead of the entry. This operation
        // is different: it ends the internal reference created solely for one
        // array-walk callback. Transfer the payload owner directly to the
        // array entry, then retire the now-unreachable reference descriptor.
        self.retain(payload)?;
        self.direct_array_entries[entry_index].value = payload;
        if let Err(error) = self.release(reference) {
            self.direct_array_entries[entry_index].value = reference;
            self.release(payload)?;
            return Err(error);
        }
        self.cross_unit_stable_values.remove(&array_index);
        Ok(true)
    }

    /// Publishes a newly produced native array whose entry handles are already
    /// individually owned by the caller. Ownership moves into the resulting
    /// slot; no Rust `PhpArray` or duplicate value tree is constructed.
    #[track_caller]
    pub(super) fn publish_owned_direct_array_entries(
        &mut self,
        entries: Vec<php_jit::JitNativeDirectArrayEntry>,
    ) -> Result<i64, String> {
        let next_append_key = self.fresh_direct_array_next_append_key(&entries);
        let release_entries =
            |context: &mut Self, entries: &[php_jit::JitNativeDirectArrayEntry]| {
                for entry in entries {
                    let _ = context.release(entry.key);
                    let _ = context.release(entry.value);
                }
            };
        let (start, capacity) = match self.reserve_direct_array_entries(entries.len()) {
            Ok(range) => range,
            Err(error) => {
                release_entries(self, &entries);
                return Err(error);
            }
        };
        self.direct_array_entries[start..start + entries.len()].copy_from_slice(&entries);
        let index = match self.reserve_direct_value_slot() {
            Ok(index) => index,
            Err(error) => {
                self.free_direct_array_entries(start, capacity);
                release_entries(self, &entries);
                return Err(error);
            }
        };
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
            flags: php_jit::jit_native_direct_array_flags(None),
            reserved: u32::try_from(capacity).unwrap_or(u32::MAX),
            payload: entries.len() as u64,
            aux: self.direct_array_entries[start..].as_ptr() as usize as u64,
        };
        self.direct_array_states[index] = php_jit::JitNativeDirectArrayState {
            next_append_key: next_append_key.unwrap_or(0),
            has_next_append_key: u32::from(next_append_key.is_some()),
            reserved: 0,
        };
        self.record_direct_array_materialization(entries.len(), std::panic::Location::caller());
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .ok_or_else(|| "direct native value handle overflow".to_owned())?;
        Ok((php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG | u64::from(runtime_index)) as i64)
    }

    #[track_caller]
    pub(super) fn clone_direct_array_handle(&mut self, encoded: i64) -> Result<i64, String> {
        let (_, source_slot) = self
            .direct_array_slot(encoded)
            .ok_or_else(|| "native value is not a direct array".to_owned())?;
        let source_index = Self::direct_value_index(encoded)
            .ok_or_else(|| "native value is not a direct array".to_owned())?;
        let source_state = self.direct_array_states[source_index];
        let entries = self
            .direct_array_entries_for(encoded)
            .ok_or_else(|| "direct native array entries are unavailable".to_owned())?
            .to_vec();
        let (start, capacity) = self.reserve_direct_array_entries(entries.len())?;
        let mut retained = Vec::with_capacity(entries.len() * 2);
        for entry in &entries {
            for child in [entry.key, entry.value] {
                if let Err(error) = self.retain(child) {
                    for child in retained {
                        let _ = self.release(child);
                    }
                    self.free_direct_array_entries(start, capacity);
                    return Err(error);
                }
                retained.push(child);
            }
        }
        self.direct_array_entries[start..start + entries.len()].copy_from_slice(&entries);
        let index = match self.reserve_direct_value_slot() {
            Ok(index) => index,
            Err(error) => {
                for child in retained {
                    let _ = self.release(child);
                }
                self.free_direct_array_entries(start, capacity);
                return Err(error);
            }
        };
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
            flags: source_slot.flags,
            reserved: u32::try_from(capacity).unwrap_or(u32::MAX),
            payload: entries.len() as u64,
            aux: self.direct_array_entries[start..].as_ptr() as usize as u64,
        };
        self.direct_array_states[index] = source_state;
        self.record_direct_array_materialization(entries.len(), std::panic::Location::caller());
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .ok_or_else(|| "direct native value handle overflow".to_owned())?;
        Ok((php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG | u64::from(runtime_index)) as i64)
    }

    pub(super) fn direct_array_insert_encoded(
        &mut self,
        encoded: i64,
        key: Option<&php_runtime::api::ArrayKey>,
        value: i64,
    ) -> Result<(), String> {
        let (array_index, mut slot) = self
            .direct_array_slot(encoded)
            .ok_or_else(|| "native value is not a direct array".to_owned())?;
        self.cross_unit_stable_values.remove(&array_index);
        if slot.refcount != 1 {
            return Err("direct native array write requires unique ownership".to_owned());
        }
        if key.is_none() && self.direct_array_can_append(encoded) == Some(false) {
            return Err(php_runtime::api::PHP_ARRAY_APPEND_OVERFLOW_MESSAGE.to_owned());
        }
        let length = usize::try_from(slot.payload)
            .map_err(|_| "direct native array length overflow".to_owned())?;
        let base = self.direct_array_entries.as_ptr() as usize;
        let address = usize::try_from(slot.aux)
            .map_err(|_| "direct native array address overflow".to_owned())?;
        let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
        let offset = address
            .checked_sub(base)
            .ok_or_else(|| "direct native array address is outside its arena".to_owned())?;
        if offset % entry_size != 0 {
            return Err("direct native array address is unaligned".to_owned());
        }
        let mut start = offset / entry_size;
        let normalized_key = match key {
            Some(key) => key.clone(),
            None => {
                let state = self.direct_array_states[array_index];
                php_runtime::api::ArrayKey::Int(if state.has_next_append_key != 0 {
                    state.next_append_key
                } else {
                    0
                })
            }
        };
        let entries = self
            .direct_array_entries
            .get(start..start.saturating_add(length))
            .ok_or_else(|| "direct native array entries are outside its arena".to_owned())?
            .to_vec();
        let mut existing = None;
        for (position, entry) in entries.iter().enumerate() {
            if self.native_encoded_matches_array_key(entry.key, &normalized_key) {
                existing = Some(position);
                break;
            }
        }
        if let Some(position) = existing {
            let entry_index = start + position;
            let previous = self.direct_array_entries[entry_index].value;
            if self.php_handle_is_reference(previous) == Some(true)
                && self.php_handle_is_reference(value) == Some(false)
            {
                let replacement = self.duplicate_dereferenced_native_value(value)?;
                if self.replace_direct_reference_payload_owned(previous, replacement)? {
                    return Ok(());
                }
                // A cold boundary may have materialized this exact reference
                // identity. Republish its payload once, then perform the
                // assignment in the authoritative native slot; never decode
                // both operands and rebuild their graphs here.
                if let Err(error) = self.restore_authoritative_direct_reference(previous) {
                    self.release(replacement)?;
                    return Err(error);
                }
                if self.replace_direct_reference_payload_owned(previous, replacement)? {
                    return Ok(());
                }
                self.release(replacement)?;
                return Err(
                    "native array reference entry could not republish its direct payload"
                        .to_owned(),
                );
            }
            self.retain(value)?;
            self.direct_array_entries[entry_index].value = value;
            self.release(previous)?;
            return Ok(());
        }

        let encoded_key = self.encode_native_array_key_owned(&normalized_key)?;
        if let Err(error) = self.retain(value) {
            let _ = self.release(encoded_key);
            return Err(error);
        }
        let capacity = slot.reserved as usize;
        if length == capacity {
            let (new_start, new_capacity) = match self.reserve_direct_array_entries(length + 1) {
                Ok(range) => range,
                Err(error) => {
                    let _ = self.release(encoded_key);
                    let _ = self.release(value);
                    return Err(error);
                }
            };
            self.direct_array_entries
                .copy_within(start..start + length, new_start);
            self.free_direct_array_entries(start, capacity);
            start = new_start;
            slot.reserved = u32::try_from(new_capacity).unwrap_or(u32::MAX);
            slot.aux = self.direct_array_entries[start..].as_ptr() as usize as u64;
        }
        self.direct_array_entries[start + length] = php_jit::JitNativeDirectArrayEntry {
            key: encoded_key,
            value,
        };
        if let php_runtime::api::ArrayKey::Int(key) = normalized_key {
            let next = key.saturating_add(1);
            let state = &mut self.direct_array_states[array_index];
            if state.has_next_append_key == 0 || next > state.next_append_key {
                state.next_append_key = next;
            }
            state.has_next_append_key = 1;
        }
        slot.payload = (length + 1) as u64;
        self.direct_value_slots[array_index] = slot;
        Ok(())
    }

    /// Removes one entry from a uniquely owned authoritative direct array.
    ///
    /// The caller performs encoded-handle COW first. Keeping removal in the
    /// direct entry plane is important for by-value call parameters: mutating
    /// a shared request slot would otherwise write through into the caller
    /// even though PHP requires the callee to observe an independent array
    /// value.
    pub(super) fn direct_array_remove_encoded(
        &mut self,
        encoded: i64,
        key: &php_runtime::api::ArrayKey,
    ) -> Result<(), String> {
        let (array_index, mut slot) = self
            .direct_array_slot(encoded)
            .ok_or_else(|| "native value is not a direct array".to_owned())?;
        self.cross_unit_stable_values.remove(&array_index);
        if slot.refcount != 1 {
            return Err("direct native array removal requires unique ownership".to_owned());
        }
        let length = usize::try_from(slot.payload)
            .map_err(|_| "direct native array length overflow".to_owned())?;
        let Some(position) = self
            .direct_array_entries_for(encoded)
            .ok_or_else(|| "direct native array entries are unavailable".to_owned())?
            .iter()
            .position(|entry| self.native_encoded_matches_array_key(entry.key, key))
        else {
            return Ok(());
        };
        let base = self.direct_array_entries.as_ptr() as usize;
        let address = usize::try_from(slot.aux)
            .map_err(|_| "direct native array address overflow".to_owned())?;
        let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
        let offset = address
            .checked_sub(base)
            .ok_or_else(|| "direct native array address is outside its arena".to_owned())?;
        if offset % entry_size != 0 {
            return Err("direct native array address is unaligned".to_owned());
        }
        let start = offset / entry_size;
        let removed = self.direct_array_entries[start + position];
        self.release(removed.key)?;
        self.release(removed.value)?;
        self.direct_array_entries
            .copy_within(start + position + 1..start + length, start + position);
        let new_length = length - 1;
        self.direct_array_entries[start + new_length] =
            php_jit::JitNativeDirectArrayEntry { key: 0, value: 0 };

        let cursor = php_jit::jit_native_direct_array_cursor(slot.flags)
            .and_then(|cursor| usize::try_from(cursor).ok())
            .filter(|cursor| *cursor < length)
            .and_then(|cursor| {
                if cursor > position {
                    Some(cursor - 1)
                } else if cursor == position && position >= new_length {
                    None
                } else {
                    Some(cursor)
                }
            })
            .and_then(|cursor| u32::try_from(cursor).ok());
        slot.flags = php_jit::jit_native_direct_array_flags(cursor);
        slot.payload = new_length as u64;
        self.direct_value_slots[array_index] = slot;
        Ok(())
    }

    pub(super) fn publish_direct_object_slots(
        &mut self,
        object: i64,
        property: &str,
        _value: i64,
        function: i64,
        continuation: i64,
        state: u32,
    ) -> Result<(), String> {
        if !matches!(
            state,
            php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_PUBLISHED
                | php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_WRITABLE
                | php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_REFERENCEABLE
                | php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_DIMENSION_WRITABLE
        ) {
            return Err(format!("invalid trusted property slot state {state}"));
        }
        let direct_index = |context: &Self| {
            let direct_object = context.dereference_direct_encoding(object);
            Self::direct_value_index(direct_object).filter(|index| {
                context.direct_value_slots.get(*index).is_some_and(|slot| {
                    slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
                })
            })
        };
        let index = if let Some(index) = direct_index(self) {
            index
        } else {
            // A cold continuation may have materialized the receiver
            // reference. Restore that exact descriptor and follow its native
            // payload; do not reconstruct or re-encode the object graph.
            self.restore_authoritative_direct_reference(object)?;
            let Some(index) = direct_index(self) else {
                return Ok(());
            };
            index
        };
        (|| {
            if !self.promote_direct_object_property_slots(index)? {
                return Ok(());
            }
            let object = self
                .direct_object(index)
                .ok_or_else(|| format!("direct native object {index} has no stable owner"))?;
            let class_name = object.class_name();
            let prepared = self.prepared_native_runtime_class(&class_name);
            let caller_function = u32::try_from(function)
                .map_err(|_| "trusted property function index overflow".to_owned())?;
            let declaration =
                native_instance_property_declaration(self, &class_name, property, caller_function);
            let state_is_stable = declaration.as_ref().is_some_and(|declaration| {
                let entry = &declaration.entry;
                let readable =
                    native_instance_property_readable(self, declaration, caller_function);
                let writable =
                    native_instance_property_writable(self, declaration, caller_function);
                let mutable = prepared
                    .as_ref()
                    .is_some_and(|class| !class.entry.flags.is_readonly)
                    && !entry.flags.is_readonly;
                match state {
                    php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_PUBLISHED => {
                        readable && entry.hooks.get.is_none()
                    }
                    php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_WRITABLE
                    | php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_REFERENCEABLE => {
                        readable
                            && writable
                            && mutable
                            && !entry.flags.is_typed
                            && entry.type_.is_none()
                            && entry.hooks.get.is_none()
                            && entry.hooks.set.is_none()
                    }
                    php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_DIMENSION_WRITABLE => {
                        readable
                            && writable
                            && mutable
                            && entry.hooks.get.is_none()
                            && entry.hooks.set.is_none()
                    }
                    _ => false,
                }
            });
            if !state_is_stable {
                return Ok(());
            }
            let Some(slot_index) = object.declared_slot_index(property) else {
                return Ok(());
            };
            let function = usize::try_from(caller_function)
                .map_err(|_| "trusted property function index overflow".to_owned())?;
            let continuation = usize::try_from(
                u32::try_from(continuation)
                    .map_err(|_| "trusted property continuation index overflow".to_owned())?,
            )
            .map_err(|_| "trusted property continuation index overflow".to_owned())?;
            let Some(base) = self
                .trusted_property_function_offsets
                .get(function)
                .copied()
                .and_then(|base| usize::try_from(base).ok())
            else {
                return Ok(());
            };
            let Some(plan) = self
                .trusted_property_slots
                .get_mut(base.saturating_add(continuation))
            else {
                return Ok(());
            };
            *plan = php_jit::JitNativeTrustedPropertySlot {
                state,
                slot_index,
                layout_id: object.class_layout_epoch(),
                property_name_bytes: 0,
                property_name_length: 0,
            };
            Ok(())
        })()
    }

    pub(super) fn register_native_execution_scope(
        &mut self,
        scope: NativeExecutionScope,
    ) -> Result<u32, String> {
        if let Some(index) = self
            .native_execution_scopes
            .iter()
            .position(|candidate| candidate.as_ref() == &scope)
        {
            return u32::try_from(index + 1)
                .map_err(|_| "native execution scope identity overflow".to_owned());
        }
        self.native_execution_scopes.push(Box::new(scope));
        u32::try_from(self.native_execution_scopes.len())
            .map_err(|_| "native execution scope identity overflow".to_owned())
    }

    pub(super) fn native_execution_target_from_state(
        &self,
        state: &php_jit::JitDeoptState,
        fallback: Option<&NativeExecutionTarget>,
    ) -> Result<NativeExecutionTarget, String> {
        let runtime_view = state.active_runtime_view();
        let identity = runtime_view.fiber_execution_scope;
        let index = usize::try_from(identity)
            .ok()
            .and_then(|identity| identity.checked_sub(1))
            .ok_or_else(|| "suspended native activation has no execution scope".to_owned())?;
        let recorded = self
            .native_execution_scopes
            .get(index)
            .ok_or_else(|| format!("suspended native execution scope {identity} is missing"))?;
        let function_entries = runtime_view.trusted_generic_function_entries;
        let inferred_unit = self
            .dynamic_units
            .iter()
            .enumerate()
            .find_map(|(unit, package)| {
                let entries = package
                    .compiled
                    .prepared_deployment_image()
                    .generic_function_entries
                    .as_ptr() as usize as u64;
                (entries == function_entries).then_some(Some(unit))
            })
            .or_else(|| {
                let entries = self
                    .compiled
                    .prepared_deployment_image()
                    .generic_function_entries
                    .as_ptr() as usize as u64;
                (self.current_dynamic_unit.is_none() && entries == function_entries).then_some(None)
            })
            .unwrap_or(recorded.unit);
        // Function IDs are dense only within one compiled unit. A nested
        // linked callee may therefore have the same numeric ID as its caller;
        // the captured runtime view, not that coincidental ID, owns the
        // continuation and its baseline metadata.
        let same_activation = fallback.is_some_and(|fallback| {
            fallback.function.raw() == state.function_id && fallback.unit == inferred_unit
        });
        let scope = fallback
            .filter(|fallback| {
                same_activation
                    || (recorded.unit != inferred_unit && fallback.unit == inferred_unit)
            })
            .map_or_else(|| recorded.as_ref().clone(), NativeExecutionTarget::scope);
        let inferred_unit = if same_activation {
            fallback.and_then(|fallback| fallback.unit)
        } else {
            inferred_unit
        };
        Ok(NativeExecutionTarget {
            unit: inferred_unit,
            function: php_ir::FunctionId::new(state.function_id),
            called_class: scope.called_class.clone(),
            scope_class: scope.scope_class.clone(),
        })
    }

    pub(super) fn run_in_native_execution_target<R, E>(
        &mut self,
        target: &NativeExecutionTarget,
        operation: impl FnOnce(&mut Self) -> Result<R, E>,
    ) -> Result<R, E>
    where
        E: From<String>,
    {
        let identity = self
            .register_native_execution_scope(target.scope())
            .map_err(E::from)?;
        let previous_identity =
            std::mem::replace(&mut self.current_native_execution_scope, identity);
        let push_called_class = target
            .called_class
            .as_ref()
            .is_some_and(|called_class| self.called_classes.last() != Some(called_class));
        if push_called_class {
            self.called_classes.push(
                target
                    .called_class
                    .as_ref()
                    .expect("called class was classified above")
                    .clone(),
            );
        }
        let push_scope_class = target.scope_class.as_ref().is_some_and(|scope_class| {
            self.lexical_scope_classes.last().map(String::as_str) != Some(scope_class.as_ref())
        });
        if push_scope_class {
            self.lexical_scope_classes.push(
                target
                    .scope_class
                    .as_ref()
                    .expect("scope class was classified above")
                    .to_string(),
            );
        }

        let target_is_active = match target.unit {
            Some(unit) => {
                self.current_dynamic_unit == Some(unit)
                    && self.dynamic_units.get(unit).is_some_and(|package| {
                        package.compiled.artifact_identity() == self.compiled.artifact_identity()
                    })
            }
            None => self.current_dynamic_unit.is_none(),
        };
        let result = if target_is_active {
            let _runtime_view = activate_native_context(self);
            operation(self)
        } else {
            match target.unit {
                Some(unit) => self
                    .with_active_dynamic_unit(unit, None, operation)
                    .map_err(E::from)?,
                None => Err(E::from(format!(
                    "root native execution target {} cannot run inside dynamic unit {:?}",
                    target.function.raw(),
                    self.current_dynamic_unit,
                ))),
            }
        };

        if push_scope_class {
            self.lexical_scope_classes.pop();
        }
        if push_called_class {
            self.called_classes.pop();
        }
        self.current_native_execution_scope = previous_identity;
        result
    }

    pub(super) fn duplicate_direct_generator_value(&mut self, encoded: i64) -> Result<i64, String> {
        self.duplicate_authoritative_native_value(encoded)?
            .ok_or_else(|| {
                format!(
                    "direct Generator value {} crossed from baseline storage",
                    self.native_encoded_type_name(encoded)
                )
            })
    }

    pub(super) fn replace_direct_generator_current_owned(
        &mut self,
        index: usize,
        key: Option<i64>,
        value: i64,
        forwarded: bool,
    ) -> Result<(i64, i64), String> {
        let (old_key, old_value, key) = {
            let generator = self
                .direct_generator_mut(index)
                .ok_or_else(|| format!("direct Generator {index} is missing"))?;
            let key = if forwarded {
                key.unwrap_or_else(|| php_jit::jit_encode_constant(u32::MAX))
            } else if let Some(key) = key {
                if let Some(explicit) = (php_jit::jit_decode_runtime_value(key).is_none()
                    && php_jit::jit_decode_constant(key).is_none())
                .then_some(key)
                    && explicit >= generator.next_auto_key
                {
                    generator.next_auto_key = explicit.saturating_add(1);
                }
                key
            } else {
                let key = generator.next_auto_key;
                generator.next_auto_key = generator.next_auto_key.saturating_add(1);
                key
            };
            let old_key = generator.current_key.replace(key);
            let old_value = generator.current_value.replace(value);
            generator.lifecycle = php_runtime::api::GeneratorState::Suspended;
            generator.yields_seen = generator.yields_seen.saturating_add(1);
            (old_key, old_value, key)
        };
        if let Some(old_key) = old_key {
            self.release(old_key)?;
        }
        if let Some(old_value) = old_value {
            self.release(old_value)?;
        }
        let output_key = self.duplicate_direct_generator_value(key)?;
        match self.duplicate_direct_generator_value(value) {
            Ok(output_value) => Ok((output_key, output_value)),
            Err(error) => {
                self.release(output_key)?;
                Err(error)
            }
        }
    }

    pub(super) fn instruction_for_continuation(
        &self,
        function: u32,
        continuation: u32,
    ) -> Option<NativeInstructionPtr> {
        self.prepared_continuation_instructions(php_ir::FunctionId::new(function))
            .and_then(|instructions| instructions.get(continuation as usize).cloned())
            .flatten()
            .map(|instruction| NativeInstructionPtr(std::sync::Arc::as_ptr(&instruction)))
    }

    pub(in crate::vm) fn instruction_kind_debug(&self, function: u32, continuation: u32) -> String {
        self.instruction_for_continuation(function, continuation)
            .map(|instruction| format!("{:?}", instruction.kind))
            .unwrap_or_else(|| "<missing continuation>".to_owned())
    }

    pub(in crate::vm) fn instruction_kind_debug_for_state(
        &self,
        state: &php_jit::JitDeoptState,
    ) -> String {
        let view = state.active_runtime_view();
        let compiled = if self
            .compiled
            .prepared_deployment_image()
            .generic_function_entries
            .as_ptr() as usize as u64
            == view.trusted_generic_function_entries
        {
            Some(&self.compiled)
        } else {
            self.dynamic_units
                .iter()
                .find(|unit| {
                    unit.published_runtime_view.trusted_generic_function_entries
                        == view.trusted_generic_function_entries
                })
                .map(|unit| &unit.compiled)
        };
        let Some(compiled) = compiled else {
            return self.instruction_kind_debug(state.function_id, state.continuation_id);
        };
        let function_id = php_ir::FunctionId::new(state.function_id);
        let continuations = compiled.prepared_continuation_instructions(function_id);
        let instruction = continuations.as_ref().and_then(|instructions| {
            instructions
                .get(state.continuation_id as usize)
                .cloned()
                .flatten()
        });
        let Some(instruction) = instruction else {
            let unit = compiled
                .unit()
                .files
                .first()
                .map_or("<unknown unit>", |file| file.path.as_str());
            let function = compiled
                .unit()
                .functions
                .get(function_id.index())
                .map_or("<unknown function>", |function| function.name.as_str());
            let continuation_count = continuations.as_ref().map_or(0, |entries| entries.len());
            return format!(
                "<missing continuation {function} in {unit}; published continuations={continuation_count}>"
            );
        };
        let function = compiled.unit().functions.get(function_id.index());
        let file = compiled.unit().files.get(instruction.span.file.index());
        match (function, file) {
            (Some(function), Some(file)) => {
                format!("{} in {}: {:?}", function.name, file.path, instruction.kind)
            }
            (Some(function), None) => format!("{}: {:?}", function.name, instruction.kind),
            _ => format!("{:?}", instruction.kind),
        }
    }
}
