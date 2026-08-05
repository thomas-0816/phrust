//! Explicit Rust `Value` materialization boundary.
//!
//! Only baseline/cold code and the final outer-result handoff may call
//! these methods. Optimizing and exact modules operate on native encodings.

use super::*;

impl<'a> NativeRequestColdState<'a> {
    pub(super) fn encode_prepared_closure(
        &mut self,
        callable: php_runtime::api::CallableValue,
    ) -> Result<i64, String> {
        let php_runtime::api::CallableValue::Closure(closure) = callable else {
            unreachable!("only Closure values have prepared closure storage")
        };
        if let Some(index) = self.direct_closure_handles.get(&closure.id).copied() {
            let slot = self
                .direct_value_slots
                .get_mut(index as usize)
                .filter(|slot| {
                    slot.refcount != 0
                        && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE
                        && slot.payload == closure.id
                })
                .ok_or_else(|| "direct native closure identity points at a dead slot".to_owned())?;
            slot.refcount = slot
                .refcount
                .checked_add(1)
                .ok_or_else(|| "direct native closure refcount overflow".to_owned())?;
            let runtime_index = index
                .checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
                .ok_or_else(|| "direct native closure handle overflow".to_owned())?;
            return Ok(php_jit::jit_encode_typed_runtime_value(
                runtime_index,
                php_jit::JIT_VALUE_RUNTIME_CALLABLE_TAG,
            ));
        }
        let implicit_this = closure
            .bound_this
            .as_ref()
            .map(|object| self.encode_native_object_owner(object.clone()))
            .transpose()?;
        let capture_descriptors = closure
            .captures
            .iter()
            .map(|capture| (capture.name.clone(), capture.reference.is_some()))
            .collect::<Vec<_>>();
        let mut capture_values = Vec::with_capacity(closure.captures.len());
        for capture in &closure.captures {
            let encoded = if capture.name.eq_ignore_ascii_case("this")
                && let Some(object) = &closure.bound_this
            {
                self.encode_native_object_owner(object.clone())
            } else if let Some(reference) = capture.reference() {
                self.encode_native_reference_owner(reference)
            } else {
                self.encode_baseline_value(
                    capture
                        .value()
                        .cloned()
                        .unwrap_or(php_runtime::api::Value::Null),
                )
            };
            match encoded {
                Ok(encoded) => capture_values.push(encoded),
                Err(error) => {
                    if let Some(implicit_this) = implicit_this {
                        let _ = self.release(implicit_this);
                    }
                    for capture in capture_values {
                        let _ = self.release(capture);
                    }
                    return Err(error);
                }
            }
        }
        let mut closure = closure;
        closure.bound_this = None;
        closure.captures.clear();
        self.publish_prepared_closure_owned(NativePreparedClosure::new(
            closure,
            Arc::from(capture_descriptors),
            implicit_this,
            capture_values.into_boxed_slice(),
            None,
            false,
            false,
            false,
            false,
        ))
    }

    pub(super) fn publish_prepared_closure_owned(
        &mut self,
        prepared: NativePreparedClosure,
    ) -> Result<i64, String> {
        let index = match self.reserve_direct_value_slot() {
            Ok(index) => index,
            Err(error) => {
                if let Some(implicit_this) = prepared.implicit_this {
                    let _ = self.release(implicit_this);
                }
                for capture in prepared.captures.iter().copied() {
                    let _ = self.release(capture);
                }
                return Err(error);
            }
        };
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .expect("direct closure index is bounded by the native value arena");
        let closure_id = prepared.closure.id;
        let runtime_view = if self.fast_state.is_null() {
            0
        } else {
            // SAFETY: request activation installs the separately boxed fast
            // state before any PHP closure can be published.
            #[allow(unsafe_code)]
            let fast = unsafe { &*self.fast_state };
            if fast.header.runtime_view_pointer == 0 {
                std::ptr::from_ref(&fast.header.runtime_view) as usize as u64
            } else {
                fast.header.runtime_view_pointer
            }
        };
        let owner = Box::into_raw(Box::new(NativePreparedCallableOwner::closure(
            prepared,
            runtime_view,
        )));
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE,
            flags: php_jit::JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION,
            payload: closure_id,
            aux: owner as usize as u64,
            ..php_jit::JitNativeValueSlot::default()
        };
        self.direct_closure_handles.insert(closure_id, index as u32);
        Ok(php_jit::jit_encode_typed_runtime_value(
            runtime_index,
            php_jit::JIT_VALUE_RUNTIME_CALLABLE_TAG,
        ))
    }
}
use php_runtime::api::PhpString;
use php_runtime::api::Value;

pub(super) fn baseline_shared_array_storage_is_empty(address: usize) -> Option<bool> {
    php_runtime::api::PhpArray::clone_from_native_storage_refcount(address)
        .map(|array| array.is_empty())
}

pub(super) fn release_baseline_shared_array_storage(address: usize) -> bool {
    php_runtime::api::PhpArray::release_native_storage_refcount(address)
}

#[derive(Clone)]
pub(super) struct NativeShutdownCallback {
    pub(super) callable: Value,
    pub(super) arguments: Vec<Value>,
    pub(super) source: php_ir::Instruction,
}

#[derive(Clone)]
pub(super) struct NativeErrorHandler {
    pub(super) callback: Value,
    pub(super) levels: i64,
}

/// Rust-value carrier used only while callbacks cross an include/eval owner
/// boundary. Active request execution promotes this record immediately into
/// [`NativeRegisteredCallbackState`].
#[derive(Default)]
pub(super) struct NativeRegisteredCallbackTransfer {
    pub(super) autoload_callbacks: Vec<Value>,
    pub(super) shutdown_callbacks: Vec<NativeShutdownCallback>,
    pub(super) error_handlers: Vec<NativeErrorHandler>,
    pub(super) exception_handlers: Vec<Value>,
}

fn extract_transient_autoload_callbacks(
    callbacks: &mut Vec<NativeRegisteredAutoloadCallback>,
) -> Vec<NativeRegisteredAutoloadCallback> {
    let mut retained = Vec::with_capacity(callbacks.len());
    let mut exported = Vec::new();
    for callback in std::mem::take(callbacks) {
        if callback.transient_export {
            exported.push(callback);
        } else {
            retained.push(callback);
        }
    }
    *callbacks = retained;
    exported
}

fn extract_transient_shutdown_callbacks(
    callbacks: &mut Vec<NativeRegisteredShutdownCallback>,
) -> Vec<NativeRegisteredShutdownCallback> {
    let mut retained = Vec::with_capacity(callbacks.len());
    let mut exported = Vec::new();
    for callback in std::mem::take(callbacks) {
        if callback.transient_export {
            exported.push(callback);
        } else {
            retained.push(callback);
        }
    }
    *callbacks = retained;
    exported
}

/// Baseline-only value payload exported by a nested include/eval VM.
///
/// Native function-entry metadata remains numeric, but constants and callback
/// payloads deliberately stay inside this explicit Rust `Value` plane until
/// the cold owner promotes them.
#[derive(Default)]
pub(super) struct NativeIncludeExports {
    pub(super) functions: Vec<(String, php_ir::FunctionId)>,
    pub(super) native_entries:
        std::sync::Arc<std::collections::BTreeMap<php_ir::FunctionId, php_jit::JitFunctionHandle>>,
    pub(super) native_entry_signature_hashes: std::collections::BTreeMap<php_ir::FunctionId, u64>,
    pub(super) classes: Vec<String>,
    pub(super) constants: std::collections::BTreeMap<String, Value>,
    pub(super) autoload_callbacks: Vec<Value>,
    pub(super) shutdown_callbacks: Vec<NativeShutdownCallback>,
}

/// Complete cold symbol/value transfer record used only while an include or
/// eval temporarily hands ownership to a nested VM.
#[derive(Default)]
pub(super) struct NativeIncludeSymbols {
    pub(super) deployment_functions:
        std::sync::Arc<std::collections::HashMap<std::sync::Arc<str>, php_ir::FunctionId>>,
    pub(super) deployment_classes: std::sync::Arc<std::collections::HashSet<std::sync::Arc<str>>>,
    pub(super) external_functions: std::collections::HashMap<String, NativeDynamicFunction>,
    pub(super) external_class_units: std::collections::HashMap<String, usize>,
    pub(super) external_signature_epoch: u64,
    pub(super) dynamic_units: Vec<NativeDynamicUnit>,
    pub(super) dynamic_classes: std::collections::BTreeSet<String>,
    pub(super) class_aliases: std::collections::BTreeMap<String, String>,
    pub(super) autoload_callbacks: Vec<Value>,
    pub(super) shutdown_callbacks: Vec<NativeShutdownCallback>,
    pub(super) static_property_transfer: std::collections::BTreeMap<(String, String), Value>,
    pub(super) typed_static_reference_constraints:
        std::collections::BTreeMap<u64, Vec<NativeTypedStaticReferenceConstraint>>,
    pub(super) static_locals:
        std::collections::BTreeMap<(u64, u32, u32), php_runtime::api::ReferenceCell>,
    pub(super) enum_cases:
        std::collections::BTreeMap<(String, String), php_runtime::api::ObjectRef>,
    pub(super) destroyed_objects: std::collections::BTreeMap<u64, WeakObjectHandle>,
    pub(super) error_reporting: Option<i64>,
    pub(super) display_errors: Option<bool>,
    pub(super) error_handlers: Vec<NativeErrorHandler>,
    pub(super) exception_handlers: Vec<Value>,
    pub(super) last_error: Option<NativeLastError>,
}

/// All request-owned Rust `Value` graphs that survive beyond one explicit
/// conversion call.
///
/// Keeping this state behind one named field makes the representation
/// boundary visible in the owner layout: native arenas remain authoritative
/// during optimizing execution, while callbacks, include transfer, exception
/// payloads, and the final globals projection live only in this baseline
/// compatibility plane.
pub(super) struct BaselineValueState {
    pub(super) decoded_constant_cache:
        RefCell<std::collections::HashMap<(Option<usize>, usize), Value>>,
    pub(super) cold_dynamic_constants: std::collections::BTreeMap<String, Value>,
    pub(super) pending_throwable: Option<Value>,
    /// Consumed once before native entry, or immediately when an include/eval
    /// transfer is restored. It is never consulted by active execution.
    pub(super) pending_registered_callbacks: Option<NativeRegisteredCallbackTransfer>,
    pub(super) inherited_globals: std::collections::BTreeMap<String, Value>,
    pub(super) direct_reference_cells:
        std::collections::HashMap<usize, php_runtime::api::ReferenceCell>,
    pub(super) materialized_direct_references: Vec<usize>,
    pub(super) direct_object_handles: std::collections::HashMap<u64, u32>,
    pub(super) direct_fiber_cells: std::collections::HashMap<usize, php_runtime::api::FiberRef>,
    pub(super) direct_fiber_handles: std::collections::HashMap<u64, u32>,
    pub(super) direct_generator_cells:
        std::collections::HashMap<usize, php_runtime::api::GeneratorRef>,
    pub(super) direct_generator_handles: std::collections::HashMap<u64, u32>,
    /// Temporary identity for one recursive `PhpArray` materialization.
    /// These maps exist only while cold code crosses into the native plane
    /// and are never retained by the shared native request pool.
    pub(super) direct_array_handles: std::collections::HashMap<(u64, u64), u32>,
    pub(super) direct_array_storage_ids: std::collections::HashMap<usize, (u64, u64)>,
    pub(super) direct_array_encode_depth: usize,
    pub(super) session_global: php_runtime::api::ReferenceCell,
    pub(super) filter_input_arrays: Rc<std::collections::BTreeMap<i64, php_runtime::api::PhpArray>>,
    pub(super) static_property_transfer: std::collections::BTreeMap<(String, String), Value>,
    pub(super) static_locals:
        std::collections::BTreeMap<(u64, u32, u32), php_runtime::api::ReferenceCell>,
    pub(super) enum_cases:
        std::collections::BTreeMap<(String, String), php_runtime::api::ObjectRef>,
}

impl BaselineValueState {
    // Architecture: this cold ownership transfer names every independently
    // owned baseline value plane once instead of hiding them in a generic bag.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        cold_dynamic_constants: std::collections::BTreeMap<String, Value>,
        autoload_callbacks: Vec<Value>,
        shutdown_callbacks: Vec<NativeShutdownCallback>,
        error_handlers: Vec<NativeErrorHandler>,
        exception_handlers: Vec<Value>,
        inherited_globals: std::collections::BTreeMap<String, Value>,
        session_global: php_runtime::api::ReferenceCell,
        filter_input_arrays: Rc<std::collections::BTreeMap<i64, php_runtime::api::PhpArray>>,
        static_property_transfer: std::collections::BTreeMap<(String, String), Value>,
        static_locals: std::collections::BTreeMap<(u64, u32, u32), php_runtime::api::ReferenceCell>,
        enum_cases: std::collections::BTreeMap<(String, String), php_runtime::api::ObjectRef>,
    ) -> Self {
        Self {
            decoded_constant_cache: RefCell::new(std::collections::HashMap::new()),
            cold_dynamic_constants,
            pending_throwable: None,
            pending_registered_callbacks: Some(NativeRegisteredCallbackTransfer {
                autoload_callbacks,
                shutdown_callbacks,
                error_handlers,
                exception_handlers,
            }),
            inherited_globals,
            direct_reference_cells: std::collections::HashMap::new(),
            materialized_direct_references: Vec::new(),
            direct_object_handles: std::collections::HashMap::new(),
            direct_fiber_cells: std::collections::HashMap::new(),
            direct_fiber_handles: std::collections::HashMap::new(),
            direct_generator_cells: std::collections::HashMap::new(),
            direct_generator_handles: std::collections::HashMap::new(),
            direct_array_handles: std::collections::HashMap::new(),
            direct_array_storage_ids: std::collections::HashMap::new(),
            direct_array_encode_depth: 0,
            session_global,
            filter_input_arrays,
            static_property_transfer,
            static_locals,
            enum_cases,
        }
    }
}

impl<'a> NativeRequestColdState<'a> {
    /// Baseline/cold publication from the Rust string facade. Common native
    /// code publishes stable byte slices directly and cannot import this
    /// conversion boundary.
    #[track_caller]
    pub(super) fn encode_native_string_owner(&mut self, string: PhpString) -> Result<i64, String> {
        self.encode_native_string_bytes_owner(string.as_bytes())
    }

    pub(super) fn release_registered_callback_state(
        &mut self,
        state: NativeRegisteredCallbackState,
    ) -> Result<(), String> {
        let mut first_error = None;
        for callback in state.autoload_callbacks {
            if let Err(error) = self.release_if_live(callback.callable) {
                first_error.get_or_insert(error);
            }
        }
        for callback in state.shutdown_callbacks {
            if let Err(error) = self.release_if_live(callback.callable) {
                first_error.get_or_insert(error);
            }
            for argument in callback.arguments {
                if let Err(error) = self.release_if_live(argument) {
                    first_error.get_or_insert(error);
                }
            }
        }
        for handler in state.error_handlers {
            if let Err(error) = self.release_if_live(handler.callback) {
                first_error.get_or_insert(error);
            }
        }
        for handler in state.exception_handlers {
            if let Err(error) = self.release_if_live(handler) {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn encode_registered_callback_transfer(
        &mut self,
        transfer: NativeRegisteredCallbackTransfer,
    ) -> Result<NativeRegisteredCallbackState, String> {
        let mut native = NativeRegisteredCallbackState::default();
        let result = (|| {
            for callback in transfer.autoload_callbacks {
                native
                    .autoload_callbacks
                    .push(NativeRegisteredAutoloadCallback {
                        callable: self.encode_baseline_value(callback)?,
                        transient_export: false,
                    });
            }
            for callback in transfer.shutdown_callbacks {
                let callable = self.encode_baseline_value(callback.callable)?;
                let mut arguments = Vec::with_capacity(callback.arguments.len());
                for argument in callback.arguments {
                    match self.encode_baseline_value(argument) {
                        Ok(argument) => arguments.push(argument),
                        Err(error) => {
                            let _ = self.release_if_live(callable);
                            for argument in arguments {
                                let _ = self.release_if_live(argument);
                            }
                            return Err(error);
                        }
                    }
                }
                native
                    .shutdown_callbacks
                    .push(NativeRegisteredShutdownCallback {
                        callable,
                        arguments,
                        source: callback.source,
                        transient_export: false,
                    });
            }
            for handler in transfer.error_handlers {
                native.error_handlers.push(NativeRegisteredErrorHandler {
                    callback: self.encode_baseline_value(handler.callback)?,
                    levels: handler.levels,
                });
            }
            for handler in transfer.exception_handlers {
                native
                    .exception_handlers
                    .push(self.encode_baseline_value(handler)?);
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = self.release_registered_callback_state(native);
            return Err(error);
        }
        Ok(native)
    }

    fn decode_registered_callback_state(
        &mut self,
        state: &NativeRegisteredCallbackState,
    ) -> Result<NativeRegisteredCallbackTransfer, String> {
        let autoload_callbacks = state
            .autoload_callbacks
            .iter()
            .map(|callback| self.decode_baseline_value(callback.callable))
            .collect::<Result<Vec<_>, _>>()?;
        let mut shutdown_callbacks = Vec::with_capacity(state.shutdown_callbacks.len());
        for callback in &state.shutdown_callbacks {
            shutdown_callbacks.push(NativeShutdownCallback {
                callable: self.decode_baseline_value(callback.callable)?,
                arguments: callback
                    .arguments
                    .iter()
                    .map(|argument| self.decode_baseline_value(*argument))
                    .collect::<Result<Vec<_>, _>>()?,
                source: callback.source.clone(),
            });
        }
        let error_handlers = state
            .error_handlers
            .iter()
            .map(|handler| {
                Ok(NativeErrorHandler {
                    callback: self.decode_baseline_value(handler.callback)?,
                    levels: handler.levels,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let exception_handlers = state
            .exception_handlers
            .iter()
            .map(|handler| self.decode_baseline_value(*handler))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(NativeRegisteredCallbackTransfer {
            autoload_callbacks,
            shutdown_callbacks,
            error_handlers,
            exception_handlers,
        })
    }

    pub(super) fn promote_pending_registered_callbacks(&mut self) -> Result<(), String> {
        let Some(transfer) = self.baseline_values.pending_registered_callbacks.take() else {
            return Ok(());
        };
        let native = self.encode_registered_callback_transfer(transfer)?;
        debug_assert!(self.registered_callbacks.autoload_callbacks.is_empty());
        debug_assert!(self.registered_callbacks.shutdown_callbacks.is_empty());
        debug_assert!(self.registered_callbacks.error_handlers.is_empty());
        debug_assert!(self.registered_callbacks.exception_handlers.is_empty());
        self.registered_callbacks = native;
        self.mark_roots_dirty(RootMutationReason::CallbackOrHandler);
        Ok(())
    }

    pub(super) fn take_registered_callback_transfer(
        &mut self,
    ) -> Result<NativeRegisteredCallbackTransfer, String> {
        let state = std::mem::take(&mut self.registered_callbacks);
        match self.decode_registered_callback_state(&state) {
            Ok(transfer) => {
                self.release_registered_callback_state(state)?;
                self.mark_roots_dirty(RootMutationReason::CallbackOrHandler);
                Ok(transfer)
            }
            Err(error) => {
                self.registered_callbacks = state;
                Err(error)
            }
        }
    }

    pub(super) fn take_registered_include_exports(
        &mut self,
    ) -> Result<(Vec<Value>, Vec<NativeShutdownCallback>), String> {
        let exported = NativeRegisteredCallbackState {
            autoload_callbacks: self
                .registered_callbacks
                .autoload_callbacks
                .iter()
                .filter(|callback| callback.transient_export)
                .cloned()
                .collect(),
            shutdown_callbacks: self
                .registered_callbacks
                .shutdown_callbacks
                .iter()
                .filter(|callback| callback.transient_export)
                .cloned()
                .collect(),
            ..NativeRegisteredCallbackState::default()
        };
        let transfer = self.decode_registered_callback_state(&exported)?;
        let moved = NativeRegisteredCallbackState {
            autoload_callbacks: extract_transient_autoload_callbacks(
                &mut self.registered_callbacks.autoload_callbacks,
            ),
            shutdown_callbacks: extract_transient_shutdown_callbacks(
                &mut self.registered_callbacks.shutdown_callbacks,
            ),
            ..NativeRegisteredCallbackState::default()
        };
        self.release_registered_callback_state(moved)?;
        self.mark_roots_dirty(RootMutationReason::CallbackOrHandler);
        Ok((transfer.autoload_callbacks, transfer.shutdown_callbacks))
    }

    pub(super) fn append_registered_include_exports(
        &mut self,
        autoload_callbacks: Vec<Value>,
        shutdown_callbacks: Vec<NativeShutdownCallback>,
    ) -> Result<(), String> {
        let native =
            self.encode_registered_callback_transfer(NativeRegisteredCallbackTransfer {
                autoload_callbacks,
                shutdown_callbacks,
                ..NativeRegisteredCallbackTransfer::default()
            })?;
        let transient_export = self.include_child;
        self.registered_callbacks.autoload_callbacks.extend(
            native.autoload_callbacks.into_iter().map(|mut callback| {
                callback.transient_export = transient_export;
                callback
            }),
        );
        self.registered_callbacks.shutdown_callbacks.extend(
            native.shutdown_callbacks.into_iter().map(|mut callback| {
                callback.transient_export = transient_export;
                callback
            }),
        );
        self.mark_roots_dirty(RootMutationReason::CallbackOrHandler);
        Ok(())
    }

    /// Crosses request input arrays once from the baseline `PhpArray` owner
    /// into five authoritative native roots. Exact FILTER_* invocations read
    /// only these handles; the Rust map remains baseline-only compatibility
    /// state.
    pub(super) fn publish_native_filter_input_roots(&mut self) -> Result<([i64; 5], u8), String> {
        let sources = [0_i64, 1, 2, 4, 5];
        let arrays = sources.map(|source| {
            self.baseline_values
                .filter_input_arrays
                .get(&source)
                .cloned()
        });
        let mut roots = [php_jit::jit_encode_constant(u32::MAX); 5];
        let mut present = 0_u8;
        for (index, array) in arrays.into_iter().enumerate() {
            let Some(array) = array else {
                continue;
            };
            roots[index] = self.encode_native_array_owner(array)?;
            present |= 1 << index;
        }
        Ok((roots, present))
    }

    #[track_caller]
    pub(super) fn baseline_decode_direct_array(&mut self, index: usize) -> Result<Value, String> {
        let slot = self
            .direct_value_slots
            .get(index)
            .filter(|slot| slot.refcount != 0)
            .ok_or_else(|| format!("direct native value {index} is missing"))?;
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY {
            return Err(format!("direct native value {index} is not an array"));
        }
        let length = usize::try_from(slot.payload)
            .map_err(|_| format!("direct native array {index} length overflow"))?;
        let cursor = php_jit::jit_native_direct_array_cursor(slot.flags)
            .and_then(|position| usize::try_from(position).ok());
        let base = self.direct_array_entries.as_ptr() as usize;
        let address = usize::try_from(slot.aux)
            .map_err(|_| format!("direct native array {index} address overflow"))?;
        let byte_offset = address
            .checked_sub(base)
            .ok_or_else(|| format!("direct native array {index} address is outside its arena"))?;
        let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
        if byte_offset % entry_size != 0 {
            return Err(format!("direct native array {index} address is unaligned"));
        }
        let start = byte_offset / entry_size;
        let entries = self
            .direct_array_entries
            .get(start..start.saturating_add(length))
            .ok_or_else(|| format!("direct native array {index} entries are outside its arena"))?
            .to_vec();
        let mut array = php_runtime::api::PhpArray::with_capacity(length);
        for (entry_index, entry) in entries.into_iter().enumerate() {
            let key = self.decode_baseline_value(entry.key).map_err(|error| {
                format!(
                    "direct native array {index} entry {entry_index} key {} could not decode: {error}",
                    entry.key
                )
            })?;
            let key = php_runtime::api::ArrayKey::from_value(&key)
                .ok_or_else(|| format!("direct native array {index} has an invalid key"))?;
            let value = self.decode_baseline_value(entry.value).map_err(|error| {
                format!(
                    "direct native array {index} entry {entry_index} value {} could not decode: {error}",
                    entry.value
                )
            })?;
            array.insert(key, value);
        }
        let state = self.direct_array_states[index];
        array.set_native_next_append_key(
            (state.has_next_append_key != 0).then_some(state.next_append_key),
        );
        array.set_native_pointer_position(cursor);
        Ok(Value::Array(array))
    }

    #[track_caller]
    pub(super) fn baseline_decode_direct_value(&mut self, index: usize) -> Result<Value, String> {
        let slot = *self
            .direct_value_slots
            .get(index)
            .filter(|slot| slot.refcount != 0)
            .ok_or_else(|| format!("direct native value {index} is missing"))?;
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_INT
            && slot.flags == php_jit::JIT_NATIVE_DIRECT_INT_ABI_VERSION
        {
            return Ok(Value::Int(slot.payload as i64));
        }
        if matches!(
            slot.kind,
            php_jit::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY
                | php_jit::JIT_NATIVE_VALUE_VIEW_BORROWED_REFERENCE_ARRAY
        ) {
            let array = php_runtime::api::PhpArray::clone_from_native_storage_refcount(
                slot.payload as usize,
            )
            .ok_or_else(|| format!("shared native array {index} storage is unavailable"))?;
            return Ok(Value::Array(array));
        }
        if matches!(
            slot.kind,
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER
                | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_FIBER
        ) {
            if let Some(fiber) = self.baseline_values.direct_fiber_cells.get(&index) {
                return Ok(Value::Fiber(fiber.clone()));
            }
            let (state, callable, return_value) = {
                let fiber = self
                    .fiber_record(index)
                    .ok_or_else(|| format!("direct native Fiber {index} has no stable record"))?;
                (fiber.state, fiber.callable, fiber.return_value)
            };
            let callable = self.decode_baseline_value(callable)?;
            if !matches!(callable, Value::Callable(_)) {
                return Err(format!(
                    "direct native Fiber {index} callable became {}",
                    native_value_type_name(&callable)
                ));
            }
            let fiber = php_runtime::api::FiberRef::new(callable);
            match state {
                php_runtime::api::FiberState::NotStarted => {}
                php_runtime::api::FiberState::Terminated => {
                    let return_value = return_value
                        .map(|value| self.decode_baseline_value(value))
                        .transpose()?;
                    fiber.terminate(return_value);
                }
                state => fiber.set_state(state),
            }
            self.direct_value_slots[index].kind = php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_FIBER;
            self.direct_value_slots[index].payload = fiber.id();
            self.baseline_values
                .direct_fiber_handles
                .insert(fiber.id(), index as u32);
            self.baseline_values
                .direct_fiber_cells
                .insert(index, fiber.clone());
            return Ok(Value::Fiber(fiber));
        }
        if matches!(
            slot.kind,
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_GENERATOR
                | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_GENERATOR
        ) {
            if let Some(generator) = self.baseline_values.direct_generator_cells.get(&index) {
                return Ok(Value::Generator(generator.clone()));
            }
            let (function, lifecycle, arguments, current_key, current_value, return_value) = {
                let generator = self.direct_generator(index).ok_or_else(|| {
                    format!("direct native Generator {index} has no stable activation")
                })?;
                (
                    generator.target.function,
                    generator.lifecycle,
                    generator.arguments.clone(),
                    generator.current_key,
                    generator.current_value,
                    generator.return_value,
                )
            };
            let arguments = arguments
                .into_iter()
                .map(|argument| self.decode_baseline_value(argument))
                .collect::<Result<Vec<_>, _>>()?;
            let generator = php_runtime::api::GeneratorRef::new(function.raw(), arguments);
            match lifecycle {
                php_runtime::api::GeneratorState::Created => {}
                php_runtime::api::GeneratorState::Suspended => {
                    let key = current_key
                        .map(|key| self.decode_baseline_value(key))
                        .transpose()?;
                    let value = current_value
                        .map(|value| self.decode_baseline_value(value))
                        .transpose()?
                        .unwrap_or(Value::Null);
                    generator.suspend_forwarded(key, value);
                }
                php_runtime::api::GeneratorState::Closed => {
                    generator.close(
                        return_value
                            .map(|value| self.decode_baseline_value(value))
                            .transpose()?,
                    );
                }
                state => generator.set_state(state),
            }
            self.direct_value_slots[index].kind =
                php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_GENERATOR;
            self.direct_value_slots[index].payload = generator.id();
            self.baseline_values
                .direct_generator_handles
                .insert(generator.id(), index as u32);
            self.baseline_values
                .direct_generator_cells
                .insert(index, generator.clone());
            return Ok(Value::Generator(generator));
        }
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_COLD_GENERATOR {
            let generator = self
                .cold_generator(index)
                .cloned()
                .ok_or_else(|| format!("cold native Generator {index} has no stable identity"))?;
            return Ok(Value::Generator(generator));
        }
        if matches!(
            slot.kind,
            php_jit::JIT_NATIVE_VALUE_VIEW_FOREACH_DIRECT
                | php_jit::JIT_NATIVE_VALUE_VIEW_COLD_ITERATOR
        ) {
            return Err(format!("direct native value {index} is a foreach iterator"));
        }
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE {
            let view = self
                .direct_prepared_callable_view(index)
                .copied()
                .ok_or_else(|| format!("direct native callable {index} has no stable record"))?;
            let name = self
                .native_callable_string(view.name_bytes, view.name_length)
                .ok_or_else(|| format!("direct native callable {index} lost its name bytes"))?;
            let method = self
                .native_callable_string(view.method_bytes, view.method_length)
                .ok_or_else(|| format!("direct native callable {index} lost its method bytes"))?;
            let class = self
                .native_callable_string(view.class_bytes, view.class_length)
                .ok_or_else(|| format!("direct native callable {index} lost its class bytes"))?;
            let scope = (view.flags & php_jit::JIT_NATIVE_PREPARED_CALLABLE_HAS_SCOPE != 0)
                .then(|| name.clone());
            match view.kind {
                php_jit::JIT_NATIVE_CALLABLE_KIND_USER_FUNCTION => {
                    return Ok(Value::Callable(Box::new(
                        php_runtime::api::CallableValue::UserFunction { name },
                    )));
                }
                php_jit::JIT_NATIVE_CALLABLE_KIND_INTERNAL_BUILTIN => {
                    return Ok(Value::Callable(Box::new(
                        php_runtime::api::CallableValue::InternalBuiltin { name },
                    )));
                }
                php_jit::JIT_NATIVE_CALLABLE_KIND_METHOD_PLACEHOLDER => {
                    return Ok(Value::Callable(Box::new(
                        php_runtime::api::CallableValue::MethodPlaceholder { target: name },
                    )));
                }
                php_jit::JIT_NATIVE_CALLABLE_KIND_UNRESOLVED_DYNAMIC => {
                    return Ok(Value::Callable(Box::new(
                        php_runtime::api::CallableValue::UnresolvedDynamic { target: name },
                    )));
                }
                php_jit::JIT_NATIVE_CALLABLE_KIND_BOUND_CLASS_METHOD => {
                    return Ok(Value::Callable(Box::new(
                        php_runtime::api::CallableValue::BoundMethod {
                            target: php_runtime::api::CallableMethodTarget::Class(class),
                            method,
                            scope,
                        },
                    )));
                }
                php_jit::JIT_NATIVE_CALLABLE_KIND_BOUND_OBJECT_METHOD => {
                    let Value::Object(object) = self.decode_baseline_value(view.receiver)? else {
                        return Err(format!(
                            "direct native callable {index} lost its bound object"
                        ));
                    };
                    return Ok(Value::Callable(Box::new(
                        php_runtime::api::CallableValue::BoundMethod {
                            target: php_runtime::api::CallableMethodTarget::Object(object),
                            method,
                            scope,
                        },
                    )));
                }
                php_jit::JIT_NATIVE_CALLABLE_KIND_CLOSURE => {}
                _ => {
                    return Err(format!(
                        "direct native callable {index} has invalid kind {}",
                        view.kind
                    ));
                }
            }
            let (mut closure, capture_descriptors, captures, implicit_this) = {
                let prepared = self
                    .direct_prepared_closure(index)
                    .ok_or_else(|| format!("direct native closure {index} has no stable record"))?;
                if prepared.closure.id != slot.payload
                    || prepared.capture_descriptors.len() != prepared.captures.len()
                {
                    return Err(format!(
                        "direct native closure {index} record is inconsistent"
                    ));
                }
                (
                    prepared.closure.clone(),
                    prepared.capture_descriptors.clone(),
                    prepared.captures.clone(),
                    prepared.implicit_this,
                )
            };
            closure.bound_this = match implicit_this {
                Some(encoded) => match self.decode_baseline_value(encoded)? {
                    Value::Object(object) => Some(object),
                    value => {
                        return Err(format!(
                            "direct native closure {index} bound object became {}",
                            native_value_type_name(&value)
                        ));
                    }
                },
                None => None,
            };
            closure.captures = capture_descriptors
                .iter()
                .zip(captures.iter().copied())
                .map(|((name, by_reference), encoded)| {
                    let value = self.decode_baseline_value(encoded)?;
                    if *by_reference {
                        let Value::Reference(reference) = value else {
                            return Err(format!(
                                "direct native closure {index} capture ${name} lost reference identity"
                            ));
                        };
                        Ok(php_runtime::api::ClosureCaptureValue::by_reference(
                            name.clone(),
                            reference,
                        ))
                    } else {
                        Ok(php_runtime::api::ClosureCaptureValue::by_value(
                            name.clone(),
                            value,
                        ))
                    }
                })
                .collect::<Result<Vec<_>, String>>()?;
            return Ok(Value::Callable(Box::new(
                php_runtime::api::CallableValue::Closure(closure),
            )));
        }
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT {
            self.demote_direct_object_property_slots(index)?;
            let object = self
                .direct_object(index)
                .ok_or_else(|| format!("direct native object {index} has no stable owner"))?;
            return Ok(Value::Object(object));
        }
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_RESOURCE {
            let resource = self
                .direct_resource(index)
                .ok_or_else(|| format!("direct native resource {index} has no stable owner"))?;
            return Ok(Value::Resource(resource));
        }
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR {
            if slot.flags != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
                || native_reference_state(slot.reserved)
                    == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
            {
                return Err(format!(
                    "direct native reference {index} has no published scalar"
                ));
            }
            let reference = self
                .baseline_values
                .direct_reference_cells
                .get(&index)
                .cloned()
                .unwrap_or_else(|| php_runtime::api::ReferenceCell::new(Value::Null));
            // Publish the stable identity before decoding its payload. A
            // recursive array/reference graph can now resolve this same
            // ReferenceCell and finish constructing the cycle instead of
            // re-entering the native payload indefinitely.
            self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
                kind: php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR,
                flags: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
                reserved: slot.reserved & php_jit::JIT_NATIVE_REFERENCE_TYPED_PROPERTY_GUARD,
                payload: reference.native_scalar_view_address() as u64,
                aux: reference.native_array_view_address() as u64,
                ..slot
            };
            self.baseline_values
                .direct_reference_cells
                .insert(index, reference.clone());
            if !self
                .baseline_values
                .materialized_direct_references
                .contains(&index)
            {
                self.baseline_values
                    .materialized_direct_references
                    .push(index);
            }
            let value = match self.decode_baseline_value(slot.payload as i64) {
                Ok(value) => value,
                Err(error) => {
                    self.baseline_values
                        .materialized_direct_references
                        .retain(|candidate| *candidate != index);
                    self.direct_value_slots[index] = slot;
                    return Err(error);
                }
            };
            reference.set(value);
            // The cold `ReferenceCell` now owns the materialized PHP value;
            // the direct payload ownership ended at this exact boundary.
            self.release(slot.payload as i64)?;
            return Ok(Value::Reference(reference));
        }
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR {
            let reference = self
                .baseline_values
                .direct_reference_cells
                .get(&index)
                .cloned()
                .ok_or_else(|| {
                    format!("materialized direct native reference {index} has no cell")
                })?;
            self.materialize_referenced_object(&reference)?;
            return Ok(Value::Reference(reference));
        }
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT {
            return Ok(Value::Float(php_runtime::api::FloatValue::from_f64(
                f64::from_bits(slot.payload),
            )));
        }
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_GLOBALS_PROXY {
            return self.materialize_native_globals_array();
        }
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_STRING {
            return self.baseline_decode_direct_array(index);
        }
        let length = usize::try_from(slot.payload)
            .map_err(|_| format!("direct native string {index} length overflow"))?;
        let base = self.direct_string_bytes.as_ptr() as usize;
        let address = usize::try_from(slot.aux)
            .map_err(|_| format!("direct native string {index} address overflow"))?;
        let start = address
            .checked_sub(base)
            .ok_or_else(|| format!("direct native string {index} is outside its arena"))?;
        let bytes = self
            .direct_string_bytes
            .get(start..start.saturating_add(length))
            .ok_or_else(|| format!("direct native string {index} bytes are outside its arena"))?;
        Ok(Value::String(PhpString::from_bytes(bytes.to_vec())))
    }

    /// Materializes one authoritative native value only after execution has
    /// entered the baseline/cold coordinator or the final outer-result
    /// boundary. Optimizing and exact native handlers cannot name this API.
    #[track_caller]
    pub(super) fn decode_baseline_value(&mut self, encoded: i64) -> Result<Value, String> {
        if let Some(constant) = php_jit::jit_decode_constant(encoded) {
            if constant == u32::MAX {
                return Ok(Value::Null);
            }
            if constant == php_jit::JIT_VALUE_UNINITIALIZED {
                return Ok(Value::Uninitialized);
            }
            if constant == php_jit::JIT_VALUE_FALSE {
                return Ok(Value::Bool(false));
            }
            if constant == php_jit::JIT_VALUE_TRUE {
                return Ok(Value::Bool(true));
            }
            let constant_index = constant as usize;
            let cache_key = (self.current_dynamic_unit, constant_index);
            if let Some(value) = self
                .baseline_values
                .decoded_constant_cache
                .borrow()
                .get(&cache_key)
            {
                return Ok(value.clone());
            }
            let constant = self.unit.constants.get(constant_index).ok_or_else(|| {
                let mut candidates = self
                    .dynamic_units
                    .iter()
                    .enumerate()
                    .filter_map(|(unit, package)| {
                        let source_unit = package.compiled.unit();
                        let candidate = source_unit.constants.get(constant_index)?;
                        let source = source_unit
                            .files
                            .first()
                            .map_or("<unknown>", |file| file.path.as_str());
                        let mut literal = format!("{candidate:?}");
                        literal.truncate(160);
                        Some(format!("dynamic[{unit}]={source}:{literal}"))
                    })
                    .take(64)
                    .collect::<Vec<_>>();
                if let Some(candidate) = self.compiled.unit().constants.get(constant_index) {
                    let source = self
                        .compiled
                        .unit()
                        .files
                        .first()
                        .map_or("<unknown>", |file| file.path.as_str());
                    let mut literal = format!("{candidate:?}");
                    literal.truncate(160);
                    candidates.insert(0, format!("compiled={source}:{literal}"));
                }
                let candidates = if candidates.is_empty() {
                    "none".to_owned()
                } else {
                    candidates.join(", ")
                };
                    format!(
                        "native constant {constant} is missing from active unit {} (dynamic={:?}, constants={}, source={}, candidates=[{}])",
                        self.unit.id.raw(),
                        self.current_dynamic_unit,
                        self.unit.constants.len(),
                        self.unit
                            .files
                            .first()
                            .map_or("<unknown>", |file| file.path.as_str()),
                        candidates,
                    )
                })?;
            // Constants embedded in native operands can still require the
            // active request context (for example a runtime-defined constant
            // used as a default argument in a bounded large-unit call graph).
            let value = native_runtime_constant_value(self, constant)?;
            self.baseline_values
                .decoded_constant_cache
                .borrow_mut()
                .insert(cache_key, value.clone());
            return Ok(value);
        }
        if let Some(index) = php_jit::jit_decode_runtime_value(encoded) {
            if let Some(direct) = Self::direct_value_index(encoded) {
                return self.baseline_decode_direct_value(direct);
            }
            return Err(format!(
                "native runtime value {index} is outside the authoritative direct slot plane"
            ));
        }
        Ok(Value::Int(encoded))
    }

    /// Publishes a cold Rust value at the baseline/cold compatibility
    /// boundary. Ordinary optimizing producers publish native slots directly.
    #[track_caller]
    pub(super) fn encode_baseline_value(&mut self, value: Value) -> Result<i64, String> {
        let root = self.begin_direct_array_encode();
        let result = self.baseline_encode_unscoped(value);
        self.finish_direct_array_encode(root, result)
    }

    #[track_caller]
    fn baseline_encode_unscoped(&mut self, value: Value) -> Result<i64, String> {
        let value = match value {
            Value::Array(array) => return self.encode_direct_array_value_unscoped(array),
            Value::String(string) => return self.encode_native_string_owner(string),
            Value::Float(value) => return self.encode_native_float_owner(value),
            Value::Object(object) => return self.encode_native_object_owner(object),
            Value::Reference(reference) => return self.encode_native_reference_owner(reference),
            Value::Callable(callable) => return self.encode_prepared_callable(callable),
            Value::Fiber(fiber) => return self.encode_native_fiber_owner(fiber),
            Value::Generator(generator) => return self.encode_native_generator_owner(generator),
            Value::Resource(resource) => return self.encode_native_resource_owner(resource),
            value => value,
        };
        match &value {
            Value::Null => return Ok(php_jit::jit_encode_constant(u32::MAX)),
            Value::Uninitialized => {
                return Ok(php_jit::jit_encode_constant(
                    php_jit::JIT_VALUE_UNINITIALIZED,
                ));
            }
            Value::Bool(false) => {
                return Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE));
            }
            Value::Bool(true) => {
                return Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE));
            }
            Value::Int(value) => return self.encode_native_int(*value),
            _ => {}
        }
        unreachable!("all PHP value families are encoded above")
    }

    /// Materializes a by-value baseline operand without first converting an
    /// authoritative direct reference into the cold `ReferenceCell` plane.
    /// Prepared builtin parameters are by value unless their arginfo says
    /// otherwise, so their ordinary path can decode the published payload
    /// directly and leave reference identity/storage native.
    pub(super) fn baseline_decode_dereferenced_native_value(
        &mut self,
        encoded: i64,
    ) -> Result<Value, String> {
        let encoded = self.dereference_direct_encoding(encoded);
        let mut value = self.decode_baseline_value(encoded)?;
        for _ in 0..16 {
            let Value::Reference(reference) = value else {
                return Ok(value);
            };
            value = reference.get();
        }
        Ok(value)
    }
}

impl<'a> NativeRequestColdState<'a> {
    /// Establishes the cold `ReferenceCell` identities used only to import
    /// request globals and to project them at an explicit baseline boundary.
    pub(super) fn ensure_native_global_references(&mut self) {
        const RUNTIME_GLOBALS: &[&str] = &[
            "argc", "argv", "_SERVER", "_ENV", "_GET", "_POST", "_COOKIE", "_REQUEST", "_FILES",
            "_SESSION",
        ];
        for name in RUNTIME_GLOBALS {
            if self.baseline_values.inherited_globals.contains_key(*name) {
                continue;
            }
            let Some(value) = self.options.runtime_context.global_value(name) else {
                continue;
            };
            let reference = match value {
                Value::Reference(reference) => reference,
                value => php_runtime::api::ReferenceCell::new(value),
            };
            self.baseline_values
                .inherited_globals
                .insert((*name).to_owned(), Value::Reference(reference));
        }
        for value in self.baseline_values.inherited_globals.values_mut() {
            if matches!(value, Value::Reference(_) | Value::Uninitialized) {
                continue;
            }
            let reference = php_runtime::api::ReferenceCell::new(value.clone());
            *value = Value::Reference(reference);
        }
    }

    /// Returns the canonical native reference for one cold-owned request
    /// global, republishing a materialized payload before native re-entry.
    pub(super) fn native_global_reference_handle(
        &mut self,
        name: &str,
    ) -> Result<Option<i64>, String> {
        self.ensure_native_global_references();
        let Some(global) = self.baseline_values.inherited_globals.get(name).cloned() else {
            return Ok(None);
        };
        if matches!(global, Value::Uninitialized) {
            return Ok(None);
        }
        let Value::Reference(reference) = global else {
            return Err(format!("native global ${name} has no reference identity"));
        };
        let reference_identity = reference.gc_debug_id();
        let reusable = self
            .native_global_reference_handles
            .get(name)
            .copied()
            .filter(|encoded| self.native_reference_identity(*encoded) == Some(reference_identity));
        let encoded = if let Some(encoded) = reusable {
            self.restore_authoritative_direct_reference(encoded)?;
            encoded
        } else {
            if let Some(stale) = self.native_global_reference_handles.remove(name) {
                self.release(stale)?;
            }
            let encoded = self.encode_native_reference_owner(reference)?;
            self.native_global_reference_handles
                .insert(name.to_owned(), encoded);
            encoded
        };
        Ok(Some(encoded))
    }

    pub(super) fn duplicate_native_global_value(
        &mut self,
        name: &str,
    ) -> Result<Option<i64>, String> {
        self.ensure_native_global_references();
        if let Some(encoded) = self
            .native_global_reference_handles
            .get(name)
            .copied()
            .filter(|encoded| self.native_reference_identity(*encoded).is_some())
        {
            if self.native_encoded_value_kind(encoded)
                == Some(NativeEncodedValueKind::Uninitialized)
            {
                return Ok(Some(php_jit::jit_encode_constant(u32::MAX)));
            }
            return self.duplicate_dereferenced_native_value(encoded).map(Some);
        }
        if matches!(
            self.baseline_values.inherited_globals.get(name),
            Some(Value::Uninitialized)
        ) {
            return Ok(Some(php_jit::jit_encode_constant(u32::MAX)));
        }
        let Some(encoded) = self.native_global_reference_handle(name)? else {
            return Ok(None);
        };
        self.duplicate_dereferenced_native_value(encoded).map(Some)
    }

    pub(super) fn native_request_local_handle(&mut self, name: &str) -> Result<i64, String> {
        self.ensure_native_global_references();
        if let Some(encoded) = self
            .native_global_reference_handles
            .get(name)
            .copied()
            .filter(|encoded| self.native_reference_identity(*encoded).is_some())
        {
            self.restore_authoritative_direct_reference(encoded)?;
            return Ok(encoded);
        }
        if let Some(encoded) = self.native_global_reference_handle(name)? {
            return Ok(encoded);
        }

        let reference = php_runtime::api::ReferenceCell::new(Value::Uninitialized);
        let encoded = self.encode_native_reference_owner(reference)?;
        if let Some(stale) = self
            .native_global_reference_handles
            .insert(name.to_owned(), encoded)
        {
            self.release(stale)?;
        }
        Ok(encoded)
    }

    pub(super) fn materialize_native_request_global(&mut self, name: &str) -> Result<(), String> {
        let Some(encoded) = self.native_global_reference_handles.get(name).copied() else {
            return Ok(());
        };
        let Value::Reference(reference) = self.decode_baseline_value(encoded).map_err(|error| {
            format!("native request global ${name} could not materialize: {error}")
        })?
        else {
            return Err(format!(
                "native request global ${name} lost its reference identity"
            ));
        };
        self.baseline_values
            .inherited_globals
            .insert(name.to_owned(), Value::Reference(reference));
        Ok(())
    }

    pub(super) fn materialize_native_request_globals(&mut self) -> Result<(), String> {
        let names = self
            .native_global_reference_handles
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for name in names {
            self.materialize_native_request_global(&name)?;
        }
        Ok(())
    }

    /// Allocates the baseline-only `$GLOBALS` marker. Optimizing global
    /// operations use trusted numeric reference slots and never consume this
    /// compatibility array view.
    pub(super) fn encode_globals_proxy(&mut self) -> Result<i64, String> {
        self.ensure_native_global_references();
        let index = self.reserve_direct_value_slot()?;
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_GLOBALS_PROXY,
            ..php_jit::JitNativeValueSlot::default()
        };
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .ok_or_else(|| "direct native globals marker handle overflow".to_owned())?;
        Ok(php_jit::jit_encode_typed_runtime_value(
            runtime_index,
            php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG,
        ))
    }

    pub(super) fn is_globals_proxy(&self, encoded: i64) -> bool {
        Self::direct_value_index(encoded).is_some_and(|index| {
            self.direct_value_slots.get(index).is_some_and(|slot| {
                slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_GLOBALS_PROXY
            })
        })
    }

    fn native_global_name<'b>(
        key: &'b php_runtime::api::ArrayKey,
    ) -> Option<std::borrow::Cow<'b, str>> {
        let php_runtime::api::ArrayKey::String(name) = key else {
            return None;
        };
        let name = String::from_utf8_lossy(name.as_bytes());
        (name.as_ref() != "GLOBALS").then_some(name)
    }

    pub(super) fn fetch_native_global_dimension(
        &mut self,
        key: &php_runtime::api::ArrayKey,
    ) -> Result<Option<Value>, String> {
        self.ensure_native_global_references();
        let Some(name) = Self::native_global_name(key) else {
            return Ok(None);
        };
        self.materialize_native_request_global(name.as_ref())?;
        Ok(self
            .baseline_values
            .inherited_globals
            .get(name.as_ref())
            .filter(|value| {
                !matches!(value, Value::Uninitialized)
                    && !matches!(value, Value::Reference(reference) if matches!(reference.get(), Value::Uninitialized))
            })
            .cloned())
    }

    pub(super) fn replace_direct_reference_cell_value(
        &mut self,
        reference: &php_runtime::api::ReferenceCell,
        replacement: Value,
    ) -> Result<Option<Value>, String> {
        let Some(index) = self
            .baseline_values
            .direct_reference_cells
            .iter()
            .find_map(|(index, candidate)| candidate.ptr_eq(reference).then_some(*index))
        else {
            return Ok(None);
        };
        let Some(slot) = self.direct_value_slots.get(index).copied().filter(|slot| {
            slot.refcount != 0
                && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
                && native_reference_state(slot.reserved)
                    != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
        }) else {
            return Ok(None);
        };
        let encoded = self.encode_baseline_value(replacement.clone())?;
        self.cross_unit_stable_values.remove(&index);
        self.direct_value_slots[index].payload = encoded as u64;
        self.direct_value_slots[index].reserved =
            php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED
                | (slot.reserved & php_jit::JIT_NATIVE_REFERENCE_TYPED_PROPERTY_GUARD);
        let previous = self.decode_baseline_value(slot.payload as i64)?;
        self.release(slot.payload as i64)?;
        // The direct descriptor is authoritative again. Keep only the stable
        // alias identity in the cold cell; a later baseline read explicitly
        // materializes the current native payload.
        reference.set(Value::Uninitialized);
        Ok(Some(previous))
    }

    pub(super) fn store_native_global_dimension(
        &mut self,
        key: &php_runtime::api::ArrayKey,
        mut replacement: Value,
    ) -> Result<bool, String> {
        self.ensure_native_global_references();
        let Some(name) = Self::native_global_name(key) else {
            return Ok(false);
        };
        self.materialize_native_request_global(name.as_ref())?;
        if let Value::Reference(reference) = replacement {
            replacement = reference.get();
        }
        if let Some(Value::Reference(reference)) = self
            .baseline_values
            .inherited_globals
            .get(name.as_ref())
            .cloned()
        {
            let previous = if let Some(previous) =
                self.replace_direct_reference_cell_value(&reference, replacement.clone())?
            {
                previous
            } else {
                let previous = reference.get();
                reference.set(replacement.clone());
                previous
            };
            self.mark_rooted_container_dirty(&Value::Reference(reference));
            self.finalize_replaced_value(previous)?;
        } else {
            self.baseline_values.inherited_globals.insert(
                name.into_owned(),
                Value::Reference(php_runtime::api::ReferenceCell::new(replacement)),
            );
            self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
        }
        Ok(true)
    }

    pub(super) fn unset_native_global_reference(
        &mut self,
        encoded_reference: i64,
    ) -> Result<bool, String> {
        self.ensure_native_global_references();
        let Some(reference_identity) = self.native_reference_identity(encoded_reference) else {
            return Ok(false);
        };
        let Some(name) = self
            .native_global_reference_handles
            .iter()
            .find_map(|(name, encoded)| {
                (self.native_reference_identity(*encoded) == Some(reference_identity))
                    .then(|| name.clone())
            })
        else {
            return Ok(false);
        };
        self.materialize_native_request_global(&name)?;
        if let Some(Value::Reference(reference)) =
            self.baseline_values.inherited_globals.get(&name).cloned()
        {
            if reference.gc_debug_id() != reference_identity {
                return Ok(false);
            }
            self.invalidate_native_global_reference(reference.gc_debug_id())?;
        }
        let previous = self
            .baseline_values
            .inherited_globals
            .insert(name, Value::Uninitialized);
        if let Some(Value::Reference(reference)) = previous {
            self.finalize_replaced_value(reference.get())?;
        }
        self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
        // Root unset detaches the old reference identity. Rebuild every
        // numeric request-local and constant-dimension plan now, at this cold
        // semantic boundary, so optimizing invocations never validate or
        // rediscover the replacement symbol.
        self.republish_trusted_global_references_for_all_units()?;
        Ok(true)
    }

    pub(super) fn rebind_native_global_reference(
        &mut self,
        destination: i64,
        source: i64,
    ) -> Result<bool, String> {
        self.ensure_native_global_references();
        let Some(destination_identity) = self.native_reference_identity(destination) else {
            return Ok(false);
        };
        let Some(name) = self
            .native_global_reference_handles
            .iter()
            .find_map(|(name, encoded)| {
                (self.native_reference_identity(*encoded) == Some(destination_identity))
                    .then(|| name.clone())
            })
        else {
            return Ok(false);
        };
        let Some(source_cell) = self.direct_native_reference_cell(source) else {
            return Ok(false);
        };
        self.materialize_native_request_global(&name)?;
        self.invalidate_native_global_reference(destination_identity)?;
        self.baseline_values
            .inherited_globals
            .insert(name.clone(), Value::Reference(source_cell));
        // The global map and every publication-time request-local slot own
        // the new canonical reference. Generated request-global reads use
        // those authoritative stable slots directly, so local, linked, and
        // late-prepared callers cannot retain the invalidated identity.
        self.rebind_native_request_local_reference(&name, source)?;
        self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
        self.republish_trusted_global_references_for_all_units()?;
        Ok(true)
    }

    pub(super) fn reference_native_global_dimension(
        &mut self,
        key: &php_runtime::api::ArrayKey,
    ) -> Result<Option<php_runtime::api::ReferenceCell>, String> {
        self.ensure_native_global_references();
        let Some(name) = Self::native_global_name(key) else {
            return Ok(None);
        };
        self.materialize_native_request_global(name.as_ref())?;
        if let Some(Value::Reference(reference)) =
            self.baseline_values.inherited_globals.get(name.as_ref())
        {
            return Ok(Some(reference.clone()));
        }
        let reference = php_runtime::api::ReferenceCell::new(Value::Null);
        self.baseline_values
            .inherited_globals
            .insert(name.into_owned(), Value::Reference(reference.clone()));
        self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
        Ok(Some(reference))
    }

    /// Republishes a reference after an explicit cold materialization.
    ///
    /// Alias identity remains in the stable `ReferenceCell`, but native
    /// re-entry receives only the authoritative direct descriptor.
    pub(super) fn restore_authoritative_direct_reference(
        &mut self,
        encoded: i64,
    ) -> Result<(), String> {
        let Some(index) = Self::direct_value_index(encoded) else {
            return Err("native request reference is not a direct handle".to_owned());
        };
        let slot = self
            .direct_value_slots
            .get(index)
            .copied()
            .filter(|slot| slot.refcount != 0)
            .ok_or_else(|| "native request reference points at a dead slot".to_owned())?;
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR {
            return Ok(());
        }
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
            || slot.flags != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
        {
            return Err("native request reference has no stable scalar representation".to_owned());
        }
        let reference = self
            .baseline_values
            .direct_reference_cells
            .get(&index)
            .cloned()
            .ok_or_else(|| "materialized native request reference has no identity".to_owned())?;
        let value = reference.get();
        let typed_guard = slot.reserved & php_jit::JIT_NATIVE_REFERENCE_TYPED_PROPERTY_GUARD;
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
            flags: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
            reserved: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY | typed_guard,
            payload: 0,
            aux: 0,
            ..slot
        };
        let payload = match self.encode_baseline_value(value) {
            Ok(payload) => payload,
            Err(error) => {
                self.direct_value_slots[index] = slot;
                return Err(error);
            }
        };
        let direct = self
            .direct_value_slots
            .get_mut(index)
            .ok_or_else(|| format!("native request reference {index} slot disappeared"))?;
        direct.reserved = php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED | typed_guard;
        direct.payload = payload as u64;
        // End the cold ownership interval here instead of leaving a second
        // authoritative Rust payload for the next native store to invalidate.
        reference.set(Value::Uninitialized);
        self.mark_rooted_container_dirty(&Value::Reference(reference));
        Ok(())
    }

    /// Ends one explicit baseline reference-materialization boundary before
    /// compiled native execution is entered or resumed.
    pub(super) fn restore_materialized_direct_references(&mut self) -> Result<(), String> {
        while let Some(index) = self.baseline_values.materialized_direct_references.pop() {
            let Some(slot) = self
                .direct_value_slots
                .get(index)
                .copied()
                .filter(|slot| slot.refcount != 0)
            else {
                continue;
            };
            if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR {
                continue;
            }
            if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
                || slot.flags != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            {
                continue;
            }
            let runtime_index = u32::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
                .ok_or_else(|| "materialized direct reference handle overflow".to_owned())?;
            let encoded =
                (php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG | u64::from(runtime_index)) as i64;
            if let Err(error) = self.restore_authoritative_direct_reference(encoded) {
                self.baseline_values
                    .materialized_direct_references
                    .push(index);
                return Err(error);
            }
        }
        Ok(())
    }

    /// Gives a baseline-native callee an independently owned argument.
    ///
    /// This is the explicit compatibility boundary for globals proxies and
    /// source-unit constants. Exact and optimizing calls use the
    /// authoritative native ownership primitives instead.
    pub(super) fn duplicate_baseline_call_argument(&mut self, encoded: i64) -> Result<i64, String> {
        if self.is_globals_proxy(encoded) {
            let globals = self.materialize_native_globals_array()?;
            return self.encode_baseline_value(globals);
        }
        if let Some(index) = Self::direct_value_index(encoded) {
            let slot = self
                .direct_value_slots
                .get_mut(index)
                .ok_or_else(|| format!("direct native value {index} is missing"))?;
            if matches!(
                slot.kind,
                php_jit::JIT_NATIVE_VALUE_VIEW_FOREACH_DIRECT
                    | php_jit::JIT_NATIVE_VALUE_VIEW_COLD_ITERATOR
            ) {
                return Err(format!("direct native value {index} is a foreach iterator"));
            }
            slot.refcount = slot
                .refcount
                .checked_add(1)
                .ok_or_else(|| format!("direct native value {index} refcount overflow"))?;
            return Ok(encoded);
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
            return self.stabilize_active_unit_constant(constant);
        }
        Ok(encoded)
    }

    /// Gives a baseline by-value read its own native owner.
    ///
    /// Authoritative references stay native. A reference that was already
    /// materialized by cold compatibility code is republished once; only a
    /// genuinely cold handle is decoded through Rust `Value`.
    pub(super) fn duplicate_dereferenced_native_value(
        &mut self,
        mut encoded: i64,
    ) -> Result<i64, String> {
        if let Some(encoded) = self.duplicate_authoritative_dereferenced_native_value(encoded)? {
            return Ok(encoded);
        }
        for _ in 0..16 {
            let Some(index) = Self::direct_value_index(encoded) else {
                break;
            };
            let Some(slot) = self.direct_value_slots.get(index).copied() else {
                break;
            };
            if slot.refcount == 0 {
                break;
            }
            if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
                && native_reference_state(slot.reserved)
                    != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
            {
                encoded = slot.payload as i64;
                continue;
            }
            if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
                && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            {
                let reference = self
                    .baseline_values
                    .direct_reference_cells
                    .get(&index)
                    .cloned()
                    .ok_or_else(|| {
                        format!("materialized direct native reference {index} has no cell")
                    })?;
                let payload = self.encode_baseline_value(reference.get())?;
                self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
                    kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
                    flags: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
                    reserved: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED
                        | (slot.reserved & php_jit::JIT_NATIVE_REFERENCE_TYPED_PROPERTY_GUARD),
                    payload: payload as u64,
                    ..slot
                };
                encoded = payload;
                continue;
            }
            break;
        }
        if self.php_handle_is_reference(encoded) == Some(true) {
            let mut value = self.decode_baseline_value(encoded)?;
            for _ in 0..16 {
                let Value::Reference(reference) = value else {
                    break;
                };
                value = reference.get();
            }
            return self.encode_baseline_value(value);
        }
        if let Some(encoded) = self.duplicate_authoritative_native_value(encoded)? {
            Ok(encoded)
        } else {
            self.duplicate_baseline_call_argument(encoded)
        }
    }

    pub(super) fn materialize_native_globals_array(&mut self) -> Result<Value, String> {
        self.materialize_native_request_globals()?;
        let mut globals =
            php_runtime::api::PhpArray::with_capacity(self.baseline_values.inherited_globals.len());
        for (name, value) in &self.baseline_values.inherited_globals {
            if name == "GLOBALS"
                || matches!(value, Value::Uninitialized)
                || matches!(value, Value::Reference(reference) if matches!(reference.get(), Value::Uninitialized))
            {
                continue;
            }
            globals.insert(
                php_runtime::api::ArrayKey::String(PhpString::from_bytes(name.as_bytes().to_vec())),
                value.clone(),
            );
        }
        Ok(Value::Array(globals))
    }

    /// Materializes the final native return/exit/throw payload for the public
    /// VM result. This is an outer request boundary after native execution;
    /// optimizing artifacts and exact handlers cannot name or import it.
    pub(in crate::vm) fn materialize_outer_result(
        &mut self,
        encoded: i64,
    ) -> Result<Value, String> {
        self.decode_baseline_value(encoded)
    }
}

impl<'a> NativeRequestColdState<'a> {
    /// Imports effect-free static-property defaults once at publication.
    ///
    /// The resulting storage is an authoritative numeric native slot. Rust
    /// `Value` exists here only while a default or include transfer crosses
    /// this explicit cold boundary.
    pub(super) fn prepare_trusted_static_properties(&mut self) {
        for function_id in self.published_native_functions() {
            let Some(function) = self.prepared_continuation_instructions(function_id) else {
                continue;
            };
            let function_index = function_id.index();
            let caller_function = function_id.raw();
            for (continuation, instruction) in function.iter().enumerate() {
                let Some(instruction) = instruction.as_ref() else {
                    continue;
                };
                let (class_name, property, writable, probe, assignment, fetch, reference) =
                    match &instruction.kind {
                        php_ir::InstructionKind::FetchStaticProperty {
                            class_name,
                            property,
                            ..
                        } => (
                            class_name.as_str(),
                            property.as_str(),
                            false,
                            false,
                            false,
                            true,
                            false,
                        ),
                        php_ir::InstructionKind::IssetStaticProperty {
                            class_name,
                            property,
                            ..
                        }
                        | php_ir::InstructionKind::EmptyStaticProperty {
                            class_name,
                            property,
                            ..
                        }
                        | php_ir::InstructionKind::IssetStaticPropertyDim {
                            class_name,
                            property,
                            ..
                        }
                        | php_ir::InstructionKind::EmptyStaticPropertyDim {
                            class_name,
                            property,
                            ..
                        } => (
                            class_name.as_str(),
                            property.as_str(),
                            false,
                            true,
                            false,
                            false,
                            false,
                        ),
                        php_ir::InstructionKind::AssignStaticProperty {
                            class_name,
                            property,
                            ..
                        } => (
                            class_name.as_str(),
                            property.as_str(),
                            true,
                            false,
                            false,
                            true,
                            false,
                        ),
                        php_ir::InstructionKind::BindReferenceStaticProperty {
                            class_name,
                            property,
                            ..
                        }
                        | php_ir::InstructionKind::BindReferenceFromStaticPropertyDim {
                            class_name,
                            property,
                            ..
                        } => (
                            class_name.as_str(),
                            property.as_str(),
                            true,
                            false,
                            false,
                            false,
                            true,
                        ),
                        php_ir::InstructionKind::UnsetStaticPropertyDim {
                            class_name,
                            property,
                            ..
                        } => (
                            class_name.as_str(),
                            property.as_str(),
                            true,
                            false,
                            false,
                            false,
                            false,
                        ),
                        _ => continue,
                    };

                let calling_class = native_calling_class(self, caller_function);
                let resolved_class = match class_name.to_ascii_lowercase().as_str() {
                    "self" => calling_class.map(|class| class.name.clone()),
                    "parent" => calling_class.and_then(|class| class.parent.clone()),
                    // The declaring slot is shared by descendants unless a
                    // child redeclares the property. Callable publication
                    // rejects such a differing late-static layout; publish
                    // the lexical slot used by every admitted generated body.
                    "static" => calling_class.map(|class| class.name.clone()),
                    _ => Some(class_name.to_owned()),
                };
                let Some(resolved_class) = resolved_class else {
                    continue;
                };
                let normalized = normalize_class_name(&resolved_class);
                let Some(class) = self
                    .unit
                    .classes
                    .iter()
                    .find(|class| class.name == normalized)
                else {
                    continue;
                };
                let class_display_name = class.display_name.clone();
                let declaration = native_static_property_declaration(
                    self,
                    &resolved_class,
                    property,
                    caller_function,
                );
                if declaration.is_none() {
                    let next = *self.static_property_next;
                    let Ok(index) = usize::try_from(next) else {
                        continue;
                    };
                    if index >= self.static_property_slots.capacity() {
                        continue;
                    }
                    self.static_property_slots[index] = php_jit::JitNativeStaticPropertySlot {
                        value: php_jit::jit_encode_constant(u32::MAX),
                        initialized: 1,
                        reserved: 0,
                    };
                    *self.static_property_next = next.saturating_add(1);

                    let contract = if probe {
                        0
                    } else {
                        let (function_name, include_function_frame) =
                            self.unit.functions.get(function_index).map_or_else(
                                || ("{main}".to_owned(), false),
                                |function| (function.name.clone(), !function.flags.is_top_level),
                            );
                        let owner = PreparedNativeThrowableOwner::StaticProperty(Box::new(
                            PreparedNativeStaticPropertyContract {
                                throwable: prepare_native_throwable_site(
                                    self,
                                    "Error",
                                    &function_name,
                                    include_function_frame,
                                    instruction.span,
                                ),
                                owner_display_name: class_display_name,
                                property: property.to_owned(),
                                type_: None,
                                strict_types: self.unit.strict_types_for_span(instruction.span),
                            },
                        ));
                        let pointer = owner.static_property_pointer().unwrap_or(0);
                        let plan_index = self
                            .trusted_property_function_offsets
                            .get(function_index)
                            .copied()
                            .and_then(|base| usize::try_from(base).ok())
                            .map(|base| base.saturating_add(continuation));
                        if let Some(plan_index) = plan_index {
                            self.trusted_exception_plan_owners.insert(plan_index, owner);
                        }
                        pointer
                    };
                    let Some(base) = self
                        .trusted_property_function_offsets
                        .get(function_index)
                        .copied()
                        .and_then(|base| usize::try_from(base).ok())
                    else {
                        continue;
                    };
                    if let Some(plan) = self
                        .trusted_static_property_slots
                        .get_mut(base.saturating_add(continuation))
                    {
                        *plan = php_jit::JitNativeTrustedStaticPropertySlot {
                            state: if probe {
                                php_jit::JIT_NATIVE_TRUSTED_STATIC_PROPERTY_ABSENT
                            } else {
                                php_jit::JIT_NATIVE_TRUSTED_STATIC_PROPERTY_ERROR
                            },
                            slot_index: next,
                            contract,
                        };
                    }
                    continue;
                }
                let declaration = declaration.expect("static declaration was checked above");
                if declaration.owner_unit.is_some()
                    || declaration.flags.is_readonly
                    || declaration.has_deferred_default
                    || ((declaration.flags.is_private || declaration.flags.is_protected)
                        && !declaration.caller_owns_scope)
                {
                    continue;
                }

                let key = (declaration.owner_name, property.to_owned());
                let slot_index = if let Some(index) = self.static_property_indices.get(&key) {
                    *index
                } else {
                    let next = *self.static_property_next;
                    let Ok(index) = usize::try_from(next) else {
                        continue;
                    };
                    if index >= self.static_property_slots.capacity() {
                        continue;
                    }
                    let inherited = self.baseline_values.static_property_transfer.remove(&key);
                    let default = declaration
                        .default
                        .and_then(|constant| self.unit.constants.get(constant.index()))
                        .cloned();
                    let (value, initialized) = match inherited {
                        Some(value) => (value, true),
                        None => match default.as_ref() {
                            Some(value) => match native_runtime_constant_value(self, value) {
                                Ok(value) => (value, true),
                                Err(_) => continue,
                            },
                            None if declaration.type_.is_some() => (Value::Uninitialized, false),
                            None => (Value::Null, true),
                        },
                    };
                    let encoded = match self.encode_baseline_value(value.clone()) {
                        Ok(encoded) => encoded,
                        Err(_) => {
                            self.baseline_values
                                .static_property_transfer
                                .insert(key.clone(), value);
                            continue;
                        }
                    };
                    self.static_property_slots[index] = php_jit::JitNativeStaticPropertySlot {
                        value: encoded,
                        initialized: u32::from(initialized),
                        reserved: 0,
                    };
                    *self.static_property_next = next.saturating_add(1);
                    self.static_property_indices.insert(key, next);
                    next
                };

                let Some(base) = self
                    .trusted_property_function_offsets
                    .get(function_index)
                    .copied()
                    .and_then(|base| usize::try_from(base).ok())
                else {
                    continue;
                };
                let contract = if (assignment || fetch || reference) && declaration.type_.is_some()
                {
                    let (function_name, include_function_frame) =
                        self.unit.functions.get(function_index).map_or_else(
                            || ("{main}".to_owned(), false),
                            |function| (function.name.clone(), !function.flags.is_top_level),
                        );
                    let owner = PreparedNativeThrowableOwner::StaticProperty(Box::new(
                        PreparedNativeStaticPropertyContract {
                            throwable: prepare_native_throwable_site(
                                self,
                                if fetch { "Error" } else { "TypeError" },
                                &function_name,
                                include_function_frame,
                                instruction.span,
                            ),
                            owner_display_name: declaration.owner_display_name.clone(),
                            property: property.to_owned(),
                            type_: declaration.type_.clone(),
                            strict_types: self.unit.strict_types_for_span(instruction.span),
                        },
                    ));
                    let pointer = owner.static_property_pointer().unwrap_or(0);
                    self.trusted_exception_plan_owners
                        .insert(base.saturating_add(continuation), owner);
                    pointer
                } else {
                    0
                };
                let Some(plan) = self
                    .trusted_static_property_slots
                    .get_mut(base.saturating_add(continuation))
                else {
                    continue;
                };
                *plan = php_jit::JitNativeTrustedStaticPropertySlot {
                    state: if (assignment || reference) && contract != 0 {
                        php_jit::JIT_NATIVE_TRUSTED_STATIC_PROPERTY_TYPED
                    } else if writable {
                        php_jit::JIT_NATIVE_TRUSTED_STATIC_PROPERTY_WRITABLE
                    } else {
                        php_jit::JIT_NATIVE_TRUSTED_STATIC_PROPERTY_READABLE
                    },
                    slot_index,
                    contract,
                };
            }
        }
    }

    pub(super) fn direct_static_property_value(
        &mut self,
        key: &(String, String),
    ) -> Option<Result<Value, String>> {
        let encoded = self.direct_static_property_encoded(key)?;
        Some(self.decode_baseline_value(encoded))
    }

    /// Publishes a lazily resolved static property into the authoritative
    /// native slot plane after its one cold semantic lookup.
    pub(super) fn ensure_direct_static_property_encoded(
        &mut self,
        key: &(String, String),
        value: Value,
    ) -> Result<i64, String> {
        if let Some(encoded) = self.direct_static_property_encoded(key) {
            return Ok(encoded);
        }
        let index = usize::try_from(*self.static_property_next)
            .map_err(|_| "native static property index overflow".to_owned())?;
        if index >= self.static_property_slots.capacity() {
            return Err(format!(
                "native static property arena exhausted at {} slots",
                index.saturating_add(1)
            ));
        }
        let encoded = self.encode_baseline_value(value)?;
        self.static_property_slots[index] = php_jit::JitNativeStaticPropertySlot {
            value: encoded,
            initialized: 1,
            reserved: 0,
        };
        *self.static_property_next = u32::try_from(index.saturating_add(1))
            .map_err(|_| "native static property index overflow".to_owned())?;
        self.static_property_indices.insert(
            key.clone(),
            u32::try_from(index).map_err(|_| "native static property index overflow".to_owned())?,
        );
        self.mark_roots_dirty(RootMutationReason::EnumOrStaticObject);
        Ok(encoded)
    }

    /// Replaces the owner held by an authoritative native static slot at a
    /// cold `Value` boundary.
    pub(super) fn store_direct_static_property_value(
        &mut self,
        key: &(String, String),
        value: Value,
    ) -> Option<Result<(), String>> {
        let index = usize::try_from(*self.static_property_indices.get(key)?).ok()?;
        let encoded = match self.encode_baseline_value(value) {
            Ok(encoded) => encoded,
            Err(error) => return Some(Err(error)),
        };
        let previous = self.static_property_slots[index].value;
        self.static_property_slots[index].value = encoded;
        self.static_property_slots[index].initialized = 1;
        self.mark_roots_dirty(RootMutationReason::EnumOrStaticObject);
        Some(self.release(previous))
    }

    /// Include execution moves static-property symbols between independently
    /// owned native contexts. Materialize only at that cold ownership
    /// boundary and relinquish every slot owner before constructing the child.
    pub(super) fn demote_trusted_static_properties(&mut self) {
        let entries = self
            .static_property_indices
            .iter()
            .map(|(key, index)| (key.clone(), *index))
            .collect::<Vec<_>>();
        for (key, index) in entries {
            let Ok(index) = usize::try_from(index) else {
                continue;
            };
            let Some(slot) = self.static_property_slots.get(index).copied() else {
                continue;
            };
            let Ok(value) = self.decode_baseline_value(slot.value) else {
                continue;
            };
            self.baseline_values
                .static_property_transfer
                .insert(key, value);
            self.static_property_slots[index] = php_jit::JitNativeStaticPropertySlot::default();
            let _ = self.release(slot.value);
        }
        let used = usize::try_from(*self.static_property_next).unwrap_or(0);
        self.static_property_slots.discard_prefix(used);
        *self.static_property_next = 0;
        self.static_property_indices.clear();
        for range in self.published_continuation_ranges() {
            self.trusted_static_property_slots[range]
                .fill(php_jit::JitNativeTrustedStaticPropertySlot::default());
        }
        self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
    }

    /// Publishes effect-free function-static defaults at the cold
    /// request/publication boundary. Native execution receives only the
    /// resulting numeric reference slot.
    pub(super) fn prepare_trusted_static_locals(&mut self) {
        for function in self.published_native_functions() {
            let Some(instructions) = self.prepared_continuation_instructions(function) else {
                continue;
            };
            let function = function.raw();
            for instruction in instructions.iter().flatten() {
                let php_ir::InstructionKind::InitStaticLocal { local, default, .. } =
                    &instruction.kind
                else {
                    continue;
                };
                let php_ir::Operand::Constant(constant) = default else {
                    continue;
                };
                let Some(constant) = self.unit.constants.get(constant.index()).cloned() else {
                    continue;
                };
                if !native_publication_constant_is_stable(&constant) {
                    continue;
                }
                let key = (self.unit_identity, function, local.raw());
                let reference = if let Some(reference) =
                    self.baseline_values.static_locals.get(&key).cloned()
                {
                    reference
                } else {
                    let Ok(default) = native_runtime_constant_value(self, &constant) else {
                        continue;
                    };
                    let reference = php_runtime::api::ReferenceCell::new(default);
                    self.baseline_values
                        .static_locals
                        .insert(key, reference.clone());
                    reference
                };
                let Ok(encoded) = self.encode_native_reference_owner(reference) else {
                    continue;
                };
                let published =
                    self.publish_trusted_static_local_reference(function, local.raw(), encoded);
                let _ = self.release(encoded);
                if published.is_err() {
                    continue;
                }
            }
        }
    }

    pub(super) fn clear_trusted_static_locals(&mut self) {
        let _ = self.materialize_trusted_static_locals();
        let mut values = Vec::new();
        for range in self.published_continuation_ranges() {
            values.extend(
                self.trusted_static_local_slots[range]
                    .iter_mut()
                    .filter_map(|slot| {
                        (slot.state == php_jit::JIT_NATIVE_TRUSTED_STATIC_LOCAL_PUBLISHED).then(
                            || {
                                let encoded = slot.encoded;
                                *slot = php_jit::JitNativeTrustedStaticLocalSlot::default();
                                encoded
                            },
                        )
                    }),
            );
        }
        for encoded in values {
            let _ = self.release_if_live(encoded);
        }
    }

    pub(super) fn materialize_trusted_static_locals(&mut self) -> Result<(), String> {
        let mut values = std::collections::BTreeSet::new();
        for range in self.published_continuation_ranges() {
            values.extend(
                self.trusted_static_local_slots[range]
                    .iter()
                    .filter_map(|slot| {
                        (slot.state == php_jit::JIT_NATIVE_TRUSTED_STATIC_LOCAL_PUBLISHED)
                            .then_some(slot.encoded)
                    }),
            );
        }
        for encoded in values {
            // Static-local cells survive compiled-unit activations and nested
            // include arenas. Move their authoritative direct payload into the
            // stable identity exactly at either cold ownership boundary.
            self.decode_baseline_value(encoded)?;
        }
        Ok(())
    }
}

impl<'a> NativeRequestColdState<'a> {
    /// Imports one newly visible PHP constant at a cold symbol-mutation
    /// boundary and immediately publishes its authoritative native owner.
    pub(super) fn insert_dynamic_constant(
        &mut self,
        name: String,
        value: Value,
    ) -> Result<(), String> {
        let encoded = self.encode_baseline_value(value)?;
        if let Some(previous) = self.native_dynamic_constants.insert(name.clone(), encoded) {
            self.release(previous)?;
        }
        self.publish_trusted_constant_encoding(&name, encoded);
        Ok(())
    }

    /// Moves constants received from an explicit include boundary back into
    /// the authoritative native registry before native execution resumes.
    pub(super) fn promote_cold_dynamic_constants(&mut self) -> Result<(), String> {
        let constants = std::mem::take(&mut self.baseline_values.cold_dynamic_constants);
        for (name, value) in constants {
            self.insert_dynamic_constant(name, value)?;
        }
        Ok(())
    }

    /// Projects native constants into the cold PHP-value registry only at an
    /// include, final-result, or introspection boundary.
    pub(super) fn materialize_native_dynamic_constants(&mut self) -> Result<(), String> {
        let native = std::mem::take(&mut self.native_dynamic_constants);
        for (name, encoded) in native {
            let value = self.decode_baseline_value(encoded)?;
            self.baseline_values
                .cold_dynamic_constants
                .insert(name, value);
            self.release(encoded)?;
        }
        Ok(())
    }

    /// Captures the authoritative native `$_SESSION` payload at the explicit
    /// transport/session compatibility boundary.
    pub(in crate::vm) fn materialize_native_session_state(&mut self) -> Result<(), String> {
        self.materialize_native_request_global("_SESSION")?;
        if self.session.destroyed() {
            return Ok(());
        }
        #[allow(unsafe_code)]
        let committed = unsafe { (*self.fast_state).session.committed };
        if let Value::Array(data) = self.decode_baseline_value(committed)? {
            self.session.set_committed_data(data);
        } else {
            return Err("native committed session payload is not an array".to_owned());
        }
        if let Value::Array(data) = self.baseline_values.session_global.get() {
            self.session.set_data(data);
        }
        Ok(())
    }

    /// Republishes the cold transport-owned commit snapshot before native
    /// execution resumes. The live `$_SESSION` reference stays native.
    pub(super) fn republish_native_session_commit(&mut self) -> Result<(), String> {
        let committed = self.encode_native_array_owner(self.session.committed_data())?;
        #[allow(unsafe_code)]
        let previous = unsafe {
            let session = &mut (*self.fast_state).session;
            std::mem::replace(&mut session.committed, committed)
        };
        self.release_if_live(previous)
    }
    /// Publishes one PHP reference identity with its contained value owned by
    /// the direct value plane. The `ReferenceCell` sidecar preserves alias
    /// identity for a later cold boundary; optimizing code reads and replaces
    /// only the encoded payload in the direct slot.
    #[track_caller]
    pub(super) fn encode_native_reference_owner(
        &mut self,
        reference: php_runtime::api::ReferenceCell,
    ) -> Result<i64, String> {
        let typed_guard = if self
            .typed_static_reference_constraints
            .contains_key(&reference.gc_debug_id())
        {
            php_jit::JIT_NATIVE_REFERENCE_TYPED_PROPERTY_GUARD
        } else {
            0
        };
        if let Some(index) = self
            .baseline_values
            .direct_reference_cells
            .iter()
            .find_map(|(index, existing)| existing.ptr_eq(&reference).then_some(*index))
        {
            let runtime_index = u32::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
                .ok_or_else(|| "direct native reference handle overflow".to_owned())?;
            let encoded =
                (php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG | u64::from(runtime_index)) as i64;
            self.restore_authoritative_direct_reference(encoded)?;
            let slot = self
                .direct_value_slots
                .get_mut(index)
                .filter(|slot| slot.refcount != 0)
                .ok_or_else(|| {
                    "direct native reference identity points at a dead slot".to_owned()
                })?;
            slot.refcount = slot
                .refcount
                .checked_add(1)
                .ok_or_else(|| "direct native reference refcount overflow".to_owned())?;
            slot.reserved |= typed_guard;
            return Ok(encoded);
        }

        // Publish the empty descriptor and identity before recursively
        // encoding the payload so a recursive PHP reference resolves to this
        // same slot instead of allocating a second identity.
        let index = self.reserve_direct_value_slot()?;
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
            flags: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
            reserved: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY | typed_guard,
            ..php_jit::JitNativeValueSlot::default()
        };
        self.baseline_values
            .direct_reference_cells
            .insert(index, reference.clone());

        let payload = match self.encode_baseline_value(reference.get()) {
            Ok(payload) => payload,
            Err(error) => {
                self.baseline_values.direct_reference_cells.remove(&index);
                let _ = self.release_direct_value_index(index);
                return Err(error);
            }
        };
        let slot = self
            .direct_value_slots
            .get_mut(index)
            .ok_or_else(|| format!("direct native reference {index} slot disappeared"))?;
        slot.reserved = php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED | typed_guard;
        slot.payload = payload as u64;
        // Promotion transfers payload ownership to the direct descriptor.
        // Preserve only ReferenceCell identity for a future explicit cold
        // materialization; do not retain a synchronized Rust value mirror.
        reference.set(Value::Uninitialized);
        self.mark_rooted_container_dirty(&Value::Reference(reference));

        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .ok_or_else(|| "direct native reference handle overflow".to_owned())?;
        Ok((php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG | u64::from(runtime_index)) as i64)
    }

    /// Moves one newly constructed PHP array into the canonical native array
    /// plane at a call-frame boundary. This is an ownership transfer, not a
    /// shadow view of a retained `PhpArray`: the direct slot and its entries
    /// become the sole representation consumed by optimizing code.
    #[track_caller]
    pub(super) fn encode_native_array_owner(
        &mut self,
        array: php_runtime::api::PhpArray,
    ) -> Result<i64, String> {
        let root = self.begin_direct_array_encode();
        let result = self.encode_direct_array_value_unscoped(array);
        self.finish_direct_array_encode(root, result)
    }

    fn begin_direct_array_encode(&mut self) -> bool {
        let root = self.baseline_values.direct_array_encode_depth == 0;
        if root {
            debug_assert!(self.baseline_values.direct_array_handles.is_empty());
            debug_assert!(self.baseline_values.direct_array_storage_ids.is_empty());
        }
        self.baseline_values.direct_array_encode_depth = self
            .baseline_values
            .direct_array_encode_depth
            .saturating_add(1);
        root
    }

    fn finish_direct_array_encode<T>(
        &mut self,
        root: bool,
        result: Result<T, String>,
    ) -> Result<T, String> {
        self.baseline_values.direct_array_encode_depth = self
            .baseline_values
            .direct_array_encode_depth
            .saturating_sub(1);
        if !root {
            return result;
        }
        let pool_owners = std::mem::take(&mut self.baseline_values.direct_array_handles)
            .into_values()
            .map(usize::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "direct native array pool index overflow".to_owned())?;
        self.baseline_values.direct_array_storage_ids.clear();
        let mut release_error = None;
        for index in pool_owners {
            if let Err(error) = self.release_direct_value_index(index) {
                release_error.get_or_insert(error);
            }
        }
        match (result, release_error) {
            (Err(error), _) | (Ok(_), Some(error)) => Err(error),
            (Ok(value), None) => Ok(value),
        }
    }

    #[track_caller]
    fn encode_direct_array_value_unscoped(
        &mut self,
        array: php_runtime::api::PhpArray,
    ) -> Result<i64, String> {
        let storage_version = (array.native_storage_id(), array.mutation_epoch());
        if let Some(index) = self
            .baseline_values
            .direct_array_handles
            .get(&storage_version)
            .copied()
        {
            let slot = self
                .direct_value_slots
                .get_mut(index as usize)
                .filter(|slot| {
                    slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
                })
                .ok_or_else(|| "interned direct array points at a dead slot".to_owned())?;
            slot.refcount = slot
                .refcount
                .checked_add(1)
                .ok_or_else(|| "direct native array refcount overflow".to_owned())?;
            let runtime_index = index
                .checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
                .ok_or_else(|| "direct native array handle overflow".to_owned())?;
            return Ok((php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG | u64::from(runtime_index)) as i64);
        }
        let cursor = array
            .native_pointer_position()
            .and_then(|position| u32::try_from(position).ok());
        let next_append_key = array.native_next_append_key();
        let (start, capacity) = self.reserve_direct_array_entries(array.len())?;
        let index = match self.reserve_direct_value_slot() {
            Ok(index) => index,
            Err(error) => {
                self.free_direct_array_entries(start, capacity);
                return Err(error);
            }
        };
        let runtime_index = match u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
        {
            Some(index) => index,
            None => {
                self.free_direct_array_entries(start, capacity);
                return Err("direct native value handle overflow".to_owned());
            }
        };
        let encoded = (php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG | u64::from(runtime_index)) as i64;
        // Publish the canonical storage identity before recursively encoding
        // entries. A reference back to this same COW storage can now retain
        // this descriptor instead of recursively rebuilding the Rust graph.
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 2,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
            flags: php_jit::jit_native_direct_array_flags(cursor),
            reserved: u32::try_from(capacity).unwrap_or(u32::MAX),
            payload: 0,
            aux: self.direct_array_entries[start..].as_ptr() as usize as u64,
        };
        self.direct_array_states[index] = php_jit::JitNativeDirectArrayState {
            next_append_key: next_append_key.unwrap_or(0),
            has_next_append_key: u32::from(next_append_key.is_some()),
            reserved: 0,
        };
        self.baseline_values
            .direct_array_handles
            .insert(storage_version, index as u32);
        self.baseline_values
            .direct_array_storage_ids
            .insert(index, storage_version);

        let mut filled = 0_usize;
        for (key, value) in array.iter() {
            let key = match key {
                php_runtime::api::ArrayKey::Int(key) => self.encode_baseline_value(Value::Int(key)),
                php_runtime::api::ArrayKey::String(key) => {
                    self.encode_native_string_owner(key.clone())
                }
            };
            let key = match key {
                Ok(key) => key,
                Err(error) => {
                    let _ = self.rollback_incomplete_direct_array(index, storage_version, filled);
                    return Err(error);
                }
            };
            let value = match self.encode_baseline_value(value.clone()) {
                Ok(value) => value,
                Err(error) => {
                    let _ = self.release(key);
                    let _ = self.rollback_incomplete_direct_array(index, storage_version, filled);
                    return Err(error);
                }
            };
            self.direct_array_entries[start + filled] =
                php_jit::JitNativeDirectArrayEntry { key, value };
            filled = filled.saturating_add(1);
            self.direct_value_slots[index].payload = filled as u64;
        }
        self.record_direct_array_materialization(filled, std::panic::Location::caller());
        Ok(encoded)
    }

    fn rollback_incomplete_direct_array(
        &mut self,
        index: usize,
        storage_version: (u64, u64),
        filled: usize,
    ) -> Result<(), String> {
        self.baseline_values
            .direct_array_handles
            .remove(&storage_version);
        self.baseline_values.direct_array_storage_ids.remove(&index);
        if let Some(slot) = self.direct_value_slots.get_mut(index) {
            slot.payload = filled as u64;
        }
        self.release_direct_value_index(index)?;
        self.release_direct_value_index(index)
    }

    pub(super) fn replace_direct_array(
        &mut self,
        index: usize,
        array: php_runtime::api::PhpArray,
    ) -> Result<(), String> {
        if let Some(storage_version) = self.baseline_values.direct_array_storage_ids.remove(&index)
        {
            if self
                .baseline_values
                .direct_array_handles
                .get(&storage_version)
                == Some(&(index as u32))
            {
                self.baseline_values
                    .direct_array_handles
                    .remove(&storage_version);
            }
            // This Rust-side mutation owns the encoded handle it is replacing.
            // Drop the pool's immutable-snapshot owner first, so a later Rust
            // alias with the old storage id materializes its unchanged COW
            // snapshot instead of observing this mutation.
            self.release_direct_value_index(index)?;
        }
        let cursor = array
            .native_pointer_position()
            .and_then(|position| u32::try_from(position).ok());
        let next_append_key = array.native_next_append_key();
        let old = *self
            .direct_value_slots
            .get(index)
            .filter(|slot| {
                slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
            })
            .ok_or_else(|| format!("direct native array {index} is missing"))?;
        let source = array
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>();
        let mut encoded_entries: Vec<php_jit::JitNativeDirectArrayEntry> =
            Vec::with_capacity(source.len());
        for (key, value) in source {
            let key = match key {
                php_runtime::api::ArrayKey::Int(key) => self.encode_baseline_value(Value::Int(key)),
                php_runtime::api::ArrayKey::String(key) => self.encode_native_string_owner(key),
            }?;
            let value = match self.encode_baseline_value(value) {
                Ok(value) => value,
                Err(error) => {
                    let _ = self.release(key);
                    for entry in encoded_entries.drain(..) {
                        let _ = self.release(entry.key);
                        let _ = self.release(entry.value);
                    }
                    return Err(error);
                }
            };
            encoded_entries.push(php_jit::JitNativeDirectArrayEntry { key, value });
        }

        let base = self.direct_array_entries.as_ptr() as usize;
        let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
        let old_start = usize::try_from(old.aux)
            .unwrap_or(base)
            .saturating_sub(base)
            / entry_size;
        let old_length = usize::try_from(old.payload).unwrap_or(0);
        let old_children = self
            .direct_array_entries
            .get(old_start..old_start.saturating_add(old_length))
            .unwrap_or_default()
            .iter()
            .flat_map(|entry| [entry.key, entry.value])
            .collect::<Vec<_>>();
        let moved = encoded_entries.len() > old.reserved as usize;
        let (start, capacity) = if !moved {
            (old_start, old.reserved as usize)
        } else {
            self.reserve_direct_array_entries(encoded_entries.len())?
        };
        self.direct_array_entries[start..start + encoded_entries.len()]
            .copy_from_slice(&encoded_entries);
        let slot = &mut self.direct_value_slots[index];
        slot.flags = php_jit::jit_native_direct_array_flags(cursor);
        slot.reserved = u32::try_from(capacity).unwrap_or(u32::MAX);
        slot.payload = encoded_entries.len() as u64;
        slot.aux = self.direct_array_entries[start..].as_ptr() as usize as u64;
        self.direct_array_states[index] = php_jit::JitNativeDirectArrayState {
            next_append_key: next_append_key.unwrap_or(0),
            has_next_append_key: u32::from(next_append_key.is_some()),
            reserved: 0,
        };
        if moved {
            self.free_direct_array_entries(old_start, old.reserved as usize);
        }
        for child in old_children {
            self.release(child)?;
        }
        Ok(())
    }
}
