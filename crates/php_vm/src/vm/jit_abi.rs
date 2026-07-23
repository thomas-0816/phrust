// Audited native ABI surface; see ADR 0017. The product compiler graph always
// includes this module.
use php_ir::module::{normalize_class_name, normalized_class_name};
use php_runtime::api::PhpString;
use php_runtime::api::Value;
use php_runtime::experimental::WeakObjectHandle;
use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;
use std::sync::Arc;

mod call_dispatch;
mod call_support;
mod diagnostic_helpers;
mod diagnostics;
mod dynamic_code;
mod dynamic_units;
mod frame_arena;
mod internal_classes;
mod native_builtins;
mod object_support;
mod request_state;
mod root_index;
mod runtime_ops;
mod semantic_dispatch;
mod telemetry;

use dynamic_units::*;
pub(super) use dynamic_units::{jit_native_function_resolve_abi, native_entries_from_records};
use frame_arena::NativeFrameArena;
pub(super) use frame_arena::{jit_native_frame_alloc_abi, jit_native_frame_release_abi};

pub(super) use call_dispatch::{
    jit_baseline_native_builtin_dispatch_abi, jit_baseline_native_builtin_dispatch_diagnostic_abi,
    jit_native_basename_abi, jit_native_call_dispatch_abi, jit_native_call_dispatch_diagnostic_abi,
    jit_native_call_user_func_abi, jit_native_call_user_func_array_abi,
    jit_native_class_exists_abi, jit_native_defined_abi, jit_native_dirname_abi,
    jit_native_enum_exists_abi, jit_native_file_exists_abi, jit_native_function_exists_abi,
    jit_native_interface_exists_abi, jit_native_json_decode_abi, jit_native_json_encode_abi,
    jit_native_json_last_error_abi, jit_native_json_last_error_msg_abi,
    jit_native_json_validate_abi, jit_native_method_exists_abi, jit_native_preg_filter_abi,
    jit_native_preg_grep_abi, jit_native_preg_last_error_abi, jit_native_preg_last_error_msg_abi,
    jit_native_preg_match_abi, jit_native_preg_match_all_abi, jit_native_preg_quote_abi,
    jit_native_preg_replace_abi, jit_native_preg_split_abi, jit_native_printf_abi,
    jit_native_property_exists_abi, jit_native_realpath_abi, jit_native_sprintf_abi,
    jit_native_trait_exists_abi, jit_native_vprintf_abi, jit_native_vsprintf_abi,
};
use call_support::*;
pub(in crate::vm) use diagnostic_helpers::*;
use diagnostics::*;
pub(super) use dynamic_code::jit_native_dynamic_code_abi;
use internal_classes::*;
use native_builtins::{
    NativeDimensionOperation, emit_native_array_dimension_conversion_diagnostic,
    emit_native_deprecated_call, emit_native_dimension_conversion_diagnostic,
    emit_native_php_diagnostic, emit_native_php_warning, exact_native_callback_is_admitted,
    execute_baseline_native_builtin, execute_baseline_native_builtin_control,
    execute_baseline_prepared_runtime_builtin, execute_native_call_user_func_array_direct,
    execute_native_call_user_func_encoded, native_builtin_class_lineage,
    native_internal_class_constant_exists, native_php_function_exists, native_source_line,
    native_source_line_for_span, native_string,
};
use object_support::*;
use request_state::{
    NativeBacktraceFrame, NativeFunctionNameScope, NativeLastError,
    NativeRegisteredExtensionRequestState,
};
use root_index::{RequestRootIndex, RootMutationReason, values_contain_object};
pub(super) use runtime_ops::{
    jit_native_argument_check_abi, jit_native_array_fetch_abi, jit_native_array_insert_abi,
    jit_native_array_insert_local_abi, jit_native_array_new_abi, jit_native_array_spread_abi,
    jit_native_array_unset_abi, jit_native_binary_abi, jit_native_cast_abi, jit_native_compare_abi,
    jit_native_constant_fetch_abi, jit_native_echo_abi, jit_native_echo_bytes_abi,
    jit_native_echo_float_abi, jit_native_echo_int_abi, jit_native_exception_new_abi,
    jit_native_execution_poll_abi, jit_native_float_to_int_abi, jit_native_float_to_string_abi,
    jit_native_foreach_cleanup_abi, jit_native_foreach_init_abi, jit_native_foreach_next_abi,
    jit_native_local_fetch_abi, jit_native_local_store_abi, jit_native_object_class_name_abi,
    jit_native_object_clone_abi, jit_native_object_clone_with_abi, jit_native_object_new_abi,
    jit_native_plain_object_clone_abi, jit_native_prepared_object_new_abi,
    jit_native_property_assign_abi, jit_native_property_fetch_abi, jit_native_reference_bind_abi,
    jit_native_return_check_abi, jit_native_runtime_fatal_abi, jit_native_stable_length_abi,
    jit_native_string_predicate_abi, jit_native_truthy_abi, jit_native_type_predicate_abi,
    jit_native_unary_abi, jit_native_value_release_abi,
};
use semantic_dispatch::*;
pub(super) use semantic_dispatch::{
    jit_native_semantic_dispatch_abi, jit_native_semantic_dispatch_diagnostic_abi,
};
use telemetry::NativeRuntimeTelemetry;

thread_local! {
    static NATIVE_INCLUDE_GLOBALS: RefCell<Option<std::collections::BTreeMap<String, Value>>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_CONSTANTS: RefCell<Option<std::collections::BTreeMap<String, Value>>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_INI: RefCell<Option<php_runtime::api::IniRegistry>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_DEFAULT_TIMEZONE: RefCell<Option<String>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_HTTP_RESPONSE: RefCell<Option<php_runtime::api::RuntimeHttpResponseState>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_FILES: RefCell<Option<std::collections::BTreeSet<std::path::PathBuf>>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_MYSQL: RefCell<Option<std::rc::Rc<RefCell<php_runtime::api::MysqlState>>>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_FILTER_INPUT_ARRAYS: RefCell<Option<Rc<std::collections::BTreeMap<i64, php_runtime::api::PhpArray>>>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_FUNCTION_NAMES: RefCell<Option<Rc<NativeFunctionNameScope>>> =
        const { RefCell::new(None) };
    static NATIVE_INCLUDE_SYMBOLS: RefCell<Option<NativeIncludeSymbols>> = const { RefCell::new(None) };
    static NATIVE_INCLUDE_EXPORTS: RefCell<Option<NativeIncludeExports>> =
        const { RefCell::new(None) };
}

static NATIVE_TEMPNAM_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Compact stable prefix passed through every generated entry and compiled
/// call. Common native operations consume `header.runtime_view` directly;
/// baseline-only semantic operations explicitly cross `cold_context`.
#[repr(C)]
#[derive(Default)]
pub(super) struct NativeRequestFastState {
    header: php_jit::JitNativeFastStateHeader,
    cold_context: *mut std::ffi::c_void,
    output: *mut php_runtime::api::OutputBuffer,
    json_state: *mut php_runtime::api::JsonRequestState,
    pcre_state: *mut php_runtime::api::PcreRequestState,
    ini_registry: *const php_runtime::api::IniRegistry,
    cwd: *const std::path::PathBuf,
    filesystem_capabilities: *const php_runtime::api::FilesystemCapabilities,
}

impl NativeRequestFastState {
    /// Borrows the two capabilities required by exact path/filesystem
    /// handlers. Their addresses are request-stable and no cold execution
    /// coordinator is recovered on this path.
    #[allow(unsafe_code)]
    fn native_filesystem_capability(
        &self,
    ) -> Option<(&std::path::Path, &php_runtime::api::FilesystemCapabilities)> {
        let cwd = unsafe { self.cwd.as_ref() }?;
        let filesystem = unsafe { self.filesystem_capabilities.as_ref() }?;
        Some((cwd.as_path(), filesystem))
    }

    #[allow(unsafe_code)]
    fn reserve_direct_value_index(&mut self) -> Result<u32, &'static str> {
        let view = self.header.runtime_view;
        let value_next = view.direct_value_next as usize as *mut u32;
        let free_head = view.direct_value_free_head as usize as *mut u32;
        let reused_bytes = view.direct_value_reused_bytes as usize as *mut u64;
        let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
        // SAFETY: runtime publication owns these stable counters and the
        // request executes synchronously on one thread.
        unsafe {
            if *free_head != php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE {
                let index = *free_head;
                *free_head = (*slots.add(index as usize)).reserved;
                *reused_bytes = (*reused_bytes)
                    .saturating_add(std::mem::size_of::<php_jit::JitNativeValueSlot>() as u64);
                return Ok(index);
            }
            let index = *value_next;
            if index as usize >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
                return Err("direct native value arena exhausted");
            }
            *value_next = index + 1;
            Ok(index)
        }
    }

    #[allow(unsafe_code)]
    fn reserve_direct_string_range(&mut self, length: usize) -> Option<(usize, u32)> {
        let capacity = length
            .max(php_jit::JIT_NATIVE_DIRECT_STRING_MIN_CAPACITY as usize)
            .checked_next_power_of_two()?;
        let capacity = u32::try_from(capacity).ok()?;
        let bucket = capacity.trailing_zeros() as usize;
        let view = self.header.runtime_view;
        let heads = view.direct_string_free_heads as usize as *mut u32;
        let bytes = view.direct_string_bytes as usize as *mut u8;
        let next = view.direct_string_next as usize as *mut u32;
        let reused_bytes = view.direct_string_reused_bytes as usize as *mut u64;
        unsafe {
            let head = *heads.add(bucket);
            if head != php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE {
                let previous = (bytes.add(head as usize) as *const u32).read_unaligned();
                *heads.add(bucket) = previous;
                *reused_bytes = (*reused_bytes).saturating_add(u64::from(capacity));
                return Some((head as usize, capacity));
            }
            let start = *next;
            let end = start.checked_add(capacity)?;
            if end as usize > php_jit::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY {
                return None;
            }
            *next = end;
            Some((start as usize, capacity))
        }
    }

    #[allow(unsafe_code)]
    fn free_direct_string_range(&mut self, start: usize, capacity: u32) {
        if capacity < php_jit::JIT_NATIVE_DIRECT_STRING_MIN_CAPACITY || !capacity.is_power_of_two()
        {
            return;
        }
        let view = self.header.runtime_view;
        let bucket = capacity.trailing_zeros() as usize;
        let heads = view.direct_string_free_heads as usize as *mut u32;
        let bytes = view.direct_string_bytes as usize as *mut u8;
        unsafe {
            let previous = *heads.add(bucket);
            (bytes.add(start) as *mut u32).write_unaligned(previous);
            *heads.add(bucket) =
                u32::try_from(start).unwrap_or(php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE);
        }
    }

    /// Publishes one exact-handler result directly into the request-owned
    /// native string/value plane. Publication metadata guarantees every
    /// pointer in the runtime view; this path performs only PHP-visible arena
    /// bounds checks and never recovers the cold execution coordinator.
    #[allow(unsafe_code)]
    fn publish_direct_string_bytes(&mut self, bytes: &[u8]) -> Result<i64, &'static str> {
        let view = self.header.runtime_view;
        let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
        let string_bytes = view.direct_string_bytes as usize as *mut u8;

        // SAFETY: `activate_native_context` publishes stable request-owned
        // arena bases and counters before generated code can invoke an exact
        // handler. A native request is single-threaded while this state is
        // active, so reservation and publication are one ordered operation.
        let (start, capacity) = self
            .reserve_direct_string_range(bytes.len())
            .ok_or("direct native string arena exhausted")?;

        let index = match self.reserve_direct_value_index() {
            Ok(index) => index,
            Err(error) => {
                self.free_direct_string_range(start, capacity);
                return Err(error);
            }
        };
        let runtime_index = index + php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE;

        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), string_bytes.add(start), bytes.len());
            *slots.add(index as usize) = php_jit::JitNativeValueSlot {
                refcount: 1,
                kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
                flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
                reserved: php_jit::jit_native_direct_string_reserved(capacity, bytes == b"0"),
                payload: bytes.len() as u64,
                aux: string_bytes.add(start) as usize as u64,
            };
        }
        Ok((php_jit::JIT_VALUE_RUNTIME_STRING_TAG | u64::from(runtime_index)) as i64)
    }

    #[allow(unsafe_code)]
    fn direct_slot(&self, encoded: i64) -> Option<(usize, php_jit::JitNativeValueSlot)> {
        let runtime_index = php_jit::jit_decode_runtime_value(encoded)?;
        let index =
            runtime_index.checked_sub(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)? as usize;
        if index >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
            return None;
        }
        let view = self.header.runtime_view;
        let slots = view.direct_value_slots as usize as *const php_jit::JitNativeValueSlot;
        let slot = unsafe { *slots.add(index) };
        (slot.refcount != 0).then_some((index, slot))
    }

    /// Returns the stable backing owner for one authoritative direct object.
    /// The owner pointer is slot-parallel and is cleared before the slot is
    /// recycled, so exact object operations need no request hash lookup.
    #[allow(unsafe_code)]
    fn direct_object(&self, encoded: i64) -> Option<&php_runtime::api::ObjectRef> {
        let (index, slot) = self.direct_slot(encoded)?;
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT {
            return None;
        }
        let owners = self.header.runtime_view.direct_object_owners as usize as *const u64;
        // SAFETY: both arrays share the direct-value capacity and stable
        // request lifetime. A nonzero owner is a Box<ObjectRef> published
        // before the direct object descriptor becomes visible.
        let owner = unsafe { *owners.add(index) } as usize as *const php_runtime::api::ObjectRef;
        unsafe { owner.as_ref() }
    }

    #[allow(unsafe_code)]
    fn native_string_view(&self, encoded: i64) -> Option<&[u8]> {
        if let Some((_, slot)) = self.direct_slot(encoded) {
            if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_STRING {
                return None;
            }
            let length = usize::try_from(slot.payload).ok()?;
            let bytes = slot.aux as usize as *const u8;
            if bytes.is_null() && length != 0 {
                return None;
            }
            // SAFETY: the direct string descriptor points into the stable
            // request-owned byte arena for its published length.
            return Some(unsafe { std::slice::from_raw_parts(bytes, length) });
        }
        let constant = php_jit::jit_decode_constant(encoded)?;
        let view = self.header.runtime_view;
        if constant >= view.trusted_constant_view_count {
            return None;
        }
        let constants =
            view.trusted_constant_views as usize as *const php_jit::JitNativeConstantView;
        // SAFETY: publication owns a dense descriptor array for the active
        // unit and the index was checked against its exact count.
        let constant = unsafe { *constants.add(constant as usize) };
        if constant.kind != php_jit::JIT_NATIVE_CONSTANT_VIEW_STRING {
            return None;
        }
        let length = usize::try_from(constant.length).ok()?;
        let bytes = constant.bytes as usize as *const u8;
        if bytes.is_null() && length != 0 {
            return None;
        }
        Some(unsafe { std::slice::from_raw_parts(bytes, length) })
    }

    /// Encodes the authoritative native scalar/array/string graph using
    /// PHP's default `json_encode` byte rules. Unsupported semantic shapes
    /// return before publication so the call can take its one baseline
    /// continuation without synchronizing a second value representation.
    #[allow(unsafe_code)]
    fn append_native_json_default(
        &self,
        mut encoded: i64,
        output: &mut String,
        depth: usize,
        maximum_depth: usize,
        active_arrays: &mut Vec<usize>,
    ) -> Option<()> {
        for _ in 0..64 {
            if let Some((index, slot)) = self.direct_slot(encoded) {
                match slot.kind {
                    php_jit::JIT_NATIVE_VALUE_VIEW_STRING => {
                        return php_runtime::api::append_json_default_string(
                            self.native_string_view(encoded)?,
                            output,
                        )
                        .ok();
                    }
                    php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                        if slot.reserved != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY =>
                    {
                        encoded = slot.payload as i64;
                        continue;
                    }
                    php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY => {
                        if depth >= maximum_depth || active_arrays.contains(&index) {
                            return None;
                        }
                        let length = usize::try_from(slot.payload).ok()?;
                        let entries =
                            slot.aux as usize as *const php_jit::JitNativeDirectArrayEntry;
                        if entries.is_null() && length != 0 {
                            return None;
                        }
                        // SAFETY: the direct-array descriptor owns this stable
                        // insertion-ordered range for the slot's live length.
                        let entries = unsafe { std::slice::from_raw_parts(entries, length) };
                        let packed = entries.iter().enumerate().all(|(position, entry)| {
                            php_jit::jit_decode_runtime_value(entry.key).is_none()
                                && php_jit::jit_decode_constant(entry.key).is_none()
                                && entry.key == i64::try_from(position).unwrap_or(i64::MAX)
                        });
                        active_arrays.push(index);
                        output.push(if packed { '[' } else { '{' });
                        for (position, entry) in entries.iter().enumerate() {
                            if position != 0 {
                                output.push(',');
                            }
                            if !packed {
                                if php_jit::jit_decode_runtime_value(entry.key).is_none()
                                    && php_jit::jit_decode_constant(entry.key).is_none()
                                {
                                    output.push('"');
                                    write!(output, "{}", entry.key).ok()?;
                                    output.push_str("\":");
                                } else {
                                    php_runtime::api::append_json_default_string(
                                        self.native_string_view(entry.key)?,
                                        output,
                                    )
                                    .ok()?;
                                    output.push(':');
                                }
                            }
                            self.append_native_json_default(
                                entry.value,
                                output,
                                depth + 1,
                                maximum_depth,
                                active_arrays,
                            )?;
                        }
                        output.push(if packed { ']' } else { '}' });
                        active_arrays.pop();
                        return Some(());
                    }
                    // Floats, objects, baseline references, and extension
                    // values retain their exact cold continuation.
                    _ => return None,
                }
            }
            if php_jit::jit_decode_runtime_value(encoded).is_none()
                && php_jit::jit_decode_constant(encoded).is_none()
            {
                write!(output, "{encoded}").ok()?;
                return Some(());
            }
            let constant = php_jit::jit_decode_constant(encoded)?;
            match constant {
                u32::MAX | php_jit::JIT_VALUE_UNINITIALIZED => output.push_str("null"),
                php_jit::JIT_VALUE_FALSE => output.push_str("false"),
                php_jit::JIT_VALUE_TRUE => output.push_str("true"),
                _ => {
                    let view = self.header.runtime_view;
                    if constant >= view.trusted_constant_view_count {
                        return None;
                    }
                    let constants = view.trusted_constant_views as usize
                        as *const php_jit::JitNativeConstantView;
                    let constant = unsafe { *constants.add(constant as usize) };
                    match constant.kind {
                        php_jit::JIT_NATIVE_CONSTANT_VIEW_NULL => output.push_str("null"),
                        php_jit::JIT_NATIVE_CONSTANT_VIEW_BOOL => {
                            output.push_str(if constant.length != 0 {
                                "true"
                            } else {
                                "false"
                            })
                        }
                        php_jit::JIT_NATIVE_CONSTANT_VIEW_INT => {
                            write!(output, "{}", constant.length as i64).ok()?;
                        }
                        php_jit::JIT_NATIVE_CONSTANT_VIEW_STRING => {
                            php_runtime::api::append_json_default_string(
                                self.native_string_view(encoded)?,
                                output,
                            )
                            .ok()?;
                        }
                        _ => return None,
                    }
                }
            }
            return Some(());
        }
        None
    }

    fn native_json_default_bytes(&self, encoded: i64, maximum_depth: usize) -> Option<Vec<u8>> {
        let mut output = String::with_capacity(64);
        self.append_native_json_default(encoded, &mut output, 0, maximum_depth, &mut Vec::new())?;
        Some(output.into_bytes())
    }

    #[allow(unsafe_code)]
    fn validate_native_json(
        &self,
        input: i64,
        depth: i64,
        flags: i64,
    ) -> Option<Result<bool, php_runtime::api::BuiltinError>> {
        let state = self.json_state;
        let input = self.native_string_view(input)?;
        let state = unsafe { state.as_mut() }?;
        Some(php_runtime::api::validate_native_json(
            state, input, depth, flags,
        ))
    }

    #[allow(unsafe_code)]
    fn decode_native_json_associative(
        &self,
        input: i64,
        depth: i64,
    ) -> Option<Result<php_runtime::api::NativeJsonDecodedValue, php_runtime::api::BuiltinError>>
    {
        let state = self.json_state;
        let input = self.native_string_view(input)?;
        let state = unsafe { state.as_mut() }?;
        Some(php_runtime::api::decode_native_json_associative(
            state, input, depth,
        ))
    }

    fn native_json_footprint(
        value: &php_runtime::api::NativeJsonDecodedValue,
    ) -> Option<(usize, usize, usize)> {
        use php_runtime::api::NativeJsonDecodedValue as Json;
        match value {
            Json::Null | Json::Bool(_) => Some((0, 0, 0)),
            Json::Int(value)
                if php_jit::jit_decode_runtime_value(*value).is_none()
                    && php_jit::jit_decode_constant(*value).is_none() =>
            {
                Some((0, 0, 0))
            }
            Json::Int(_) => None,
            Json::Float(_) => Some((1, 0, 0)),
            Json::String(bytes) => Some((1, 0, bytes.len())),
            Json::Array(values) => {
                let capacity = values
                    .len()
                    .max(php_jit::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize)
                    .next_power_of_two();
                values.iter().try_fold(
                    (1_usize, capacity, 0_usize),
                    |(slots, entries, bytes), value| {
                        let child = Self::native_json_footprint(value)?;
                        Some((
                            slots.checked_add(child.0)?,
                            entries.checked_add(child.1)?,
                            bytes.checked_add(child.2)?,
                        ))
                    },
                )
            }
            Json::Object(values) => {
                let capacity = values
                    .len()
                    .max(php_jit::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize)
                    .next_power_of_two();
                values.iter().try_fold(
                    (1_usize, capacity, 0_usize),
                    |(slots, entries, bytes), (key, value)| {
                        let child = Self::native_json_footprint(value)?;
                        Some((
                            slots.checked_add(1)?.checked_add(child.0)?,
                            entries.checked_add(child.1)?,
                            bytes.checked_add(key.len())?.checked_add(child.2)?,
                        ))
                    },
                )
            }
        }
    }

    #[allow(unsafe_code)]
    fn reserve_sequential_direct_value_index(&mut self) -> Option<u32> {
        let view = self.header.runtime_view;
        let next = view.direct_value_next as usize as *mut u32;
        let index = unsafe { *next };
        if index as usize >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
            return None;
        }
        unsafe { *next = index + 1 };
        Some(index)
    }

    #[allow(unsafe_code)]
    fn publish_sequential_json_string(&mut self, bytes: &[u8]) -> Option<i64> {
        let view = self.header.runtime_view;
        let (start, capacity) = self.reserve_direct_string_range(bytes.len())?;
        let index = match self.reserve_sequential_direct_value_index() {
            Some(index) => index,
            None => {
                self.free_direct_string_range(start, capacity);
                return None;
            }
        };
        let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
        let storage = view.direct_string_bytes as usize as *mut u8;
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), storage.add(start), bytes.len());
            *slots.add(index as usize) = php_jit::JitNativeValueSlot {
                refcount: 1,
                kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
                flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
                reserved: php_jit::jit_native_direct_string_reserved(capacity, bytes == b"0"),
                payload: bytes.len() as u64,
                aux: storage.add(start) as usize as u64,
            };
        }
        let runtime_index = index + php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE;
        Some((php_jit::JIT_VALUE_RUNTIME_STRING_TAG | u64::from(runtime_index)) as i64)
    }

    #[allow(unsafe_code)]
    fn publish_sequential_json_value(
        &mut self,
        value: php_runtime::api::NativeJsonDecodedValue,
    ) -> Option<i64> {
        use php_runtime::api::NativeJsonDecodedValue as Json;
        match value {
            Json::Null => Some(php_jit::jit_encode_constant(u32::MAX)),
            Json::Bool(value) => Some(php_jit::jit_encode_constant(if value {
                php_jit::JIT_VALUE_TRUE
            } else {
                php_jit::JIT_VALUE_FALSE
            })),
            Json::Int(value) => Some(value),
            Json::Float(value) => {
                let view = self.header.runtime_view;
                let index = self.reserve_sequential_direct_value_index()?;
                let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
                unsafe {
                    *slots.add(index as usize) = php_jit::JitNativeValueSlot {
                        refcount: 1,
                        kind: php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT,
                        payload: value.to_bits(),
                        ..php_jit::JitNativeValueSlot::default()
                    };
                }
                let runtime_index = index + php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE;
                Some((php_jit::JIT_VALUE_RUNTIME_FLOAT_TAG | u64::from(runtime_index)) as i64)
            }
            Json::String(bytes) => self.publish_sequential_json_string(&bytes),
            Json::Array(values) => {
                let entries = values
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        Some(php_jit::JitNativeDirectArrayEntry {
                            key: i64::try_from(index).ok()?,
                            value: self.publish_sequential_json_value(value)?,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?;
                self.publish_sequential_direct_array(entries)
            }
            Json::Object(values) => {
                let entries = values
                    .into_iter()
                    .map(|(key, value)| {
                        Some(php_jit::JitNativeDirectArrayEntry {
                            key: self.publish_sequential_json_string(&key)?,
                            value: self.publish_sequential_json_value(value)?,
                        })
                    })
                    .collect::<Option<Vec<_>>>()?;
                self.publish_sequential_direct_array(entries)
            }
        }
    }

    #[allow(unsafe_code)]
    fn publish_sequential_direct_array(
        &mut self,
        entries: Vec<php_jit::JitNativeDirectArrayEntry>,
    ) -> Option<i64> {
        let view = self.header.runtime_view;
        let next_append_key = entries
            .iter()
            .filter_map(|entry| {
                (php_jit::jit_decode_runtime_value(entry.key).is_none()
                    && php_jit::jit_decode_constant(entry.key).is_none())
                .then_some(entry.key)
            })
            .map(|key| key.saturating_add(1))
            .max();
        let capacity = entries
            .len()
            .max(php_jit::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize)
            .next_power_of_two();
        let array_next = view.direct_array_next as usize as *mut u32;
        let start = unsafe { *array_next as usize };
        let end = start.checked_add(capacity)?;
        if end > php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY {
            return None;
        }
        let index = self.reserve_sequential_direct_value_index()?;
        let storage = view.direct_array_entries as usize as *mut php_jit::JitNativeDirectArrayEntry;
        let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
        let states = view.direct_array_states as usize as *mut php_jit::JitNativeDirectArrayState;
        unsafe {
            std::ptr::copy_nonoverlapping(entries.as_ptr(), storage.add(start), entries.len());
            *array_next = end as u32;
            *slots.add(index as usize) = php_jit::JitNativeValueSlot {
                refcount: 1,
                kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
                flags: php_jit::jit_native_direct_array_flags(None),
                reserved: capacity as u32,
                payload: entries.len() as u64,
                aux: storage.add(start) as usize as u64,
            };
            *states.add(index as usize) = php_jit::JitNativeDirectArrayState {
                next_append_key: next_append_key.unwrap_or(0),
                has_next_append_key: u32::from(next_append_key.is_some()),
                reserved: 0,
            };
        }
        let runtime_index = index + php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE;
        Some((php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG | u64::from(runtime_index)) as i64)
    }

    #[allow(unsafe_code)]
    fn publish_native_json_decoded(
        &mut self,
        value: php_runtime::api::NativeJsonDecodedValue,
    ) -> Option<i64> {
        let (slots, entries, bytes) = Self::native_json_footprint(&value)?;
        let view = self.header.runtime_view;
        let value_next = unsafe { *(view.direct_value_next as usize as *const u32) } as usize;
        let array_next = unsafe { *(view.direct_array_next as usize as *const u32) } as usize;
        let string_next = unsafe { *(view.direct_string_next as usize as *const u32) } as usize;
        if value_next.checked_add(slots)? > php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY
            || array_next.checked_add(entries)? > php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY
            || string_next.checked_add(bytes)? > php_jit::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY
        {
            return None;
        }
        self.publish_sequential_json_value(value)
    }

    #[allow(unsafe_code)]
    fn native_json_last_error(&self) -> Option<(i64, Vec<u8>)> {
        let state = unsafe { self.json_state.as_ref() }?;
        let (code, message) = state.value();
        Some((code, message.as_bytes().to_vec()))
    }

    #[allow(unsafe_code)]
    fn native_pcre_last_error(&self) -> Option<(i64, Vec<u8>)> {
        let state = unsafe { self.pcre_state.as_ref() }?.last_error();
        Some((state.code(), state.message().as_bytes().to_vec()))
    }

    #[allow(unsafe_code)]
    fn native_preg_match(
        &self,
        pattern: i64,
        subject: i64,
        flags: i64,
        offset: i64,
    ) -> Option<
        Result<Option<php_runtime::api::NativePregMatchResult>, php_runtime::api::BuiltinError>,
    > {
        let limits = self.native_pcre_limits()?;
        let state = self.pcre_state;
        let pattern = self.native_string_view(pattern)?;
        let subject = self.native_string_view(subject)?;
        let state = unsafe { state.as_mut() }?;
        Some(php_runtime::api::native_preg_match(
            state, limits, pattern, subject, flags, offset,
        ))
    }

    #[allow(unsafe_code)]
    fn native_preg_match_all(
        &self,
        pattern: i64,
        subject: i64,
        flags: i64,
        offset: i64,
    ) -> Option<
        Result<Option<php_runtime::api::NativePregMatchAllResult>, php_runtime::api::BuiltinError>,
    > {
        let limits = self.native_pcre_limits()?;
        let state = self.pcre_state;
        let pattern = self.native_string_view(pattern)?;
        let subject = self.native_string_view(subject)?;
        let state = unsafe { state.as_mut() }?;
        Some(php_runtime::api::native_preg_match_all(
            state, limits, pattern, subject, flags, offset,
        ))
    }

    #[allow(unsafe_code)]
    fn native_preg_replace_scalar(
        &self,
        pattern: i64,
        replacement: i64,
        subject: i64,
        limit: i64,
        filter: bool,
    ) -> Option<php_runtime::api::NativePregReplaceResult> {
        let limits = self.native_pcre_limits()?;
        let state = self.pcre_state;
        let pattern = self.native_string_view(pattern)?;
        let replacement = self.native_string_view(replacement)?;
        let subject = self.native_string_view(subject)?;
        let state = unsafe { state.as_mut() }?;
        php_runtime::api::native_preg_replace_scalar(
            state,
            limits,
            pattern,
            replacement,
            subject,
            limit,
            filter,
        )
    }

    #[allow(unsafe_code)]
    fn native_preg_split(
        &self,
        pattern: i64,
        subject: i64,
        limit: i64,
        flags: i64,
    ) -> Option<php_runtime::api::NativeJsonDecodedValue> {
        let limits = self.native_pcre_limits()?;
        let state = self.pcre_state;
        let pattern = self.native_string_view(pattern)?;
        let subject = self.native_string_view(subject)?;
        let state = unsafe { state.as_mut() }?;
        php_runtime::api::native_preg_split(state, limits, pattern, subject, limit, flags)
    }

    #[allow(unsafe_code)]
    fn native_direct_array_entries(
        &self,
        encoded: i64,
    ) -> Option<Vec<php_jit::JitNativeDirectArrayEntry>> {
        let (_, slot) = self.direct_slot(encoded)?;
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY {
            return None;
        }
        let length = usize::try_from(slot.payload).ok()?;
        let entries = slot.aux as usize as *const php_jit::JitNativeDirectArrayEntry;
        if entries.is_null() && length != 0 {
            return None;
        }
        Some(unsafe { std::slice::from_raw_parts(entries, length) }.to_vec())
    }

    #[allow(unsafe_code)]
    fn native_preg_replace_many(
        &self,
        pattern: i64,
        replacement: i64,
        input: i64,
        limit: i64,
        filter: bool,
    ) -> Option<(
        Vec<php_jit::JitNativeDirectArrayEntry>,
        php_runtime::api::NativePregReplaceManyResult,
    )> {
        let entries = self.native_direct_array_entries(input)?;
        let subjects = entries
            .iter()
            .map(|entry| self.native_string_view(entry.value))
            .collect::<Option<Vec<_>>>()?;
        let limits = self.native_pcre_limits()?;
        let state = self.pcre_state;
        let pattern = self.native_string_view(pattern)?;
        let replacement = self.native_string_view(replacement)?;
        let state = unsafe { state.as_mut() }?;
        let result = php_runtime::api::native_preg_replace_many(
            state,
            limits,
            pattern,
            replacement,
            &subjects,
            limit,
            filter,
        )?;
        Some((entries, result))
    }

    #[allow(unsafe_code)]
    fn publish_preg_replace_array(
        &mut self,
        entries: Vec<php_jit::JitNativeDirectArrayEntry>,
        values: Vec<Option<Vec<u8>>>,
    ) -> Result<i64, &'static str> {
        if entries.len() != values.len() {
            return Err("native preg_replace result shape mismatch");
        }
        let output_len = values.iter().filter(|value| value.is_some()).count();
        let capacity = output_len
            .max(php_jit::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize)
            .next_power_of_two();
        let string_bytes = values.iter().try_fold(0usize, |total, value| {
            total.checked_add(value.as_ref().map_or(0, Vec::len))
        });
        let Some(string_bytes) = string_bytes else {
            return Err("native preg_replace result size overflow");
        };
        let view = self.header.runtime_view;
        let value_next = unsafe { *(view.direct_value_next as usize as *const u32) } as usize;
        let array_next = unsafe { *(view.direct_array_next as usize as *const u32) } as usize;
        let string_next = unsafe { *(view.direct_string_next as usize as *const u32) } as usize;
        if value_next
            .checked_add(output_len.saturating_add(1))
            .is_none_or(|end| end > php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY)
            || array_next
                .checked_add(capacity)
                .is_none_or(|end| end > php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY)
            || string_next
                .checked_add(string_bytes)
                .is_none_or(|end| end > php_jit::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY)
        {
            return Err("native preg_replace array exceeded its stable arenas");
        }
        let mut retained_keys = Vec::with_capacity(output_len);
        for (entry, value) in entries.iter().zip(&values) {
            if value.is_none() {
                continue;
            }
            if let Err(error) = self.retain_direct_encoded(entry.key) {
                for retained in retained_keys.into_iter().rev() {
                    self.rollback_direct_retain(retained);
                }
                return Err(error);
            }
            retained_keys.push(entry.key);
        }
        let output = entries
            .into_iter()
            .zip(values)
            .filter_map(|(entry, value)| value.map(|value| (entry.key, value)))
            .map(|(key, value)| {
                self.publish_sequential_json_string(&value)
                    .map(|value| php_jit::JitNativeDirectArrayEntry { key, value })
            })
            .collect::<Option<Vec<_>>>();
        let Some(output) = output else {
            for retained in retained_keys.into_iter().rev() {
                self.rollback_direct_retain(retained);
            }
            return Err("native preg_replace string publication failed");
        };
        self.publish_sequential_direct_array(output)
            .ok_or("native preg_replace array publication failed")
    }

    #[allow(unsafe_code)]
    fn native_preg_grep(
        &self,
        pattern: i64,
        input: i64,
        flags: i64,
    ) -> Option<Vec<php_jit::JitNativeDirectArrayEntry>> {
        let entries = self.native_direct_array_entries(input)?;
        let subjects = entries
            .iter()
            .map(|entry| self.native_string_view(entry.value))
            .collect::<Option<Vec<_>>>()?;
        let limits = self.native_pcre_limits()?;
        let state = self.pcre_state;
        let pattern = self.native_string_view(pattern)?;
        let state = unsafe { state.as_mut() }?;
        let selected =
            php_runtime::api::native_preg_grep(state, limits, pattern, &subjects, flags)?;
        Some(
            entries
                .into_iter()
                .zip(selected)
                .filter_map(|(entry, selected)| selected.then_some(entry))
                .collect(),
        )
    }

    #[allow(unsafe_code)]
    fn publish_retained_direct_array(
        &mut self,
        entries: Vec<php_jit::JitNativeDirectArrayEntry>,
    ) -> Result<i64, &'static str> {
        let capacity = entries
            .len()
            .max(php_jit::JIT_NATIVE_DIRECT_ARRAY_INITIAL_CAPACITY as usize)
            .next_power_of_two();
        let view = self.header.runtime_view;
        let value_next = unsafe { *(view.direct_value_next as usize as *const u32) } as usize;
        let array_next = unsafe { *(view.direct_array_next as usize as *const u32) } as usize;
        if value_next >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY
            || array_next
                .checked_add(capacity)
                .is_none_or(|end| end > php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY)
        {
            return Err("native direct-array result exceeded its stable arenas");
        }
        let mut retained = Vec::with_capacity(entries.len().saturating_mul(2));
        for entry in &entries {
            for encoded in [entry.key, entry.value] {
                if let Err(error) = self.retain_direct_encoded(encoded) {
                    for retained in retained.into_iter().rev() {
                        self.rollback_direct_retain(retained);
                    }
                    return Err(error);
                }
                retained.push(encoded);
            }
        }
        match self.publish_sequential_direct_array(entries) {
            Some(array) => Ok(array),
            None => {
                for retained in retained.into_iter().rev() {
                    self.rollback_direct_retain(retained);
                }
                Err("native direct-array result publication failed")
            }
        }
    }

    #[allow(unsafe_code)]
    fn native_pcre_limits(&self) -> Option<php_runtime::api::PcreMatchLimits> {
        let ini = unsafe { self.ini_registry.as_ref() }?;
        Some(php_runtime::api::PcreMatchLimits {
            backtrack_limit: ini
                .get("pcre.backtrack_limit")
                .and_then(|value| value.trim().parse().ok()),
            recursion_limit: ini
                .get("pcre.recursion_limit")
                .and_then(|value| value.trim().parse().ok()),
            jit: ini
                .get("pcre.jit")
                .is_none_or(|value| !matches!(value.trim(), "" | "0" | "Off" | "off" | "false")),
        })
    }

    #[allow(unsafe_code)]
    fn replace_empty_direct_reference(&mut self, reference: i64, value: i64) -> bool {
        let Some((index, slot)) = self.direct_slot(reference) else {
            return false;
        };
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            || slot.flags != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            || php_jit::jit_decode_runtime_value(slot.payload as i64).is_some()
        {
            return false;
        }
        let slots = self.header.runtime_view.direct_value_slots as usize
            as *mut php_jit::JitNativeValueSlot;
        unsafe {
            (*slots.add(index)).payload = value as u64;
            (*slots.add(index)).reserved = php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED;
        }
        true
    }

    #[allow(unsafe_code)]
    fn clear_json_error(&mut self) -> Result<(), &'static str> {
        let state =
            unsafe { self.json_state.as_mut() }.ok_or("native JSON state is unavailable")?;
        state.set(0);
        Ok(())
    }

    #[allow(unsafe_code)]
    fn native_printf_scalar(
        &self,
        mut encoded: i64,
    ) -> Option<php_runtime::api::NativePrintfScalar<'_>> {
        for _ in 0..64 {
            if let Some((_, slot)) = self.direct_slot(encoded) {
                match slot.kind {
                    php_jit::JIT_NATIVE_VALUE_VIEW_STRING => {
                        return self
                            .native_string_view(encoded)
                            .map(php_runtime::api::NativePrintfScalar::String);
                    }
                    php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT => {
                        return Some(php_runtime::api::NativePrintfScalar::Float(f64::from_bits(
                            slot.payload,
                        )));
                    }
                    php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                        if slot.reserved != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY =>
                    {
                        encoded = slot.payload as i64;
                        continue;
                    }
                    _ => return None,
                }
            }
            if php_jit::jit_decode_runtime_value(encoded).is_none()
                && php_jit::jit_decode_constant(encoded).is_none()
            {
                return Some(php_runtime::api::NativePrintfScalar::Int(encoded));
            }
            let constant = php_jit::jit_decode_constant(encoded)?;
            if constant == u32::MAX {
                return Some(php_runtime::api::NativePrintfScalar::Null);
            }
            if constant == php_jit::JIT_VALUE_FALSE {
                return Some(php_runtime::api::NativePrintfScalar::Bool(false));
            }
            if constant == php_jit::JIT_VALUE_TRUE {
                return Some(php_runtime::api::NativePrintfScalar::Bool(true));
            }
            let view = self.header.runtime_view;
            if constant >= view.trusted_constant_view_count {
                return None;
            }
            let constants =
                view.trusted_constant_views as usize as *const php_jit::JitNativeConstantView;
            let constant = unsafe { *constants.add(constant as usize) };
            return match constant.kind {
                php_jit::JIT_NATIVE_CONSTANT_VIEW_NULL => {
                    Some(php_runtime::api::NativePrintfScalar::Null)
                }
                php_jit::JIT_NATIVE_CONSTANT_VIEW_BOOL => Some(
                    php_runtime::api::NativePrintfScalar::Bool(constant.length != 0),
                ),
                php_jit::JIT_NATIVE_CONSTANT_VIEW_INT => Some(
                    php_runtime::api::NativePrintfScalar::Int(constant.length as i64),
                ),
                php_jit::JIT_NATIVE_CONSTANT_VIEW_FLOAT => Some(
                    php_runtime::api::NativePrintfScalar::Float(f64::from_bits(constant.length)),
                ),
                php_jit::JIT_NATIVE_CONSTANT_VIEW_STRING => self
                    .native_string_view(encoded)
                    .map(php_runtime::api::NativePrintfScalar::String),
                _ => None,
            };
        }
        None
    }

    #[allow(unsafe_code)]
    fn native_printf_array_entries(
        &self,
        encoded: i64,
    ) -> Option<&[php_jit::JitNativeDirectArrayEntry]> {
        let (_, slot) = self.direct_slot(encoded)?;
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY {
            return None;
        }
        let length = usize::try_from(slot.payload).ok()?;
        let entries = slot.aux as usize as *const php_jit::JitNativeDirectArrayEntry;
        if entries.is_null() && length != 0 {
            return None;
        }
        Some(unsafe { std::slice::from_raw_parts(entries, length) })
    }

    #[allow(unsafe_code)]
    fn write_output_bytes(&mut self, bytes: Vec<u8>) -> Result<(), &'static str> {
        let output = unsafe { self.output.as_mut() }.ok_or("native output is unavailable")?;
        output.write_bytes(bytes);
        Ok(())
    }

    #[allow(unsafe_code)]
    fn retain_direct_encoded(&mut self, encoded: i64) -> Result<(), &'static str> {
        let Some(runtime_index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Ok(());
        };
        let Some(index) = runtime_index.checked_sub(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
        else {
            return Err("prepared value belongs to the cold value plane");
        };
        if index as usize >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
            return Err("prepared direct value index is outside its arena");
        }
        let slots = self.header.runtime_view.direct_value_slots as usize
            as *mut php_jit::JitNativeValueSlot;
        // SAFETY: the encoded owner was published in this request's stable
        // direct arena and remains live through the source/template owner.
        let slot = unsafe { &mut *slots.add(index as usize) };
        if slot.refcount == 0 {
            return Err("prepared direct value owner is no longer live");
        }
        slot.refcount = slot
            .refcount
            .checked_add(1)
            .ok_or("prepared direct value refcount overflow")?;
        Ok(())
    }

    #[allow(unsafe_code)]
    fn rollback_direct_retain(&mut self, encoded: i64) {
        let Some(runtime_index) = php_jit::jit_decode_runtime_value(encoded) else {
            return;
        };
        let Some(index) = runtime_index.checked_sub(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
        else {
            return;
        };
        let slots = self.header.runtime_view.direct_value_slots as usize
            as *mut php_jit::JitNativeValueSlot;
        // SAFETY: called only for a successful retain above; the preceding
        // owner remains live, so rollback cannot reach zero.
        let slot = unsafe { &mut *slots.add(index as usize) };
        debug_assert!(slot.refcount > 1);
        slot.refcount -= 1;
    }

    /// Publishes a freshly created object into the authoritative direct plane.
    /// Any already-installed native declared slots are exposed immediately in
    /// the value descriptor; no Rust `Value` or cold identity map is created.
    #[allow(unsafe_code)]
    fn publish_direct_object(
        &mut self,
        object: php_runtime::api::ObjectRef,
    ) -> Result<i64, &'static str> {
        let index = self.reserve_direct_value_index()?;
        let runtime_index = index + php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE;
        let layout_id = object.class_layout_epoch();
        let native_slots = object.native_declared_slots_view(layout_id);
        let object_id = object.id();
        let owner = Box::into_raw(Box::new(object));
        let view = self.header.runtime_view;
        let slots = view.direct_value_slots as usize as *mut php_jit::JitNativeValueSlot;
        let owners = view.direct_object_owners as usize as *mut u64;
        let (flags, reserved, payload, aux) =
            native_slots.map_or((0, 0, object_id, 0), |(base, count)| {
                (
                    php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION,
                    u32::try_from(count).unwrap_or(u32::MAX),
                    layout_id,
                    base as usize as u64,
                )
            });
        unsafe {
            *owners.add(index as usize) = owner as usize as u64;
            *slots.add(index as usize) = php_jit::JitNativeValueSlot {
                refcount: 1,
                kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT,
                flags,
                reserved,
                payload,
                aux,
            };
        }
        Ok((php_jit::JIT_VALUE_RUNTIME_OBJECT_TAG | u64::from(runtime_index)) as i64)
    }
}
// Real applications routinely cross dozens of PHP frames (for example,
// WordPress metadata and hook dispatch). Keep a deterministic native-stack
// guard, but leave enough headroom for those non-recursive call chains.
const NATIVE_CALL_DEPTH_LIMIT: usize = 256;
const NATIVE_RUNTIME_ERROR_MARKER: &str = "E_PHP_NATIVE_RUNTIME_ERROR";

#[derive(Default)]
struct NativeIncludeExports {
    functions: Vec<(String, php_ir::FunctionId)>,
    native_entries:
        std::sync::Arc<std::collections::BTreeMap<php_ir::FunctionId, php_jit::JitFunctionHandle>>,
    native_entry_signature_hashes: std::collections::BTreeMap<php_ir::FunctionId, u64>,
    classes: Vec<String>,
    constants: std::collections::BTreeMap<String, Value>,
    autoload_callbacks: Vec<Value>,
    shutdown_callbacks: Vec<NativeShutdownCallback>,
}

#[derive(Clone, Default)]
struct NativeIncludeSymbols {
    deployment_functions:
        std::sync::Arc<std::collections::HashMap<std::sync::Arc<str>, php_ir::FunctionId>>,
    deployment_classes: std::sync::Arc<std::collections::HashSet<std::sync::Arc<str>>>,
    external_functions: std::collections::HashMap<String, NativeDynamicFunction>,
    external_class_units: std::collections::HashMap<String, usize>,
    external_signature_epoch: u64,
    dynamic_units: Vec<NativeDynamicUnit>,
    dynamic_classes: std::collections::BTreeSet<String>,
    class_aliases: std::collections::BTreeMap<String, String>,
    autoload_callbacks: Vec<Value>,
    shutdown_callbacks: Vec<NativeShutdownCallback>,
    static_properties: std::collections::BTreeMap<(String, String), Value>,
    static_locals: std::collections::BTreeMap<(u64, u32, u32), php_runtime::api::ReferenceCell>,
    enum_cases: std::collections::BTreeMap<(String, String), php_runtime::api::ObjectRef>,
    destroyed_objects: std::collections::BTreeMap<u64, WeakObjectHandle>,
    error_reporting: Option<i64>,
    display_errors: Option<bool>,
    error_handlers: Vec<NativeErrorHandler>,
    exception_handlers: Vec<Value>,
    last_error: Option<NativeLastError>,
}

#[derive(Clone)]
struct NativeShutdownCallback {
    callable: Value,
    arguments: Vec<Value>,
    source: php_ir::Instruction,
}

#[derive(Clone)]
struct NativeErrorHandler {
    callback: Value,
    levels: i64,
}

#[derive(Clone, Copy)]
struct NativeDynamicFunction {
    unit: usize,
    function: php_ir::FunctionId,
}

#[derive(Clone, Copy)]
enum NativeMethodPicTarget {
    CurrentUnit {
        function: php_ir::FunctionId,
        is_static: bool,
    },
    DynamicUnit {
        function: NativeDynamicFunction,
        is_static: bool,
    },
}

struct NativeMethodPicEntry {
    receiver_class: std::sync::Arc<str>,
    method: std::sync::Arc<str>,
    class_layout_epoch: u64,
    method_table_epoch: u64,
    target: NativeMethodPicTarget,
}

#[derive(Default)]
struct NativeMethodPic {
    entries: Vec<NativeMethodPicEntry>,
    megamorphic: bool,
}

const NATIVE_METHOD_PIC_LIMIT: usize = 4;

#[derive(Clone)]
struct NativeDynamicUnit {
    compiled: crate::compiled_unit::CompiledUnit,
    native_entries:
        std::sync::Arc<std::collections::BTreeMap<php_ir::FunctionId, php_jit::JitFunctionHandle>>,
    native_entry_signature_hashes: std::collections::BTreeMap<php_ir::FunctionId, u64>,
    native_entry_signature_epochs: std::collections::BTreeMap<php_ir::FunctionId, u64>,
}

fn native_active_class_handle(
    context: &NativeRequestColdState<'_>,
    name: &str,
) -> Option<crate::compiled_unit::CompiledClass> {
    context.current_dynamic_unit.map_or_else(
        || context.compiled.lookup_unit_class_handle(name),
        |unit| {
            context
                .dynamic_units
                .get(unit)?
                .compiled
                .lookup_unit_class_handle(name)
        },
    )
}

#[derive(Clone, Copy)]
struct ActiveNativeUnit(*const php_ir::IrUnit);

impl ActiveNativeUnit {
    fn new(compiled: &crate::compiled_unit::CompiledUnit) -> Self {
        Self(compiled.unit() as *const php_ir::IrUnit)
    }
}

// SAFETY: The pointed-to IR is owned by `NativeRequestColdState::compiled` or
// by one of its `dynamic_units`. Scoped unit switches retain the prior and new
// `CompiledUnit` handles until after this pointer is restored.
#[allow(unsafe_code)]
impl std::ops::Deref for ActiveNativeUnit {
    type Target = php_ir::IrUnit;

    fn deref(&self) -> &Self::Target {
        // SAFETY: Established by `ActiveNativeUnit::new` and the context
        // ownership invariant documented on this implementation.
        unsafe { &*self.0 }
    }
}

#[derive(Clone, Copy)]
struct NativeInstructionPtr(*const php_ir::Instruction);

// SAFETY: Continuation instructions are owned by the active immutable
// CompiledUnit (or its immutable IR unit fallback). Both outlive every
// synchronous native helper invocation that receives this pointer.
#[allow(unsafe_code)]
impl std::ops::Deref for NativeInstructionPtr {
    type Target = php_ir::Instruction;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.0 }
    }
}

#[derive(Clone, Copy)]
pub(super) struct NativeFunctionMetadataPtr(
    *const crate::compiled_unit::PreparedNativeFunctionMetadata,
);

impl NativeFunctionMetadataPtr {
    fn from_compiled(
        compiled: &crate::compiled_unit::CompiledUnit,
        function: php_ir::FunctionId,
    ) -> Option<Self> {
        compiled
            .prepared_native_function_metadata_ptr(function)
            .map(Self)
    }
}

// SAFETY: Prepared function metadata is immutable and owned by the active
// CompiledUnit. NativeRequestColdState retains that unit (including dynamic
// units) for the lifetime of every synchronous native frame using this view.
#[allow(unsafe_code)]
impl std::ops::Deref for NativeFunctionMetadataPtr {
    type Target = crate::compiled_unit::PreparedNativeFunctionMetadata;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.0 }
    }
}

pub(super) struct NativeRequestColdState<'a> {
    compiled: crate::compiled_unit::CompiledUnit,
    unit: ActiveNativeUnit,
    unit_identity: u64,
    options: &'a super::VmOptions,
    worker_state: &'a super::VmWorkerState,
    /// Stable owner-published fast-state address used only when cold code
    /// re-enters a native artifact. Direct generated operations never reach
    /// this back-pointer through the cold state.
    fast_state: *mut NativeRequestFastState,
    native_entries:
        std::sync::Arc<std::collections::BTreeMap<php_ir::FunctionId, php_jit::JitFunctionHandle>>,
    native_call_encoded_scratch: Vec<i64>,
    native_frame_arena: NativeFrameArena,
    /// Demand-backed native continuation stack used only when a compiled
    /// caller observes `SUSPEND_FIBER`. Generated code writes these records
    /// through the fast-state view; cold code consumes them exactly once when
    /// it installs the suspended Fiber execution tree.
    fiber_suspension_states: php_runtime::api::StableNativeArena<php_jit::JitDeoptState>,
    fiber_suspension_next: Box<u32>,
    native_method_pics: std::collections::BTreeMap<u64, NativeMethodPic>,
    pub(super) output: php_runtime::api::OutputBuffer,
    values: Vec<Option<NativeStoredValue>>,
    value_slots: php_runtime::api::StableNativeArena<php_jit::JitNativeValueSlot>,
    direct_value_slots: php_runtime::api::StableNativeArena<php_jit::JitNativeValueSlot>,
    direct_value_next: Box<u32>,
    direct_object_owners: php_runtime::api::StableNativeArena<u64>,
    direct_array_states: php_runtime::api::StableNativeArena<php_jit::JitNativeDirectArrayState>,
    direct_array_entries: php_runtime::api::StableNativeArena<php_jit::JitNativeDirectArrayEntry>,
    direct_array_next: Box<u32>,
    direct_value_free_head: Box<u32>,
    direct_value_reused_bytes: Box<u64>,
    direct_array_free_heads: Box<[u32; php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS]>,
    direct_array_reused_bytes: Box<u64>,
    direct_string_bytes: php_runtime::api::StableNativeArena<u8>,
    direct_string_next: Box<u32>,
    direct_string_free_heads: Box<[u32; php_jit::JIT_NATIVE_DIRECT_STRING_FREE_BUCKETS]>,
    direct_string_reused_bytes: Box<u64>,
    /// Authoritative storage for exact static properties admitted at request
    /// publication. Dynamic/autoloaded declarations remain in the cold map.
    static_property_slots:
        php_runtime::api::StableNativeArena<php_jit::JitNativeStaticPropertySlot>,
    static_property_next: Box<u32>,
    static_property_indices: std::collections::BTreeMap<(String, String), u32>,
    /// Cold materializations of optimizing-only direct reference slots.
    /// The slot index remains the request-wide alias identity; once a slot
    /// crosses into baseline code its descriptor is rewritten to point at
    /// this cell's stable native scalar view.
    direct_reference_cells: std::collections::HashMap<usize, php_runtime::api::ReferenceCell>,
    /// Request-owned authoritative handles for PHP globals. The Rust
    /// `ReferenceCell` remains the cold identity sidecar; ordinary reads and
    /// writes use the direct reference payload without rebuilding its graph.
    native_global_reference_handles: std::collections::BTreeMap<String, i64>,
    direct_object_handles: std::collections::HashMap<u64, u32>,
    /// Request-wide identity for authoritative direct closure records. The
    /// record itself is owned by the direct value slot's `aux` pointer.
    direct_closure_handles: std::collections::HashMap<u64, u32>,
    /// Cold Fiber identities that have crossed into or originated in the
    /// runtime value API. Ordinary native Fibers are identified by their
    /// direct slot and never enter these maps.
    direct_fiber_handles: std::collections::HashMap<u64, u32>,
    direct_fiber_cells: std::collections::HashMap<usize, php_runtime::api::FiberRef>,
    /// Request-wide content identity for immutable direct strings. Array keys
    /// and values reuse one authoritative native slot instead of rebuilding
    /// the same byte value for every materialized array graph.
    direct_string_handles: std::collections::HashMap<PhpString, u32>,
    direct_string_keys: std::collections::HashMap<usize, PhpString>,
    /// Native identity for one immutable/COW snapshot during a contiguous
    /// recursive materialization. The map is emptied before PHP can mutate
    /// either representation, so it cannot serve a stale cross-operation view.
    direct_array_handles: std::collections::HashMap<(u64, u64), u32>,
    direct_array_storage_ids: std::collections::HashMap<usize, (u64, u64)>,
    direct_array_encode_depth: usize,
    interned_value_handles: NativeValueIdentityMap,
    native_poll_counter: Box<u32>,
    native_root_mutation_pending: Box<u32>,
    free_value_slots: Vec<u32>,
    /// Successfully resolved IR constants are immutable for the lifetime of
    /// their owning unit. Keep one request-local value per unit/index so hot
    /// native operands do not repeatedly search runtime constant registries.
    decoded_constant_cache:
        RefCell<std::collections::HashMap<(Option<usize>, usize), php_runtime::api::Value>>,
    runtime_class_cache:
        RefCell<std::collections::HashMap<(Option<usize>, String), Rc<PreparedNativeRuntimeClass>>>,
    /// Dense, active-unit class allocation plans published once before native
    /// execution. Generated code indexes this table by immutable ClassId.
    trusted_class_plans: Vec<php_jit::JitNativePreparedClassPlan>,
    /// Long-lived request roots (globals, statics, callbacks, sessions, and
    /// suspended state). This index must not be invalidated by every call.
    root_index: RequestRootIndex,
    resources: php_runtime::api::ResourceTable,
    builtin_request_state: php_runtime::api::BuiltinRequestState,
    registered_extensions: NativeRegisteredExtensionRequestState,
    pub(super) http_response: php_runtime::api::RuntimeHttpResponseState,
    pub(super) upload_registry: php_runtime::api::UploadRegistry,
    pub(super) session: php_runtime::api::SessionState,
    session_global: php_runtime::api::ReferenceCell,
    filter_input_arrays: Rc<std::collections::BTreeMap<i64, php_runtime::api::PhpArray>>,
    ini_registry: php_runtime::api::IniRegistry,
    default_timezone: String,
    mysql_state: std::rc::Rc<RefCell<php_runtime::api::MysqlState>>,
    dynamic_constants: std::collections::BTreeMap<String, Value>,
    visible_function_names: Rc<NativeFunctionNameScope>,
    inherited_autoload_callback_count: usize,
    inherited_shutdown_callback_count: usize,
    dynamic_functions: std::collections::BTreeMap<String, php_ir::FunctionId>,
    deployment_functions:
        std::sync::Arc<std::collections::HashMap<std::sync::Arc<str>, php_ir::FunctionId>>,
    deployment_classes: std::sync::Arc<std::collections::HashSet<std::sync::Arc<str>>>,
    external_functions: std::collections::HashMap<String, NativeDynamicFunction>,
    external_class_units: std::collections::HashMap<String, usize>,
    /// Monotonic identity of the visible cross-unit by-reference signature
    /// set. By-value declarations cannot alter generated caller binding, so
    /// they must not invalidate every already-published native entry.
    external_signature_epoch: u64,
    dynamic_units: Vec<NativeDynamicUnit>,
    current_dynamic_unit: Option<usize>,
    static_properties: std::collections::BTreeMap<(String, String), Value>,
    static_locals: std::collections::BTreeMap<(u64, u32, u32), php_runtime::api::ReferenceCell>,
    enum_cases: std::collections::BTreeMap<(String, String), php_runtime::api::ObjectRef>,
    class_constant_cache: std::collections::HashMap<
        (Option<usize>, u32),
        std::collections::HashMap<String, std::collections::HashMap<String, i64>>,
    >,
    generator_iterators: std::collections::BTreeMap<u64, i64>,
    fiber_executions: std::collections::BTreeMap<u64, NativeFiberExecution>,
    active_fiber: Option<u64>,
    pending_fiber_suspension_value: Option<i64>,
    completed_nested_fiber_call: Option<(u32, u32, php_jit::JitCallStatus, i64)>,
    pending_throwable: Option<Value>,
    called_classes: Vec<Arc<str>>,
    lexical_scope_classes: Vec<String>,
    call_frames: Vec<NativeBacktraceFrame>,
    dynamic_classes: std::collections::BTreeSet<String>,
    class_aliases: std::collections::BTreeMap<String, String>,
    autoload_callbacks: Vec<Value>,
    shutdown_callbacks: Vec<NativeShutdownCallback>,
    destroyed_objects: std::collections::BTreeMap<u64, WeakObjectHandle>,
    autoload_in_progress: std::collections::BTreeSet<String>,
    error_reporting: i64,
    display_errors: bool,
    last_error: Option<NativeLastError>,
    error_handlers: Vec<NativeErrorHandler>,
    exception_handlers: Vec<Value>,
    explicit_reference_ids: std::collections::BTreeSet<u64>,
    environment: std::sync::Arc<Vec<(String, String)>>,
    included_files: std::collections::BTreeSet<std::path::PathBuf>,
    include_path: Arc<Vec<std::path::PathBuf>>,
    cwd: std::path::PathBuf,
    inherited_globals: std::collections::BTreeMap<String, Value>,
    /// Stable request owner loaded directly for the special `$GLOBALS`
    /// local in optimizing functions.
    trusted_globals_proxy: i64,
    /// Authoritative numeric lvalue slots for top-level/include locals and
    /// superglobals, indexed by immutable `(FunctionId, LocalId)` offsets.
    trusted_request_local_function_offsets: Vec<u32>,
    trusted_request_local_slots: Vec<php_jit::JitNativeRequestLocalSlot>,
    continuation_instructions:
        std::sync::Arc<Vec<Vec<Option<std::sync::Arc<php_ir::Instruction>>>>>,
    trusted_property_function_offsets: Vec<u32>,
    trusted_property_slots: Vec<php_jit::JitNativeTrustedPropertySlot>,
    /// Exact global constants resolved by their cold continuation once. This
    /// parallel continuation table owns one encoded value per published slot.
    trusted_constant_slots: Vec<php_jit::JitNativeTrustedConstantSlot>,
    trusted_global_reference_slots: Vec<php_jit::JitNativeTrustedGlobalReferenceSlot>,
    trusted_global_reference_names: Vec<Option<Box<str>>>,
    trusted_static_local_slots: Vec<php_jit::JitNativeTrustedStaticLocalSlot>,
    trusted_static_property_slots: Vec<php_jit::JitNativeTrustedStaticPropertySlot>,
    /// Dense static `instanceof` plans indexed by the existing continuation
    /// offsets. Their immutable entries are rebuilt only when the active unit
    /// changes; generated code performs an exact layout-id lookup.
    trusted_instanceof_plans: Vec<php_jit::JitNativeInstanceOfPlan>,
    trusted_instanceof_entries: Vec<php_jit::JitNativeInstanceOfEntry>,
    native_callsites: std::sync::Arc<
        Vec<Vec<Option<std::sync::Arc<crate::compiled_unit::NativeCallSiteDescriptor>>>>,
    >,
    include_child: bool,
    execution_deadline_at: Option<std::time::Instant>,
    execution_deadline_mutable: bool,
    runtime_telemetry: Rc<RefCell<NativeRuntimeTelemetry>>,
    pub(super) diagnostic: Option<php_runtime::api::RuntimeDiagnostic>,
}

/// Request lifetime owner. Fast and cold state are separately allocated so
/// generated code can retain the compact ABI pointer without pointing at a
/// facade whose first operation recovers the complete Rust coordinator.
pub(super) struct NativeRequestOwner<'a> {
    cold: Box<NativeRequestColdState<'a>>,
    _fast: Box<NativeRequestFastState>,
}

impl<'a> NativeRequestOwner<'a> {
    pub(super) fn new(
        compiled: &'a crate::compiled_unit::CompiledUnit,
        unit_identity: u64,
        options: &'a super::VmOptions,
        worker_state: &'a super::VmWorkerState,
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
        let mut fast = Box::<NativeRequestFastState>::default();
        let fast_ptr = std::ptr::from_mut(fast.as_mut());
        let cold_ptr = std::ptr::from_mut(cold.as_mut()).cast();
        cold.fast_state = fast_ptr;
        fast.cold_context = cold_ptr;
        fast.output = std::ptr::from_mut(&mut cold.output);
        fast.json_state = std::ptr::from_mut(cold.builtin_request_state.json_mut());
        fast.pcre_state = std::ptr::from_mut(cold.builtin_request_state.pcre_mut());
        fast.ini_registry = std::ptr::from_ref(&cold.ini_registry);
        cold.trusted_globals_proxy = cold
            .encode_globals_proxy()
            .expect("request globals proxy must fit the native value arena");
        cold.prepare_trusted_constant_fetches();
        cold.prepare_trusted_request_locals();
        cold.prepare_trusted_global_references();
        cold.prepare_trusted_static_locals();
        cold.prepare_trusted_static_properties();
        cold.prepare_trusted_class_plans();
        cold.prepare_trusted_declared_properties();
        cold.prepare_trusted_instanceof_plans();
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

// Generated code holds raw pointers into these parallel vectors while a
// request is active, so their allocations must not move.
const NATIVE_COLD_VALUE_SLOT_LIMIT: usize = 1 << 20;
fn stored_value_slot(value: &NativeStoredValue) -> php_jit::JitNativeValueSlot {
    let mut slot = php_jit::JitNativeValueSlot {
        refcount: 1,
        ..php_jit::JitNativeValueSlot::default()
    };
    match value {
        NativeStoredValue::Php(Value::String(value)) => {
            slot.kind = php_jit::JIT_NATIVE_VALUE_VIEW_STRING;
            slot.flags = php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION;
            slot.reserved =
                u32::from(value.as_bytes() == b"0") * php_jit::JIT_NATIVE_STRING_VALUE_ZERO;
            slot.payload = u64::try_from(value.len()).unwrap_or(u64::MAX);
            slot.aux = value.as_bytes().as_ptr() as usize as u64;
        }
        NativeStoredValue::Php(Value::Reference(reference)) => {
            slot.kind = php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR;
            slot.flags = php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION;
            slot.payload = reference.native_scalar_view_address() as u64;
            slot.aux = reference.native_array_view_address() as u64;
        }
        NativeStoredValue::ArrayIterator(iterator) => {
            if let Some(direct) = iterator.direct.as_ref() {
                slot.kind = php_jit::JIT_NATIVE_VALUE_VIEW_FOREACH_DIRECT;
                slot.flags = php_jit::JIT_NATIVE_FOREACH_VIEW_ABI_VERSION;
                slot.payload = std::ptr::from_ref(direct.view.as_ref()) as usize as u64;
            }
        }
        _ => {}
    }
    slot
}

enum NativeStoredValue {
    Php(Value),
    GlobalsProxy,
    ArrayIterator(Box<NativeArrayIteratorState>),
    Iterator(Box<NativeIteratorState>),
    GeneratorIterator(Box<NativeGeneratorIteratorState>),
}

/// Value family observed directly from an encoded native value.  This is a
/// classification of the authoritative slot, not a second value
/// representation: it owns no payload and cannot outlive the query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeEncodedValueKind {
    Null,
    Uninitialized,
    Bool(bool),
    Int,
    Float,
    String,
    Array,
    Object,
    Callable,
    Resource,
    Generator,
    Fiber,
    Reference,
}

/// Structured control leaving one native PHP call. Successful calls carry the
/// authoritative native encoding unchanged; exceptional control is kept
/// typed until the ABI boundary instead of being serialized into `E_PHP_*`
/// marker strings and parsed again by the caller.
#[derive(Debug)]
enum NativeCallControl {
    Rethrow,
    Throw {
        class: String,
        message: String,
    },
    /// A nested native activation already produced an authoritative encoded
    /// PHP control value. Re-entering its caller must preserve both fields
    /// verbatim so the caller's generated unwind/catch path handles them.
    Propagate {
        status: php_jit::JitCallStatus,
        value: i64,
    },
    SuspendFiber {
        state: Option<Box<php_jit::JitDeoptState>>,
    },
    Exit(i64),
    PublishedRuntimeError,
    RuntimeError(String),
    BaselineRequired,
}

type NativeCallResult = Result<i64, NativeCallControl>;

impl NativeCallControl {
    fn throw(class: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Throw {
            class: class.into(),
            message: message.into(),
        }
    }

    /// Compatibility serialization for baseline-only callers that still use
    /// the legacy Rust semantic result. Optimizing exact handlers never call
    /// this function.
    fn into_baseline_error(self) -> String {
        match self {
            Self::Rethrow => "E_PHP_RETHROW".to_owned(),
            Self::Throw { class, message } => format!("E_PHP_THROW:{class}:{message}"),
            Self::Propagate { status, value } => format!(
                "native encoded control status={} value={value} escaped into the baseline boundary",
                status.0
            ),
            Self::SuspendFiber { .. } => "E_PHP_SUSPEND_FIBER".to_owned(),
            Self::Exit(value) => format!("E_PHP_EXIT:{value}"),
            Self::PublishedRuntimeError => NATIVE_RUNTIME_ERROR_MARKER.to_owned(),
            Self::RuntimeError(message) => message,
            Self::BaselineRequired => {
                "native call requested its baseline continuation before effects".to_owned()
            }
        }
    }

    /// Parses the legacy baseline semantic envelope at the baseline ABI
    /// boundary. Exact optimizing handlers construct typed control directly
    /// and never enter this compatibility conversion.
    fn from_baseline_error(message: String) -> Self {
        if message == "E_PHP_RETHROW" {
            return Self::Rethrow;
        }
        if let Some(payload) = message.strip_prefix("E_PHP_THROW:") {
            let (class, message) = payload.split_once(':').unwrap_or(("Error", payload));
            return Self::throw(class, message);
        }
        if message == "E_PHP_SUSPEND_FIBER" {
            return Self::SuspendFiber { state: None };
        }
        if let Some(value) = message.strip_prefix("E_PHP_EXIT:")
            && let Ok(value) = value.parse::<i64>()
        {
            return Self::Exit(value);
        }
        if message == NATIVE_RUNTIME_ERROR_MARKER {
            return Self::PublishedRuntimeError;
        }
        Self::RuntimeError(message)
    }
}

impl From<String> for NativeCallControl {
    fn from(message: String) -> Self {
        Self::from_baseline_error(message)
    }
}

impl From<&str> for NativeCallControl {
    fn from(message: &str) -> Self {
        Self::RuntimeError(message.to_owned())
    }
}

/// Baseline/cold callers still expose the legacy semantic error envelope.
/// This conversion is intentionally one-way: typed native call control is
/// never reconstructed by parsing these strings in optimizing exact code.
impl From<NativeCallControl> for String {
    fn from(control: NativeCallControl) -> Self {
        control.into_baseline_error()
    }
}

struct NativePreparedClosure {
    /// PHP closure metadata only. `captures` and `bound_this` are always
    /// empty here; their authoritative owners are the encoded fields below.
    closure: php_runtime::api::ClosurePayload,
    capture_descriptors: Box<[(String, bool)]>,
    implicit_this: Option<i64>,
    captures: Box<[i64]>,
}

enum NativePreparedCallable {
    UserFunction {
        name: String,
    },
    Closure(NativePreparedClosure),
    InternalBuiltin {
        name: String,
    },
    BoundMethod {
        target: NativePreparedCallableMethodTarget,
        method: String,
        scope: Option<String>,
    },
    MethodPlaceholder {
        target: String,
    },
    UnresolvedDynamic {
        target: String,
    },
}

enum NativePreparedCallableMethodTarget {
    Object(i64),
    Class(String),
}

enum NativePreparedCallableDispatch {
    Closure,
    Named(String),
    BoundMethod {
        target: php_runtime::api::CallableMethodTarget,
        method: String,
    },
    Invalid(String),
}

struct NativeDirectFiber {
    state: php_runtime::api::FiberState,
    callable: i64,
    return_value: Option<i64>,
}

enum NativeFiberReceiver {
    Direct(i64),
    Materialized(php_runtime::api::FiberRef),
}

struct NativeArrayIteratorState {
    source: php_runtime::api::PhpArray,
    index: usize,
    direct: Option<Box<NativeDirectForeachState>>,
}

struct NativeDirectForeachState {
    view: Box<php_jit::JitNativeForeachView>,
    entries: Box<[php_jit::JitNativeForeachEntry]>,
}

struct NativeIteratorState {
    entries: Vec<(Value, Value)>,
    index: usize,
    live_source: Option<i64>,
    live_global: Option<String>,
    live_object: Option<php_runtime::api::ObjectRef>,
    user_iterator: Option<php_runtime::api::ObjectRef>,
    user_iterator_started: bool,
}

struct NativeGeneratorIteratorState {
    generator: php_runtime::api::GeneratorRef,
    handle: Box<php_jit::JitFunctionHandle>,
    arguments: Vec<i64>,
    state: Box<Option<php_jit::JitDeoptState>>,
    delegation: Option<NativeGeneratorDelegation>,
    yields_seen: u64,
    finished: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum NativeValueIdentity {
    Object(u64),
    Reference(u64),
    String(php_runtime::api::PhpString),
    GlobalsProxy,
}

#[derive(Default)]
struct NativeValueIdentityHasher(u64);

impl std::hash::Hasher for NativeValueIdentityHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(std::mem::size_of::<u64>()) {
            let mut word = [0_u8; std::mem::size_of::<u64>()];
            word[..chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_ne_bytes(word));
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = self.0.rotate_left(17) ^ value.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }
}

type NativeValueIdentityMap = std::collections::HashMap<
    NativeValueIdentity,
    u32,
    std::hash::BuildHasherDefault<NativeValueIdentityHasher>,
>;

pub(super) struct NativeValueArenaBuffers {
    values: Vec<Option<NativeStoredValue>>,
    value_slots: php_runtime::api::StableNativeArena<php_jit::JitNativeValueSlot>,
    direct_value_slots: php_runtime::api::StableNativeArena<php_jit::JitNativeValueSlot>,
    direct_value_next: Box<u32>,
    direct_object_owners: php_runtime::api::StableNativeArena<u64>,
    direct_array_states: php_runtime::api::StableNativeArena<php_jit::JitNativeDirectArrayState>,
    direct_array_entries: php_runtime::api::StableNativeArena<php_jit::JitNativeDirectArrayEntry>,
    direct_array_next: Box<u32>,
    direct_value_free_head: Box<u32>,
    direct_value_reused_bytes: Box<u64>,
    direct_array_free_heads: Box<[u32; php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS]>,
    direct_array_reused_bytes: Box<u64>,
    direct_string_bytes: php_runtime::api::StableNativeArena<u8>,
    direct_string_next: Box<u32>,
    direct_string_free_heads: Box<[u32; php_jit::JIT_NATIVE_DIRECT_STRING_FREE_BUCKETS]>,
    direct_string_reused_bytes: Box<u64>,
    interned_value_handles: NativeValueIdentityMap,
    free_value_slots: Vec<u32>,
}

impl Default for NativeValueArenaBuffers {
    fn default() -> Self {
        Self {
            // The Rust compatibility plane is cold and may grow normally.
            // Its ABI records still need a stable base for baseline-native
            // continuations, but demand-backed storage avoids constructing or
            // touching the million-slot upper bound.
            values: Vec::new(),
            value_slots: php_runtime::api::StableNativeArena::new(NATIVE_COLD_VALUE_SLOT_LIMIT),
            direct_value_slots: php_runtime::api::StableNativeArena::new(
                php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY,
            ),
            direct_value_next: Box::new(0),
            direct_object_owners: php_runtime::api::StableNativeArena::new(
                php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY,
            ),
            direct_array_states: php_runtime::api::StableNativeArena::new(
                php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY,
            ),
            direct_array_entries: php_runtime::api::StableNativeArena::new(
                php_jit::JIT_NATIVE_DIRECT_ARRAY_ENTRY_CAPACITY,
            ),
            direct_array_next: Box::new(0),
            direct_value_free_head: Box::new(php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE),
            direct_value_reused_bytes: Box::new(0),
            direct_array_free_heads: Box::new(
                [php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
                    php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_BUCKETS],
            ),
            direct_array_reused_bytes: Box::new(0),
            direct_string_bytes: php_runtime::api::StableNativeArena::new(
                php_jit::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY,
            ),
            direct_string_next: Box::new(0),
            direct_string_free_heads: Box::new(
                [php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
                    php_jit::JIT_NATIVE_DIRECT_STRING_FREE_BUCKETS],
            ),
            direct_string_reused_bytes: Box::new(0),
            interned_value_handles: NativeValueIdentityMap::default(),
            free_value_slots: Vec::new(),
        }
    }
}

impl std::fmt::Debug for NativeValueArenaBuffers {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeValueArenaBuffers")
            .field("value_capacity", &self.values.capacity())
            .field("slot_capacity", &self.value_slots.capacity())
            .finish()
    }
}

thread_local! {
    static NATIVE_VALUE_ARENA_POOL: RefCell<Vec<NativeValueArenaBuffers>> = const {
        RefCell::new(Vec::new())
    };
}

fn take_native_value_arena() -> NativeValueArenaBuffers {
    NATIVE_VALUE_ARENA_POOL.with(|arenas| arenas.borrow_mut().pop().unwrap_or_default())
}

fn recycle_native_value_arena(arena: NativeValueArenaBuffers) {
    debug_assert!(arena.values.is_empty());
    debug_assert!(arena.interned_value_handles.is_empty());
    debug_assert!(arena.free_value_slots.is_empty());
    const MAX_RETAINED_NATIVE_VALUE_ARENAS: usize = 1;
    NATIVE_VALUE_ARENA_POOL.with(|arenas| {
        let mut arenas = arenas.borrow_mut();
        if arenas.len() < MAX_RETAINED_NATIVE_VALUE_ARENAS {
            arenas.push(arena);
        }
    });
}

fn trusted_property_storage(
    continuations: &[Vec<Option<std::sync::Arc<php_ir::Instruction>>>],
) -> (Vec<u32>, Vec<php_jit::JitNativeTrustedPropertySlot>) {
    let mut offsets = Vec::with_capacity(continuations.len());
    let mut count = 0_usize;
    for function in continuations {
        offsets.push(u32::try_from(count).unwrap_or(u32::MAX));
        count = count.saturating_add(function.len());
    }
    (
        offsets,
        vec![php_jit::JitNativeTrustedPropertySlot::default(); count],
    )
}

fn trusted_request_local_storage(
    unit: &php_ir::IrUnit,
) -> (Vec<u32>, Vec<php_jit::JitNativeRequestLocalSlot>) {
    let mut offsets = Vec::with_capacity(unit.functions.len());
    let mut count = 0_usize;
    for function in &unit.functions {
        offsets.push(u32::try_from(count).unwrap_or(u32::MAX));
        count = count.saturating_add(function.locals.len());
    }
    (
        offsets,
        vec![php_jit::JitNativeRequestLocalSlot::default(); count],
    )
}

fn native_request_local_name(function: &php_ir::IrFunction, local: usize) -> Option<&str> {
    const SUPERGLOBALS: &[&str] = &[
        "_GET", "_POST", "_COOKIE", "_REQUEST", "_SERVER", "_ENV", "_FILES", "_SESSION",
    ];
    let name = function.locals.get(local)?.as_str();
    ((function.flags.is_top_level && name != "GLOBALS") || SUPERGLOBALS.contains(&name))
        .then_some(name)
}

fn stored_value_identity(value: &NativeStoredValue) -> Option<NativeValueIdentity> {
    match value {
        NativeStoredValue::Php(Value::Object(object)) => {
            Some(NativeValueIdentity::Object(object.id()))
        }
        NativeStoredValue::Php(Value::Reference(reference)) => {
            Some(NativeValueIdentity::Reference(reference.gc_debug_id()))
        }
        NativeStoredValue::Php(Value::String(string)) => {
            Some(NativeValueIdentity::String(string.clone()))
        }
        NativeStoredValue::GlobalsProxy => Some(NativeValueIdentity::GlobalsProxy),
        _ => None,
    }
}

fn stored_value_tag(value: &NativeStoredValue) -> u64 {
    match value {
        NativeStoredValue::Php(Value::Reference(_)) => php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG,
        NativeStoredValue::Php(Value::Array(_)) => php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG,
        NativeStoredValue::Php(Value::Object(_)) => php_jit::JIT_VALUE_RUNTIME_OBJECT_TAG,
        NativeStoredValue::Php(Value::String(_)) => php_jit::JIT_VALUE_RUNTIME_STRING_TAG,
        NativeStoredValue::Php(Value::Float(_)) => php_jit::JIT_VALUE_RUNTIME_FLOAT_TAG,
        NativeStoredValue::Php(Value::Callable(_)) => php_jit::JIT_VALUE_RUNTIME_CALLABLE_TAG,
        NativeStoredValue::Php(Value::Resource(_)) => php_jit::JIT_VALUE_RUNTIME_RESOURCE_TAG,
        NativeStoredValue::Php(Value::Generator(_)) => php_jit::JIT_VALUE_RUNTIME_GENERATOR_TAG,
        NativeStoredValue::Php(Value::Fiber(_)) => php_jit::JIT_VALUE_RUNTIME_FIBER_TAG,
        NativeStoredValue::GlobalsProxy => php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG,
        NativeStoredValue::ArrayIterator(_)
        | NativeStoredValue::Iterator(_)
        | NativeStoredValue::GeneratorIterator(_) => php_jit::JIT_VALUE_RUNTIME_ITERATOR_TAG,
        NativeStoredValue::Php(
            Value::Null | Value::Bool(_) | Value::Int(_) | Value::Uninitialized,
        ) => php_jit::JIT_VALUE_RUNTIME_TAG,
    }
}

fn stored_value_kind(value: &NativeStoredValue) -> &'static str {
    match value {
        NativeStoredValue::Php(Value::Null) => "null",
        NativeStoredValue::Php(Value::Bool(_)) => "bool",
        NativeStoredValue::Php(Value::Int(_)) => "int",
        NativeStoredValue::Php(Value::Float(_)) => "float",
        NativeStoredValue::Php(Value::String(_)) => "string",
        NativeStoredValue::Php(Value::Array(_)) => "array",
        NativeStoredValue::Php(Value::Object(_)) => "object",
        NativeStoredValue::Php(Value::Resource(_)) => "resource",
        NativeStoredValue::Php(Value::Reference(_)) => "reference",
        NativeStoredValue::Php(Value::Callable(_)) => "callable",
        NativeStoredValue::Php(Value::Generator(_)) => "generator",
        NativeStoredValue::Php(Value::Fiber(_)) => "fiber",
        NativeStoredValue::Php(Value::Uninitialized) => "uninitialized",
        NativeStoredValue::GlobalsProxy => "globals_proxy",
        NativeStoredValue::ArrayIterator(_) => "array_iterator",
        NativeStoredValue::Iterator(_) => "iterator",
        NativeStoredValue::GeneratorIterator(_) => "generator_iterator",
    }
}

struct PreparedNativeRuntimeClass {
    entry: php_runtime::api::ClassEntry,
    display_name: String,
    layout_id: u64,
    /// One request-owned native owner per initialized default. Each object
    /// instance retains these encoded values into its cloned slot vector.
    default_native_slots: Box<[php_runtime::api::NativeDeclaredPropertySlot]>,
}

#[derive(Clone)]
enum NativeGeneratorDelegation {
    Array {
        entries: Vec<(Value, Value)>,
        index: usize,
    },
    Generator {
        generator: php_runtime::api::GeneratorRef,
        iterator: i64,
    },
}

struct NativeFiberExecution {
    handle: php_jit::JitFunctionHandle,
    arguments: Vec<i64>,
    state: php_jit::JitDeoptState,
    nested: Option<Box<NativeFiberExecution>>,
}

impl<'a> NativeRequestColdState<'a> {
    pub(super) fn native_runtime_ptr(&mut self) -> *mut std::ffi::c_void {
        self.fast_state.cast()
    }

    fn take_native_fiber_suspension_state(
        &mut self,
        handle: u64,
    ) -> Result<Option<php_jit::JitDeoptState>, String> {
        if handle == 0 {
            return Ok(None);
        }
        let next = usize::try_from(*self.fiber_suspension_next)
            .map_err(|_| "native Fiber suspension stack is invalid".to_owned())?;
        let index = usize::try_from(handle - 1)
            .map_err(|_| "native Fiber suspension handle is invalid".to_owned())?;
        if index >= self.fiber_suspension_states.capacity() || index + 1 != next {
            return Err(format!(
                "native Fiber suspension stack is not LIFO: handle={handle} depth={next}"
            ));
        }
        *self.fiber_suspension_next = u32::try_from(index).unwrap_or(0);
        Ok(Some(self.fiber_suspension_states[index]))
    }

    fn discard_native_fiber_suspension_states(&mut self) {
        // Stack entries are snapshots of owners already carried by generated
        // activation state; the arena itself owns no encoded values.
        *self.fiber_suspension_next = 0;
    }

    /// Releases the owners captured in a suspended native activation when no
    /// generated continuation will ever resume it. Normal return/unwind runs
    /// the generated epilogue and must not pass through this path.
    fn abandon_native_fiber_execution(
        &mut self,
        execution: NativeFiberExecution,
    ) -> Result<(), String> {
        let NativeFiberExecution {
            handle,
            arguments: _,
            state,
            nested,
        } = execution;
        if let Some(nested) = nested {
            self.abandon_native_fiber_execution(*nested)?;
        }

        let metadata = handle
            .region_state_metadata()
            .ok_or_else(|| "suspended native Fiber has no state metadata".to_owned())?;
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
                    "suspended native Fiber state {}:{} has no ownership metadata",
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

    fn mark_roots_dirty(&mut self, reason: RootMutationReason) {
        self.root_index.mark_dirty(reason);
    }

    fn mark_rooted_container_dirty(&mut self, value: &Value) {
        self.root_index
            .mark_dirty(RootMutationReason::RootedContainer);
        self.root_index.refresh_container(value);
    }

    fn value_has_native_destructor(&self, value: &Value) -> bool {
        let mut value = value.clone();
        for _ in 0..16 {
            match value {
                Value::Reference(reference) => value = reference.get(),
                Value::Object(object) => {
                    return self.object_has_native_destructor(&object.class_name());
                }
                _ => return false,
            }
        }
        false
    }

    fn synchronize_destructor_root_change(&mut self, previous: &Value, replacement: &Value) {
        if self.value_has_native_destructor(previous)
            || self.value_has_native_destructor(replacement)
        {
            self.synchronize_request_roots();
        }
    }

    fn add_rooted_nested_container(&mut self, parent: &Value, child: &Value) {
        if self.root_index.is_dirty() || self.root_index.contains_container(parent) {
            self.root_index.add_nested_container(parent, child);
        }
    }

    fn request_root_values(&self) -> Vec<Value> {
        let mut roots = self
            .static_properties
            .values()
            .chain(self.dynamic_constants.values())
            .chain(self.inherited_globals.values())
            .chain(self.autoload_callbacks.iter())
            .chain(self.exception_handlers.iter())
            .cloned()
            .collect::<Vec<_>>();
        roots.extend(self.static_locals.values().cloned().map(Value::Reference));
        roots.push(Value::Reference(self.session_global.clone()));
        for callback in &self.shutdown_callbacks {
            roots.push(callback.callable.clone());
            roots.extend(callback.arguments.iter().cloned());
        }
        roots.extend(
            self.error_handlers
                .iter()
                .map(|handler| handler.callback.clone()),
        );
        roots.extend(self.pending_throwable.iter().cloned());
        roots.extend(self.enum_cases.values().cloned().map(Value::Object));
        roots
    }

    fn synchronize_request_roots(&mut self) {
        self.consume_native_root_mutation();
        if self.root_index.is_dirty() {
            let roots = self.request_root_values();
            self.root_index.synchronize(&roots);
        }
    }

    fn consume_native_root_mutation(&mut self) {
        if *self.native_root_mutation_pending == 0 {
            return;
        }
        *self.native_root_mutation_pending = 0;
        self.root_index
            .mark_dirty(RootMutationReason::RootedContainer);
    }

    fn finalize_replaced_value(&mut self, previous: Value) -> Result<(), String> {
        if let Value::Object(object) = previous {
            let class_name = object.class_name();
            if self.object_has_native_destructor(&class_name)
                && !self.object_is_request_rooted(object.id())
            {
                self.run_object_destructor(object)?;
            }
        }
        Ok(())
    }

    pub(super) const fn process_exit_terminates_process(&self) -> bool {
        self.registered_extensions.is_fork_child()
    }

    pub(super) fn new(
        compiled: &'a crate::compiled_unit::CompiledUnit,
        unit_identity: u64,
        options: &'a super::VmOptions,
        worker_state: &'a super::VmWorkerState,
        output: php_runtime::api::OutputBuffer,
        native_entries: std::sync::Arc<
            std::collections::BTreeMap<php_ir::FunctionId, php_jit::JitFunctionHandle>,
        >,
    ) -> Self {
        let unit = compiled.unit();
        let inherited_globals = NATIVE_INCLUDE_GLOBALS.with(|globals| globals.borrow_mut().take());
        let inherited_constants =
            NATIVE_INCLUDE_CONSTANTS.with(|constants| constants.borrow_mut().take());
        let inherited_ini = NATIVE_INCLUDE_INI.with(|ini| ini.borrow_mut().take());
        let inherited_default_timezone =
            NATIVE_INCLUDE_DEFAULT_TIMEZONE.with(|timezone| timezone.borrow_mut().take());
        let inherited_http_response =
            NATIVE_INCLUDE_HTTP_RESPONSE.with(|response| response.borrow_mut().take());
        let inherited_files = NATIVE_INCLUDE_FILES.with(|files| files.borrow_mut().take());
        let inherited_mysql = NATIVE_INCLUDE_MYSQL.with(|mysql| mysql.borrow_mut().take());
        let inherited_filter_input_arrays =
            NATIVE_INCLUDE_FILTER_INPUT_ARRAYS.with(|arrays| arrays.borrow_mut().take());
        let inherited_function_names = NATIVE_INCLUDE_FUNCTION_NAMES.with(|names| {
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
        let inherited_symbols =
            NATIVE_INCLUDE_SYMBOLS.with(|symbols| symbols.borrow_mut().take().unwrap_or_default());
        let inherited_error_reporting = inherited_symbols.error_reporting;
        let inherited_display_errors = inherited_symbols.display_errors;
        let inherited_autoload_callback_count = inherited_symbols.autoload_callbacks.len();
        let inherited_shutdown_callback_count = inherited_symbols.shutdown_callbacks.len();
        let include_child = inherited_globals.is_some();
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
        let mut dynamic_constants = inherited_constants.unwrap_or_default();
        dynamic_constants
            .entry("STDIN".to_owned())
            .or_insert(Value::Resource(stdin));
        dynamic_constants
            .entry("STDOUT".to_owned())
            .or_insert(Value::Resource(stdout));
        dynamic_constants
            .entry("STDERR".to_owned())
            .or_insert(Value::Resource(stderr));
        let continuation_instructions = compiled.prepared_continuation_instructions();
        let (trusted_property_function_offsets, trusted_property_slots) =
            trusted_property_storage(&continuation_instructions);
        let (trusted_request_local_function_offsets, trusted_request_local_slots) =
            trusted_request_local_storage(compiled.unit());
        let trusted_constant_slots =
            vec![php_jit::JitNativeTrustedConstantSlot::default(); trusted_property_slots.len()];
        let trusted_global_reference_slots = vec![
            php_jit::JitNativeTrustedGlobalReferenceSlot::default();
            trusted_property_slots.len()
        ];
        let trusted_global_reference_names =
            (0..trusted_property_slots.len()).map(|_| None).collect();
        let trusted_static_local_slots =
            vec![php_jit::JitNativeTrustedStaticLocalSlot::default(); trusted_property_slots.len()];
        let trusted_static_property_slots = vec![
                php_jit::JitNativeTrustedStaticPropertySlot::default();
                trusted_property_slots.len()
            ];
        let trusted_instanceof_plans =
            vec![php_jit::JitNativeInstanceOfPlan::default(); trusted_property_slots.len()];
        let native_callsites = compiled.prepared_native_callsites();
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
        let value_arena = take_native_value_arena();
        Self {
            compiled: compiled.clone(),
            unit: ActiveNativeUnit::new(compiled),
            unit_identity,
            options,
            worker_state,
            fast_state: std::ptr::null_mut(),
            native_entries,
            native_call_encoded_scratch: Vec::with_capacity(native_call_argument_capacity),
            native_frame_arena: NativeFrameArena::default(),
            fiber_suspension_states: php_runtime::api::StableNativeArena::new(
                php_jit::JIT_NATIVE_FIBER_SUSPENSION_CAPACITY,
            ),
            fiber_suspension_next: Box::new(0),
            native_method_pics: std::collections::BTreeMap::new(),
            output,
            values: value_arena.values,
            value_slots: value_arena.value_slots,
            direct_value_slots: value_arena.direct_value_slots,
            direct_value_next: value_arena.direct_value_next,
            direct_object_owners: value_arena.direct_object_owners,
            direct_array_states: value_arena.direct_array_states,
            direct_array_entries: value_arena.direct_array_entries,
            direct_array_next: value_arena.direct_array_next,
            direct_value_free_head: value_arena.direct_value_free_head,
            direct_value_reused_bytes: value_arena.direct_value_reused_bytes,
            direct_array_free_heads: value_arena.direct_array_free_heads,
            direct_array_reused_bytes: value_arena.direct_array_reused_bytes,
            direct_string_bytes: value_arena.direct_string_bytes,
            direct_string_next: value_arena.direct_string_next,
            direct_string_free_heads: value_arena.direct_string_free_heads,
            direct_string_reused_bytes: value_arena.direct_string_reused_bytes,
            static_property_slots: php_runtime::api::StableNativeArena::new(
                php_jit::JIT_NATIVE_STATIC_PROPERTY_CAPACITY,
            ),
            static_property_next: Box::new(0),
            static_property_indices: std::collections::BTreeMap::new(),
            direct_reference_cells: std::collections::HashMap::new(),
            native_global_reference_handles: std::collections::BTreeMap::new(),
            direct_object_handles: std::collections::HashMap::new(),
            direct_closure_handles: std::collections::HashMap::new(),
            direct_fiber_handles: std::collections::HashMap::new(),
            direct_fiber_cells: std::collections::HashMap::new(),
            direct_string_handles: std::collections::HashMap::new(),
            direct_string_keys: std::collections::HashMap::new(),
            direct_array_handles: std::collections::HashMap::new(),
            direct_array_storage_ids: std::collections::HashMap::new(),
            direct_array_encode_depth: 0,
            interned_value_handles: value_arena.interned_value_handles,
            // Wrapping 4095 + 1 makes the first loop-header visit poll. Native
            // code then checks the deadline once per 4096 loop-header visits.
            native_poll_counter: Box::new(4095),
            native_root_mutation_pending: Box::new(0),
            free_value_slots: value_arena.free_value_slots,
            decoded_constant_cache: RefCell::new(std::collections::HashMap::new()),
            runtime_class_cache: RefCell::new(std::collections::HashMap::new()),
            trusted_class_plans: Vec::new(),
            root_index: RequestRootIndex::new_dirty(),
            resources,
            builtin_request_state: php_runtime::api::BuiltinRequestState::new(),
            registered_extensions: NativeRegisteredExtensionRequestState::default(),
            http_response: inherited_http_response.unwrap_or_default(),
            upload_registry: options.runtime_context.upload_registry(),
            session,
            session_global,
            filter_input_arrays,
            ini_registry: inherited_ini.unwrap_or_else(|| options.runtime_context.ini_registry()),
            default_timezone: inherited_default_timezone
                .unwrap_or_else(|| php_runtime::api::datetime::DEFAULT_TIMEZONE.to_owned()),
            mysql_state: inherited_mysql
                .unwrap_or_else(|| std::rc::Rc::new(RefCell::new(Default::default()))),
            dynamic_constants,
            visible_function_names,
            inherited_autoload_callback_count,
            inherited_shutdown_callback_count,
            dynamic_functions: std::collections::BTreeMap::new(),
            deployment_functions: inherited_symbols.deployment_functions,
            deployment_classes: inherited_symbols.deployment_classes,
            external_functions: inherited_symbols.external_functions,
            external_class_units: inherited_symbols.external_class_units,
            external_signature_epoch: inherited_symbols.external_signature_epoch,
            dynamic_units: inherited_symbols.dynamic_units,
            current_dynamic_unit: None,
            static_properties: inherited_symbols.static_properties,
            static_locals: inherited_symbols.static_locals,
            enum_cases: inherited_symbols.enum_cases,
            class_constant_cache: std::collections::HashMap::new(),
            generator_iterators: std::collections::BTreeMap::new(),
            fiber_executions: std::collections::BTreeMap::new(),
            active_fiber: None,
            pending_fiber_suspension_value: None,
            completed_nested_fiber_call: None,
            pending_throwable: None,
            called_classes: Vec::new(),
            lexical_scope_classes: Vec::new(),
            call_frames: Vec::new(),
            dynamic_classes: inherited_symbols.dynamic_classes,
            class_aliases: inherited_symbols.class_aliases,
            autoload_callbacks: inherited_symbols.autoload_callbacks,
            shutdown_callbacks: inherited_symbols.shutdown_callbacks,
            destroyed_objects: inherited_symbols.destroyed_objects,
            autoload_in_progress: std::collections::BTreeSet::new(),
            error_reporting: inherited_error_reporting
                .unwrap_or(options.runtime_context.ini.error_reporting.mask),
            display_errors: inherited_display_errors
                .unwrap_or(options.runtime_context.ini.display_errors),
            last_error: inherited_symbols.last_error,
            error_handlers: inherited_symbols.error_handlers,
            exception_handlers: inherited_symbols.exception_handlers,
            explicit_reference_ids: std::collections::BTreeSet::new(),
            environment,
            included_files: inherited_files.unwrap_or_default(),
            include_path: Arc::new(options.runtime_context.include_path.clone()),
            cwd: options.runtime_context.cwd.clone(),
            inherited_globals,
            trusted_globals_proxy: php_jit::jit_encode_constant(php_jit::JIT_VALUE_UNINITIALIZED),
            trusted_request_local_function_offsets,
            trusted_request_local_slots,
            continuation_instructions,
            trusted_property_function_offsets,
            trusted_property_slots,
            trusted_constant_slots,
            trusted_global_reference_slots,
            trusted_global_reference_names,
            trusted_static_local_slots,
            trusted_static_property_slots,
            trusted_instanceof_plans,
            trusted_instanceof_entries: Vec::new(),
            native_callsites,
            include_child,
            execution_deadline_at: options
                .runtime_context
                .execution_time_limit
                .and_then(|limit| std::time::Instant::now().checked_add(limit)),
            execution_deadline_mutable: options.runtime_context.execution_time_limit.is_some(),
            runtime_telemetry: Rc::new(RefCell::new(NativeRuntimeTelemetry::default())),
            diagnostic: None,
        }
    }

    /// Resolve immutable local class layouts once for the active source unit.
    /// Plans with request-dependent defaults or unresolved external parents
    /// remain empty and retain their single baseline continuation.
    fn prepare_trusted_class_plans(&mut self) {
        let owner = self.current_dynamic_unit;
        let classes = self.unit.classes.clone();
        self.trusted_class_plans =
            vec![php_jit::JitNativePreparedClassPlan::default(); classes.len()];
        for (index, class) in classes.iter().enumerate() {
            if !native_class_is_publication_allocatable(&classes, &self.unit.constants, class) {
                continue;
            }
            let key = (owner, class.name.clone());
            let cached = { self.runtime_class_cache.borrow().get(&key).cloned() };
            let prepared = if let Some(cached) = cached {
                Some(cached)
            } else {
                let Ok(entry) = native_runtime_class_with_owner(self, owner, class) else {
                    continue;
                };
                let default_declared_slots = php_runtime::api::ObjectRef::default_declared_slots(
                    &entry,
                    &class.display_name,
                );
                let mut owned_defaults = Vec::new();
                let mut default_native_slots = Vec::with_capacity(default_declared_slots.len());
                let mut failed = false;
                for default in default_declared_slots {
                    let encoded = match default {
                        None => {
                            default_native_slots
                                .push(php_runtime::api::NativeDeclaredPropertySlot::default());
                            continue;
                        }
                        Some(Value::Uninitialized) => {
                            php_jit::jit_encode_constant(php_jit::JIT_VALUE_UNINITIALIZED)
                        }
                        Some(value) => match self.encode(value) {
                            Ok(encoded) => encoded,
                            Err(_) => {
                                failed = true;
                                break;
                            }
                        },
                    };
                    if let Some(runtime_index) = php_jit::jit_decode_runtime_value(encoded) {
                        if runtime_index < php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE {
                            let _ = self.release(encoded);
                            failed = true;
                            break;
                        }
                        owned_defaults.push(encoded);
                    }
                    default_native_slots.push(php_runtime::api::NativeDeclaredPropertySlot {
                        initialized: 1,
                        reserved: 0,
                        value: encoded,
                    });
                }
                if failed {
                    for encoded in owned_defaults {
                        let _ = self.release(encoded);
                    }
                    continue;
                }
                let layout_id =
                    php_runtime::api::ObjectRef::prepared_layout_id(&entry, &class.display_name);
                let prepared = Rc::new(PreparedNativeRuntimeClass {
                    entry,
                    display_name: class.display_name.clone(),
                    layout_id,
                    default_native_slots: default_native_slots.into_boxed_slice(),
                });
                self.runtime_class_cache
                    .borrow_mut()
                    .insert(key.clone(), Rc::clone(&prepared));
                Some(prepared)
            };
            let Some(prepared) = prepared else {
                continue;
            };
            self.trusted_class_plans[index] = php_jit::JitNativePreparedClassPlan {
                prepared: Rc::as_ptr(&prepared) as usize as u64,
                display_name_bytes: prepared.display_name.as_ptr() as usize as u64,
                display_name_length: prepared.display_name.len() as u64,
                state: php_jit::JIT_NATIVE_PREPARED_CLASS_ALLOCATABLE,
                reserved: 0,
            };
        }
    }

    /// Resolve primary global-constant names at the request/publication
    /// boundary. A namespace fallback is deliberately not cached: defining
    /// the primary name later in the request must change subsequent lookup.
    fn prepare_trusted_constant_fetches(&mut self) {
        let names = self
            .continuation_instructions
            .iter()
            .flat_map(|function| function.iter())
            .filter_map(|instruction| {
                let instruction = instruction.as_ref()?;
                let php_ir::InstructionKind::FetchConst { name, .. } = &instruction.kind else {
                    return None;
                };
                Some(name.clone())
            })
            .collect::<std::collections::BTreeSet<_>>();
        for name in names {
            self.publish_trusted_constant_name(&name);
        }
    }

    /// Cold symbol-mutation hook for one newly visible constant. Resolution
    /// and encoding occur once here; every exact callsite receives an owned
    /// handle and generated code subsequently performs only a numeric load.
    fn publish_trusted_constant_name(&mut self, name: &str) {
        let Ok(value) = self.lookup_constant(name) else {
            return;
        };
        let Ok(encoded) = self.encode(value) else {
            return;
        };
        let continuations = std::sync::Arc::clone(&self.continuation_instructions);
        for (function, instructions) in continuations.iter().enumerate() {
            let Ok(function) = u32::try_from(function) else {
                continue;
            };
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
        let _ = self.release(encoded);
    }

    fn insert_dynamic_constant(&mut self, name: String, value: Value) {
        self.dynamic_constants.insert(name.clone(), value);
        self.publish_trusted_constant_name(&name);
    }

    /// Publish exact declared-property slots for statically proven object
    /// classes. Visibility, hooks, readonly/type constraints, layout identity,
    /// and numeric storage location are resolved once before native entry.
    fn prepare_trusted_declared_properties(&mut self) {
        let sites = self.compiled.prepared_native_property_sites();
        let owner = self.current_dynamic_unit;
        for (function, instructions) in sites.iter().enumerate() {
            let Some(base) = self
                .trusted_property_function_offsets
                .get(function)
                .copied()
                .and_then(|base| usize::try_from(base).ok())
            else {
                continue;
            };
            for (continuation, site) in instructions.iter().enumerate() {
                let Some(site) = site.as_ref() else {
                    continue;
                };
                let Some(class) = self.unit.classes.get(site.class_index as usize) else {
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
                let Some(property) = prepared.entry.properties.iter().find(|property| {
                    property.name == site.property.as_ref() && !property.flags.is_static
                }) else {
                    continue;
                };
                let readable = !property.flags.is_private
                    && !property.flags.is_protected
                    && property.hooks.get_function_id.is_none();
                let writable = readable
                    && !prepared.entry.flags.is_readonly
                    && !property.flags.is_readonly
                    && !property.flags.set_is_private
                    && !property.flags.set_is_protected
                    && !property.flags.is_typed
                    && property.type_.is_none()
                    && property.hooks.set_function_id.is_none();
                let referenceable = writable && property.hooks.get_function_id.is_none();
                let dimension_writable = readable
                    && !prepared.entry.flags.is_readonly
                    && !property.flags.is_readonly
                    && !property.flags.set_is_private
                    && !property.flags.set_is_protected
                    && property.hooks.set_function_id.is_none();
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
                let Some(slot_index) = php_runtime::api::ObjectRef::prepared_declared_slot_index(
                    &prepared.entry,
                    &prepared.display_name,
                    &site.property,
                ) else {
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
                    layout_id: prepared.layout_id,
                };
            }
        }
    }

    /// Resolve fixed `instanceof C` sites into immutable layout-id hash
    /// tables. Every class whose object layout is currently visible receives
    /// an exact boolean result. A class loaded later has a new unknown layout
    /// and therefore takes the site's single baseline continuation.
    fn prepare_trusted_instanceof_plans(&mut self) {
        self.trusted_instanceof_plans.fill(Default::default());
        self.trusted_instanceof_entries.clear();

        let mut seen = std::collections::BTreeSet::new();
        let mut declarations = Vec::new();
        for class in &self.unit.classes {
            if class.flags.is_conditional && !self.class_is_visible(&class.name) {
                continue;
            }
            if seen.insert(class.name.clone()) {
                declarations.push((self.current_dynamic_unit, class.clone()));
            }
        }
        let external = self
            .external_class_units
            .iter()
            .map(|(name, unit)| (name.clone(), *unit))
            .collect::<Vec<_>>();
        for (name, unit) in external {
            if self.current_dynamic_unit == Some(unit) || !seen.insert(name.clone()) {
                continue;
            }
            let Some(class) = self
                .dynamic_units
                .get(unit)
                .and_then(|package| package.compiled.lookup_unit_class(&name))
                .cloned()
            else {
                continue;
            };
            declarations.push((Some(unit), class));
        }

        let known_names = declarations
            .iter()
            .map(|(_, class)| class.name.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let layouts = declarations
            .iter()
            .filter(|(_, class)| {
                !class.flags.is_abstract && !class.flags.is_interface && !class.flags.is_trait
            })
            .filter_map(|(owner, class)| {
                let runtime = native_runtime_class_with_owner(self, *owner, class).ok()?;
                let layout_id =
                    php_runtime::api::ObjectRef::prepared_layout_id(&runtime, &class.display_name);
                Some((class.name.clone(), layout_id))
            })
            .collect::<Vec<_>>();

        let continuations = std::sync::Arc::clone(&self.continuation_instructions);
        for (function, instructions) in continuations.iter().enumerate() {
            let Some(base) = self
                .trusted_property_function_offsets
                .get(function)
                .copied()
                .and_then(|base| usize::try_from(base).ok())
            else {
                continue;
            };
            let Ok(caller_function) = u32::try_from(function) else {
                continue;
            };
            for (continuation, instruction) in instructions.iter().enumerate() {
                let Some(instruction) = instruction.as_ref() else {
                    continue;
                };
                let php_ir::InstructionKind::InstanceOf { class_name, .. } = &instruction.kind
                else {
                    continue;
                };
                if class_name.eq_ignore_ascii_case("static") {
                    continue;
                }
                let Ok(target) =
                    native_resolve_scoped_class_name(self, class_name, caller_function)
                else {
                    continue;
                };
                let target = normalize_class_name(&target);
                if self.class_aliases.contains_key(&target) || !known_names.contains(&target) {
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
            }
        }
    }

    /// Publish effect-free constant function-static defaults before native
    /// entry. Dynamic expressions and unresolved constants remain cold and
    /// publish the same dense slot after their first baseline initialization.
    fn prepare_trusted_static_locals(&mut self) {
        let continuations = std::sync::Arc::clone(&self.continuation_instructions);
        for (function, instructions) in continuations.iter().enumerate() {
            let Ok(function) = u32::try_from(function) else {
                continue;
            };
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
                let reference = if let Some(reference) = self.static_locals.get(&key).cloned() {
                    reference
                } else {
                    let Ok(default) = native_runtime_constant_value(self, &constant) else {
                        continue;
                    };
                    let reference = php_runtime::api::ReferenceCell::new(default);
                    self.static_locals.insert(key, reference.clone());
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

    /// Resolve exact local static-property sites once per request. Generated
    /// code subsequently indexes the authoritative slot directly; dynamic
    /// class names, late-static binding, typed/deferred declarations, and
    /// autoloaded classes retain their single baseline continuation.
    fn prepare_trusted_static_properties(&mut self) {
        let continuations = std::sync::Arc::clone(&self.continuation_instructions);
        for (function_index, function) in continuations.iter().enumerate() {
            let Ok(caller_function) = u32::try_from(function_index) else {
                continue;
            };
            for (continuation, instruction) in function.iter().enumerate() {
                let Some(instruction) = instruction.as_ref() else {
                    continue;
                };
                let (class_name, property, writable) = match &instruction.kind {
                    php_ir::InstructionKind::FetchStaticProperty {
                        class_name,
                        property,
                        ..
                    }
                    | php_ir::InstructionKind::IssetStaticProperty {
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
                    } => (class_name.as_str(), property.as_str(), false),
                    php_ir::InstructionKind::AssignStaticProperty {
                        class_name,
                        property,
                        ..
                    }
                    | php_ir::InstructionKind::BindReferenceStaticProperty {
                        class_name,
                        property,
                        ..
                    }
                    | php_ir::InstructionKind::BindReferenceFromStaticPropertyDim {
                        class_name,
                        property,
                        ..
                    }
                    | php_ir::InstructionKind::UnsetStaticPropertyDim {
                        class_name,
                        property,
                        ..
                    } => (class_name.as_str(), property.as_str(), true),
                    _ => continue,
                };

                let calling_class = native_calling_class(self, caller_function);
                let resolved_class = match class_name.to_ascii_lowercase().as_str() {
                    "self" => calling_class.map(|class| class.name.clone()),
                    "parent" => calling_class.and_then(|class| class.parent.clone()),
                    // Late-static binding depends on the active call chain and
                    // therefore cannot be published as a request-stable slot.
                    "static" => None,
                    _ => Some(class_name.to_owned()),
                };
                let Some(resolved_class) = resolved_class else {
                    continue;
                };
                let normalized = normalize_class_name(&resolved_class);
                if !self
                    .unit
                    .classes
                    .iter()
                    .any(|class| class.name == normalized)
                {
                    continue;
                }
                let Some(declaration) = native_static_property_declaration(
                    self,
                    &resolved_class,
                    property,
                    caller_function,
                ) else {
                    continue;
                };
                if declaration.owner_unit.is_some()
                    || declaration.type_.is_some()
                    || declaration.flags.is_typed
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
                    let inherited = self.static_properties.remove(&key);
                    let default = declaration
                        .default
                        .and_then(|constant| self.unit.constants.get(constant.index()))
                        .cloned();
                    let value = match inherited {
                        Some(value) => value,
                        None => match default.as_ref() {
                            Some(value) => match native_runtime_constant_value(self, value) {
                                Ok(value) => value,
                                Err(_) => continue,
                            },
                            None => Value::Null,
                        },
                    };
                    let encoded = match self.encode(value.clone()) {
                        Ok(encoded) => encoded,
                        Err(_) => {
                            self.static_properties.insert(key.clone(), value);
                            continue;
                        }
                    };
                    self.static_property_slots[index] = php_jit::JitNativeStaticPropertySlot {
                        value: encoded,
                        initialized: 1,
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
                let Some(plan) = self
                    .trusted_static_property_slots
                    .get_mut(base.saturating_add(continuation))
                else {
                    continue;
                };
                *plan = php_jit::JitNativeTrustedStaticPropertySlot {
                    state: if writable {
                        php_jit::JIT_NATIVE_TRUSTED_STATIC_PROPERTY_WRITABLE
                    } else {
                        php_jit::JIT_NATIVE_TRUSTED_STATIC_PROPERTY_READABLE
                    },
                    slot_index,
                };
            }
        }
    }

    fn direct_static_property_value(
        &mut self,
        key: &(String, String),
    ) -> Option<Result<Value, String>> {
        let encoded = self.direct_static_property_encoded(key)?;
        Some(self.decode(encoded))
    }

    fn direct_static_property_encoded(&self, key: &(String, String)) -> Option<i64> {
        let index = usize::try_from(*self.static_property_indices.get(key)?).ok()?;
        let slot = self.static_property_slots.get(index)?;
        (slot.initialized != 0).then_some(slot.value)
    }

    /// Publishes a lazily resolved static property into the authoritative
    /// native slot plane. Dynamic/include-owned classes are not necessarily
    /// known during root-unit preparation, so their first cold lookup must
    /// allocate the same stable storage used by prepared local classes.
    fn ensure_direct_static_property_encoded(
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
        let encoded = self.encode(value)?;
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

    /// Publish the immutable result of one exact `FetchConst` continuation.
    /// The plan retains its own owner; the caller keeps the owner returned by
    /// the baseline operation for the current SSA result.
    fn publish_trusted_constant_fetch(
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

    fn clear_trusted_constant_fetches(&mut self) {
        let values = self
            .trusted_constant_slots
            .iter_mut()
            .filter_map(|slot| {
                (slot.state == php_jit::JIT_NATIVE_TRUSTED_CONSTANT_PUBLISHED).then(|| {
                    let value = slot.value;
                    *slot = php_jit::JitNativeTrustedConstantSlot::default();
                    value
                })
            })
            .collect::<Vec<_>>();
        for value in values {
            let _ = self.release_if_live(value);
        }
    }

    /// Replace the owner held by an authoritative native static slot. The
    /// caller supplies a PHP value only on the cold path; the stored result is
    /// immediately native again and the superseded owner is released once.
    fn store_direct_static_property_value(
        &mut self,
        key: &(String, String),
        value: Value,
    ) -> Option<Result<(), String>> {
        let index = usize::try_from(*self.static_property_indices.get(key)?).ok()?;
        let encoded = match self.encode(value) {
            Ok(encoded) => encoded,
            Err(error) => return Some(Err(error)),
        };
        let previous = self.static_property_slots[index].value;
        self.static_property_slots[index].value = encoded;
        self.static_property_slots[index].initialized = 1;
        self.mark_roots_dirty(RootMutationReason::EnumOrStaticObject);
        Some(self.release(previous))
    }

    /// Include execution moves request symbols between independently owned
    /// native contexts. Materialize only at that cold ownership boundary and
    /// relinquish every slot owner before the child context is constructed.
    fn demote_trusted_static_properties(&mut self) {
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
            let Ok(value) = self.decode(slot.value) else {
                continue;
            };
            self.static_properties.insert(key, value);
            self.static_property_slots[index] = php_jit::JitNativeStaticPropertySlot::default();
            let _ = self.release(slot.value);
        }
        let used = usize::try_from(*self.static_property_next).unwrap_or(0);
        self.static_property_slots.discard_prefix(used);
        *self.static_property_next = 0;
        self.static_property_indices.clear();
        self.trusted_static_property_slots
            .fill(php_jit::JitNativeTrustedStaticPropertySlot::default());
        self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
    }

    pub(super) fn recycle_native_value_arena(&mut self) {
        self.clear_trusted_constant_fetches();
        self.clear_trusted_request_locals();
        self.clear_trusted_global_references();
        self.clear_trusted_static_locals();
        let suspended_fibers = std::mem::take(&mut self.fiber_executions);
        for (_, execution) in suspended_fibers {
            let _ = self.abandon_native_fiber_execution(execution);
        }
        if let Some(value) = self.pending_fiber_suspension_value.take() {
            let _ = self.release_if_live(value);
        }
        if let Some((_, _, _, value)) = self.completed_nested_fiber_call.take() {
            let _ = self.release_if_live(value);
        }
        self.discard_native_fiber_suspension_states();
        self.active_fiber = None;
        // ObjectRef identities may escape an include/nested VM through
        // globals or returned symbols. Their native property cells point into
        // this request arena, so restore every such object before the arena is
        // force-recycled. Doing this after individual slots were reclaimed
        // made graph order observable and could leave an escaped empty shell.
        let _ = self.demote_all_direct_objects();
        let cold_value_used = self.values.len();
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
        self.values.clear();
        self.value_slots.discard_prefix(cold_value_used);
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
        self.direct_reference_cells.clear();
        self.native_global_reference_handles.clear();
        self.direct_object_handles.clear();
        debug_assert!(self.direct_closure_handles.is_empty());
        self.direct_closure_handles.clear();
        self.direct_fiber_handles.clear();
        self.direct_fiber_cells.clear();
        self.direct_string_handles.clear();
        self.direct_string_keys.clear();
        self.direct_array_handles.clear();
        self.direct_array_storage_ids.clear();
        self.direct_array_encode_depth = 0;
        self.class_constant_cache.clear();
        self.interned_value_handles.clear();
        self.free_value_slots.clear();
        recycle_native_value_arena(NativeValueArenaBuffers {
            values: std::mem::take(&mut self.values),
            value_slots: std::mem::take(&mut self.value_slots),
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
            interned_value_handles: std::mem::take(&mut self.interned_value_handles),
            free_value_slots: std::mem::take(&mut self.free_value_slots),
        });
    }

    fn reset_execution_deadline_seconds(&mut self, seconds: u64) {
        if !self.execution_deadline_mutable {
            return;
        }
        self.execution_deadline_at = if seconds == 0 {
            None
        } else {
            std::time::Instant::now().checked_add(std::time::Duration::from_secs(seconds))
        };
    }

    fn publish_native_entry_address(&self, function: php_ir::FunctionId, address: usize) {
        if let Some(cell) = self
            .compiled
            .prepared_deployment_image()
            .native_function_entries
            .get(function.index())
        {
            cell.store(address, std::sync::atomic::Ordering::Release);
        }
    }

    pub(super) fn attach_root_deployment_image(
        &mut self,
        compiled: crate::compiled_unit::CompiledUnit,
    ) {
        if self.include_child || self.current_dynamic_unit.is_some() {
            return;
        }
        let unit = self.dynamic_units.len();
        let deployment = compiled.prepared_deployment_image();
        for (function, handle) in self.native_entries.iter() {
            if !handle.region_state_metadata().is_some_and(|metadata| {
                metadata.compiler_tier == php_jit::region_ir::NativeCompilerTier::Baseline
            }) {
                continue;
            }
            if let (Some(cell), Some(address)) = (
                deployment.native_function_entries.get(function.index()),
                handle.native_entry_address(),
            ) {
                cell.store(address, std::sync::atomic::Ordering::Release);
            }
        }
        // Before the root image is attached there are no runtime declaration
        // overlays. Its compiled entries therefore all share the empty
        // external-signature set; do not rediscover call targets per request.
        let empty_signature_hash = super::external_function_signatures_hash(&[]);
        let native_entry_signature_hashes = self
            .native_entries
            .keys()
            .copied()
            .map(|function| (function, empty_signature_hash))
            .collect();
        if !deployment.function_exports.is_empty() {
            self.external_signature_epoch = self.external_signature_epoch.saturating_add(1);
        }
        let native_entry_signature_epochs = self
            .native_entries
            .keys()
            .copied()
            .map(|function| (function, self.external_signature_epoch))
            .collect();
        self.dynamic_units.push(NativeDynamicUnit {
            compiled: compiled.clone(),
            native_entries: self.native_entries.clone(),
            native_entry_signature_hashes,
            native_entry_signature_epochs,
        });
        debug_assert_eq!(unit, 0, "immutable deployment must be the root native unit");
        self.deployment_functions = std::sync::Arc::clone(&deployment.function_exports);
        self.deployment_classes = std::sync::Arc::clone(&deployment.exported_classes);
        self.current_dynamic_unit = Some(unit);
    }

    fn class_is_visible(&self, normalized: &str) -> bool {
        self.deployment_classes.contains(normalized) || self.dynamic_classes.contains(normalized)
    }

    fn ensure_native_global_references(&mut self) {
        const RUNTIME_GLOBALS: &[&str] = &[
            "argc", "argv", "_SERVER", "_ENV", "_GET", "_POST", "_COOKIE", "_REQUEST", "_FILES",
            "_SESSION",
        ];
        for name in RUNTIME_GLOBALS {
            if self.inherited_globals.contains_key(*name) {
                continue;
            }
            let Some(value) = self.options.runtime_context.global_value(name) else {
                continue;
            };
            let reference = match value {
                Value::Reference(reference) => reference,
                value => php_runtime::api::ReferenceCell::new(value),
            };
            self.inherited_globals
                .insert((*name).to_owned(), Value::Reference(reference));
        }
        for value in self.inherited_globals.values_mut() {
            if matches!(value, Value::Reference(_) | Value::Uninitialized) {
                continue;
            }
            let reference = php_runtime::api::ReferenceCell::new(value.clone());
            *value = Value::Reference(reference);
        }
    }

    /// Returns the request-owned direct reference for one global. Dynamic-unit
    /// publication borrows this canonical handle instead of rebuilding the
    /// referenced value tree for every include activation.
    fn native_global_reference_handle(&mut self, name: &str) -> Result<Option<i64>, String> {
        self.ensure_native_global_references();
        let Some(global) = self.inherited_globals.get(name).cloned() else {
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

    fn duplicate_native_global_value(&mut self, name: &str) -> Result<Option<i64>, String> {
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
        if matches!(self.inherited_globals.get(name), Some(Value::Uninitialized)) {
            return Ok(Some(php_jit::jit_encode_constant(u32::MAX)));
        }
        let Some(encoded) = self.native_global_reference_handle(name)? else {
            return Ok(None);
        };
        self.duplicate_dereferenced_native_value(encoded).map(Some)
    }

    fn native_request_local_handle(&mut self, name: &str) -> Result<i64, String> {
        self.ensure_native_global_references();
        if let Some(encoded) = self
            .native_global_reference_handles
            .get(name)
            .copied()
            .filter(|encoded| self.native_reference_identity(*encoded).is_some())
        {
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

    fn rebind_native_request_local_reference(
        &mut self,
        name: &str,
        encoded: i64,
    ) -> Result<(), String> {
        if self.native_reference_identity(encoded).is_none() {
            return Err(format!(
                "native request local ${name} was rebound to a non-reference value"
            ));
        }
        let slot_indices = self
            .unit
            .functions
            .iter()
            .enumerate()
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

    fn prepare_trusted_request_locals(&mut self) {
        self.ensure_native_global_references();
        let sites = self
            .unit
            .functions
            .iter()
            .enumerate()
            .flat_map(|(function, definition)| {
                definition
                    .locals
                    .iter()
                    .enumerate()
                    .filter_map(move |(local, _)| {
                        native_request_local_name(definition, local)
                            .map(|name| (function, local, name.to_owned()))
                    })
            })
            .collect::<Vec<_>>();
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

    fn materialize_native_request_global(&mut self, name: &str) -> Result<(), String> {
        let Some(encoded) = self.native_global_reference_handles.get(name).copied() else {
            return Ok(());
        };
        let Value::Reference(reference) = self.decode(encoded)? else {
            return Err(format!(
                "native request global ${name} lost its reference identity"
            ));
        };
        self.inherited_globals
            .insert(name.to_owned(), Value::Reference(reference));
        Ok(())
    }

    fn materialize_native_request_globals(&mut self) -> Result<(), String> {
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

    fn clear_trusted_request_locals(&mut self) {
        let values = self
            .trusted_request_local_slots
            .iter_mut()
            .filter_map(|slot| {
                (slot.state == php_jit::JIT_NATIVE_REQUEST_LOCAL_PUBLISHED).then(|| {
                    let encoded = slot.encoded;
                    *slot = php_jit::JitNativeRequestLocalSlot::default();
                    encoded
                })
            })
            .collect::<Vec<_>>();
        for encoded in values {
            let _ = self.release_if_live(encoded);
        }
    }

    /// Publish references for globals that already exist at request entry.
    /// Missing globals are deliberately left unpublished so preparing native
    /// code cannot make them visible before PHP executes the `global` binding.
    fn prepare_trusted_global_references(&mut self) {
        self.ensure_native_global_references();
        let continuations = std::sync::Arc::clone(&self.continuation_instructions);
        let sites = continuations
            .iter()
            .enumerate()
            .flat_map(|(function, instructions)| {
                instructions
                    .iter()
                    .enumerate()
                    .filter_map(move |(continuation, instruction)| {
                        let instruction = instruction.as_ref()?;
                        let php_ir::InstructionKind::BindGlobal { name, .. } = &instruction.kind
                        else {
                            return None;
                        };
                        Some((function, continuation, name.clone()))
                    })
            })
            .collect::<Vec<_>>();
        for (function, continuation, name) in sites {
            let Ok(Some(encoded)) = self.native_global_reference_handle(&name) else {
                continue;
            };
            let (Ok(function), Ok(continuation)) =
                (u32::try_from(function), u32::try_from(continuation))
            else {
                continue;
            };
            let published =
                self.publish_native_global_reference(function, continuation, &name, encoded);
            if published.is_err() {
                continue;
            }
        }
    }

    fn native_reference_identity(&self, encoded: i64) -> Option<u64> {
        if encoded as u64 & php_jit::JIT_VALUE_RUNTIME_KIND_MASK
            != php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG
        {
            return None;
        }
        if let Some(index) = Self::direct_value_index(encoded) {
            return self
                .direct_reference_cells
                .get(&index)
                .map(php_runtime::api::ReferenceCell::gc_debug_id);
        }
        let value_index = php_jit::jit_decode_runtime_value(encoded)? as usize;
        match self.values.get(value_index).and_then(Option::as_ref) {
            Some(NativeStoredValue::Php(Value::Reference(reference))) => {
                Some(reference.gc_debug_id())
            }
            _ => None,
        }
    }

    fn invalidate_native_global_reference(
        &mut self,
        reference_identity: u64,
    ) -> Result<(), String> {
        let retained = self
            .trusted_global_reference_slots
            .iter_mut()
            .zip(&mut self.trusted_global_reference_names)
            .filter_map(|(slot, name)| {
                if slot.state != php_jit::JIT_NATIVE_TRUSTED_GLOBAL_REFERENCE_PUBLISHED
                    || slot.reference_identity != reference_identity
                {
                    return None;
                }
                let encoded = slot.encoded;
                *slot = php_jit::JitNativeTrustedGlobalReferenceSlot::default();
                *name = None;
                Some(encoded)
            })
            .collect::<Vec<_>>();
        for encoded in retained {
            self.release(encoded)?;
        }
        let global_handles = self
            .native_global_reference_handles
            .iter()
            .filter_map(|(name, encoded)| {
                (self.native_reference_identity(*encoded) == Some(reference_identity))
                    .then_some(name.clone())
            })
            .collect::<Vec<_>>();
        for name in global_handles {
            if let Some(encoded) = self.native_global_reference_handles.remove(&name) {
                self.release(encoded)?;
            }
        }
        Ok(())
    }

    fn reconcile_trusted_global_references(&mut self) -> Result<(), String> {
        let stale = self
            .trusted_global_reference_slots
            .iter()
            .zip(&self.trusted_global_reference_names)
            .enumerate()
            .filter_map(|(index, (slot, name))| {
                if slot.state != php_jit::JIT_NATIVE_TRUSTED_GLOBAL_REFERENCE_PUBLISHED {
                    return None;
                }
                let still_bound = name.as_deref().is_some_and(|name| {
                    matches!(
                        self.inherited_globals.get(name),
                        Some(Value::Reference(reference))
                            if reference.gc_debug_id() == slot.reference_identity
                    )
                });
                (!still_bound).then_some(index)
            })
            .collect::<Vec<_>>();
        let retained = stale
            .into_iter()
            .map(|index| {
                let encoded = self.trusted_global_reference_slots[index].encoded;
                self.trusted_global_reference_slots[index] =
                    php_jit::JitNativeTrustedGlobalReferenceSlot::default();
                self.trusted_global_reference_names[index] = None;
                encoded
            })
            .collect::<Vec<_>>();
        for encoded in retained {
            self.release(encoded)?;
        }
        Ok(())
    }

    fn publish_native_global_reference(
        &mut self,
        function: u32,
        continuation: u32,
        name: &str,
        encoded: i64,
    ) -> Result<(), String> {
        let Some(reference_identity) = self.native_reference_identity(encoded) else {
            return Err("native global binding reference handle has no reference cell".to_owned());
        };
        let Some(base) = self
            .trusted_property_function_offsets
            .get(function as usize)
            .copied()
            .and_then(|base| usize::try_from(base).ok())
        else {
            return Err("native global-binding function index is missing".to_owned());
        };
        let index = base
            .checked_add(continuation as usize)
            .ok_or_else(|| "native global-binding continuation index overflow".to_owned())?;
        let previous = self
            .trusted_global_reference_slots
            .get(index)
            .copied()
            .ok_or_else(|| "native global-binding continuation is missing".to_owned())?;
        if previous.state == php_jit::JIT_NATIVE_TRUSTED_GLOBAL_REFERENCE_PUBLISHED
            && previous.encoded == encoded
            && previous.reference_identity == reference_identity
            && self.trusted_global_reference_names[index].as_deref() == Some(name)
        {
            return Ok(());
        }

        // The call result already owns one handle for the destination local.
        // The trusted slot owns another until replacement or request reset.
        self.retain(encoded)?;
        self.trusted_global_reference_slots[index] = php_jit::JitNativeTrustedGlobalReferenceSlot {
            encoded,
            reference_identity,
            state: php_jit::JIT_NATIVE_TRUSTED_GLOBAL_REFERENCE_PUBLISHED,
            reserved: 0,
            reserved_wide: 0,
        };
        self.trusted_global_reference_names[index] = Some(name.into());
        if previous.state == php_jit::JIT_NATIVE_TRUSTED_GLOBAL_REFERENCE_PUBLISHED {
            self.release(previous.encoded)?;
        }
        Ok(())
    }

    fn clear_trusted_global_references(&mut self) {
        let values = self
            .trusted_global_reference_slots
            .iter_mut()
            .zip(&mut self.trusted_global_reference_names)
            .filter_map(|(slot, name)| {
                let published =
                    slot.state == php_jit::JIT_NATIVE_TRUSTED_GLOBAL_REFERENCE_PUBLISHED;
                let encoded = slot.encoded;
                *slot = php_jit::JitNativeTrustedGlobalReferenceSlot::default();
                *name = None;
                published.then_some(encoded)
            })
            .collect::<Vec<_>>();
        for encoded in values {
            let _ = self.release_if_live(encoded);
        }
    }

    fn publish_trusted_static_local_reference(
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
        let sites = self
            .continuation_instructions
            .get(function as usize)
            .into_iter()
            .flatten()
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

    fn clear_trusted_static_locals(&mut self) {
        let values = self
            .trusted_static_local_slots
            .iter_mut()
            .filter_map(|slot| {
                (slot.state == php_jit::JIT_NATIVE_TRUSTED_STATIC_LOCAL_PUBLISHED).then(|| {
                    let encoded = slot.encoded;
                    *slot = php_jit::JitNativeTrustedStaticLocalSlot::default();
                    encoded
                })
            })
            .collect::<Vec<_>>();
        for encoded in values {
            let _ = self.release_if_live(encoded);
        }
    }

    fn materialize_native_globals_array(&mut self) -> Result<Value, String> {
        self.materialize_native_request_globals()?;
        let mut globals = php_runtime::api::PhpArray::with_capacity(self.inherited_globals.len());
        for (name, value) in &self.inherited_globals {
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

    fn encode_globals_proxy(&mut self) -> Result<i64, String> {
        self.ensure_native_global_references();
        self.encode_stored_value(NativeStoredValue::GlobalsProxy)
    }

    fn is_globals_proxy(&self, encoded: i64) -> bool {
        php_jit::jit_decode_runtime_value(encoded).is_some_and(|index| {
            matches!(
                self.values.get(index as usize).and_then(Option::as_ref),
                Some(NativeStoredValue::GlobalsProxy)
            )
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

    fn fetch_native_global_dimension(
        &mut self,
        key: &php_runtime::api::ArrayKey,
    ) -> Result<Option<Value>, String> {
        self.ensure_native_global_references();
        let Some(name) = Self::native_global_name(key) else {
            return Ok(None);
        };
        self.materialize_native_request_global(name.as_ref())?;
        Ok(self
            .inherited_globals
            .get(name.as_ref())
            .filter(|value| {
                !matches!(value, Value::Uninitialized)
                    && !matches!(value, Value::Reference(reference) if matches!(reference.get(), Value::Uninitialized))
            })
            .cloned())
    }

    fn replace_direct_reference_cell_value(
        &mut self,
        reference: &php_runtime::api::ReferenceCell,
        replacement: Value,
    ) -> Result<Option<Value>, String> {
        let Some(index) = self
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
                && slot.reserved != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
        }) else {
            return Ok(None);
        };
        let encoded = self.encode(replacement.clone())?;
        self.direct_value_slots[index].payload = encoded as u64;
        self.direct_value_slots[index].reserved =
            php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED;
        reference.set(replacement);
        let previous = self.decode(slot.payload as i64)?;
        self.release(slot.payload as i64)?;
        Ok(Some(previous))
    }

    fn store_native_global_dimension(
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
        if let Some(Value::Reference(reference)) =
            self.inherited_globals.get(name.as_ref()).cloned()
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
            self.inherited_globals.insert(
                name.into_owned(),
                Value::Reference(php_runtime::api::ReferenceCell::new(replacement)),
            );
            self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
        }
        Ok(true)
    }

    fn unset_native_global_dimension(
        &mut self,
        key: &php_runtime::api::ArrayKey,
    ) -> Result<bool, String> {
        self.ensure_native_global_references();
        let Some(name) = Self::native_global_name(key) else {
            return Ok(false);
        };
        self.materialize_native_request_global(name.as_ref())?;
        if let Some(Value::Reference(reference)) = self.inherited_globals.get(name.as_ref()) {
            self.invalidate_native_global_reference(reference.gc_debug_id())?;
        }
        let previous = self
            .inherited_globals
            .insert(name.into_owned(), Value::Uninitialized);
        if let Some(Value::Reference(reference)) = previous {
            self.finalize_replaced_value(reference.get())?;
        }
        self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
        Ok(true)
    }

    fn reference_native_global_dimension(
        &mut self,
        key: &php_runtime::api::ArrayKey,
    ) -> Result<Option<php_runtime::api::ReferenceCell>, String> {
        self.ensure_native_global_references();
        let Some(name) = Self::native_global_name(key) else {
            return Ok(None);
        };
        self.materialize_native_request_global(name.as_ref())?;
        if let Some(Value::Reference(reference)) = self.inherited_globals.get(name.as_ref()) {
            return Ok(Some(reference.clone()));
        }
        let reference = php_runtime::api::ReferenceCell::new(Value::Null);
        self.inherited_globals
            .insert(name.into_owned(), Value::Reference(reference.clone()));
        self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
        Ok(Some(reference))
    }

    fn direct_value_index(encoded: i64) -> Option<usize> {
        let index = php_jit::jit_decode_runtime_value(encoded)?;
        let index = index.checked_sub(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)? as usize;
        (index < php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY).then_some(index)
    }

    /// Clones the stable backing owner of one live direct object. The
    /// slot-parallel pointer arena is authoritative; no object-value HashMap
    /// participates in ordinary lookup.
    #[allow(unsafe_code)]
    fn direct_object(&self, index: usize) -> Option<php_runtime::api::ObjectRef> {
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
    fn direct_object_owner(&self, index: usize) -> Option<php_runtime::api::ObjectRef> {
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

    /// Borrows the authoritative closure record published directly by this
    /// value slot. The pointer is stable until the final encoded owner is
    /// released; no `NativeStoredValue` mirror participates in lookup.
    #[allow(unsafe_code)]
    fn direct_prepared_callable(&self, index: usize) -> Option<&NativePreparedCallable> {
        let slot = *self.direct_value_slots.get(index)?;
        if slot.refcount == 0
            || slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE
            || slot.flags != php_jit::JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION
        {
            return None;
        }
        let owner = slot.aux as usize as *const NativePreparedCallable;
        // SAFETY: publication installs exactly one boxed record before the
        // descriptor becomes visible, and final release reclaims both.
        unsafe { owner.as_ref() }
    }

    #[allow(unsafe_code)]
    fn direct_prepared_callable_mut(
        &mut self,
        index: usize,
    ) -> Option<&mut NativePreparedCallable> {
        let slot = *self.direct_value_slots.get(index)?;
        if slot.refcount == 0
            || slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE
            || slot.flags != php_jit::JIT_NATIVE_PREPARED_CALLABLE_ABI_VERSION
        {
            return None;
        }
        let owner = slot.aux as usize as *mut NativePreparedCallable;
        // SAFETY: mutation requires `&mut self`, so no competing record
        // borrow can exist on this request thread.
        unsafe { owner.as_mut() }
    }

    #[allow(unsafe_code)]
    fn fiber_record(&self, index: usize) -> Option<&NativeDirectFiber> {
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

    fn direct_fiber(&self, index: usize) -> Option<&NativeDirectFiber> {
        (self.direct_value_slots.get(index)?.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER)
            .then(|| self.fiber_record(index))
            .flatten()
    }

    #[allow(unsafe_code)]
    fn direct_fiber_mut(&mut self, index: usize) -> Option<&mut NativeDirectFiber> {
        let slot = self.direct_value_slots.get(index)?;
        if slot.refcount == 0
            || slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER
            || slot.flags != php_jit::JIT_NATIVE_DIRECT_FIBER_ABI_VERSION
        {
            return None;
        }
        let owner = slot.aux as usize as *mut NativeDirectFiber;
        // SAFETY: `&mut self` excludes a competing record borrow on the
        // request thread.
        unsafe { owner.as_mut() }
    }

    fn direct_fiber_index(&self, encoded: i64) -> Option<usize> {
        let index = Self::direct_value_index(encoded)?;
        self.direct_fiber(index).map(|_| index)
    }

    fn native_fiber_state(&self, encoded: i64) -> Option<php_runtime::api::FiberState> {
        let index = self.direct_fiber_index(encoded)?;
        self.direct_fiber(index).map(|fiber| fiber.state)
    }

    fn native_fiber_callable(&self, encoded: i64) -> Option<i64> {
        let index = self.direct_fiber_index(encoded)?;
        self.direct_fiber(index).map(|fiber| fiber.callable)
    }

    fn native_fiber_return_value(&self, encoded: i64) -> Option<Option<i64>> {
        let index = self.direct_fiber_index(encoded)?;
        self.direct_fiber(index).map(|fiber| fiber.return_value)
    }

    fn set_native_fiber_state(
        &mut self,
        encoded: i64,
        state: php_runtime::api::FiberState,
    ) -> Result<(), String> {
        let index = self
            .direct_fiber_index(encoded)
            .ok_or_else(|| "native Fiber has no direct record".to_owned())?;
        self.direct_fiber_mut(index)
            .ok_or_else(|| "native Fiber record disappeared".to_owned())?
            .state = state;
        Ok(())
    }

    fn terminate_native_fiber(
        &mut self,
        encoded: i64,
        return_value: Option<i64>,
    ) -> Result<(), String> {
        let index = self
            .direct_fiber_index(encoded)
            .ok_or_else(|| "native Fiber has no direct record".to_owned())?;
        let previous = {
            let fiber = self
                .direct_fiber_mut(index)
                .ok_or_else(|| "native Fiber record disappeared".to_owned())?;
            fiber.state = php_runtime::api::FiberState::Terminated;
            std::mem::replace(&mut fiber.return_value, return_value)
        };
        if let Some(previous) = previous {
            self.release(previous)?;
        }
        Ok(())
    }

    fn native_fiber_receiver(
        &mut self,
        encoded: i64,
    ) -> Result<Option<NativeFiberReceiver>, String> {
        let encoded = self.dereference_direct_encoding(encoded);
        if self.direct_fiber_index(encoded).is_some() {
            return Ok(Some(NativeFiberReceiver::Direct(encoded)));
        }
        if self.native_encoded_value_kind(encoded) != Some(NativeEncodedValueKind::Fiber) {
            return Ok(None);
        }
        match self.decode(encoded)? {
            Value::Fiber(fiber) => Ok(Some(NativeFiberReceiver::Materialized(fiber))),
            _ => Ok(None),
        }
    }

    fn fiber_receiver_id(&self, fiber: &NativeFiberReceiver) -> Result<u64, String> {
        match fiber {
            NativeFiberReceiver::Direct(encoded) => Self::direct_value_index(*encoded)
                .map(|index| index as u64)
                .ok_or_else(|| "native Fiber identity is missing".to_owned()),
            NativeFiberReceiver::Materialized(fiber) => Ok(self
                .direct_fiber_handles
                .get(&fiber.id())
                .map_or_else(|| fiber.id(), |index| u64::from(*index))),
        }
    }

    fn fiber_receiver_state(
        &self,
        fiber: &NativeFiberReceiver,
    ) -> Result<php_runtime::api::FiberState, String> {
        match fiber {
            NativeFiberReceiver::Direct(encoded) => self
                .native_fiber_state(*encoded)
                .ok_or_else(|| "native Fiber state is missing".to_owned()),
            NativeFiberReceiver::Materialized(fiber) => Ok(fiber.state()),
        }
    }

    fn set_fiber_receiver_state(
        &mut self,
        fiber: &NativeFiberReceiver,
        state: php_runtime::api::FiberState,
    ) -> Result<(), String> {
        match fiber {
            NativeFiberReceiver::Direct(encoded) => self.set_native_fiber_state(*encoded, state),
            NativeFiberReceiver::Materialized(fiber) => {
                fiber.set_state(state);
                Ok(())
            }
        }
    }

    fn fiber_receiver_callable(&mut self, fiber: &NativeFiberReceiver) -> Result<i64, String> {
        match fiber {
            NativeFiberReceiver::Direct(encoded) => self
                .native_fiber_callable(*encoded)
                .ok_or_else(|| "native Fiber callable is missing".to_owned()),
            NativeFiberReceiver::Materialized(fiber) => self.encode(fiber.callable()),
        }
    }

    fn fiber_receiver_return_value(
        &mut self,
        fiber: &NativeFiberReceiver,
    ) -> Result<Option<i64>, String> {
        match fiber {
            NativeFiberReceiver::Direct(encoded) => {
                let value = self
                    .native_fiber_return_value(*encoded)
                    .ok_or_else(|| "native Fiber return slot is missing".to_owned())?;
                value
                    .map(|value| {
                        self.duplicate_authoritative_native_value(value)?
                            .ok_or_else(|| {
                                "direct Fiber return value is not authoritative native data"
                                    .to_owned()
                            })
                    })
                    .transpose()
            }
            NativeFiberReceiver::Materialized(fiber) => fiber
                .return_value()
                .map(|value| self.encode(value))
                .transpose(),
        }
    }

    fn terminate_fiber_receiver(
        &mut self,
        fiber: &NativeFiberReceiver,
        return_value: Option<i64>,
    ) -> Result<(), String> {
        match fiber {
            NativeFiberReceiver::Direct(encoded) => {
                self.terminate_native_fiber(*encoded, return_value)
            }
            NativeFiberReceiver::Materialized(fiber) => {
                let return_value = return_value.map(|value| self.decode(value)).transpose()?;
                fiber.terminate(return_value);
                Ok(())
            }
        }
    }

    /// A materialized ReferenceCell can outlive a direct object handle and can
    /// later be reached by a cold semantic operation. Restore native declared
    /// slots before exposing that referenced object to Rust property APIs.
    fn materialize_referenced_object(
        &mut self,
        reference: &php_runtime::api::ReferenceCell,
    ) -> Result<(), String> {
        let mut value = reference.get();
        for _ in 0..16 {
            let Value::Reference(next) = value else {
                break;
            };
            value = next.get();
        }
        let Value::Object(object) = value else {
            return Ok(());
        };
        self.materialize_direct_object_alias(&object)
    }

    fn materialize_direct_object_alias(
        &mut self,
        object: &php_runtime::api::ObjectRef,
    ) -> Result<(), String> {
        if object
            .native_declared_slots_view(object.class_layout_epoch())
            .is_none()
        {
            return Ok(());
        }
        let object_id = object.id();
        let index = self
            .direct_object_handles
            .get(&object_id)
            .copied()
            .and_then(|index| usize::try_from(index).ok())
            .filter(|index| {
                self.direct_value_slots.get(*index).is_some_and(|slot| {
                    slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
                        && slot.flags == php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION
                }) && self
                    .direct_object_owner(*index)
                    .is_some_and(|candidate| candidate.id() == object_id)
            })
            .or_else(|| {
                let used = usize::try_from(*self.direct_value_next).ok()?;
                (0..used).find(|index| {
                    self.direct_value_slots.get(*index).is_some_and(|slot| {
                        slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
                            && slot.flags == php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION
                    }) && self
                        .direct_object_owner(*index)
                        .is_some_and(|candidate| candidate.id() == object_id)
                })
            })
            .ok_or_else(|| format!("native object {object_id} has no live direct descriptor"))?;
        let was_dead = self.direct_value_slots[index].refcount == 0;
        if was_dead {
            // Recover a descriptor whose retained call-owner was decremented
            // to zero without the last-owner commit. It is revived only long
            // enough to materialize its escaped ObjectRef and retire cleanly.
            self.direct_value_slots[index].refcount = 1;
        }
        self.demote_direct_object_declared_slots(index)?;
        if was_dead {
            self.release_direct_value_index(index)?;
        }
        Ok(())
    }

    fn reserve_direct_value_slot(&mut self) -> Result<usize, String> {
        if *self.direct_value_free_head != php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE {
            let index = *self.direct_value_free_head as usize;
            let slot = self
                .direct_value_slots
                .get(index)
                .ok_or_else(|| "direct native value free-list entry is missing".to_owned())?;
            *self.direct_value_free_head = slot.reserved;
            *self.direct_value_reused_bytes = self
                .direct_value_reused_bytes
                .saturating_add(std::mem::size_of::<php_jit::JitNativeValueSlot>() as u64);
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
        Ok(index)
    }

    /// Publishes a PHP string directly into the authoritative request-owned
    /// native byte/value plane. The Rust `PhpString` is consumed at this
    /// boundary and is not mirrored in `NativeStoredValue`.
    #[track_caller]
    fn encode_native_string_owner(&mut self, string: PhpString) -> Result<i64, String> {
        if let Some(index) = self.direct_string_handles.get(&string).copied() {
            let slot = self
                .direct_value_slots
                .get_mut(index as usize)
                .filter(|slot| {
                    slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_STRING
                })
                .ok_or_else(|| "interned direct string points at a dead slot".to_owned())?;
            slot.refcount = slot
                .refcount
                .checked_add(1)
                .ok_or_else(|| "direct native string refcount overflow".to_owned())?;
            let runtime_index = index
                .checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
                .ok_or_else(|| "direct native string handle overflow".to_owned())?;
            return Ok((php_jit::JIT_VALUE_RUNTIME_STRING_TAG | u64::from(runtime_index)) as i64);
        }
        let encoded = self.encode_direct_string_bytes(string.as_bytes())?;
        let index = Self::direct_value_index(encoded)
            .and_then(|index| u32::try_from(index).ok())
            .ok_or_else(|| "direct native string index is invalid".to_owned())?;
        // The interning table owns one native reference for the request. This
        // lets generated release code remain a pure native refcount operation:
        // an interned slot cannot reach zero behind the cold table's back.
        self.retain(encoded)?;
        self.direct_string_handles.insert(string.clone(), index);
        self.direct_string_keys.insert(index as usize, string);
        Ok(encoded)
    }

    fn direct_string_capacity(length: usize) -> Result<usize, String> {
        length
            .max(php_jit::JIT_NATIVE_DIRECT_STRING_MIN_CAPACITY as usize)
            .checked_next_power_of_two()
            .ok_or_else(|| "direct native string capacity overflow".to_owned())
    }

    fn reserve_direct_string_bytes(&mut self, length: usize) -> Result<(usize, usize), String> {
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

    fn free_direct_string_bytes(&mut self, start: usize, capacity: usize) {
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

    fn encode_direct_string_bytes(&mut self, bytes: &[u8]) -> Result<i64, String> {
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
                self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
                    reserved: *self.direct_value_free_head,
                    ..php_jit::JitNativeValueSlot::default()
                };
                *self.direct_value_free_head = index as u32;
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
    /// cross-unit call boundary. Scalar and string literals never materialize
    /// a Rust `Value`; dynamic/class/array constants retain their exact cold
    /// resolution path.
    fn stabilize_active_unit_constant(&mut self, index: u32) -> Result<i64, String> {
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
            php_ir::IrConstant::Int(value) => Ok(value),
            php_ir::IrConstant::Float(value) => {
                self.encode_native_float_owner(php_runtime::api::FloatValue::from_f64(value))
            }
            php_ir::IrConstant::String(value) => self.encode_direct_string_bytes(value.as_bytes()),
            php_ir::IrConstant::StringBytes(value) => self.encode_direct_string_bytes(&value),
            php_ir::IrConstant::NamedConstant(_)
            | php_ir::IrConstant::ClassConstant { .. }
            | php_ir::IrConstant::Array(_) => {
                let encoded = php_jit::jit_encode_constant(index);
                self.decode(encoded).and_then(|value| self.encode(value))
            }
        }
    }

    /// Publishes a parameter/default constant directly into the native value
    /// plane.  Scalar and array defaults are common call-frame data and must
    /// not be constructed as a temporary Rust `Value` merely because the
    /// caller omitted an argument.
    fn encode_native_ir_constant_owned(
        &mut self,
        constant: &php_ir::IrConstant,
    ) -> Result<i64, String> {
        match constant {
            php_ir::IrConstant::Null => Ok(php_jit::jit_encode_constant(u32::MAX)),
            php_ir::IrConstant::Bool(false) => {
                Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE))
            }
            php_ir::IrConstant::Bool(true) => {
                Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE))
            }
            php_ir::IrConstant::Int(value) => Ok(*value),
            php_ir::IrConstant::Float(value) => {
                self.encode_native_float_owner(php_runtime::api::FloatValue::from_f64(*value))
            }
            php_ir::IrConstant::String(value) => self.encode_direct_string_bytes(value.as_bytes()),
            php_ir::IrConstant::StringBytes(value) => self.encode_direct_string_bytes(value),
            php_ir::IrConstant::Array(source) => {
                let mut entries =
                    Vec::<php_jit::JitNativeDirectArrayEntry>::with_capacity(source.len());
                let mut next_index = Some(0i64);
                for source_entry in source {
                    let value = match self.encode_native_ir_constant_owned(&source_entry.value) {
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
                        Some(key) => self.encode_native_constant_array_key_owned(key),
                        None => next_index.ok_or_else(|| {
                            php_runtime::api::PHP_ARRAY_APPEND_OVERFLOW_MESSAGE.to_owned()
                        }),
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
                    if let Some(key_value) = self.native_encoded_int(key)
                        && key_value >= 0
                        && next_index.is_some_and(|next| key_value >= next)
                    {
                        next_index = key_value.checked_add(1);
                    } else if source_entry.key.is_none() {
                        next_index = next_index.and_then(|next| next.checked_add(1));
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
            php_ir::IrConstant::NamedConstant(_) | php_ir::IrConstant::ClassConstant { .. } => {
                let value = native_runtime_constant_value(self, constant)?;
                self.encode(value)
            }
        }
    }

    fn encode_native_constant_array_key_owned(
        &mut self,
        constant: &php_ir::IrConstant,
    ) -> Result<i64, String> {
        match constant {
            php_ir::IrConstant::Null => self.encode_direct_string_bytes(&[]),
            php_ir::IrConstant::Bool(value) => Ok(i64::from(*value)),
            php_ir::IrConstant::Int(value) => Ok(*value),
            php_ir::IrConstant::Float(value) => Ok(*value as i64),
            php_ir::IrConstant::String(value) => self.encode_direct_string_bytes(value.as_bytes()),
            php_ir::IrConstant::StringBytes(value) => self.encode_direct_string_bytes(value),
            php_ir::IrConstant::Array(_) => Err("native constant array key is invalid".to_owned()),
            php_ir::IrConstant::NamedConstant(_) | php_ir::IrConstant::ClassConstant { .. } => {
                let value = native_runtime_constant_value(self, constant)?;
                let key = php_runtime::api::ArrayKey::from_value(&value)
                    .ok_or_else(|| "native constant array key is invalid".to_owned())?;
                match key {
                    php_runtime::api::ArrayKey::Int(value) => Ok(value),
                    php_runtime::api::ArrayKey::String(value) => {
                        self.encode_native_string_owner(value)
                    }
                }
            }
        }
    }

    fn native_encoded_array_keys_equal(&self, left: i64, right: i64) -> bool {
        match (
            self.native_encoded_int(left),
            self.native_encoded_int(right),
        ) {
            (Some(left), Some(right)) => left == right,
            (None, None) => {
                self.native_string_name_bytes(left) == self.native_string_name_bytes(right)
            }
            _ => false,
        }
    }

    fn native_encoded_matches_array_key(
        &self,
        encoded: i64,
        key: &php_runtime::api::ArrayKey,
    ) -> bool {
        match key {
            php_runtime::api::ArrayKey::Int(key) => self.native_encoded_int(encoded) == Some(*key),
            php_runtime::api::ArrayKey::String(key) => self
                .native_string_name_bytes(encoded)
                .is_some_and(|bytes| bytes == key.as_bytes()),
        }
    }

    fn encode_native_array_key_owned(
        &mut self,
        key: &php_runtime::api::ArrayKey,
    ) -> Result<i64, String> {
        match key {
            php_runtime::api::ArrayKey::Int(key) => Ok(*key),
            php_runtime::api::ArrayKey::String(key) => self.encode_native_string_owner(key.clone()),
        }
    }

    /// Converts the two diagnostic-free PHP array-key families directly.
    /// Float/bool/null/object conversions remain at the semantic boundary
    /// because they may emit PHP-visible diagnostics.
    fn native_encoded_plain_array_key(&self, encoded: i64) -> Option<php_runtime::api::ArrayKey> {
        let encoded = self.dereference_direct_encoding(encoded);
        match self.native_encoded_value_kind(encoded)? {
            NativeEncodedValueKind::Int => self
                .native_encoded_int(encoded)
                .map(php_runtime::api::ArrayKey::Int),
            NativeEncodedValueKind::String => self
                .native_string_name_bytes(encoded)
                .map(PhpString::from_bytes)
                .map(php_runtime::api::ArrayKey::from_php_string),
            _ => None,
        }
    }

    /// Publishes one IEEE-754 scalar directly. The payload is authoritative;
    /// no `NativeStoredValue::Php` mirror is retained.
    fn encode_native_float_owner(
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

    /// Publishes one PHP reference identity with its contained value owned by
    /// the direct value plane. The `ReferenceCell` sidecar preserves alias
    /// identity for a later cold boundary; optimizing code reads and replaces
    /// only the encoded payload in the direct slot.
    #[track_caller]
    fn encode_native_reference_owner(
        &mut self,
        reference: php_runtime::api::ReferenceCell,
    ) -> Result<i64, String> {
        if let Some(index) = self
            .direct_reference_cells
            .iter()
            .find_map(|(index, existing)| existing.ptr_eq(&reference).then_some(*index))
        {
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
            let runtime_index = u32::try_from(index)
                .ok()
                .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
                .ok_or_else(|| "direct native reference handle overflow".to_owned())?;
            return Ok(
                (php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG | u64::from(runtime_index)) as i64,
            );
        }

        // Publish the empty descriptor and identity before recursively
        // encoding the payload so a recursive PHP reference resolves to this
        // same slot instead of allocating a second identity.
        let index = self.reserve_direct_value_slot()?;
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
            flags: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
            reserved: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY,
            ..php_jit::JitNativeValueSlot::default()
        };
        self.direct_reference_cells.insert(index, reference.clone());

        let payload = match self.encode(reference.get()) {
            Ok(payload) => payload,
            Err(error) => {
                self.direct_reference_cells.remove(&index);
                let _ = self.release_direct_value_index(index);
                return Err(error);
            }
        };
        let slot = self
            .direct_value_slots
            .get_mut(index)
            .ok_or_else(|| format!("direct native reference {index} slot disappeared"))?;
        slot.reserved = php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED;
        slot.payload = payload as u64;

        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .ok_or_else(|| "direct native reference handle overflow".to_owned())?;
        Ok((php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG | u64::from(runtime_index)) as i64)
    }

    /// Publishes object identity and PHP ownership in the direct plane. The
    /// slot-parallel stable owner supplies the backing identity needed at a
    /// cold boundary; declared values move into native slots immediately.
    #[track_caller]
    fn encode_native_object_owner(
        &mut self,
        object: php_runtime::api::ObjectRef,
    ) -> Result<i64, String> {
        let object_id = object.id();
        let existing = self
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
            self.direct_object_handles.insert(object_id, index);
            let runtime_index = index
                .checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
                .ok_or_else(|| "direct native object handle overflow".to_owned())?;
            if self.direct_value_slots[index as usize].flags
                != php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION
                && let Err(error) = self.promote_direct_object_declared_slots(index as usize)
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
        let owner = Box::into_raw(Box::new(object));
        self.direct_object_owners[index] = owner as usize as u64;
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT,
            payload: object_id,
            ..php_jit::JitNativeValueSlot::default()
        };
        self.direct_object_handles.insert(
            object_id,
            u32::try_from(index).map_err(|_| "direct native object index overflow".to_owned())?,
        );
        if let Err(error) = self.promote_direct_object_declared_slots(index) {
            let _ = self.release_direct_value_index(index);
            return Err(error);
        }
        Ok((php_jit::JIT_VALUE_RUNTIME_OBJECT_TAG | u64::from(runtime_index)) as i64)
    }

    #[track_caller]
    fn promote_direct_object_declared_slots(&mut self, index: usize) -> Result<bool, String> {
        let object = self
            .direct_object(index)
            .ok_or_else(|| format!("direct native object {index} has no stable owner"))?;
        let layout_id = object.class_layout_epoch();
        if object.native_declared_slots_view(layout_id).is_some() {
            return Ok(true);
        }
        let Some(rust_slots) = object.take_declared_slots_for_native(layout_id) else {
            return Ok(false);
        };
        self.record_direct_object_promotion(std::panic::Location::caller());
        let mut native_slots: Vec<php_runtime::api::NativeDeclaredPropertySlot> =
            Vec::with_capacity(rust_slots.len());
        for slot in &rust_slots {
            let encoded = match slot {
                Some(value) => match self.encode(value.clone()) {
                    Ok(encoded) => php_runtime::api::NativeDeclaredPropertySlot {
                        initialized: 1,
                        reserved: 0,
                        value: encoded,
                    },
                    Err(error) => {
                        for slot in native_slots {
                            if slot.initialized != 0 {
                                let _ = self.release(slot.value);
                            }
                        }
                        let _ = object.restore_declared_slots_from_native(layout_id, rust_slots);
                        return Err(error);
                    }
                },
                None => php_runtime::api::NativeDeclaredPropertySlot::default(),
            };
            native_slots.push(encoded);
        }
        if !object.install_native_declared_slots(layout_id, native_slots.into_boxed_slice()) {
            let _ = object.restore_declared_slots_from_native(layout_id, rust_slots);
            return Ok(false);
        }
        let Some((base, count)) = object.native_declared_slots_view(layout_id) else {
            return Err("native object slots disappeared during publication".to_owned());
        };
        let slot = self
            .direct_value_slots
            .get_mut(index)
            .ok_or_else(|| format!("direct native object {index} slot is missing"))?;
        slot.flags = php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION;
        slot.reserved = u32::try_from(count).unwrap_or(u32::MAX);
        slot.payload = layout_id;
        slot.aux = base as usize as u64;
        Ok(true)
    }

    #[track_caller]
    fn demote_direct_object_declared_slots(&mut self, index: usize) -> Result<(), String> {
        let object = self
            .direct_object_owner(index)
            .ok_or_else(|| format!("direct native object {index} has no stable owner"))?;
        let descriptor = *self
            .direct_value_slots
            .get(index)
            .ok_or_else(|| format!("direct native object {index} slot is missing"))?;
        if descriptor.flags != php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION {
            return Ok(());
        }
        self.record_direct_object_demotion(std::panic::Location::caller());
        let layout_id = descriptor.payload;
        let Some(native_slots) = object.take_native_declared_slots(layout_id) else {
            return Err(format!(
                "direct native object {index} lost its declared-slot storage"
            ));
        };
        // Mark the descriptor cold before decoding so a self-referential
        // object slot does not recursively attempt the same demotion.
        if let Some(slot) = self.direct_value_slots.get_mut(index) {
            slot.flags = 0;
            slot.reserved = 0;
            slot.payload = object.id();
            slot.aux = 0;
        }
        let mut rust_slots = Vec::with_capacity(native_slots.len());
        for slot in &native_slots {
            if slot.initialized == 0 {
                rust_slots.push(None);
            } else {
                match self.decode(slot.value) {
                    Ok(value) => rust_slots.push(Some(value)),
                    Err(error) => {
                        let _ = object.install_native_declared_slots(layout_id, native_slots);
                        if let Some(slot) = self.direct_value_slots.get_mut(index) {
                            *slot = descriptor;
                        }
                        return Err(error);
                    }
                }
            }
        }
        if !object.restore_declared_slots_from_native(layout_id, rust_slots) {
            let _ = object.install_native_declared_slots(layout_id, native_slots);
            if let Some(slot) = self.direct_value_slots.get_mut(index) {
                *slot = descriptor;
            }
            return Err("failed to restore cold object property slots".to_owned());
        }
        let mut release_error = None;
        for slot in native_slots.iter().filter(|slot| slot.initialized != 0) {
            if let Err(error) = self.release(slot.value) {
                release_error.get_or_insert(error);
            }
        }
        if let Some(error) = release_error {
            return Err(error);
        }
        Ok(())
    }

    /// Removes authoritative native properties from an object that is about
    /// to die without running user code. The encoded children are returned to
    /// the central direct release walk; no Rust `Value` is reconstructed.
    fn take_direct_object_children(&mut self, index: usize) -> Result<Vec<i64>, String> {
        let object = self
            .direct_object_owner(index)
            .ok_or_else(|| format!("direct native object {index} has no stable owner"))?;
        let descriptor = *self
            .direct_value_slots
            .get(index)
            .ok_or_else(|| format!("direct native object {index} slot is missing"))?;
        if descriptor.flags != php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION {
            return Ok(Vec::new());
        }
        let slots = object
            .take_native_declared_slots(descriptor.payload)
            .ok_or_else(|| format!("direct native object {index} lost its declared slots"))?;
        let children = slots
            .iter()
            .filter(|slot| slot.initialized != 0)
            .map(|slot| slot.value)
            .collect();
        self.direct_value_slots[index].flags = 0;
        self.direct_value_slots[index].reserved = 0;
        self.direct_value_slots[index].payload = object.id();
        self.direct_value_slots[index].aux = 0;
        Ok(children)
    }

    /// Moves one newly constructed PHP array into the canonical native array
    /// plane at a call-frame boundary. This is an ownership transfer, not a
    /// shadow view of a retained `PhpArray`: the direct slot and its entries
    /// become the sole representation consumed by optimizing code.
    #[track_caller]
    fn encode_native_array_owner(
        &mut self,
        array: php_runtime::api::PhpArray,
    ) -> Result<i64, String> {
        let root = self.begin_direct_array_encode();
        let result = self.encode_direct_array_value_unscoped(array);
        self.finish_direct_array_encode(root, result)
    }

    fn begin_direct_array_encode(&mut self) -> bool {
        let root = self.direct_array_encode_depth == 0;
        if root {
            debug_assert!(self.direct_array_handles.is_empty());
            debug_assert!(self.direct_array_storage_ids.is_empty());
        }
        self.direct_array_encode_depth = self.direct_array_encode_depth.saturating_add(1);
        root
    }

    fn finish_direct_array_encode<T>(
        &mut self,
        root: bool,
        result: Result<T, String>,
    ) -> Result<T, String> {
        self.direct_array_encode_depth = self.direct_array_encode_depth.saturating_sub(1);
        if !root {
            return result;
        }
        let pool_owners = std::mem::take(&mut self.direct_array_handles)
            .into_values()
            .map(usize::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "direct native array pool index overflow".to_owned())?;
        self.direct_array_storage_ids.clear();
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
        if let Some(index) = self.direct_array_handles.get(&storage_version).copied() {
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
        let mut entries: Vec<php_jit::JitNativeDirectArrayEntry> = Vec::with_capacity(array.len());
        for (key, value) in array.iter() {
            let key = match key {
                php_runtime::api::ArrayKey::Int(key) => self.encode(Value::Int(key)),
                php_runtime::api::ArrayKey::String(key) => {
                    self.encode_native_string_owner(key.clone())
                }
            };
            let key = match key {
                Ok(key) => key,
                Err(error) => {
                    for entry in entries {
                        let _ = self.release(entry.key);
                        let _ = self.release(entry.value);
                    }
                    return Err(error);
                }
            };
            let value = match self.encode(value.clone()) {
                Ok(value) => value,
                Err(error) => {
                    let _ = self.release(key);
                    for entry in entries {
                        let _ = self.release(entry.key);
                        let _ = self.release(entry.value);
                    }
                    return Err(error);
                }
            };
            entries.push(php_jit::JitNativeDirectArrayEntry { key, value });
        }

        let (start, capacity) = match self.reserve_direct_array_entries(entries.len()) {
            Ok(reserved) => reserved,
            Err(error) => {
                for entry in entries {
                    let _ = self.release(entry.key);
                    let _ = self.release(entry.value);
                }
                return Err(error);
            }
        };
        self.direct_array_entries[start..start + entries.len()].copy_from_slice(&entries);

        let index = match self.reserve_direct_value_slot() {
            Ok(index) => index,
            Err(error) => {
                self.free_direct_array_entries(start, capacity);
                for entry in entries {
                    let _ = self.release(entry.key);
                    let _ = self.release(entry.value);
                }
                return Err(error);
            }
        };
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
            flags: php_jit::jit_native_direct_array_flags(cursor),
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
        let encoded = (php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG | u64::from(runtime_index)) as i64;
        // One request-owned native reference represents the canonical COW
        // snapshot. Encoded PHP owners receive the original reference above.
        if let Err(error) = self.retain(encoded) {
            let _ = self.release_direct_value_index(index);
            return Err(error);
        }
        self.direct_array_handles
            .insert(storage_version, index as u32);
        self.direct_array_storage_ids.insert(index, storage_version);
        Ok(encoded)
    }

    fn reserve_direct_array_entries(&mut self, length: usize) -> Result<(usize, usize), String> {
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

    fn free_direct_array_entries(&mut self, start: usize, capacity: usize) {
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

    fn replace_direct_array(
        &mut self,
        index: usize,
        array: php_runtime::api::PhpArray,
    ) -> Result<(), String> {
        if let Some(storage_version) = self.direct_array_storage_ids.remove(&index) {
            if self.direct_array_handles.get(&storage_version) == Some(&(index as u32)) {
                self.direct_array_handles.remove(&storage_version);
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
                php_runtime::api::ArrayKey::Int(key) => self.encode(Value::Int(key)),
                php_runtime::api::ArrayKey::String(key) => self.encode_native_string_owner(key),
            }?;
            let value = match self.encode(value) {
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

    #[track_caller]
    fn decode_direct_array(&mut self, index: usize) -> Result<Value, String> {
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
        for entry in entries {
            let key = self.decode(entry.key)?;
            let key = php_runtime::api::ArrayKey::from_value(&key)
                .ok_or_else(|| format!("direct native array {index} has an invalid key"))?;
            array.insert(key, self.decode(entry.value)?);
        }
        let state = self.direct_array_states[index];
        array.set_native_next_append_key(
            (state.has_next_append_key != 0).then_some(state.next_append_key),
        );
        array.set_native_pointer_position(cursor);
        Ok(Value::Array(array))
    }

    #[track_caller]
    fn decode_direct_value(&mut self, index: usize) -> Result<Value, String> {
        let slot = *self
            .direct_value_slots
            .get(index)
            .filter(|slot| slot.refcount != 0)
            .ok_or_else(|| format!("direct native value {index} is missing"))?;
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
            if let Some(fiber) = self.direct_fiber_cells.get(&index) {
                return Ok(Value::Fiber(fiber.clone()));
            }
            let (state, callable, return_value) = {
                let fiber = self
                    .fiber_record(index)
                    .ok_or_else(|| format!("direct native Fiber {index} has no stable record"))?;
                (fiber.state, fiber.callable, fiber.return_value)
            };
            let callable = self.decode(callable)?;
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
                    let return_value = return_value.map(|value| self.decode(value)).transpose()?;
                    fiber.terminate(return_value);
                }
                state => fiber.set_state(state),
            }
            self.direct_value_slots[index].kind = php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_FIBER;
            self.direct_value_slots[index].payload = fiber.id();
            self.direct_fiber_handles.insert(fiber.id(), index as u32);
            self.direct_fiber_cells.insert(index, fiber.clone());
            return Ok(Value::Fiber(fiber));
        }
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE {
            let prepared = self
                .direct_prepared_callable(index)
                .ok_or_else(|| format!("direct native callable {index} has no stable record"))?;
            match prepared {
                NativePreparedCallable::UserFunction { name } => {
                    return Ok(Value::Callable(Box::new(
                        php_runtime::api::CallableValue::UserFunction { name: name.clone() },
                    )));
                }
                NativePreparedCallable::InternalBuiltin { name } => {
                    return Ok(Value::Callable(Box::new(
                        php_runtime::api::CallableValue::InternalBuiltin { name: name.clone() },
                    )));
                }
                NativePreparedCallable::MethodPlaceholder { target } => {
                    return Ok(Value::Callable(Box::new(
                        php_runtime::api::CallableValue::MethodPlaceholder {
                            target: target.clone(),
                        },
                    )));
                }
                NativePreparedCallable::UnresolvedDynamic { target } => {
                    return Ok(Value::Callable(Box::new(
                        php_runtime::api::CallableValue::UnresolvedDynamic {
                            target: target.clone(),
                        },
                    )));
                }
                NativePreparedCallable::BoundMethod {
                    target: NativePreparedCallableMethodTarget::Class(class),
                    method,
                    scope,
                } => {
                    return Ok(Value::Callable(Box::new(
                        php_runtime::api::CallableValue::BoundMethod {
                            target: php_runtime::api::CallableMethodTarget::Class(class.clone()),
                            method: method.clone(),
                            scope: scope.clone(),
                        },
                    )));
                }
                NativePreparedCallable::Closure(_) | NativePreparedCallable::BoundMethod { .. } => {
                }
            }
            if let NativePreparedCallable::BoundMethod {
                target: NativePreparedCallableMethodTarget::Object(object),
                method,
                scope,
            } = prepared
            {
                let (object, method, scope) = (*object, method.clone(), scope.clone());
                let Value::Object(object) = self.decode(object)? else {
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
            let (mut closure, capture_descriptors, captures, implicit_this) = {
                let NativePreparedCallable::Closure(prepared) = self
                    .direct_prepared_callable(index)
                    .ok_or_else(|| format!("direct native closure {index} has no stable record"))?
                else {
                    return Err(format!(
                        "direct native callable {index} has an invalid target"
                    ));
                };
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
                Some(encoded) => match self.decode(encoded)? {
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
                    let value = self.decode(encoded)?;
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
            self.demote_direct_object_declared_slots(index)?;
            let object = self
                .direct_object(index)
                .ok_or_else(|| format!("direct native object {index} has no stable owner"))?;
            return Ok(Value::Object(object));
        }
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR {
            if slot.flags != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
                || slot.reserved == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
            {
                return Err(format!(
                    "direct native reference {index} has no published scalar"
                ));
            }
            let value = self.decode(slot.payload as i64)?;
            let reference = self
                .direct_reference_cells
                .get(&index)
                .cloned()
                .unwrap_or_else(|| php_runtime::api::ReferenceCell::new(Value::Null));
            reference.set(value);
            self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
                kind: php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR,
                flags: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
                reserved: 0,
                payload: reference.native_scalar_view_address() as u64,
                aux: reference.native_array_view_address() as u64,
                ..slot
            };
            self.direct_reference_cells.insert(index, reference.clone());
            // The cold `ReferenceCell` now owns the materialized PHP value;
            // the direct payload ownership ended at this exact boundary.
            self.release(slot.payload as i64)?;
            return Ok(Value::Reference(reference));
        }
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR {
            let reference = self
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
        if slot.kind != php_jit::JIT_NATIVE_VALUE_VIEW_STRING {
            return self.decode_direct_array(index);
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

    #[track_caller]
    fn decode(&mut self, encoded: i64) -> Result<Value, String> {
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
            if let Some(value) = self.decoded_constant_cache.borrow().get(&cache_key) {
                return Ok(value.clone());
            }
            let constant = self
                .unit
                .constants
                .get(constant_index)
                .ok_or_else(|| {
                    format!(
                        "native constant {constant} is missing from active unit {} (dynamic={:?}, constants={}, source={})",
                        self.unit.id.raw(),
                        self.current_dynamic_unit,
                        self.unit.constants.len(),
                        self.unit
                            .files
                            .first()
                            .map_or("<unknown>", |file| file.path.as_str()),
                    )
                })?;
            // Constants embedded in native operands can still require the
            // active request context (for example a runtime-defined constant
            // used as a default argument in a bounded large-unit call graph).
            let value = native_runtime_constant_value(self, constant)?;
            self.decoded_constant_cache
                .borrow_mut()
                .insert(cache_key, value.clone());
            return Ok(value);
        }
        if let Some(index) = php_jit::jit_decode_runtime_value(encoded) {
            if let Some(direct) = Self::direct_value_index(encoded) {
                return self.decode_direct_value(direct);
            }
            if let Some(NativeStoredValue::Php(Value::Reference(reference))) =
                self.values.get(index as usize).and_then(Option::as_ref)
            {
                let reference = reference.clone();
                self.materialize_referenced_object(&reference)?;
                return Ok(Value::Reference(reference));
            }
            if let Some(NativeStoredValue::Php(Value::Object(object))) =
                self.values.get(index as usize).and_then(Option::as_ref)
            {
                let object = object.clone();
                self.materialize_direct_object_alias(&object)?;
                return Ok(Value::Object(object));
            }
            return match self.values.get(index as usize).and_then(Option::as_ref) {
                Some(NativeStoredValue::Php(value)) => Ok(value.clone()),
                Some(NativeStoredValue::GlobalsProxy) => self.materialize_native_globals_array(),
                Some(
                    NativeStoredValue::ArrayIterator(_)
                    | NativeStoredValue::Iterator(_)
                    | NativeStoredValue::GeneratorIterator(_),
                ) => Err(format!(
                    "native runtime value {index} is a foreach iterator"
                )),
                None => Err(format!("native runtime value {index} is missing")),
            };
        }
        Ok(Value::Int(encoded))
    }

    fn encode_prepared_callable(
        &mut self,
        callable: Box<php_runtime::api::CallableValue>,
    ) -> Result<i64, String> {
        if matches!(
            callable.as_ref(),
            php_runtime::api::CallableValue::Closure(_)
        ) {
            return self.encode_prepared_closure(callable);
        }
        let prepared = match *callable {
            php_runtime::api::CallableValue::UserFunction { name } => {
                NativePreparedCallable::UserFunction { name }
            }
            php_runtime::api::CallableValue::InternalBuiltin { name } => {
                NativePreparedCallable::InternalBuiltin { name }
            }
            php_runtime::api::CallableValue::BoundMethod {
                target,
                method,
                scope,
            } => {
                let target = match target {
                    php_runtime::api::CallableMethodTarget::Object(object) => {
                        NativePreparedCallableMethodTarget::Object(
                            self.encode_native_object_owner(object)?,
                        )
                    }
                    php_runtime::api::CallableMethodTarget::Class(class) => {
                        NativePreparedCallableMethodTarget::Class(class)
                    }
                };
                NativePreparedCallable::BoundMethod {
                    target,
                    method,
                    scope,
                }
            }
            php_runtime::api::CallableValue::MethodPlaceholder { target } => {
                NativePreparedCallable::MethodPlaceholder { target }
            }
            php_runtime::api::CallableValue::UnresolvedDynamic { target } => {
                NativePreparedCallable::UnresolvedDynamic { target }
            }
            php_runtime::api::CallableValue::Closure(_) => unreachable!(),
        };
        let index = match self.reserve_direct_value_slot() {
            Ok(index) => index,
            Err(error) => {
                if let NativePreparedCallable::BoundMethod {
                    target: NativePreparedCallableMethodTarget::Object(object),
                    ..
                } = &prepared
                {
                    let _ = self.release(*object);
                }
                return Err(error);
            }
        };
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .expect("direct callable index is bounded by the native value arena");
        let owner = Box::into_raw(Box::new(prepared));
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

    fn publish_native_fiber(
        &mut self,
        callable: i64,
        state: php_runtime::api::FiberState,
        return_value: Option<i64>,
        materialized: Option<php_runtime::api::FiberRef>,
    ) -> Result<i64, String> {
        let index = match self.reserve_direct_value_slot() {
            Ok(index) => index,
            Err(error) => {
                let _ = self.release(callable);
                if let Some(return_value) = return_value {
                    let _ = self.release(return_value);
                }
                return Err(error);
            }
        };
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .expect("direct Fiber index is bounded by the native value arena");
        let identity = materialized.as_ref().map(php_runtime::api::FiberRef::id);
        let owner = Box::into_raw(Box::new(NativeDirectFiber {
            state,
            callable,
            return_value,
        }));
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: if materialized.is_some() {
                php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_FIBER
            } else {
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER
            },
            flags: php_jit::JIT_NATIVE_DIRECT_FIBER_ABI_VERSION,
            payload: identity.unwrap_or(0),
            aux: owner as usize as u64,
            ..php_jit::JitNativeValueSlot::default()
        };
        if let Some(fiber) = materialized {
            self.direct_fiber_handles.insert(fiber.id(), index as u32);
            self.direct_fiber_cells.insert(index, fiber);
        }
        Ok(php_jit::jit_encode_typed_runtime_value(
            runtime_index,
            php_jit::JIT_VALUE_RUNTIME_FIBER_TAG,
        ))
    }

    fn encode_native_fiber(&mut self, callable: i64) -> Result<i64, String> {
        self.retain(callable)?;
        self.publish_native_fiber(
            callable,
            php_runtime::api::FiberState::NotStarted,
            None,
            None,
        )
    }

    fn encode_native_fiber_owner(
        &mut self,
        fiber: php_runtime::api::FiberRef,
    ) -> Result<i64, String> {
        if let Some(index) = self.direct_fiber_handles.get(&fiber.id()).copied() {
            let slot = self
                .direct_value_slots
                .get_mut(index as usize)
                .filter(|slot| {
                    slot.refcount != 0
                        && matches!(
                            slot.kind,
                            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER
                                | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_FIBER
                        )
                })
                .ok_or_else(|| "native Fiber identity points at a dead slot".to_owned())?;
            slot.refcount = slot
                .refcount
                .checked_add(1)
                .ok_or_else(|| "native Fiber refcount overflow".to_owned())?;
            let runtime_index = index
                .checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
                .ok_or_else(|| "native Fiber handle overflow".to_owned())?;
            return Ok(php_jit::jit_encode_typed_runtime_value(
                runtime_index,
                php_jit::JIT_VALUE_RUNTIME_FIBER_TAG,
            ));
        }
        let callable = self.encode(fiber.callable())?;
        let return_value = match fiber.return_value().map(|value| self.encode(value)) {
            Some(Ok(value)) => Some(value),
            Some(Err(error)) => {
                let _ = self.release(callable);
                return Err(error);
            }
            None => None,
        };
        self.publish_native_fiber(callable, fiber.state(), return_value, Some(fiber))
    }

    fn encode_prepared_closure(
        &mut self,
        callable: Box<php_runtime::api::CallableValue>,
    ) -> Result<i64, String> {
        let closure = match *callable {
            php_runtime::api::CallableValue::Closure(closure) => closure,
            _ => unreachable!(),
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
            let encoded = (|| {
                if capture.name.eq_ignore_ascii_case("this")
                    && let Some(object) = &closure.bound_this
                {
                    self.encode_native_object_owner(object.clone())
                } else if let Some(reference) = capture.reference() {
                    self.encode_native_reference_owner(reference)
                } else {
                    self.encode(capture.value().cloned().unwrap_or(Value::Null))
                }
            })();
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
        let index = match self.reserve_direct_value_slot() {
            Ok(index) => index,
            Err(error) => {
                if let Some(implicit_this) = implicit_this {
                    let _ = self.release(implicit_this);
                }
                for capture in capture_values {
                    let _ = self.release(capture);
                }
                return Err(error);
            }
        };
        let runtime_index = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
            .expect("direct closure index is bounded by the native value arena");
        let closure_id = closure.id;
        let mut closure = closure;
        closure.bound_this = None;
        closure.captures.clear();
        let owner = Box::into_raw(Box::new(NativePreparedCallable::Closure(
            NativePreparedClosure {
                closure,
                capture_descriptors: capture_descriptors.into_boxed_slice(),
                implicit_this,
                captures: capture_values.into_boxed_slice(),
            },
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

    #[track_caller]
    fn encode(&mut self, value: Value) -> Result<i64, String> {
        let root = self.begin_direct_array_encode();
        let result = self.encode_unscoped(value);
        self.finish_direct_array_encode(root, result)
    }

    #[track_caller]
    fn encode_unscoped(&mut self, value: Value) -> Result<i64, String> {
        let value = match value {
            Value::Array(array) => return self.encode_direct_array_value_unscoped(array),
            Value::String(string) => return self.encode_native_string_owner(string),
            Value::Float(value) => return self.encode_native_float_owner(value),
            Value::Object(object) => return self.encode_native_object_owner(object),
            Value::Reference(reference) => return self.encode_native_reference_owner(reference),
            Value::Callable(callable) => return self.encode_prepared_callable(callable),
            Value::Fiber(fiber) => return self.encode_native_fiber_owner(fiber),
            value => value,
        };
        match &value {
            Value::Null => return Ok(php_jit::jit_encode_constant(u32::MAX)),
            Value::Bool(false) => {
                return Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE));
            }
            Value::Bool(true) => {
                return Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE));
            }
            Value::Int(value)
                if php_jit::jit_decode_constant(*value).is_none()
                    && php_jit::jit_decode_runtime_value(*value).is_none() =>
            {
                return Ok(*value);
            }
            _ => {}
        }
        self.encode_stored_value(NativeStoredValue::Php(value))
    }

    /// Moves a compatibility result into the authoritative native value plane.
    /// Baseline-native is a continuation tier, not a second value ABI: arrays
    /// must therefore cross this boundary as direct handles as well. Their
    /// native refcount supplies the COW separation used by later writes.
    #[track_caller]
    fn encode_baseline_call_value(&mut self, value: Value) -> Result<i64, String> {
        self.encode(value)
    }

    fn encode_stored_value(&mut self, value: NativeStoredValue) -> Result<i64, String> {
        let tag = stored_value_tag(&value);
        let kind = stored_value_kind(&value);
        let native_slot = stored_value_slot(&value);
        let identity = stored_value_identity(&value);
        if let Some(index) = identity
            .as_ref()
            .and_then(|identity| self.interned_value_handles.get(identity).copied())
        {
            let index = index as usize;
            self.retain_runtime_value_index(index)?;
            return Ok(php_jit::jit_encode_typed_runtime_value(index as u32, tag));
        }
        if let Some(index) = self.free_value_slots.pop() {
            let slot = self
                .values
                .get_mut(index as usize)
                .ok_or_else(|| format!("native free value slot {index} is missing"))?;
            if slot.is_some()
                || self
                    .value_slots
                    .get(index as usize)
                    .is_none_or(|slot| slot.refcount != 0)
            {
                return Err(format!("native free value slot {index} is still live"));
            }
            *slot = Some(value);
            self.value_slots[index as usize] = native_slot;
            if let Some(identity) = identity {
                self.interned_value_handles.insert(identity, index);
            }
            self.record_value_table_reuse(kind);
            return Ok(php_jit::jit_encode_typed_runtime_value(index, tag));
        }
        let index = u32::try_from(self.values.len())
            .map_err(|_| "native runtime value table exhausted".to_owned())?;
        if self.values.len() >= self.value_slots.capacity() {
            return Err(format!(
                "native runtime value plane exhausted at {} entries",
                self.values.len()
            ));
        }
        self.values.push(Some(value));
        self.value_slots[index as usize] = native_slot;
        if let Some(identity) = identity {
            self.interned_value_handles.insert(identity, index);
        }
        self.record_value_table_allocation(self.values.len(), kind);
        Ok(php_jit::jit_encode_typed_runtime_value(index, tag))
    }

    /// Give a native callee an independently owned argument without routing
    /// every value through `Value::clone` and a second arena lookup.
    ///
    /// Runtime handles are request-wide, not unit-local. Objects, references,
    /// strings, callables, resources, generators, fibers, and stored scalars
    /// can therefore share the existing slot by incrementing its arena
    /// refcount. Arrays need a distinct `PhpArray` facade so a write in the
    /// callee triggers the runtime's copy-on-write separation instead of
    /// mutating the caller's facade in place. Unit-local constant operands are
    /// materialized before an external-unit switch because their indexes are
    /// interpreted against the active unit.
    fn duplicate_baseline_call_argument(&mut self, encoded: i64) -> Result<i64, String> {
        if let Some(index) = Self::direct_value_index(encoded) {
            let refcount = &mut self
                .direct_value_slots
                .get_mut(index)
                .ok_or_else(|| format!("direct native value {index} is missing"))?
                .refcount;
            *refcount = refcount
                .checked_add(1)
                .ok_or_else(|| format!("direct native value {index} refcount overflow"))?;
            return Ok(encoded);
        }
        if let Some(index) = php_jit::jit_decode_runtime_value(encoded) {
            let index = index as usize;
            if matches!(
                self.values.get(index).and_then(Option::as_ref),
                Some(NativeStoredValue::GlobalsProxy)
            ) {
                let globals = self.materialize_native_globals_array()?;
                return self.encode(globals);
            }
            match self.values.get(index).and_then(Option::as_ref) {
                Some(NativeStoredValue::Php(Value::Array(array))) => {
                    // Retire a compatibility array the first time it reaches a
                    // native call boundary. New baseline results no longer
                    // create this representation.
                    return self.encode_native_array_owner(array.clone());
                }
                Some(NativeStoredValue::Php(_)) => {}
                Some(NativeStoredValue::GlobalsProxy) => unreachable!(),
                Some(
                    NativeStoredValue::ArrayIterator(_)
                    | NativeStoredValue::Iterator(_)
                    | NativeStoredValue::GeneratorIterator(_),
                ) => {
                    return Err(format!(
                        "native runtime value {index} is a foreach iterator"
                    ));
                }
                None => return Err(format!("native runtime value {index} is missing")),
            }
            self.retain_runtime_value_index(index)?;
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

    /// Gives an encoded value one additional request-arena owner without
    /// decoding or reconstructing it. Direct values are authoritative;
    /// compatibility scalars, objects, and references can still retain their
    /// stable request slot until they naturally retire.
    /// `None` is reserved for baseline-only arrays/proxies/iterators whose
    /// legacy facade semantics require an explicit cold operation.
    fn duplicate_authoritative_native_value(
        &mut self,
        encoded: i64,
    ) -> Result<Option<i64>, String> {
        if Self::direct_value_index(encoded).is_some() {
            self.retain(encoded)?;
            return Ok(Some(encoded));
        }
        if let Some(index) = php_jit::jit_decode_runtime_value(encoded) {
            let index = index as usize;
            return match self.values.get(index).and_then(Option::as_ref) {
                Some(NativeStoredValue::Php(Value::Array(_)) | NativeStoredValue::GlobalsProxy) => {
                    Ok(None)
                }
                Some(NativeStoredValue::Php(_)) => {
                    self.retain_runtime_value_index(index)?;
                    Ok(Some(encoded))
                }
                Some(
                    NativeStoredValue::ArrayIterator(_)
                    | NativeStoredValue::Iterator(_)
                    | NativeStoredValue::GeneratorIterator(_),
                ) => Ok(None),
                None => Err(format!("native runtime value {index} is missing")),
            };
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

    fn prepared_closure_invocation(
        &self,
        encoded: i64,
    ) -> Option<(
        php_runtime::api::ClosurePayload,
        Option<i64>,
        smallvec::SmallVec<[i64; 8]>,
    )> {
        let index = Self::direct_value_index(encoded)?;
        let NativePreparedCallable::Closure(prepared) = self.direct_prepared_callable(index)?
        else {
            return None;
        };
        if prepared.closure.id != self.direct_value_slots.get(index)?.payload
            || prepared.capture_descriptors.len() != prepared.captures.len()
        {
            return None;
        }
        Some((
            prepared.closure.clone(),
            prepared.implicit_this,
            smallvec::SmallVec::from_slice(&prepared.captures),
        ))
    }

    fn prepared_closure_payload(&self, encoded: i64) -> Option<&php_runtime::api::ClosurePayload> {
        let index = Self::direct_value_index(encoded)?;
        let NativePreparedCallable::Closure(prepared) = self.direct_prepared_callable(index)?
        else {
            return None;
        };
        (prepared.closure.id == self.direct_value_slots.get(index)?.payload
            && prepared.capture_descriptors.len() == prepared.captures.len())
        .then_some(&prepared.closure)
    }

    fn prepared_callable_dispatch(&self, encoded: i64) -> Option<NativePreparedCallableDispatch> {
        let index = Self::direct_value_index(encoded)?;
        match self.direct_prepared_callable(index)? {
            NativePreparedCallable::Closure(_) => Some(NativePreparedCallableDispatch::Closure),
            NativePreparedCallable::UserFunction { name }
            | NativePreparedCallable::InternalBuiltin { name } => {
                Some(NativePreparedCallableDispatch::Named(name.clone()))
            }
            NativePreparedCallable::BoundMethod { target, method, .. } => {
                let target = match target {
                    NativePreparedCallableMethodTarget::Object(object) => {
                        php_runtime::api::CallableMethodTarget::Object(
                            self.native_query_object(*object)?,
                        )
                    }
                    NativePreparedCallableMethodTarget::Class(class) => {
                        php_runtime::api::CallableMethodTarget::Class(class.clone())
                    }
                };
                Some(NativePreparedCallableDispatch::BoundMethod {
                    target,
                    method: method.clone(),
                })
            }
            NativePreparedCallable::MethodPlaceholder { target }
            | NativePreparedCallable::UnresolvedDynamic { target } => {
                Some(NativePreparedCallableDispatch::Invalid(target.clone()))
            }
        }
    }

    /// Move an owned result from the active external unit back to its caller.
    /// Runtime handles already belong to the request-wide arena and need no
    /// clone or replacement slot. Only unit-indexed constants and an unowned
    /// closure require translation.
    fn transfer_external_return(&mut self, encoded: i64, owner_unit: usize) -> Result<i64, String> {
        if let Some(index) = Self::direct_value_index(encoded) {
            if let Some(NativePreparedCallable::Closure(prepared)) =
                self.direct_prepared_callable_mut(index)
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

    fn retain(&mut self, encoded: i64) -> Result<(), String> {
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
        let index = index as usize;
        if self.values.get(index).and_then(Option::as_ref).is_none() {
            return Err(format!("native runtime value {index} is missing"));
        }
        let refcount = &mut self
            .value_slots
            .get_mut(index)
            .ok_or_else(|| format!("native runtime value {index} has no slot"))?
            .refcount;
        *refcount = refcount
            .checked_add(1)
            .ok_or_else(|| format!("native runtime value {index} refcount overflow"))?;
        Ok(())
    }

    fn native_scalar_encoding(&mut self, value: &Value) -> Option<i64> {
        matches!(
            value,
            Value::Null | Value::Bool(_) | Value::Int(_) | Value::Uninitialized
        )
        .then(|| self.encode(value.clone()).ok())
        .flatten()
    }

    /// Classify an encoded PHP value without cloning it out of the request
    /// arena. Immediates are always plain; runtime iterator/control records
    /// are deliberately excluded because they are not PHP local values.
    fn php_handle_is_reference(&self, encoded: i64) -> Option<bool> {
        if let Some(index) = Self::direct_value_index(encoded) {
            return self.direct_value_slots.get(index).and_then(|slot| {
                (slot.refcount != 0).then_some(matches!(
                    slot.kind,
                    php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
                        | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                ))
            });
        }
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Some(false);
        };
        match self.values.get(index as usize).and_then(Option::as_ref) {
            Some(NativeStoredValue::Php(Value::Reference(_))) => Some(true),
            Some(NativeStoredValue::Php(_) | NativeStoredValue::GlobalsProxy) => Some(false),
            Some(
                NativeStoredValue::ArrayIterator(_)
                | NativeStoredValue::Iterator(_)
                | NativeStoredValue::GeneratorIterator(_),
            )
            | None => None,
        }
    }

    /// Borrow a plain PHP local through its existing opaque handle. A local
    /// read owns one reference to its result, so the arena refcount is bumped
    /// instead of decoding, cloning, and allocating an equivalent handle.
    fn retain_plain_php_handle(&mut self, encoded: i64) -> Result<Option<i64>, String> {
        let Some(index) = self.plain_php_storage_index(encoded).flatten() else {
            return Ok(None);
        };
        self.retain_runtime_value_index(index)?;
        Ok(Some(encoded))
    }

    /// Classifies a plain PHP value without repeatedly decoding its arena ID.
    /// `Some(None)` denotes an immediate or immutable constant handle;
    /// `Some(Some(index))` denotes a non-reference PHP arena value.
    fn plain_php_storage_index(&self, encoded: i64) -> Option<Option<usize>> {
        if Self::direct_value_index(encoded).is_some() {
            return None;
        }
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Some(None);
        };
        let index = index as usize;
        match self.values.get(index).and_then(Option::as_ref) {
            Some(NativeStoredValue::Php(Value::Reference(_)))
            | Some(NativeStoredValue::GlobalsProxy)
            | Some(
                NativeStoredValue::ArrayIterator(_)
                | NativeStoredValue::Iterator(_)
                | NativeStoredValue::GeneratorIterator(_),
            )
            | None => None,
            Some(NativeStoredValue::Php(_)) => Some(Some(index)),
        }
    }

    fn borrowed_php_value(&self, encoded: i64) -> Option<&Value> {
        let index = php_jit::jit_decode_runtime_value(encoded)? as usize;
        match self.values.get(index).and_then(Option::as_ref) {
            Some(NativeStoredValue::Php(value)) => Some(value),
            Some(NativeStoredValue::GlobalsProxy) => None,
            Some(
                NativeStoredValue::ArrayIterator(_)
                | NativeStoredValue::Iterator(_)
                | NativeStoredValue::GeneratorIterator(_),
            )
            | None => None,
        }
    }

    /// Copy one native string name for a cold capability lookup without
    /// materializing a PHP `Value`. Symbol tables own Rust strings, so this
    /// allocation is the exact query payload rather than a value-plane
    /// conversion.
    fn native_string_name_bytes(&self, encoded: i64) -> Option<Vec<u8>> {
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
                .get(start..start.checked_add(length)?)
                .map(<[u8]>::to_vec);
        }
        if let Some(index) = php_jit::jit_decode_runtime_value(encoded) {
            return match self.values.get(index as usize)?.as_ref()? {
                NativeStoredValue::Php(Value::String(value)) => Some(value.as_bytes().to_vec()),
                _ => None,
            };
        }
        let constant = php_jit::jit_decode_constant(encoded)?;
        match self.unit.constants.get(constant as usize)? {
            php_ir::IrConstant::String(value) => Some(value.as_bytes().to_vec()),
            php_ir::IrConstant::StringBytes(value) => Some(value.clone()),
            _ => None,
        }
    }

    /// Borrows the stable owner of a direct native object without demoting its
    /// authoritative declared-slot storage or constructing a Rust `Value`.
    fn native_query_object(&self, encoded: i64) -> Option<php_runtime::api::ObjectRef> {
        let encoded = self.dereference_direct_encoding(encoded);
        if let Some(index) = Self::direct_value_index(encoded) {
            return self.direct_object(index);
        }
        let index = php_jit::jit_decode_runtime_value(encoded)? as usize;
        match self.values.get(index)?.as_ref()? {
            NativeStoredValue::Php(Value::Object(object)) => Some(object.clone()),
            _ => None,
        }
    }

    /// Reads one declared property cell from the authoritative native object
    /// representation without materializing the remaining object slots.
    #[allow(unsafe_code)]
    fn native_declared_property_slot(
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
    fn native_declared_property_slot_location(
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
        if descriptor.flags != php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION
            && !self.promote_direct_object_declared_slots(index).ok()?
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

    /// Creates a direct reference whose payload ownership is supplied by the
    /// caller. The cold ReferenceCell is identity-only until an explicit cold
    /// boundary materializes the authoritative payload.
    fn encode_direct_reference_payload_owned(&mut self, payload: i64) -> Result<i64, String> {
        let index = self.reserve_direct_value_slot()?;
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
            flags: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
            reserved: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED,
            payload: payload as u64,
            ..php_jit::JitNativeValueSlot::default()
        };
        self.direct_reference_cells
            .insert(index, php_runtime::api::ReferenceCell::new(Value::Null));
        let Some(runtime_index) = u32::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE))
        else {
            self.direct_reference_cells.remove(&index);
            self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
                reserved: *self.direct_value_free_head,
                ..php_jit::JitNativeValueSlot::default()
            };
            *self.direct_value_free_head =
                u32::try_from(index).unwrap_or(php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE);
            return Err("direct native reference handle overflow".to_owned());
        };
        Ok((php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG | u64::from(runtime_index)) as i64)
    }

    /// Turns one authoritative declared-property cell into a direct reference
    /// without materializing the object or any sibling property. The property
    /// owns one reference handle and the returned handle is an independent
    /// owner for the callee frame.
    #[allow(unsafe_code)]
    fn bind_native_declared_property_reference(
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

    /// Gives a by-value read its own native owner. A compatibility consumer may
    /// temporarily materialize a direct reference into its `ReferenceCell`.
    /// The next native read moves that current value back into the authoritative
    /// direct payload once, so subsequent reads do not repeatedly rebuild an
    /// array/object tree through Rust `Value`.
    fn duplicate_dereferenced_native_value(&mut self, mut encoded: i64) -> Result<i64, String> {
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
                && slot.reserved != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
            {
                encoded = slot.payload as i64;
                continue;
            }
            if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
                && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            {
                let reference = self
                    .direct_reference_cells
                    .get(&index)
                    .cloned()
                    .ok_or_else(|| {
                        format!("materialized direct native reference {index} has no cell")
                    })?;
                let payload = self.encode(reference.get())?;
                self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
                    kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
                    flags: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
                    reserved: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED,
                    payload: payload as u64,
                    ..slot
                };
                encoded = payload;
                continue;
            }
            break;
        }
        if self.php_handle_is_reference(encoded) == Some(true) {
            let mut value = self.decode(encoded)?;
            for _ in 0..16 {
                let Value::Reference(reference) = value else {
                    break;
                };
                value = reference.get();
            }
            return self.encode_baseline_call_value(value);
        }
        if let Some(encoded) = self.duplicate_authoritative_native_value(encoded)? {
            Ok(encoded)
        } else {
            self.duplicate_baseline_call_argument(encoded)
        }
    }

    /// Gives an exact native call an independently owned dereferenced value
    /// without entering `ReferenceCell` or the Rust `Value` plane. `None`
    /// means the caller must take its one baseline continuation before any
    /// PHP-visible call binding effect.
    fn duplicate_authoritative_dereferenced_native_value(
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
                        && slot.reserved != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY =>
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

    /// Materializes a by-value baseline operand without first converting an
    /// authoritative direct reference into the cold `ReferenceCell` plane.
    /// Prepared builtin parameters are by value unless their arginfo says
    /// otherwise, so their ordinary path can decode the published payload
    /// directly and leave reference identity/storage native.
    fn decode_dereferenced_native_value(&mut self, encoded: i64) -> Result<Value, String> {
        let encoded = self.dereference_direct_encoding(encoded);
        let mut value = self.decode(encoded)?;
        for _ in 0..16 {
            let Value::Reference(reference) = value else {
                return Ok(value);
            };
            value = reference.get();
        }
        Ok(value)
    }

    fn direct_reference_payload(&self, encoded: i64) -> Option<i64> {
        let index = Self::direct_value_index(encoded)?;
        let slot = *self.direct_value_slots.get(index)?;
        (slot.refcount != 0
            && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
            && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
            && slot.reserved != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY)
            .then_some(slot.payload as i64)
    }

    fn dereference_direct_encoding(&self, mut encoded: i64) -> i64 {
        for _ in 0..16 {
            let Some(payload) = self.direct_reference_payload(encoded) else {
                break;
            };
            encoded = payload;
        }
        encoded
    }

    fn native_encoded_value_kind(&self, encoded: i64) -> Option<NativeEncodedValueKind> {
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
                php_jit::JIT_NATIVE_VALUE_VIEW_STRING => Some(NativeEncodedValueKind::String),
                php_jit::JIT_NATIVE_VALUE_VIEW_ARRAY
                | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
                | php_jit::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY
                | php_jit::JIT_NATIVE_VALUE_VIEW_BORROWED_REFERENCE_ARRAY => {
                    Some(NativeEncodedValueKind::Array)
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT => Some(NativeEncodedValueKind::Float),
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT => {
                    Some(NativeEncodedValueKind::Object)
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE => {
                    Some(NativeEncodedValueKind::Callable)
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER
                | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_FIBER => {
                    Some(NativeEncodedValueKind::Fiber)
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
                | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR => {
                    Some(NativeEncodedValueKind::Reference)
                }
                _ => None,
            };
        }
        let index = php_jit::jit_decode_runtime_value(encoded)? as usize;
        match self.values.get(index)?.as_ref()? {
            NativeStoredValue::Php(Value::Null) => Some(NativeEncodedValueKind::Null),
            NativeStoredValue::Php(Value::Uninitialized) => {
                Some(NativeEncodedValueKind::Uninitialized)
            }
            NativeStoredValue::Php(Value::Bool(value)) => {
                Some(NativeEncodedValueKind::Bool(*value))
            }
            NativeStoredValue::Php(Value::Int(_)) => Some(NativeEncodedValueKind::Int),
            NativeStoredValue::Php(Value::Float(_)) => Some(NativeEncodedValueKind::Float),
            NativeStoredValue::Php(Value::String(_)) => Some(NativeEncodedValueKind::String),
            NativeStoredValue::Php(Value::Array(_)) => Some(NativeEncodedValueKind::Array),
            NativeStoredValue::Php(Value::Object(_)) => Some(NativeEncodedValueKind::Object),
            NativeStoredValue::Php(Value::Callable(_)) => Some(NativeEncodedValueKind::Callable),
            NativeStoredValue::Php(Value::Resource(_)) => Some(NativeEncodedValueKind::Resource),
            NativeStoredValue::Php(Value::Generator(_)) => Some(NativeEncodedValueKind::Generator),
            NativeStoredValue::Php(Value::Fiber(_)) => Some(NativeEncodedValueKind::Fiber),
            NativeStoredValue::Php(Value::Reference(_)) => Some(NativeEncodedValueKind::Reference),
            NativeStoredValue::GlobalsProxy
            | NativeStoredValue::ArrayIterator(_)
            | NativeStoredValue::Iterator(_)
            | NativeStoredValue::GeneratorIterator(_) => None,
        }
    }

    fn native_encoded_int(&self, encoded: i64) -> Option<i64> {
        let encoded = self.dereference_direct_encoding(encoded);
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
        let index = php_jit::jit_decode_runtime_value(encoded)? as usize;
        match self.values.get(index)?.as_ref()? {
            NativeStoredValue::Php(Value::Int(value)) => Some(*value),
            _ => None,
        }
    }

    fn native_encoded_float(&self, encoded: i64) -> Option<f64> {
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
        let index = php_jit::jit_decode_runtime_value(encoded)? as usize;
        match self.values.get(index)?.as_ref()? {
            NativeStoredValue::Php(Value::Float(value)) => Some(value.to_f64()),
            _ => None,
        }
    }

    fn native_encoded_bool(&self, encoded: i64) -> Option<bool> {
        match self.native_encoded_value_kind(encoded)? {
            NativeEncodedValueKind::Bool(value) => Some(value),
            _ => None,
        }
    }

    fn native_encoded_type_name(&self, encoded: i64) -> &'static str {
        match self.native_encoded_value_kind(encoded) {
            Some(NativeEncodedValueKind::Null) => "null",
            Some(NativeEncodedValueKind::Uninitialized) => "uninitialized",
            Some(NativeEncodedValueKind::Bool(_)) => "bool",
            Some(NativeEncodedValueKind::Int) => "int",
            Some(NativeEncodedValueKind::Float) => "float",
            Some(NativeEncodedValueKind::String) => "string",
            Some(NativeEncodedValueKind::Array) => "array",
            Some(NativeEncodedValueKind::Object) => "object",
            Some(NativeEncodedValueKind::Callable) => "callable",
            Some(NativeEncodedValueKind::Resource) => "resource",
            Some(NativeEncodedValueKind::Generator) => "Generator",
            Some(NativeEncodedValueKind::Fiber) => "Fiber",
            Some(NativeEncodedValueKind::Reference) => "reference",
            None => "unknown",
        }
    }

    fn native_encoded_is_callable(&self, encoded: i64) -> Option<bool> {
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
                let bytes = self.native_string_name_bytes(encoded)?;
                let name = String::from_utf8_lossy(&bytes);
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
                let method = self.native_string_name_bytes(method)?;
                let method = String::from_utf8_lossy(&method);
                if let Some(object) = self.native_query_object(target) {
                    let class = object.class_name();
                    Some(
                        native_method_in_hierarchy(self, &class, &method).is_some()
                            || native_external_method(self, &class, &method).is_some(),
                    )
                } else {
                    let class = self.native_string_name_bytes(target)?;
                    let class = String::from_utf8_lossy(&class);
                    Some(
                        native_method_in_hierarchy(self, &class, &method).is_some()
                            || native_external_method(self, &class, &method).is_some(),
                    )
                }
            }
            _ => Some(false),
        }
    }

    fn native_encoded_matches_ir_type(
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
            Ir::Iterable => Some(matches!(
                kind,
                NativeEncodedValueKind::Array | NativeEncodedValueKind::Object
            )),
            Ir::Object => Some(kind == NativeEncodedValueKind::Object),
            Ir::Bool => Some(matches!(kind, NativeEncodedValueKind::Bool(_))),
            Ir::Null | Ir::Void => Some(kind == NativeEncodedValueKind::Null),
            Ir::Mixed => Some(true),
            Ir::Never => Some(false),
            Ir::False => Some(kind == NativeEncodedValueKind::Bool(false)),
            Ir::True => Some(kind == NativeEncodedValueKind::Bool(true)),
            Ir::Class { name, .. } => Some(
                self.native_query_object(encoded)
                    .is_some_and(|object| native_class_is_a(self, &object.class_name(), name)),
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

    /// Produces one owned native value for a typed by-value call parameter.
    /// `None` denotes a compatibility-only shape which has already crossed a
    /// cold call boundary and still requires the baseline `Value` coercer.
    fn coerce_native_call_argument_encoded(
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
                    .native_string_name_bytes(encoded)
                    .expect("classified native string has bytes");
                String::from_utf8_lossy(&bytes).trim().parse::<i64>().ok()
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
                    .native_string_name_bytes(encoded)
                    .expect("classified native string has bytes");
                if let Ok(value) = String::from_utf8_lossy(&bytes).trim().parse::<f64>() {
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

    /// Replaces an authoritative direct-reference payload. `replacement` is
    /// moved into the reference slot; the previous payload owner is released.
    fn replace_direct_reference_payload_owned(
        &mut self,
        reference: i64,
        replacement: i64,
    ) -> Result<bool, String> {
        let Some(index) = Self::direct_value_index(reference) else {
            return Ok(false);
        };
        let Some(slot) = self.direct_value_slots.get(index).copied().filter(|slot| {
            slot.refcount != 0
                && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
                && slot.reserved != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
        }) else {
            return Ok(false);
        };
        self.direct_value_slots[index].payload = replacement as u64;
        self.direct_value_slots[index].reserved =
            php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED;
        self.release(slot.payload as i64)?;
        Ok(true)
    }

    /// Returns `None` for a shape that needs baseline semantics, otherwise an
    /// exact PHP isset classification without constructing a Rust `Value`.
    fn native_encoded_is_set(&self, encoded: i64) -> Option<bool> {
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
            return (slot.refcount != 0
                && !matches!(
                    slot.kind,
                    php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
                        | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                ))
            .then_some(true);
        }
        let index = php_jit::jit_decode_runtime_value(encoded)? as usize;
        match self.values.get(index)?.as_ref()? {
            NativeStoredValue::Php(Value::Null | Value::Uninitialized) => Some(false),
            NativeStoredValue::Php(Value::Reference(_)) => None,
            NativeStoredValue::Php(_) | NativeStoredValue::GlobalsProxy => Some(true),
            NativeStoredValue::ArrayIterator(_)
            | NativeStoredValue::Iterator(_)
            | NativeStoredValue::GeneratorIterator(_) => None,
        }
    }

    /// Exact native truthiness for scalar/string/array common shapes. Objects
    /// and materialized compatibility references remain baseline because
    /// SimpleXML and user-visible reference state require cold semantics.
    fn native_encoded_truthy(&self, encoded: i64) -> Option<bool> {
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
                php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT => Some(f64::from_bits(slot.payload) != 0.0),
                php_jit::JIT_NATIVE_VALUE_VIEW_STRING => Some(
                    slot.payload != 0 && slot.reserved & php_jit::JIT_NATIVE_STRING_VALUE_ZERO == 0,
                ),
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY => Some(slot.payload != 0),
                php_jit::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY
                | php_jit::JIT_NATIVE_VALUE_VIEW_BORROWED_REFERENCE_ARRAY => {
                    php_runtime::api::PhpArray::clone_from_native_storage_refcount(
                        slot.payload as usize,
                    )
                    .map(|array| !array.is_empty())
                }
                php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
                | php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
                | php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR => None,
                _ => Some(true),
            };
        }
        let index = php_jit::jit_decode_runtime_value(encoded)? as usize;
        match self.values.get(index)?.as_ref()? {
            NativeStoredValue::Php(Value::Object(_) | Value::Reference(_)) => None,
            NativeStoredValue::Php(value) => Some(native_property_truthy(value)),
            NativeStoredValue::GlobalsProxy
            | NativeStoredValue::ArrayIterator(_)
            | NativeStoredValue::Iterator(_)
            | NativeStoredValue::GeneratorIterator(_) => None,
        }
    }

    /// Outer `None` means a non-direct shape; inner `None` means a valid
    /// direct traversal whose key is absent.
    fn direct_dimension_path_encoded(
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

    fn php_handle_is_uninitialized(&self, encoded: i64) -> bool {
        if php_jit::jit_decode_constant(encoded) == Some(php_jit::JIT_VALUE_UNINITIALIZED) {
            return true;
        }
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return false;
        };
        if Self::direct_value_index(encoded).is_some() {
            return false;
        }
        matches!(
            self.values.get(index as usize).and_then(Option::as_ref),
            Some(NativeStoredValue::Php(Value::Uninitialized))
        )
    }

    fn retain_runtime_value_index(&mut self, index: usize) -> Result<(), String> {
        let refcount = &mut self
            .value_slots
            .get_mut(index)
            .ok_or_else(|| format!("native runtime value {index} has no slot"))?
            .refcount;
        *refcount = refcount
            .checked_add(1)
            .ok_or_else(|| format!("native runtime value {index} refcount overflow"))?;
        Ok(())
    }

    fn replace_plain_php_handle(&mut self, current: i64, value: i64) -> Result<Option<()>, String> {
        let Some(current_index) = self.plain_php_storage_index(current) else {
            return Ok(None);
        };
        let Some(value_index) = self.plain_php_storage_index(value) else {
            return Ok(None);
        };
        if let Some(index) = value_index {
            self.retain_runtime_value_index(index)?;
        }
        if let Some(index) = current_index {
            self.release_runtime_value_index(index)?;
        }
        Ok(Some(()))
    }

    fn release(&mut self, encoded: i64) -> Result<(), String> {
        if let Some(index) = Self::direct_value_index(encoded) {
            return self.release_direct_value_index(index);
        }
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Ok(());
        };
        self.release_runtime_value_index(index as usize)
    }

    fn release_direct_value_index(&mut self, index: usize) -> Result<(), String> {
        let reached_zero = {
            let slot = self
                .direct_value_slots
                .get_mut(index)
                .ok_or_else(|| format!("direct native value {index} is missing"))?;
            if slot.refcount == 0 {
                return Err(format!("direct native value {index} was already released"));
            }
            slot.refcount -= 1;
            slot.refcount == 0
        };
        if !reached_zero {
            return Ok(());
        }
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
                self.demote_direct_object_declared_slots(index)?;
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
            if self.direct_object_handles.get(&object.id()) == Some(&(index as u32)) {
                self.direct_object_handles.remove(&object.id());
            }
            Some(object)
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
                unsafe { Box::from_raw(slot.aux as usize as *mut NativePreparedCallable) };
            if let NativePreparedCallable::Closure(closure) = callable.as_ref()
                && self.direct_closure_handles.get(&closure.closure.id) == Some(&(index as u32))
            {
                self.direct_closure_handles.remove(&closure.closure.id);
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
            self.direct_fiber_handles
                .retain(|_, mapped| *mapped as usize != index);
            self.direct_fiber_cells.remove(&index);
            Some(fiber)
        } else {
            None
        };
        let released_fiber_execution = released_fiber
            .as_ref()
            .and_then(|_| self.fiber_executions.remove(&(index as u64)));
        if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY
            && !php_runtime::api::PhpArray::release_native_storage_refcount(slot.payload as usize)
        {
            return Err(format!(
                "shared native array {index} storage was already released"
            ));
        }
        let freed_string_range = if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_STRING {
            if let Some(key) = self.direct_string_keys.remove(&index)
                && self.direct_string_handles.get(&key) == Some(&(index as u32))
            {
                self.direct_string_handles.remove(&key);
            }
            let base = self.direct_string_bytes.as_ptr() as usize;
            let address = usize::try_from(slot.aux).unwrap_or(base);
            let start = address.saturating_sub(base);
            let capacity = php_jit::jit_native_direct_string_capacity(slot.reserved) as usize;
            (capacity != 0).then_some((start, capacity))
        } else {
            None
        };
        let (mut children, freed_array_range) =
            if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FOREACH {
                (vec![slot.payload as i64], None)
            } else if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY {
                if let Some(storage_version) = self.direct_array_storage_ids.remove(&index)
                    && self.direct_array_handles.get(&storage_version) == Some(&(index as u32))
                {
                    self.direct_array_handles.remove(&storage_version);
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
                && slot.reserved != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
            {
                (vec![slot.payload as i64], None)
            } else {
                (Vec::new(), None)
            };
        if let Some(callable) = released_callable {
            match callable.as_ref() {
                NativePreparedCallable::Closure(closure) => {
                    children.extend(closure.implicit_this);
                    children.extend(closure.captures.iter().copied());
                }
                NativePreparedCallable::BoundMethod {
                    target: NativePreparedCallableMethodTarget::Object(object),
                    ..
                } => children.push(*object),
                _ => {}
            }
        }
        if let Some(fiber) = released_fiber {
            children.push(fiber.callable);
            children.extend(fiber.return_value);
        }
        children.extend(direct_object_children);
        self.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            reserved: *self.direct_value_free_head,
            ..php_jit::JitNativeValueSlot::default()
        };
        self.direct_array_states[index] = php_jit::JitNativeDirectArrayState::default();
        *self.direct_value_free_head = index as u32;
        self.direct_reference_cells.remove(&index);
        if let Some((start, capacity)) = freed_array_range {
            self.free_direct_array_entries(start, capacity);
        }
        if let Some((start, capacity)) = freed_string_range {
            self.free_direct_string_bytes(start, capacity);
        }
        for child in children {
            self.release(child)?;
        }
        if let Some(execution) = released_fiber_execution {
            self.abandon_native_fiber_execution(execution)?;
        }
        if let Some(object) = released_object {
            let class_name = object.class_name();
            if self.object_has_native_destructor(&class_name) {
                let uniquely_owned = object.gc_refcount_estimate() == 1;
                if uniquely_owned {
                    self.record_object_release_root_check(true);
                }
                if uniquely_owned || !self.object_is_request_rooted(object.id()) {
                    self.run_object_destructor(object)?;
                }
            }
        }
        Ok(())
    }

    fn release_runtime_value_index(&mut self, index: usize) -> Result<(), String> {
        let reached_zero = {
            let refcount = &mut self
                .value_slots
                .get_mut(index)
                .ok_or_else(|| format!("native runtime value {index} has no slot"))?
                .refcount;
            if *refcount == 0 {
                return Err(format!("native runtime value {index} was already released"));
            }
            *refcount -= 1;
            *refcount == 0
        };
        if reached_zero {
            self.record_release_to_zero();
            let value = self
                .values
                .get_mut(index)
                .ok_or_else(|| format!("native runtime value {index} is missing"))?
                .take();
            if let Some(identity) = value.as_ref().and_then(stored_value_identity)
                && self.interned_value_handles.get(&identity) == Some(&(index as u32))
            {
                self.interned_value_handles.remove(&identity);
            }
            self.value_slots[index] = php_jit::JitNativeValueSlot::default();
            match value {
                Some(NativeStoredValue::Php(Value::Object(object))) => {
                    let class_name = object.class_name();
                    // Root membership is observable only for objects whose class
                    // can run user code during destruction. Scanning the complete
                    // request graph for ordinary objects cannot change PHP output
                    // and made every short-lived WordPress value pay for the
                    // largest live global array.
                    if self.object_has_native_destructor(&class_name) {
                        let uniquely_owned = object.gc_refcount_estimate() == 1;
                        if uniquely_owned {
                            self.record_object_release_root_check(true);
                        }
                        if uniquely_owned || !self.object_is_request_rooted(object.id()) {
                            self.run_object_destructor(object)?;
                        }
                    }
                }
                _ => {}
            }
            self.free_value_slots.push(index as u32);
        }
        Ok(())
    }

    fn release_if_live(&mut self, encoded: i64) -> Result<(), String> {
        if let Some(index) = Self::direct_value_index(encoded) {
            if self.direct_value_slots[index].refcount == 0 {
                return Ok(());
            }
            return self.release_direct_value_index(index);
        }
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return Ok(());
        };
        if self
            .value_slots
            .get(index as usize)
            .is_some_and(|slot| slot.refcount == 0)
        {
            return Ok(());
        }
        self.release(encoded)
    }

    fn object_is_request_rooted(&mut self, object_id: u64) -> bool {
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

    fn live_native_values_contain_object(&self, object_id: u64) -> bool {
        let cold_contains = self.values.iter().flatten().any(|stored| match stored {
            NativeStoredValue::Php(value) => values_contain_object([value], object_id),
            NativeStoredValue::GlobalsProxy => false,
            NativeStoredValue::ArrayIterator(iterator) => {
                values_contain_object(iterator.source.iter().map(|(_, value)| value), object_id)
            }
            NativeStoredValue::Iterator(iterator) => {
                values_contain_object(
                    iterator
                        .entries
                        .iter()
                        .flat_map(|(key, value)| [key, value]),
                    object_id,
                ) || iterator
                    .live_object
                    .as_ref()
                    .is_some_and(|object| object.id() == object_id)
                    || iterator
                        .user_iterator
                        .as_ref()
                        .is_some_and(|object| object.id() == object_id)
            }
            NativeStoredValue::GeneratorIterator(iterator) => iterator
                .delegation
                .as_ref()
                .is_some_and(|delegation| match delegation {
                    NativeGeneratorDelegation::Array { entries, .. } => values_contain_object(
                        entries.iter().flat_map(|(key, value)| [key, value]),
                        object_id,
                    ),
                    NativeGeneratorDelegation::Generator { .. } => false,
                }),
        });
        if cold_contains {
            return true;
        }
        let mut visited = std::collections::HashSet::new();
        let used = usize::try_from(*self.direct_value_next).unwrap_or(0);
        (0..used).any(|index| {
            self.direct_value_slots
                .get(index)
                .is_some_and(|slot| slot.refcount != 0)
                && self.direct_slot_contains_object(index, object_id, &mut visited)
        })
    }

    fn direct_slot_contains_object(
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
                if slot.flags != php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION {
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
                if address < base || (address - base) % entry_size != 0 {
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
            php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE => self
                .direct_prepared_callable(index)
                .is_some_and(|callable| match callable {
                    NativePreparedCallable::Closure(closure) => {
                        closure.implicit_this.is_some_and(|value| {
                            self.encoded_value_contains_object(value, object_id, visited)
                        }) || closure.captures.iter().copied().any(|value| {
                            self.encoded_value_contains_object(value, object_id, visited)
                        })
                    }
                    NativePreparedCallable::BoundMethod {
                        target: NativePreparedCallableMethodTarget::Object(object),
                        ..
                    } => self.encoded_value_contains_object(*object, object_id, visited),
                    _ => false,
                }),
            php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FIBER
            | php_jit::JIT_NATIVE_VALUE_VIEW_MATERIALIZED_FIBER => {
                let native_contains = self.fiber_record(index).is_some_and(|fiber| {
                    self.encoded_value_contains_object(fiber.callable, object_id, visited)
                        || fiber.return_value.is_some_and(|value| {
                            self.encoded_value_contains_object(value, object_id, visited)
                        })
                });
                native_contains
                    || self.direct_fiber_cells.get(&index).is_some_and(|fiber| {
                        let callable = fiber.callable();
                        values_contain_object([&callable], object_id)
                            || fiber
                                .return_value()
                                .is_some_and(|value| values_contain_object([&value], object_id))
                    })
            }
            php_jit::JIT_NATIVE_VALUE_VIEW_SHARED_ARRAY
            | php_jit::JIT_NATIVE_VALUE_VIEW_BORROWED_REFERENCE_ARRAY => {
                php_runtime::api::PhpArray::clone_from_native_storage_refcount(
                    slot.payload as usize,
                )
                .is_some_and(|array| {
                    values_contain_object(array.iter().map(|(_, value)| value), object_id)
                })
            }
            _ => false,
        }
    }

    fn encoded_value_contains_object(
        &self,
        encoded: i64,
        object_id: u64,
        visited: &mut std::collections::HashSet<usize>,
    ) -> bool {
        if let Some(index) = Self::direct_value_index(encoded) {
            return self.direct_slot_contains_object(index, object_id, visited);
        }
        self.borrowed_php_value(encoded)
            .is_some_and(|value| values_contain_object([value], object_id))
    }

    fn run_object_destructor(&mut self, object: php_runtime::api::ObjectRef) -> Result<(), String> {
        if self
            .destroyed_objects
            .get(&object.id())
            .is_some_and(WeakObjectHandle::is_alive)
        {
            return Ok(());
        }
        self.destroyed_objects
            .insert(object.id(), object.weak_handle());
        let class_name = object.class_name();
        let receiver = self.encode_native_object_owner(object)?;
        if let Some(function) = self
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
            .map(|method| method.function)
        {
            let _ = invoke_native_method(self, function, &[receiver])?;
        } else if let Some((function, _)) = native_external_method(self, &class_name, "__destruct")
        {
            let _ = invoke_native_external_function(
                self,
                function,
                &[receiver],
                Some(class_name),
                self.unit.strict_types,
            )?;
        }
        self.release(receiver)
    }

    fn object_has_native_destructor(&self, class_name: &str) -> bool {
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

    fn function_id(&self, name: &str) -> Option<php_ir::FunctionId> {
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

    fn visible_include_function_names(&self) -> Rc<NativeFunctionNameScope> {
        self.visible_function_names.clone()
    }

    fn publish_function_names(&mut self, names: impl IntoIterator<Item = String>) {
        self.visible_function_names =
            NativeFunctionNameScope::child(self.visible_function_names.clone(), names);
    }

    fn demote_all_direct_objects(&mut self) -> Result<(), String> {
        let native_objects = (0..usize::try_from(*self.direct_value_next).unwrap_or(0))
            .filter_map(|index| {
                self.direct_value_slots
                    .get(index)
                    .is_some_and(|slot| {
                        slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
                            && slot.flags == php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION
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

    fn take_include_symbols(&mut self) -> Result<NativeIncludeSymbols, String> {
        self.demote_trusted_static_properties();
        // Include/eval hands Rust-owned request state to a separately owned
        // native arena. No ObjectRef crossing that ownership boundary may
        // retain declared-property slots encoded against this arena.
        self.demote_all_direct_objects()?;
        self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
        Ok(NativeIncludeSymbols {
            deployment_functions: std::sync::Arc::clone(&self.deployment_functions),
            deployment_classes: std::sync::Arc::clone(&self.deployment_classes),
            external_functions: std::mem::take(&mut self.external_functions),
            external_class_units: std::mem::take(&mut self.external_class_units),
            external_signature_epoch: self.external_signature_epoch,
            dynamic_units: std::mem::take(&mut self.dynamic_units),
            dynamic_classes: std::mem::take(&mut self.dynamic_classes),
            class_aliases: std::mem::take(&mut self.class_aliases),
            autoload_callbacks: std::mem::take(&mut self.autoload_callbacks),
            shutdown_callbacks: std::mem::take(&mut self.shutdown_callbacks),
            static_properties: std::mem::take(&mut self.static_properties),
            static_locals: std::mem::take(&mut self.static_locals),
            enum_cases: std::mem::take(&mut self.enum_cases),
            destroyed_objects: std::mem::take(&mut self.destroyed_objects),
            error_reporting: Some(self.error_reporting),
            display_errors: Some(self.display_errors),
            error_handlers: std::mem::take(&mut self.error_handlers),
            exception_handlers: std::mem::take(&mut self.exception_handlers),
            last_error: self.last_error.take(),
        })
    }

    fn restore_include_symbols(&mut self, symbols: NativeIncludeSymbols) {
        self.deployment_functions = symbols.deployment_functions;
        self.deployment_classes = symbols.deployment_classes;
        self.external_functions = symbols.external_functions;
        self.external_class_units = symbols.external_class_units;
        self.external_signature_epoch = symbols.external_signature_epoch;
        self.dynamic_units = symbols.dynamic_units;
        self.dynamic_classes = symbols.dynamic_classes;
        self.class_aliases = symbols.class_aliases;
        self.autoload_callbacks = symbols.autoload_callbacks;
        self.shutdown_callbacks = symbols.shutdown_callbacks;
        self.static_properties = symbols.static_properties;
        self.static_locals = symbols.static_locals;
        self.enum_cases = symbols.enum_cases;
        self.destroyed_objects = symbols.destroyed_objects;
        if let Some(error_reporting) = symbols.error_reporting {
            self.error_reporting = error_reporting;
        }
        if let Some(display_errors) = symbols.display_errors {
            self.display_errors = display_errors;
        }
        self.error_handlers = symbols.error_handlers;
        self.exception_handlers = symbols.exception_handlers;
        self.last_error = symbols.last_error;
        self.prepare_trusted_static_properties();
        self.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
    }

    fn external_function(&self, name: &str) -> Option<NativeDynamicFunction> {
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

    fn can_invoke_external_in_place(&self, target: NativeDynamicFunction) -> bool {
        self.dynamic_units.get(target.unit).is_some_and(|package| {
            package
                .compiled
                .unit()
                .functions
                .get(target.function.index())
                .is_some()
        })
    }

    fn with_active_dynamic_unit<R>(
        &mut self,
        unit: usize,
        operation: impl FnOnce(&mut Self) -> R,
    ) -> Result<R, String> {
        let compiled = self
            .dynamic_units
            .get(unit)
            .map(|package| package.compiled.clone())
            .ok_or_else(|| "dynamic native unit is missing".to_owned())?;
        let active_entries = std::mem::take(
            &mut self
                .dynamic_units
                .get_mut(unit)
                .expect("dynamic native unit was already validated")
                .native_entries,
        );
        let previous_compiled = std::mem::replace(&mut self.compiled, compiled.clone());
        let previous_unit = std::mem::replace(&mut self.unit, ActiveNativeUnit::new(&compiled));
        let previous_identity =
            std::mem::replace(&mut self.unit_identity, compiled.artifact_identity());
        let previous_entries = std::mem::replace(&mut self.native_entries, active_entries);
        let active_continuations = compiled.prepared_continuation_instructions();
        let (active_property_offsets, active_property_slots) =
            trusted_property_storage(&active_continuations);
        let (active_request_local_offsets, active_request_local_slots) =
            trusted_request_local_storage(compiled.unit());
        let active_constant_slots =
            vec![php_jit::JitNativeTrustedConstantSlot::default(); active_property_slots.len()];
        let active_global_reference_slots = vec![
            php_jit::JitNativeTrustedGlobalReferenceSlot::default();
            active_property_slots.len()
        ];
        let active_global_reference_names =
            (0..active_property_slots.len()).map(|_| None).collect();
        let active_static_local_slots =
            vec![php_jit::JitNativeTrustedStaticLocalSlot::default(); active_property_slots.len()];
        let active_static_property_slots = vec![
            php_jit::JitNativeTrustedStaticPropertySlot::default(
            );
            active_property_slots.len()
        ];
        let active_instanceof_plans =
            vec![php_jit::JitNativeInstanceOfPlan::default(); active_property_slots.len()];
        let previous_continuations =
            std::mem::replace(&mut self.continuation_instructions, active_continuations);
        let previous_property_offsets = std::mem::replace(
            &mut self.trusted_property_function_offsets,
            active_property_offsets,
        );
        let previous_property_slots =
            std::mem::replace(&mut self.trusted_property_slots, active_property_slots);
        let previous_request_local_offsets = std::mem::replace(
            &mut self.trusted_request_local_function_offsets,
            active_request_local_offsets,
        );
        let previous_request_local_slots = std::mem::replace(
            &mut self.trusted_request_local_slots,
            active_request_local_slots,
        );
        let previous_constant_slots =
            std::mem::replace(&mut self.trusted_constant_slots, active_constant_slots);
        let previous_global_reference_slots = std::mem::replace(
            &mut self.trusted_global_reference_slots,
            active_global_reference_slots,
        );
        let previous_global_reference_names = std::mem::replace(
            &mut self.trusted_global_reference_names,
            active_global_reference_names,
        );
        let previous_static_local_slots = std::mem::replace(
            &mut self.trusted_static_local_slots,
            active_static_local_slots,
        );
        let previous_static_property_slots = std::mem::replace(
            &mut self.trusted_static_property_slots,
            active_static_property_slots,
        );
        let previous_instanceof_plans =
            std::mem::replace(&mut self.trusted_instanceof_plans, active_instanceof_plans);
        let previous_instanceof_entries = std::mem::take(&mut self.trusted_instanceof_entries);
        let previous_callsites = std::mem::replace(
            &mut self.native_callsites,
            compiled.prepared_native_callsites(),
        );
        let previous_class_plans = std::mem::take(&mut self.trusted_class_plans);
        let previous_dynamic_unit = self.current_dynamic_unit.replace(unit);
        self.prepare_trusted_static_properties();
        self.prepare_trusted_constant_fetches();
        self.prepare_trusted_request_locals();
        self.prepare_trusted_global_references();
        self.prepare_trusted_static_locals();
        self.prepare_trusted_class_plans();
        self.prepare_trusted_declared_properties();
        self.prepare_trusted_instanceof_plans();

        // Native code in an included/eval unit uses that unit's dense trusted
        // function-cell table. The outer request activation describes the
        // root deployment; refresh the by-value runtime view for the scoped
        // unit before constructing any nested JitDeoptState. Without this,
        // FunctionId N from an include indexed root FunctionId N and could
        // indirect-call arbitrary data as an address.
        let _runtime_view = activate_native_context(self);
        let result = operation(self);

        let active_entries = std::mem::replace(&mut self.native_entries, previous_entries);
        self.dynamic_units
            .get_mut(unit)
            .expect("active dynamic native unit disappeared")
            .native_entries = active_entries;
        self.clear_trusted_constant_fetches();
        self.clear_trusted_request_locals();
        self.clear_trusted_global_references();
        self.clear_trusted_static_locals();
        self.current_dynamic_unit = previous_dynamic_unit;
        self.native_callsites = previous_callsites;
        self.continuation_instructions = previous_continuations;
        self.trusted_property_function_offsets = previous_property_offsets;
        self.trusted_property_slots = previous_property_slots;
        self.trusted_request_local_function_offsets = previous_request_local_offsets;
        self.trusted_request_local_slots = previous_request_local_slots;
        self.trusted_constant_slots = previous_constant_slots;
        self.trusted_global_reference_slots = previous_global_reference_slots;
        self.trusted_global_reference_names = previous_global_reference_names;
        self.trusted_static_local_slots = previous_static_local_slots;
        self.trusted_static_property_slots = previous_static_property_slots;
        self.trusted_instanceof_plans = previous_instanceof_plans;
        self.trusted_instanceof_entries = previous_instanceof_entries;
        self.trusted_class_plans = previous_class_plans;
        self.unit_identity = previous_identity;
        self.unit = previous_unit;
        self.compiled = previous_compiled;
        Ok(result)
    }

    fn direct_array_slot(&self, encoded: i64) -> Option<(usize, php_jit::JitNativeValueSlot)> {
        let index = Self::direct_value_index(encoded)?;
        let slot = *self.direct_value_slots.get(index)?;
        (slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY)
            .then_some((index, slot))
    }

    /// Resolves a direct-reference chain to its authoritative direct array.
    /// The returned encoding owns no additional reference; callers either
    /// borrow entries or retain the selected child explicitly.
    fn direct_array_encoding(&self, encoded: i64) -> Option<i64> {
        let encoded = self.dereference_direct_encoding(encoded);
        self.direct_array_slot(encoded).map(|_| encoded)
    }

    fn direct_array_entries_for(
        &self,
        encoded: i64,
    ) -> Option<&[php_jit::JitNativeDirectArrayEntry]> {
        let encoded = self.dereference_direct_encoding(encoded);
        let (_, slot) = self.direct_array_slot(encoded)?;
        let length = usize::try_from(slot.payload).ok()?;
        let base = self.direct_array_entries.as_ptr() as usize;
        let address = usize::try_from(slot.aux).ok()?;
        let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
        let offset = address.checked_sub(base)?;
        (offset % entry_size == 0).then_some(())?;
        let start = offset / entry_size;
        self.direct_array_entries
            .get(start..start.checked_add(length)?)
    }

    /// Rewrites unit-indexed constants embedded in a native array tree to
    /// request-owned native values exactly once before the tree crosses an
    /// IR-unit boundary.  The array slots remain the authoritative storage;
    /// this deliberately does not decode the tree to `PhpArray` or allocate
    /// a second direct-array facade.
    fn stabilize_direct_array_for_cross_unit(&mut self, encoded: i64) -> Result<(), String> {
        let mut pending = vec![encoded];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(array) = pending.pop() {
            let Some((index, slot)) = self.direct_array_slot(array) else {
                continue;
            };
            if !visited.insert(index) {
                continue;
            }
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
                let key = self.stabilize_cross_unit_value(entry.key)?;
                let value = self.stabilize_cross_unit_value(entry.value)?;
                self.direct_array_entries[entry_index] =
                    php_jit::JitNativeDirectArrayEntry { key, value };
                if Self::direct_value_index(key).is_some() {
                    pending.push(key);
                }
                if Self::direct_value_index(value).is_some() {
                    pending.push(value);
                }
            }
        }
        Ok(())
    }

    fn stabilize_cross_unit_value(&mut self, encoded: i64) -> Result<i64, String> {
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
        let value = self.decode(encoded)?;
        self.encode(value)
    }

    fn direct_array_length(&self, encoded: i64) -> Option<usize> {
        self.direct_array_entries_for(encoded).map(<[_]>::len)
    }

    fn direct_array_is_unique(&self, encoded: i64) -> Option<bool> {
        self.direct_array_slot(encoded)
            .map(|(_, slot)| slot.refcount == 1)
    }

    fn direct_array_can_append(&self, encoded: i64) -> Option<bool> {
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

    fn fresh_direct_array_next_append_key(
        &self,
        entries: &[php_jit::JitNativeDirectArrayEntry],
    ) -> Option<i64> {
        entries
            .iter()
            .filter_map(|entry| self.native_encoded_int(entry.key))
            .map(|key| key.saturating_add(1))
            .max()
    }

    fn direct_array_find_encoded(
        &mut self,
        encoded: i64,
        key: &php_runtime::api::ArrayKey,
    ) -> Result<Option<i64>, String> {
        let Some(entries) = self.direct_array_entries_for(encoded).map(<[_]>::to_vec) else {
            return Err("native value is not a direct array".to_owned());
        };
        for entry in entries {
            if self.native_encoded_matches_array_key(entry.key, key) {
                return Ok(Some(entry.value));
            }
        }
        Ok(None)
    }

    /// Binds one entry of an authoritative direct array as a PHP reference.
    ///
    /// The direct array remains the only array representation: its entry owns
    /// one reference handle and the returned handle is an independent owner
    /// for the callee. A shared array is deliberately rejected here because
    /// its COW replacement must also update the containing lvalue.
    fn bind_native_direct_array_element_reference(
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

    /// Publishes a newly produced native array whose entry handles are already
    /// individually owned by the caller. Ownership moves into the resulting
    /// slot; no Rust `PhpArray` or duplicate value tree is constructed.
    #[track_caller]
    fn publish_owned_direct_array_entries(
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
    fn clone_direct_array_handle(&mut self, encoded: i64) -> Result<i64, String> {
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
        let index = if *self.direct_value_free_head != php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE {
            let index = *self.direct_value_free_head as usize;
            let slot = self
                .direct_value_slots
                .get(index)
                .ok_or_else(|| "direct native value free-list entry is missing".to_owned())?;
            *self.direct_value_free_head = slot.reserved;
            *self.direct_value_reused_bytes = self
                .direct_value_reused_bytes
                .saturating_add(std::mem::size_of::<php_jit::JitNativeValueSlot>() as u64);
            index
        } else {
            let index = usize::try_from(*self.direct_value_next)
                .map_err(|_| "direct native value index overflow".to_owned())?;
            if index >= self.direct_value_slots.len() {
                for child in retained {
                    let _ = self.release(child);
                }
                self.free_direct_array_entries(start, capacity);
                return Err(format!(
                    "direct native value arena exhausted at {} slots",
                    index.saturating_add(1)
                ));
            }
            *self.direct_value_next = u32::try_from(index + 1)
                .map_err(|_| "direct native value index overflow".to_owned())?;
            index
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

    fn direct_array_insert_encoded(
        &mut self,
        encoded: i64,
        key: Option<&php_runtime::api::ArrayKey>,
        value: i64,
    ) -> Result<(), String> {
        let (array_index, mut slot) = self
            .direct_array_slot(encoded)
            .ok_or_else(|| "native value is not a direct array".to_owned())?;
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
                self.release(replacement)?;
                // A materialized compatibility reference is an explicit cold
                // array shape and keeps the complete ReferenceCell semantics.
                if let Value::Reference(reference) = self.decode(previous)? {
                    reference.set(self.decode(value)?);
                    return Ok(());
                }
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

    fn publish_direct_object_slots(
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
        let direct_object = self.dereference_direct_encoding(object);
        let direct_index = Self::direct_value_index(direct_object).filter(|index| {
            self.direct_value_slots.get(*index).is_some_and(|slot| {
                slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT
            })
        });
        let mut temporary_owner = None;
        let index = if let Some(index) = direct_index {
            index
        } else {
            // A baseline continuation may have materialized a direct reference
            // receiver before publishing this exact property site. Resolve the
            // referenced object once here; never reinterpret that reference's
            // slot index as an object descriptor.
            let mut value = self.decode(object)?;
            for _ in 0..16 {
                let Value::Reference(reference) = value else {
                    break;
                };
                value = reference.get();
            }
            let Value::Object(object) = value else {
                return Ok(());
            };
            let encoded = self.encode_native_object_owner(object)?;
            let index = Self::direct_value_index(encoded)
                .ok_or_else(|| "published property receiver is not direct".to_owned())?;
            temporary_owner = Some(encoded);
            index
        };
        let published = (|| {
            if !self.promote_direct_object_declared_slots(index)? {
                return Ok(());
            }
            let object = self
                .direct_object(index)
                .ok_or_else(|| format!("direct native object {index} has no stable owner"))?;
            let Some(slot_index) = object.declared_slot_index(property) else {
                return Ok(());
            };
            let function = usize::try_from(function as u64 as u32)
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
            };
            Ok(())
        })();
        if let Some(encoded) = temporary_owner {
            let released = self.release(encoded);
            published.and(released)
        } else {
            published
        }
    }

    fn mutate_array(
        &mut self,
        encoded: i64,
        mutate: impl FnOnce(&mut php_runtime::api::PhpArray),
    ) -> Result<(), String> {
        self.mutate_array_with(encoded, mutate)
    }

    fn mutate_array_with<T>(
        &mut self,
        encoded: i64,
        mutate: impl FnOnce(&mut php_runtime::api::PhpArray) -> T,
    ) -> Result<T, String> {
        if encoded as u64 & php_jit::JIT_VALUE_RUNTIME_KIND_MASK
            == php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG
        {
            if let Some(index) = Self::direct_value_index(encoded) {
                let slot = self
                    .direct_value_slots
                    .get(index)
                    .copied()
                    .filter(|slot| slot.refcount != 0)
                    .ok_or_else(|| format!("direct native reference {index} is missing"))?;
                if slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
                    && slot.flags == php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
                    && slot.reserved != php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_EMPTY
                {
                    // The direct payload, not the cold ReferenceCell sidecar,
                    // is authoritative until materialization. Mutate that
                    // payload in place so foreach-by-reference preserves both
                    // the reference identity and the array's native handle.
                    return self.mutate_array_with(slot.payload as i64, mutate);
                }
            }
            if let Value::Reference(reference) = self.decode(encoded)? {
                let mut value = reference.get();
                let Value::Array(array) = &mut value else {
                    return Err("native reference does not contain an array".to_owned());
                };
                let result = mutate(array);
                reference.set(value);
                return Ok(result);
            }
            return Err("native reference handle is unavailable".to_owned());
        }
        if let Some(index) = Self::direct_value_index(encoded) {
            let Value::Array(mut array) = self.decode_direct_array(index)? else {
                return Err("direct native value is not an array".to_owned());
            };
            let result = mutate(&mut array);
            self.replace_direct_array(index, array)?;
            return Ok(result);
        }
        let index = php_jit::jit_decode_runtime_value(encoded)
            .and_then(|index| usize::try_from(index).ok())
            .ok_or_else(|| "native value is not an array handle".to_owned())?;
        let array = match self.values.get_mut(index).and_then(Option::as_mut) {
            Some(NativeStoredValue::Php(Value::Array(array))) => array,
            _ => return Err("native value is not an array or array reference".to_owned()),
        };
        Ok(mutate(array))
    }

    fn encode_iterator(
        &mut self,
        entries: Vec<(Value, Value)>,
        live_source: Option<i64>,
        live_global: Option<String>,
        live_object: Option<php_runtime::api::ObjectRef>,
        user_iterator: Option<php_runtime::api::ObjectRef>,
    ) -> Result<i64, String> {
        self.encode_stored_value(NativeStoredValue::Iterator(Box::new(NativeIteratorState {
            entries,
            index: 0,
            live_source,
            live_global,
            live_object,
            user_iterator,
            user_iterator_started: false,
        })))
    }

    fn encode_array_iterator(&mut self, source: php_runtime::api::PhpArray) -> Result<i64, String> {
        // A by-value foreach over an immutable COW snapshot can publish all
        // non-reference entries once. Ordinary loop iterations then advance a
        // request-owned ABI cursor without crossing back into Rust. Reference
        // elements remain on the semantic helper path because their value is
        // intentionally observed at each iteration.
        let snapshot = source
            .iter()
            .map(|(key, value)| {
                let key = match key {
                    php_runtime::api::ArrayKey::Int(key) => Value::Int(key),
                    php_runtime::api::ArrayKey::String(key) => Value::String(key.clone()),
                };
                (key, value.clone())
            })
            .collect::<Vec<_>>();
        let direct = if snapshot
            .iter()
            .any(|(_, value)| matches!(value, Value::Reference(_)))
        {
            None
        } else {
            let mut entries = Vec::with_capacity(snapshot.len());
            for (key, value) in snapshot {
                let key = match self.encode(key) {
                    Ok(key) => key,
                    Err(error) => {
                        self.release_direct_foreach_entries(&entries);
                        return Err(error);
                    }
                };
                let value = match self.encode(value) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = self.release(key);
                        self.release_direct_foreach_entries(&entries);
                        return Err(error);
                    }
                };
                entries.push(php_jit::JitNativeForeachEntry { key, value });
            }
            let entries = entries.into_boxed_slice();
            let view = Box::new(php_jit::JitNativeForeachView {
                cursor: 0,
                length: entries.len() as u64,
                entries: entries.as_ptr() as usize as u64,
            });
            Some(Box::new(NativeDirectForeachState { view, entries }))
        };
        self.encode_stored_value(NativeStoredValue::ArrayIterator(Box::new(
            NativeArrayIteratorState {
                source,
                index: 0,
                direct,
            },
        )))
    }

    fn release_direct_foreach_entries(&mut self, entries: &[php_jit::JitNativeForeachEntry]) {
        for entry in entries {
            for encoded in [entry.key, entry.value] {
                let _ = self.release(encoded);
            }
        }
    }

    fn encode_generator_iterator(
        &mut self,
        generator: php_runtime::api::GeneratorRef,
    ) -> Result<i64, String> {
        let function = php_ir::FunctionId::new(generator.function());
        let handle = ensure_native_entry(self, function)?;
        let arguments = generator
            .args()
            .into_iter()
            .map(|value| self.encode(value))
            .collect::<Result<Vec<_>, _>>()?;
        self.encode_stored_value(NativeStoredValue::GeneratorIterator(Box::new(
            NativeGeneratorIteratorState {
                generator,
                handle: Box::new(handle),
                arguments,
                state: Box::new(None),
                delegation: None,
                yields_seen: 0,
                finished: false,
            },
        )))
    }

    fn generator_iterator(
        &mut self,
        generator: php_runtime::api::GeneratorRef,
    ) -> Result<i64, String> {
        if let Some(encoded) = self.generator_iterators.get(&generator.id()).copied() {
            return Ok(encoded);
        }
        let id = generator.id();
        let encoded = self.encode_generator_iterator(generator)?;
        self.generator_iterators.insert(id, encoded);
        Ok(encoded)
    }

    fn generator_resume(
        &mut self,
        encoded: i64,
        resume_kind: php_jit::JitNativeResumeInputKind,
        resume_value: i64,
    ) -> Result<Option<(Value, Value)>, String> {
        let index = php_jit::jit_decode_runtime_value(encoded)
            .ok_or_else(|| "native value is not a foreach iterator handle".to_owned())?;
        let user_iterator = match self.values.get(index as usize).and_then(Option::as_ref) {
            Some(NativeStoredValue::Iterator(iterator)) => iterator
                .user_iterator
                .as_ref()
                .map(|object| (object.clone(), iterator.user_iterator_started)),
            _ => None,
        };
        if let Some((object, started)) = user_iterator {
            let class_name = object.class_name();
            let receiver = self.encode_native_object_owner(object)?;
            if started {
                let next = native_method_in_hierarchy(self, &class_name, "next")
                    .ok_or_else(|| "Iterator::next() is missing".to_owned())?;
                invoke_native_method(self, next, &[receiver])?;
            }
            let valid = native_method_in_hierarchy(self, &class_name, "valid")
                .ok_or_else(|| "Iterator::valid() is missing".to_owned())?;
            let valid = invoke_native_method(self, valid, &[receiver])?;
            if !native_property_truthy(&self.decode(valid)?) {
                return Ok(None);
            }
            let current = native_method_in_hierarchy(self, &class_name, "current")
                .ok_or_else(|| "Iterator::current() is missing".to_owned())?;
            let key = native_method_in_hierarchy(self, &class_name, "key")
                .ok_or_else(|| "Iterator::key() is missing".to_owned())?;
            let current = invoke_native_method(self, current, &[receiver])?;
            let key = invoke_native_method(self, key, &[receiver])?;
            if let Some(NativeStoredValue::Iterator(iterator)) =
                self.values.get_mut(index as usize).and_then(Option::as_mut)
            {
                iterator.user_iterator_started = true;
            }
            return Ok(Some((self.decode(key)?, self.decode(current)?)));
        }
        let object_entry = match self.values.get(index as usize).and_then(Option::as_ref) {
            Some(NativeStoredValue::Iterator(iterator)) => {
                iterator.live_object.as_ref().and_then(|object| {
                    iterator
                        .entries
                        .get(iterator.index)
                        .map(|(key, _)| (object.clone(), key.clone(), iterator.index))
                })
            }
            _ => None,
        };
        if let Some((object, key, cursor)) = object_entry {
            let name = match &key {
                Value::Int(key) => key.to_string(),
                Value::String(key) => key.to_string_lossy(),
                _ => return Err("native object iterator key is invalid".to_owned()),
            };
            let value = object.get_property(&name).unwrap_or(Value::Null);
            let value = match value {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            if let Some(NativeStoredValue::Iterator(iterator)) =
                self.values.get_mut(index as usize).and_then(Option::as_mut)
            {
                iterator.index = cursor.saturating_add(1);
            }
            return Ok(Some((key, value)));
        }
        let live = match self.values.get(index as usize).and_then(Option::as_ref) {
            Some(NativeStoredValue::Iterator(iterator)) => iterator
                .live_source
                .map(|source| (source, iterator.index, iterator.live_global.clone())),
            _ => None,
        };
        if let Some((source, cursor, live_global)) = live {
            let reference_entry = |array: &mut php_runtime::api::PhpArray| {
                let (key, value) = array
                    .iter()
                    .nth(cursor)
                    .map(|(key, value)| (key.clone(), value.clone()))?;
                let reference = match value {
                    Value::Reference(reference) => reference,
                    value => {
                        let reference = php_runtime::api::ReferenceCell::new(value);
                        array.insert(key.clone(), Value::Reference(reference.clone()));
                        reference
                    }
                };
                let key = match key {
                    php_runtime::api::ArrayKey::Int(key) => Value::Int(key),
                    php_runtime::api::ArrayKey::String(key) => Value::String(key),
                };
                Some((key, Value::Reference(reference)))
            };
            let entry = if let Some(global) = live_global {
                let Some(root) = self.inherited_globals.get(&global).cloned() else {
                    return Ok(None);
                };
                match root {
                    Value::Reference(reference) => {
                        let Value::Array(mut array) = reference.get() else {
                            return Ok(None);
                        };
                        let entry = reference_entry(&mut array);
                        reference.set(Value::Array(array));
                        entry
                    }
                    Value::Array(mut array) => {
                        let entry = reference_entry(&mut array);
                        self.inherited_globals.insert(global, Value::Array(array));
                        entry
                    }
                    _ => None,
                }
            } else {
                self.mutate_array_with(source, reference_entry)?
            };
            let Some(entry) = entry else {
                return Ok(None);
            };
            if let Some(NativeStoredValue::Iterator(iterator)) =
                self.values.get_mut(index as usize).and_then(Option::as_mut)
            {
                iterator.index = iterator.index.saturating_add(1);
            }
            return Ok(Some(entry));
        }
        if let Some(NativeStoredValue::Iterator(iterator)) =
            self.values.get_mut(index as usize).and_then(Option::as_mut)
        {
            let entry = iterator
                .entries
                .get(iterator.index)
                .cloned()
                .map(|(key, value)| {
                    let value = match value {
                        Value::Reference(reference) => reference.get(),
                        value => value,
                    };
                    (key, value)
                });
            iterator.index = iterator.index.saturating_add(usize::from(entry.is_some()));
            return Ok(entry);
        }
        let (generator, handle, arguments, state, delegation, finished) =
            match self.values.get(index as usize).and_then(Option::as_ref) {
                Some(NativeStoredValue::GeneratorIterator(iterator)) => (
                    iterator.generator.clone(),
                    iterator.handle.clone(),
                    iterator.arguments.clone(),
                    iterator.state.clone(),
                    iterator.delegation.clone(),
                    iterator.finished,
                ),
                _ => return Err(format!("native foreach iterator {index} is missing")),
            };
        if finished {
            return Ok(None);
        }
        let mut effective_resume_kind = resume_kind;
        let mut effective_resume_value = resume_value;
        if let Some(delegation) = delegation {
            match delegation {
                NativeGeneratorDelegation::Array {
                    entries,
                    index: cursor,
                } => {
                    if let Some((key, value)) = entries.get(cursor).cloned() {
                        if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                            self.values.get_mut(index as usize).and_then(Option::as_mut)
                            && let Some(NativeGeneratorDelegation::Array {
                                index: saved_cursor,
                                ..
                            }) = iterator.delegation.as_mut()
                        {
                            *saved_cursor = saved_cursor.saturating_add(1);
                        }
                        generator.suspend_forwarded(Some(key.clone()), value.clone());
                        if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                            self.values.get_mut(index as usize).and_then(Option::as_mut)
                        {
                            iterator.yields_seen = iterator.yields_seen.saturating_add(1);
                        }
                        return Ok(Some((key, value)));
                    }
                    if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                        self.values.get_mut(index as usize).and_then(Option::as_mut)
                    {
                        iterator.delegation = None;
                    }
                    effective_resume_kind = php_jit::JitNativeResumeInputKind::VALUE;
                    effective_resume_value = php_jit::jit_encode_constant(u32::MAX);
                }
                NativeGeneratorDelegation::Generator {
                    generator: delegated,
                    iterator,
                } => {
                    if let Some((key, value)) = self.iterator_next(iterator)? {
                        generator.suspend_forwarded(Some(key.clone()), value.clone());
                        if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                            self.values.get_mut(index as usize).and_then(Option::as_mut)
                        {
                            iterator.yields_seen = iterator.yields_seen.saturating_add(1);
                        }
                        return Ok(Some((key, value)));
                    }
                    effective_resume_kind = php_jit::JitNativeResumeInputKind::VALUE;
                    effective_resume_value =
                        self.encode(delegated.return_value().unwrap_or(Value::Null))?;
                    if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                        self.values.get_mut(index as usize).and_then(Option::as_mut)
                    {
                        iterator.delegation = None;
                    }
                }
            }
        }
        let outcome = if let Some(state) = state.as_ref() {
            let runtime = self.native_runtime_ptr();
            handle.invoke_i64_suspension_resume_with_native_unwind_runtime(
                &arguments,
                state,
                effective_resume_kind,
                effective_resume_value,
                php_jit::JIT_RUNTIME_ABI_HASH,
                runtime,
                |types, value| native_catch_matches(self, types, value),
            )
        } else {
            let runtime = self.native_runtime_ptr();
            handle.invoke_i64_with_deopt_runtime(&arguments, php_jit::JIT_RUNTIME_ABI_HASH, runtime)
        }
        .map_err(|error| format!("native generator invocation failed: {error:?}"))?;
        match outcome {
            php_jit::JitI64InvokeOutcome::SideExit {
                status,
                value,
                state,
            } if status == php_jit::JitCallStatus::SUSPEND_GENERATOR.0 as i32 => {
                if state.suspend_kind == php_jit::JitNativeSuspendKind::GENERATOR_DELEGATE.0 {
                    let delegated = self.decode(state.delegation_handle as i64)?;
                    let delegation = match delegated {
                        Value::Array(array) => NativeGeneratorDelegation::Array {
                            entries: array
                                .iter()
                                .map(|(key, value)| {
                                    let key = match key {
                                        php_runtime::api::ArrayKey::Int(value) => Value::Int(value),
                                        php_runtime::api::ArrayKey::String(value) => {
                                            Value::String(value.clone())
                                        }
                                    };
                                    (key, value.clone())
                                })
                                .collect(),
                            index: 0,
                        },
                        Value::Generator(delegated) => NativeGeneratorDelegation::Generator {
                            iterator: self.generator_iterator(delegated.clone())?,
                            generator: delegated,
                        },
                        other => {
                            return Err(format!(
                                "yield from expects an array or Traversable, got {}",
                                native_value_type_name(&other)
                            ));
                        }
                    };
                    if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                        self.values.get_mut(index as usize).and_then(Option::as_mut)
                    {
                        *iterator.state = Some(state);
                        iterator.delegation = Some(delegation);
                    }
                    return self.iterator_next(encoded);
                }
                let key = if state.suspend_flags & 1 != 0 {
                    Some(self.decode(state.yielded_key)?)
                } else {
                    None
                };
                let value = self.decode(value)?;
                generator.suspend(key, value.clone());
                if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                    self.values.get_mut(index as usize).and_then(Option::as_mut)
                {
                    *iterator.state = Some(state);
                }
                if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                    self.values.get_mut(index as usize).and_then(Option::as_mut)
                {
                    iterator.yields_seen = iterator.yields_seen.saturating_add(1);
                }
                let (key, value) = generator
                    .current()
                    .ok_or_else(|| "native generator suspension value is missing".to_owned())?;
                Ok(Some((key.unwrap_or(Value::Null), value)))
            }
            php_jit::JitI64InvokeOutcome::Returned(value)
            | php_jit::JitI64InvokeOutcome::SideExit {
                status: 1 | 2,
                value,
                ..
            } => {
                generator.close(Some(self.decode(value)?));
                if let Some(NativeStoredValue::GeneratorIterator(iterator)) =
                    self.values.get_mut(index as usize).and_then(Option::as_mut)
                {
                    iterator.finished = true;
                }
                Ok(None)
            }
            php_jit::JitI64InvokeOutcome::SideExit { status, .. } => {
                Err(format!("native generator returned status {status}"))
            }
        }
    }

    fn iterator_next(&mut self, encoded: i64) -> Result<Option<(Value, Value)>, String> {
        if let Some(entry) = self.array_iterator_next(encoded) {
            return Ok(entry);
        }
        self.generator_resume(
            encoded,
            php_jit::JitNativeResumeInputKind::VALUE,
            php_jit::jit_encode_constant(u32::MAX),
        )
    }

    fn iterator_next_encoded(&mut self, encoded: i64) -> Result<Option<(i64, i64)>, String> {
        if let Some(index) = Self::direct_value_index(encoded) {
            let iterator = *self
                .direct_value_slots
                .get(index)
                .ok_or_else(|| "direct foreach iterator slot is missing".to_owned())?;
            if iterator.refcount != 0
                && iterator.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FOREACH
            {
                let cursor = usize::try_from(iterator.aux)
                    .map_err(|_| "direct foreach cursor is invalid".to_owned())?;
                let length = iterator.reserved as usize;
                if cursor >= length {
                    return Ok(None);
                }
                let source_index = Self::direct_value_index(iterator.payload as i64)
                    .ok_or_else(|| "direct foreach source handle is invalid".to_owned())?;
                let source = *self
                    .direct_value_slots
                    .get(source_index)
                    .ok_or_else(|| "direct foreach source slot is missing".to_owned())?;
                let base = self.direct_array_entries.as_ptr() as usize;
                let address = usize::try_from(source.aux)
                    .map_err(|_| "direct foreach entry address is invalid".to_owned())?;
                let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
                let start = address
                    .checked_sub(base)
                    .map(|offset| offset / entry_size)
                    .ok_or_else(|| "direct foreach entry range is invalid".to_owned())?;
                let entry = *self
                    .direct_array_entries
                    .get(start.saturating_add(cursor))
                    .ok_or_else(|| "direct foreach entry is missing".to_owned())?;
                let key = self
                    .duplicate_authoritative_native_value(entry.key)?
                    .ok_or_else(|| "direct foreach key is not authoritative".to_owned())?;
                let value =
                    match self.duplicate_authoritative_dereferenced_native_value(entry.value) {
                        Ok(Some(value)) => value,
                        Ok(None) => match self.duplicate_dereferenced_native_value(entry.value) {
                            Ok(value) => value,
                            Err(error) => {
                                self.release(key)?;
                                return Err(error);
                            }
                        },
                        Err(error) => {
                            self.release(key)?;
                            return Err(error);
                        }
                    };
                self.direct_value_slots[index].aux = iterator.aux.saturating_add(1);
                return Ok(Some((key, value)));
            }
        }
        self.iterator_next(encoded)?
            .map(|(key, value)| {
                let key = self.encode(key)?;
                match self.encode(value) {
                    Ok(value) => Ok((key, value)),
                    Err(error) => {
                        self.release(key)?;
                        Err(error)
                    }
                }
            })
            .transpose()
    }

    fn array_iterator_next(&mut self, encoded: i64) -> Option<Option<(Value, Value)>> {
        if let Some(index) = Self::direct_value_index(encoded) {
            let iterator = *self.direct_value_slots.get(index)?;
            if iterator.refcount == 0
                || iterator.kind != php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_FOREACH
            {
                return None;
            }
            let cursor = usize::try_from(iterator.aux).ok()?;
            let length = iterator.reserved as usize;
            if cursor >= length {
                return Some(None);
            }
            let source = Self::direct_value_index(iterator.payload as i64)?;
            let source = *self.direct_value_slots.get(source)?;
            let base = self.direct_array_entries.as_ptr() as usize;
            let address = usize::try_from(source.aux).ok()?;
            let entry_size = std::mem::size_of::<php_jit::JitNativeDirectArrayEntry>();
            let start = address.checked_sub(base)? / entry_size;
            let entry = *self.direct_array_entries.get(start.checked_add(cursor)?)?;
            self.direct_value_slots[index].aux = iterator.aux.saturating_add(1);
            let key = self.decode(entry.key).ok()?;
            let value = self.decode(entry.value).ok()?;
            return Some(Some((key, value)));
        }
        let index = php_jit::jit_decode_runtime_value(encoded)? as usize;
        let NativeStoredValue::ArrayIterator(iterator) = self.values.get_mut(index)?.as_mut()?
        else {
            return None;
        };
        Some(
            iterator
                .source
                .next_pair_at_cursor(&mut iterator.index)
                .map(|(key, value)| {
                    let key = match key {
                        php_runtime::api::ArrayKey::Int(key) => Value::Int(key),
                        php_runtime::api::ArrayKey::String(key) => Value::String(key),
                    };
                    let value = match value {
                        Value::Reference(reference) => reference.get(),
                        value => value,
                    };
                    (key, value)
                }),
        )
    }

    fn generator_can_rewind(&self, encoded: i64) -> bool {
        let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
            return false;
        };
        self.values
            .get(index as usize)
            .and_then(Option::as_ref)
            .is_some_and(|value| match value {
                NativeStoredValue::GeneratorIterator(iterator) => {
                    matches!(iterator.yields_seen, 0 | 1) && !iterator.finished
                }
                _ => false,
            })
    }

    fn close_iterator(&mut self, encoded: i64) -> Result<(), String> {
        if let Some(index) = Self::direct_value_index(encoded) {
            return self.release_direct_value_index(index);
        }
        let index = php_jit::jit_decode_runtime_value(encoded)
            .ok_or_else(|| "native value is not a foreach iterator handle".to_owned())?;
        let value = self
            .values
            .get_mut(index as usize)
            .ok_or_else(|| format!("native foreach iterator {index} is missing"))?;
        match value.take() {
            Some(NativeStoredValue::ArrayIterator(iterator)) => {
                if let Some(slot) = self.value_slots.get_mut(index as usize) {
                    *slot = php_jit::JitNativeValueSlot::default();
                }
                if let Some(direct) = iterator.direct.as_ref() {
                    let entries = direct.entries.to_vec();
                    drop(iterator);
                    self.release_direct_foreach_entries(&entries);
                }
                Ok(())
            }
            Some(NativeStoredValue::Iterator(_) | NativeStoredValue::GeneratorIterator(_)) => {
                if let Some(slot) = self.value_slots.get_mut(index as usize) {
                    *slot = php_jit::JitNativeValueSlot::default();
                }
                Ok(())
            }
            other => {
                *value = other;
                Err(format!("native foreach iterator {index} is missing"))
            }
        }
    }

    fn instruction_for_continuation(
        &self,
        function: u32,
        continuation: u32,
    ) -> Option<NativeInstructionPtr> {
        self.continuation_instructions
            .get(function as usize)
            .and_then(|instructions| instructions.get(continuation as usize))
            .and_then(Option::as_ref)
            .map(|instruction| NativeInstructionPtr(std::sync::Arc::as_ptr(instruction)))
    }

    pub(super) fn instruction_kind_debug(&self, function: u32, continuation: u32) -> String {
        self.instruction_for_continuation(function, continuation)
            .map(|instruction| format!("{:?}", instruction.kind))
            .unwrap_or_else(|| "<missing continuation>".to_owned())
    }

    fn prepared_native_callsite(
        &self,
        function: u32,
        continuation: u32,
    ) -> Option<*const crate::compiled_unit::NativeCallSiteDescriptor> {
        self.native_callsites
            .get(function as usize)
            .and_then(|callsites| callsites.get(continuation as usize))
            .and_then(Option::as_ref)
            .map(std::sync::Arc::as_ptr)
    }

    fn deferred_function_argument_requires_reference(
        &self,
        function: u32,
        continuation: u32,
        argument: usize,
    ) -> Option<bool> {
        let descriptor = self
            .native_callsites
            .get(function as usize)
            .and_then(|callsites| callsites.get(continuation as usize))
            .and_then(Option::as_deref)?;
        if !matches!(
            descriptor.kind,
            crate::compiled_unit::NativeCallSiteKind::Function
        ) {
            return None;
        }
        let name = descriptor.target_symbol.as_deref()?;
        let parameters = if let Some(function) = self.function_id(name) {
            self.unit
                .functions
                .get(function.index())
                .map(|function| function.params.as_slice())
        } else if let Some(target) = self.external_function(name) {
            self.dynamic_units
                .get(target.unit)
                .and_then(|unit| unit.compiled.unit().functions.get(target.function.index()))
                .map(|function| function.params.as_slice())
        } else {
            None
        }?;
        call_dispatch::native_function_argument_requires_reference_at(
            descriptor.arguments.as_ref(),
            parameters,
            argument,
        )
    }

    fn native_method_epochs(&self) -> (u64, u64) {
        let dynamic_epoch = self.dynamic_units.len() as u64;
        (
            self.unit_identity ^ dynamic_epoch.rotate_left(17),
            self.unit_identity.rotate_left(29) ^ dynamic_epoch,
        )
    }

    fn lookup_native_method_pic(
        &self,
        descriptor: &crate::compiled_unit::NativeCallSiteDescriptor,
        receiver_class: &str,
        method: &str,
    ) -> Option<NativeMethodPicTarget> {
        let (class_layout_epoch, method_table_epoch) = self.native_method_epochs();
        if let Some((function, is_static)) = descriptor.lookup_method_pic(
            receiver_class,
            method,
            class_layout_epoch,
            method_table_epoch,
        ) {
            return Some(NativeMethodPicTarget::CurrentUnit {
                function,
                is_static,
            });
        }
        let pic = self.native_method_pics.get(&descriptor.pic_slot)?;
        if pic.megamorphic {
            return None;
        }
        pic.entries
            .iter()
            .find(|entry| {
                entry.receiver_class.eq_ignore_ascii_case(receiver_class)
                    && entry.method.eq_ignore_ascii_case(method)
                    && entry.class_layout_epoch == class_layout_epoch
                    && entry.method_table_epoch == method_table_epoch
            })
            .map(|entry| entry.target)
    }

    fn install_native_method_pic(
        &mut self,
        descriptor: &crate::compiled_unit::NativeCallSiteDescriptor,
        receiver_class: &str,
        method: &str,
        target: NativeMethodPicTarget,
    ) -> bool {
        let (class_layout_epoch, method_table_epoch) = self.native_method_epochs();
        if let NativeMethodPicTarget::CurrentUnit {
            function,
            is_static,
        } = target
        {
            return descriptor.install_method_pic(
                receiver_class,
                method,
                class_layout_epoch,
                method_table_epoch,
                function,
                is_static,
            );
        }
        let pic = self
            .native_method_pics
            .entry(descriptor.pic_slot)
            .or_default();
        if pic.megamorphic {
            return false;
        }
        if pic.entries.iter().any(|entry| {
            entry.receiver_class.eq_ignore_ascii_case(receiver_class)
                && entry.method.eq_ignore_ascii_case(method)
                && entry.class_layout_epoch == class_layout_epoch
                && entry.method_table_epoch == method_table_epoch
        }) {
            return true;
        }
        if pic.entries.len() >= NATIVE_METHOD_PIC_LIMIT {
            pic.entries.clear();
            pic.megamorphic = true;
            return false;
        }
        pic.entries.push(NativeMethodPicEntry {
            receiver_class: std::sync::Arc::from(receiver_class),
            method: std::sync::Arc::from(method),
            class_layout_epoch,
            method_table_epoch,
            target,
        });
        true
    }

    fn lookup_constant(&self, name: &str) -> Result<Value, String> {
        if let Some(value) = self.dynamic_constants.get(name) {
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

    fn visible_include_constants(&self) -> std::collections::BTreeMap<String, Value> {
        let mut constants = self.dynamic_constants.clone();
        for entry in &self.unit.constant_table {
            if let Some(value) = self.unit.constants.get(entry.value.index())
                && let Ok(value) = ir_constant_value(value)
            {
                constants.entry(entry.name.clone()).or_insert(value);
            }
        }
        constants
    }

    pub(super) fn decode_result(&mut self, encoded: i64) -> Result<Value, String> {
        self.decode(encoded)
    }

    fn record_last_error(&mut self, error_type: i64, message: &str, file: &str, line: usize) {
        self.last_error = Some(NativeLastError {
            error_type,
            message: message.to_owned(),
            file: file.to_owned(),
            line,
        });
    }

    fn last_error_value(&self) -> Value {
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

    pub(super) fn take_pending_throwable(&mut self) -> Option<Value> {
        let throwable = self.pending_throwable.take();
        if throwable.is_some() {
            self.mark_roots_dirty(RootMutationReason::PendingThrowable);
        }
        throwable
    }

    pub(super) fn run_shutdown_callbacks(&mut self) -> Result<(), String> {
        if self.include_child {
            return Ok(());
        }
        while !self.shutdown_callbacks.is_empty() {
            let NativeShutdownCallback {
                callable,
                arguments,
                source,
            } = self.shutdown_callbacks.remove(0);
            self.mark_roots_dirty(RootMutationReason::CallbackOrHandler);
            let result = invoke_native_callable_value(self, callable, &arguments, &source, None);
            if matches!(&result, Err(error) if error == "E_PHP_RETHROW")
                && let Some(throwable) = self.take_pending_throwable()
            {
                self.pending_throwable = Some(native_throwable_with_internal_frame(
                    self, throwable, &source,
                ));
                self.mark_roots_dirty(RootMutationReason::PendingThrowable);
            }
            result?;
        }
        loop {
            let mut objects = Vec::new();
            let mut seen = std::collections::BTreeSet::new();
            let used = usize::try_from(*self.direct_value_next).unwrap_or(0);
            for index in 0..used {
                let Some(object) = self.direct_object(index) else {
                    continue;
                };
                if !self
                    .destroyed_objects
                    .get(&object.id())
                    .is_some_and(WeakObjectHandle::is_alive)
                    && seen.insert(object.id())
                {
                    objects.push(object);
                }
            }
            for stored in &self.values {
                let Some(NativeStoredValue::Php(Value::Object(object))) = stored else {
                    continue;
                };
                if !self
                    .destroyed_objects
                    .get(&object.id())
                    .is_some_and(WeakObjectHandle::is_alive)
                    && seen.insert(object.id())
                {
                    objects.push(object.clone());
                }
            }
            let Some(object) = objects.pop() else {
                break;
            };
            self.destroyed_objects
                .insert(object.id(), object.weak_handle());
            let class_name = object.class_name();
            let receiver = self.encode_native_object_owner(object)?;
            if let Some(function) = self
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
                .map(|method| method.function)
            {
                let _ = invoke_native_method(self, function, &[receiver])?;
            } else if let Some((function, _)) =
                native_external_method(self, &class_name, "__destruct")
            {
                let _ = invoke_native_external_function(
                    self,
                    function,
                    &[receiver],
                    Some(class_name),
                    self.unit.strict_types,
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn handle_uncaught_throwable(&mut self, encoded: i64) -> Result<bool, String> {
        let Some(handler) = self.exception_handlers.last().cloned() else {
            return Ok(false);
        };
        let throwable = self.decode(encoded)?;
        let source = self
            .unit
            .functions
            .get(self.unit.entry.index())
            .and_then(|function| {
                function
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .next()
            })
            .cloned()
            .ok_or_else(|| "exception handler call source is missing".to_owned())?;
        let _ = invoke_native_callable_value(self, handler, &[throwable], &source, None)?;
        Ok(true)
    }

    pub(super) fn publish_include_globals(&mut self) -> Result<(), String> {
        if self.include_child {
            self.materialize_native_request_globals()?;
            let entry_file = self
                .unit
                .functions
                .get(self.unit.entry.index())
                .map(|function| function.span.file);
            NATIVE_INCLUDE_GLOBALS.with(|globals| {
                globals.replace(Some(std::mem::take(&mut self.inherited_globals)));
            });
            NATIVE_INCLUDE_INI.with(|ini| {
                ini.replace(Some(std::mem::take(&mut self.ini_registry)));
            });
            NATIVE_INCLUDE_DEFAULT_TIMEZONE.with(|timezone| {
                timezone.replace(Some(std::mem::take(&mut self.default_timezone)));
            });
            NATIVE_INCLUDE_HTTP_RESPONSE.with(|response| {
                response.replace(Some(std::mem::take(&mut self.http_response)));
            });
            NATIVE_INCLUDE_FILES.with(|files| {
                files.replace(Some(std::mem::take(&mut self.included_files)));
            });
            NATIVE_INCLUDE_MYSQL.with(|mysql| {
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
            NATIVE_INCLUDE_CONSTANTS.with(|constants| {
                constants.replace(Some(std::mem::take(&mut self.dynamic_constants)));
            });
            let autoload_callbacks = self
                .autoload_callbacks
                .split_off(self.inherited_autoload_callback_count);
            let shutdown_callbacks = self
                .shutdown_callbacks
                .split_off(self.inherited_shutdown_callback_count);
            let native_entry_signature_hashes = self
                .native_entries
                .keys()
                .copied()
                .map(|function| {
                    let signatures =
                        visible_external_function_signatures(self, &self.compiled, function);
                    (
                        function,
                        super::external_function_signatures_hash(&signatures),
                    )
                })
                .collect();
            let mut symbols = self.take_include_symbols()?;
            for class in &classes {
                let class = normalize_class_name(class);
                symbols.dynamic_classes.remove(&class);
                symbols.external_class_units.remove(&class);
            }
            NATIVE_INCLUDE_SYMBOLS.with(|slot| {
                slot.replace(Some(symbols));
            });
            NATIVE_INCLUDE_EXPORTS.with(|exports| {
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

pub(super) struct NativeRequestActivationGuard {
    _runtime_view: php_jit::JitNativeRuntimeViewGuard,
    fast_state: *mut NativeRequestFastState,
    previous_header: php_jit::JitNativeFastStateHeader,
}

impl Drop for NativeRequestActivationGuard {
    fn drop(&mut self) {
        // SAFETY: the request owner keeps the separately allocated fast state
        // stable for the complete synchronous activation. Nested unit
        // activations overwrite only this request-owned header and unwind in
        // strict stack order, so restoring the captured header returns direct
        // callees to the outer unit's dense publication tables.
        #[allow(unsafe_code)]
        unsafe {
            (*self.fast_state).header = self.previous_header;
        }
    }
}

fn rooted_membership_may_change(previous: &Value, replacement: &Value) -> bool {
    match (previous, replacement) {
        (Value::Object(lhs), Value::Object(rhs)) => lhs.id() != rhs.id(),
        (Value::Array(lhs), Value::Array(rhs)) => lhs.gc_debug_id() != rhs.gc_debug_id(),
        (Value::Reference(lhs), Value::Reference(rhs)) => !lhs.ptr_eq(rhs),
        (
            Value::Object(_) | Value::Array(_) | Value::Reference(_),
            Value::Object(_) | Value::Array(_) | Value::Reference(_),
        ) => true,
        (Value::Object(_) | Value::Array(_) | Value::Reference(_), _) => true,
        (_, Value::Object(_) | Value::Array(_) | Value::Reference(_)) => true,
        _ => false,
    }
}

pub(super) fn activate_native_context(
    context: &mut NativeRequestColdState<'_>,
) -> NativeRequestActivationGuard {
    let deployment = context.compiled.prepared_deployment_image();
    let view = php_jit::JitNativeRuntimeView {
        abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
        value_slot_capacity: u32::try_from(context.value_slots.capacity()).unwrap_or(u32::MAX),
        value_slots: context.value_slots.as_mut_ptr() as usize as u64,
        direct_value_slots: context.direct_value_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(context.direct_value_next.as_mut()) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(context.direct_value_free_head.as_mut()) as usize
            as u64,
        direct_value_reused_bytes: std::ptr::from_mut(context.direct_value_reused_bytes.as_mut())
            as usize as u64,
        direct_object_owners: context.direct_object_owners.as_mut_ptr() as usize as u64,
        direct_array_states: context.direct_array_states.as_mut_ptr() as usize as u64,
        direct_array_entries: context.direct_array_entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(context.direct_array_next.as_mut()) as usize as u64,
        direct_array_free_heads: context.direct_array_free_heads.as_mut_ptr() as usize as u64,
        direct_array_reused_bytes: std::ptr::from_mut(context.direct_array_reused_bytes.as_mut())
            as usize as u64,
        direct_string_bytes: context.direct_string_bytes.as_mut_ptr() as usize as u64,
        direct_string_next: std::ptr::from_mut(context.direct_string_next.as_mut()) as usize as u64,
        direct_string_free_heads: context.direct_string_free_heads.as_mut_ptr() as usize as u64,
        direct_string_reused_bytes: std::ptr::from_mut(context.direct_string_reused_bytes.as_mut())
            as usize as u64,
        trusted_globals_proxy: context.trusted_globals_proxy,
        trusted_request_local_function_offsets: context
            .trusted_request_local_function_offsets
            .as_ptr() as usize as u64,
        trusted_request_local_function_count: u32::try_from(
            context.trusted_request_local_function_offsets.len(),
        )
        .unwrap_or(u32::MAX),
        trusted_request_local_reserved: 0,
        trusted_request_local_slots: context.trusted_request_local_slots.as_ptr() as usize as u64,
        trusted_request_local_slot_count: u32::try_from(context.trusted_request_local_slots.len())
            .unwrap_or(u32::MAX),
        trusted_request_local_slot_reserved: 0,
        trusted_constant_views: deployment.constant_views.as_ptr() as usize as u64,
        trusted_constant_view_count: u32::try_from(deployment.constant_views.len())
            .unwrap_or(u32::MAX),
        trusted_constant_view_reserved: 0,
        trusted_constant_slots: context.trusted_constant_slots.as_mut_ptr() as usize as u64,
        trusted_constant_slot_count: u32::try_from(context.trusted_constant_slots.len())
            .unwrap_or(u32::MAX),
        trusted_constant_slot_reserved: 0,
        trusted_class_plans: context.trusted_class_plans.as_ptr() as usize as u64,
        trusted_class_plan_count: u32::try_from(context.trusted_class_plans.len())
            .unwrap_or(u32::MAX),
        trusted_class_plan_reserved: 0,
        trusted_function_entries: deployment.native_function_entries.as_ptr() as usize as u64,
        trusted_function_entry_count: u32::try_from(deployment.native_function_entries.len())
            .unwrap_or(u32::MAX),
        trusted_function_entry_reserved: 0,
        trusted_optimizing_function_entries: deployment.optimizing_function_entries.as_ptr()
            as usize as u64,
        trusted_optimizing_function_entry_count: u32::try_from(
            deployment.optimizing_function_entries.len(),
        )
        .unwrap_or(u32::MAX),
        trusted_optimizing_function_entry_reserved: 0,
        fiber_suspension_states: context.fiber_suspension_states.as_mut_ptr() as usize as u64,
        fiber_suspension_next: std::ptr::from_mut(context.fiber_suspension_next.as_mut()) as usize
            as u64,
        fiber_suspension_capacity: u32::try_from(context.fiber_suspension_states.capacity())
            .unwrap_or(u32::MAX),
        fiber_suspension_reserved: 0,
        poll_counter: std::ptr::from_mut(context.native_poll_counter.as_mut()) as usize as u64,
        root_mutation_pending: std::ptr::from_mut(context.native_root_mutation_pending.as_mut())
            as usize as u64,
        trusted_property_function_offsets: context.trusted_property_function_offsets.as_ptr()
            as usize as u64,
        trusted_property_function_count: u32::try_from(
            context.trusted_property_function_offsets.len(),
        )
        .unwrap_or(u32::MAX),
        trusted_property_reserved: 0,
        trusted_property_slots: context.trusted_property_slots.as_mut_ptr() as usize as u64,
        trusted_property_slot_count: u32::try_from(context.trusted_property_slots.len())
            .unwrap_or(u32::MAX),
        trusted_property_slot_reserved: 0,
        trusted_global_reference_slots: context.trusted_global_reference_slots.as_ptr() as usize
            as u64,
        trusted_global_reference_slot_count: u32::try_from(
            context.trusted_global_reference_slots.len(),
        )
        .unwrap_or(u32::MAX),
        trusted_global_reference_slot_reserved: 0,
        trusted_static_local_slots: context.trusted_static_local_slots.as_ptr() as usize as u64,
        trusted_static_local_slot_count: u32::try_from(context.trusted_static_local_slots.len())
            .unwrap_or(u32::MAX),
        trusted_static_local_slot_reserved: 0,
        static_property_slots: context.static_property_slots.as_mut_ptr() as usize as u64,
        static_property_slot_count: *context.static_property_next,
        static_property_slot_reserved: 0,
        trusted_static_property_slots: context.trusted_static_property_slots.as_mut_ptr() as usize
            as u64,
        trusted_static_property_slot_count: u32::try_from(
            context.trusted_static_property_slots.len(),
        )
        .unwrap_or(u32::MAX),
        trusted_static_property_slot_reserved: 0,
        trusted_instanceof_plans: context.trusted_instanceof_plans.as_ptr() as usize as u64,
        trusted_instanceof_plan_count: u32::try_from(context.trusted_instanceof_plans.len())
            .unwrap_or(u32::MAX),
        trusted_instanceof_plan_reserved: 0,
        trusted_instanceof_entries: context.trusted_instanceof_entries.as_ptr() as usize as u64,
        trusted_instanceof_entry_count: u32::try_from(context.trusted_instanceof_entries.len())
            .unwrap_or(u32::MAX),
        trusted_instanceof_entry_reserved: 0,
        error_reporting: std::ptr::from_mut(&mut context.error_reporting) as usize as u64,
    };
    let cold_context = std::ptr::from_mut(&mut *context).cast();
    // SAFETY: `NativeRequestOwner` allocates the fast state separately and
    // wires this stable pointer before exposing the cold state.
    let fast_state = context.fast_state;
    let previous_header;
    #[allow(unsafe_code)]
    unsafe {
        previous_header = (*fast_state).header;
        (*fast_state).header = php_jit::JitNativeFastStateHeader {
            abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
            flags: 0,
            runtime_view: view,
        };
        (*context.fast_state).cold_context = cold_context;
        (*context.fast_state).output = std::ptr::from_mut(&mut context.output);
        (*context.fast_state).json_state =
            std::ptr::from_mut(context.builtin_request_state.json_mut());
        (*context.fast_state).pcre_state =
            std::ptr::from_mut(context.builtin_request_state.pcre_mut());
        (*context.fast_state).ini_registry = std::ptr::from_ref(&context.ini_registry);
        (*context.fast_state).cwd = std::ptr::from_ref(&context.cwd);
        (*context.fast_state).filesystem_capabilities =
            std::ptr::from_ref(&context.options.runtime_context.filesystem);
    }
    let runtime_view = php_jit::activate_native_runtime_view(view);
    NativeRequestActivationGuard {
        _runtime_view: runtime_view,
        fast_state,
        previous_header,
    }
}

#[allow(unsafe_code)]
fn with_native_context_for<R>(
    runtime: *mut NativeRequestFastState,
    _helper_id: &'static str,
    operation: impl FnOnce(&mut NativeRequestColdState<'_>) -> R,
) -> Option<R> {
    // SAFETY: baseline semantics and explicitly approved exact capability
    // calls receive the stable fast-state pointer. Its owner refreshes
    // `cold_context` at every activation, and the synchronous call cannot
    // outlive the request coordinator.
    let context = unsafe { native_cold_context(runtime) };
    Some(operation(context))
}

/// Enters the cold Rust semantic coordinator from a baseline/diagnostic ABI
/// or one exact capability call such as a symbol-table query. Direct value and
/// data operations never recover the coordinator.
#[allow(unsafe_code)]
unsafe fn native_cold_context<'a>(
    runtime: *mut NativeRequestFastState,
) -> &'a mut NativeRequestColdState<'a> {
    // SAFETY: the request owner refreshes this pointer before every native
    // activation, and all cold crossings are synchronous.
    unsafe { &mut *(*runtime).cold_context.cast::<NativeRequestColdState<'a>>() }
}

fn ir_constant_value(constant: &php_ir::IrConstant) -> Result<Value, String> {
    match constant {
        php_ir::IrConstant::Null => Ok(Value::Null),
        php_ir::IrConstant::Bool(value) => Ok(Value::Bool(*value)),
        php_ir::IrConstant::Int(value) => Ok(Value::Int(*value)),
        php_ir::IrConstant::Float(value) => Ok(Value::float(*value)),
        php_ir::IrConstant::String(value) => Ok(Value::String(PhpString::from_bytes(
            value.as_bytes().to_vec(),
        ))),
        php_ir::IrConstant::StringBytes(value) => {
            Ok(Value::String(PhpString::from_bytes(value.clone())))
        }
        php_ir::IrConstant::Array(entries) => {
            let mut array = php_runtime::api::PhpArray::new();
            for entry in entries {
                let value = ir_constant_value(&entry.value)?;
                if let Some(key) = &entry.key {
                    let key = ir_constant_value(key)?;
                    let key = php_runtime::api::ArrayKey::from_value(&key)
                        .ok_or_else(|| "native constant array key is invalid".to_owned())?;
                    array.insert(key, value);
                } else {
                    array
                        .try_append(value)
                        .map_err(|error| format!("E_PHP_THROW:Error:{error}"))?;
                }
            }
            Ok(Value::Array(array))
        }
        other => Err(format!(
            "native constant {other:?} requires runtime resolution"
        )),
    }
}

fn native_runtime_constant_value(
    context: &NativeRequestColdState<'_>,
    constant: &php_ir::IrConstant,
) -> Result<Value, String> {
    fn resolve(
        context: &NativeRequestColdState<'_>,
        constant: &php_ir::IrConstant,
        depth: usize,
    ) -> Result<Value, String> {
        if depth > 32 {
            return Err("native constant resolution exceeded its recursion limit".to_owned());
        }
        match constant {
            php_ir::IrConstant::NamedConstant(name) => context.lookup_constant(name),
            php_ir::IrConstant::ClassConstant {
                class_name,
                display_class_name: _,
                constant_name,
            } => {
                let normalized = normalize_class_name(class_name);
                if let Some(entry) = context
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
                {
                    if let Some(value) = entry
                        .value
                        .and_then(|id| context.unit.constants.get(id.index()))
                    {
                        return resolve(context, value, depth + 1);
                    }
                    if let Some(reference) = &entry.value_named_constant {
                        for name in &reference.names {
                            if let Ok(value) = context.lookup_constant(name) {
                                return Ok(value);
                            }
                        }
                    }
                }
                if let Some((unit, class)) = native_external_class_handle(context, &normalized)
                    && let Some(entry) = class
                        .constants
                        .iter()
                        .find(|entry| entry.name.eq_ignore_ascii_case(constant_name))
                    && let Some(value) = entry.value.and_then(|id| {
                        context
                            .dynamic_units
                            .get(unit)
                            .and_then(|package| package.compiled.unit().constants.get(id.index()))
                    })
                {
                    return resolve(context, value, depth + 1);
                }
                Err(format!("Undefined constant {class_name}::{constant_name}"))
            }
            php_ir::IrConstant::Array(entries) => {
                let mut array = php_runtime::api::PhpArray::new();
                for entry in entries {
                    let value = resolve(context, &entry.value, depth + 1)?;
                    if let Some(key) = &entry.key {
                        let key = resolve(context, key, depth + 1)?;
                        let key = php_runtime::api::ArrayKey::from_value(&key)
                            .ok_or_else(|| "native constant array key is invalid".to_owned())?;
                        array.insert(key, value);
                    } else {
                        array
                            .try_append(value)
                            .map_err(|error| format!("E_PHP_THROW:Error:{error}"))?;
                    }
                }
                Ok(Value::Array(array))
            }
            value => ir_constant_value(value),
        }
    }
    resolve(context, constant, 0)
}

fn native_runtime_type(type_: &php_ir::IrReturnType) -> php_runtime::api::RuntimeType {
    use php_ir::IrReturnType as Ir;
    use php_runtime::api::RuntimeType as Runtime;
    match type_ {
        Ir::Int => Runtime::Int,
        Ir::Float => Runtime::Float,
        Ir::String => Runtime::String,
        Ir::Array => Runtime::Array,
        Ir::Callable => Runtime::Callable,
        Ir::Iterable => Runtime::Iterable,
        Ir::Object => Runtime::Object,
        Ir::Bool => Runtime::Bool,
        Ir::Null => Runtime::Null,
        Ir::Void => Runtime::Void,
        Ir::Mixed => Runtime::Mixed,
        Ir::Never => Runtime::Never,
        Ir::False => Runtime::False,
        Ir::True => Runtime::True,
        Ir::Class { name, display_name } => Runtime::Class {
            name: name.clone(),
            display_name: display_name.clone(),
        },
        Ir::Nullable { inner } => Runtime::Nullable {
            inner: Box::new(native_runtime_type(inner)),
        },
        Ir::Union { members } => Runtime::Union {
            members: members.iter().map(native_runtime_type).collect(),
        },
        Ir::Intersection { members } => Runtime::Intersection {
            members: members.iter().map(native_runtime_type).collect(),
        },
        Ir::Dnf { members } => Runtime::Dnf {
            clauses: members.iter().map(native_runtime_type).collect(),
        },
    }
}

fn native_value_matches_ir_type(value: &Value, type_: &php_ir::IrReturnType) -> bool {
    use php_ir::IrReturnType as Ir;
    let value = match value {
        Value::Reference(reference) => {
            return native_value_matches_ir_type(&reference.get(), type_);
        }
        value => value,
    };
    match type_ {
        Ir::Int => matches!(value, Value::Int(_)),
        Ir::Float => matches!(value, Value::Float(_) | Value::Int(_)),
        Ir::String => matches!(value, Value::String(_)),
        Ir::Array => matches!(value, Value::Array(_)),
        Ir::Callable => matches!(value, Value::Callable(_)),
        Ir::Iterable => matches!(value, Value::Array(_) | Value::Object(_)),
        Ir::Object | Ir::Class { .. } => matches!(value, Value::Object(_)),
        Ir::Bool => matches!(value, Value::Bool(_)),
        Ir::Null | Ir::Void => matches!(value, Value::Null),
        Ir::Mixed => true,
        Ir::Never => false,
        Ir::False => matches!(value, Value::Bool(false)),
        Ir::True => matches!(value, Value::Bool(true)),
        Ir::Nullable { inner } => {
            matches!(value, Value::Null) || native_value_matches_ir_type(value, inner)
        }
        Ir::Union { members } => members
            .iter()
            .any(|member| native_value_matches_ir_type(value, member)),
        Ir::Intersection { members } => members
            .iter()
            .all(|member| native_value_matches_ir_type(value, member)),
        Ir::Dnf { members } => members
            .iter()
            .any(|member| native_value_matches_ir_type(value, member)),
    }
}

fn native_value_matches_ir_type_in_context(
    context: &NativeRequestColdState<'_>,
    value: &Value,
    type_: &php_ir::IrReturnType,
) -> bool {
    use php_ir::IrReturnType as Ir;
    let value = match value {
        Value::Reference(reference) => {
            return native_value_matches_ir_type_in_context(context, &reference.get(), type_);
        }
        value => value,
    };
    match type_ {
        Ir::Class { name, .. } => match value {
            Value::Object(object) => native_class_is_a(context, &object.class_name(), name),
            _ => false,
        },
        Ir::Nullable { inner } => {
            matches!(value, Value::Null)
                || native_value_matches_ir_type_in_context(context, value, inner)
        }
        Ir::Union { members } | Ir::Dnf { members } => members
            .iter()
            .any(|member| native_value_matches_ir_type_in_context(context, value, member)),
        Ir::Intersection { members } => members
            .iter()
            .all(|member| native_value_matches_ir_type_in_context(context, value, member)),
        _ => native_value_matches_ir_type(value, type_),
    }
}

fn native_value_is_callable(context: &NativeRequestColdState<'_>, value: &Value) -> bool {
    match value {
        Value::Reference(reference) => native_value_is_callable(context, &reference.get()),
        Value::Callable(_) => true,
        Value::Object(object) => {
            native_method_in_hierarchy(context, &object.class_name(), "__invoke").is_some()
                || native_external_method(context, &object.class_name(), "__invoke").is_some()
        }
        Value::String(name) => {
            let name = name.to_string_lossy();
            if let Some((class, method)) = name.split_once("::") {
                native_method_in_hierarchy(context, class, method).is_some()
                    || native_external_method(context, class, method).is_some()
            } else {
                context.function_id(&name).is_some()
                    || context.external_function(&name).is_some()
                    || php_extensions::BuiltinRegistry::new().contains(&name.to_ascii_lowercase())
            }
        }
        Value::Array(array) if array.len() == 2 => {
            let target = array.get(&php_runtime::api::ArrayKey::Int(0));
            let method = array.get(&php_runtime::api::ArrayKey::Int(1));
            match (target, method) {
                (Some(Value::Object(object)), Some(Value::String(method))) => {
                    let class = object.class_name();
                    native_method_in_hierarchy(context, &class, &method.to_string_lossy()).is_some()
                        || native_external_method(context, &class, &method.to_string_lossy())
                            .is_some()
                }
                (Some(Value::String(class)), Some(Value::String(method))) => {
                    let class = class.to_string_lossy();
                    native_method_in_hierarchy(context, &class, &method.to_string_lossy()).is_some()
                        || native_external_method(context, &class, &method.to_string_lossy())
                            .is_some()
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn native_ir_type_name(type_: &php_ir::IrReturnType) -> String {
    use php_ir::IrReturnType as Ir;
    match type_ {
        Ir::Int => "int".to_owned(),
        Ir::Float => "float".to_owned(),
        Ir::String => "string".to_owned(),
        Ir::Array => "array".to_owned(),
        Ir::Callable => "callable".to_owned(),
        Ir::Iterable => "iterable".to_owned(),
        Ir::Object => "object".to_owned(),
        Ir::Bool => "bool".to_owned(),
        Ir::Null => "null".to_owned(),
        Ir::Void => "void".to_owned(),
        Ir::Mixed => "mixed".to_owned(),
        Ir::Never => "never".to_owned(),
        Ir::False => "false".to_owned(),
        Ir::True => "true".to_owned(),
        Ir::Class { display_name, name } => display_name.clone().unwrap_or_else(|| name.clone()),
        Ir::Nullable { inner } => format!("?{}", native_ir_type_name(inner)),
        Ir::Union { members } => {
            let mut names = members.iter().map(native_ir_type_name).collect::<Vec<_>>();
            if names.len() == 2
                && names.iter().any(|name| name == "int")
                && names.iter().any(|name| name == "string")
            {
                names = vec!["string".to_owned(), "int".to_owned()];
            }
            names.join("|")
        }
        Ir::Intersection { members } => members
            .iter()
            .map(native_ir_type_name)
            .collect::<Vec<_>>()
            .join("&"),
        Ir::Dnf { members } => members
            .iter()
            .map(native_ir_type_name)
            .collect::<Vec<_>>()
            .join("|"),
    }
}

fn native_publication_constant_is_stable(constant: &php_ir::IrConstant) -> bool {
    match constant {
        php_ir::IrConstant::Null
        | php_ir::IrConstant::Bool(_)
        | php_ir::IrConstant::Int(_)
        | php_ir::IrConstant::Float(_)
        | php_ir::IrConstant::String(_)
        | php_ir::IrConstant::StringBytes(_) => true,
        php_ir::IrConstant::Array(entries) => entries.iter().all(|entry| {
            entry
                .key
                .as_ref()
                .is_none_or(native_publication_constant_is_stable)
                && native_publication_constant_is_stable(&entry.value)
        }),
        php_ir::IrConstant::NamedConstant(_) | php_ir::IrConstant::ClassConstant { .. } => false,
    }
}

fn native_class_is_publication_allocatable(
    classes: &[php_ir::module::ClassEntry],
    constants: &[php_ir::IrConstant],
    class: &php_ir::module::ClassEntry,
) -> bool {
    if class.flags.is_abstract
        || class.flags.is_interface
        || class.flags.is_trait
        || class.flags.is_enum
    {
        return false;
    }
    let mut current = Some(class);
    let mut visited = std::collections::BTreeSet::new();
    while let Some(candidate) = current {
        if !visited.insert(candidate.name.as_str()) {
            return false;
        }
        if candidate.properties.iter().any(|property| {
            property
                .default
                .and_then(|constant| constants.get(constant.index()))
                .is_some_and(|constant| !native_publication_constant_is_stable(constant))
        }) {
            return false;
        }
        current = match candidate.parent.as_deref() {
            None => None,
            Some(parent) => {
                let parent = normalize_class_name(parent);
                if let Some(parent) = classes.iter().find(|class| class.name == parent) {
                    Some(parent)
                } else {
                    let internal = php_std::ExtensionRegistry::standard_library()
                        .enabled_class(&parent)
                        .is_some()
                        || matches!(
                            parent.as_str(),
                            "stdclass"
                                | "exception"
                                | "errorexception"
                                | "error"
                                | "typeerror"
                                | "valueerror"
                                | "argumentcounterror"
                                | "fibererror"
                                | "closure"
                                | "generator"
                                | "fiber"
                                | "arrayobject"
                                | "arrayiterator"
                        );
                    if !internal {
                        return false;
                    }
                    None
                }
            }
        };
    }
    true
}

fn native_runtime_class_with_owner(
    context: &NativeRequestColdState<'_>,
    owner_unit: Option<usize>,
    class: &php_ir::module::ClassEntry,
) -> Result<php_runtime::api::ClassEntry, String> {
    use php_runtime::api as runtime;

    let owner_ir_unit = |owner: Option<usize>| -> Option<&php_ir::IrUnit> {
        match owner {
            None => Some(&*context.unit),
            Some(unit) => context
                .dynamic_units
                .get(unit)
                .map(|package| package.compiled.unit()),
        }
    };
    let mut lineage = Vec::new();
    let mut current = Some((owner_unit, class));
    let mut visited = std::collections::BTreeSet::new();
    while let Some((owner, candidate)) = current {
        if !visited.insert(candidate.name.clone()) {
            return Err(format!(
                "native class hierarchy for {} contains a cycle",
                class.display_name
            ));
        }
        let parent = candidate.parent.clone();
        lineage.push((owner, candidate));
        current = parent.as_deref().and_then(|parent| {
            let parent = normalize_class_name(parent);
            owner_ir_unit(owner)
                .into_iter()
                .flat_map(|unit| &unit.classes)
                .find(|class| class.name == parent)
                .map(|class| (owner, class))
                .or_else(|| {
                    native_external_class_ref(context, &parent)
                        .map(|(unit, class)| (Some(unit), class))
                })
        });
    }
    lineage.reverse();
    let properties = lineage
        .iter()
        .flat_map(|(owner, class)| {
            class
                .properties
                .iter()
                .map(move |property| (*owner, property))
        })
        .map(|(owner, property)| {
            let default = property
                .default
                .and_then(|constant| owner_ir_unit(owner)?.constants.get(constant.index()))
                .map(|value| native_runtime_constant_value(context, value))
                .transpose()?
                .unwrap_or_else(|| {
                    if property.flags.is_typed {
                        Value::Uninitialized
                    } else {
                        Value::Null
                    }
                });
            Ok(runtime::ClassPropertyEntry {
                name: property.name.clone(),
                default,
                type_: property.type_.as_ref().map(native_runtime_type),
                flags: runtime::ClassPropertyFlags {
                    is_static: property.flags.is_static,
                    is_private: property.flags.is_private,
                    is_protected: property.flags.is_protected,
                    set_is_private: property.flags.set_is_private,
                    set_is_protected: property.flags.set_is_protected,
                    is_readonly: property.flags.is_readonly,
                    is_typed: property.flags.is_typed,
                },
                hooks: runtime::ClassPropertyHooks {
                    get_function_id: property.hooks.get.map(|function| function.raw()),
                    set_function_id: property.hooks.set.map(|function| function.raw()),
                    backed: property.hooks.backed,
                },
                attributes: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let runtime_class = runtime::ClassEntry {
        name: class.name.clone().into(),
        parent: class.parent.clone(),
        interfaces: class.interfaces.clone(),
        methods: lineage
            .iter()
            .flat_map(|(_, class)| &class.methods)
            .map(|method| runtime::ClassMethodEntry {
                name: method.name.clone(),
                origin_class: method.origin_class.clone(),
                function_id: method.function.raw(),
                flags: runtime::ClassMethodFlags {
                    is_static: method.flags.is_static,
                    is_private: method.flags.is_private,
                    is_protected: method.flags.is_protected,
                    is_abstract: method.flags.is_abstract,
                    is_final: method.flags.is_final,
                },
                attributes: Vec::new(),
            })
            .collect(),
        properties,
        constants: class
            .constants
            .iter()
            .filter_map(|constant| {
                let value = constant
                    .value
                    .and_then(|value| owner_ir_unit(owner_unit)?.constants.get(value.index()))
                    .and_then(|value| native_runtime_constant_value(context, value).ok())?;
                Some(runtime::ClassConstantEntry {
                    name: constant.name.clone(),
                    value,
                    flags: runtime::ClassConstantFlags {
                        is_private: constant.flags.is_private,
                        is_protected: constant.flags.is_protected,
                    },
                    attributes: Vec::new(),
                })
            })
            .collect(),
        enum_cases: class
            .enum_cases
            .iter()
            .map(|case| runtime::ClassEnumCaseEntry {
                name: case.name.clone(),
                value: case
                    .value
                    .and_then(|value| owner_ir_unit(owner_unit)?.constants.get(value.index()))
                    .and_then(|value| ir_constant_value(value).ok()),
                attributes: Vec::new(),
            })
            .collect(),
        attributes: Vec::new(),
        enum_backing_type: class.enum_backing_type.map(|backing| match backing {
            php_ir::module::ClassEnumBackingType::Int => runtime::ClassEnumBackingType::Int,
            php_ir::module::ClassEnumBackingType::String => runtime::ClassEnumBackingType::String,
        }),
        constructor_id: class.constructor.map(|function| function.raw()),
        flags: runtime::ClassFlags {
            is_abstract: class.flags.is_abstract || class.flags.is_trait,
            is_final: class.flags.is_final,
            is_readonly: class.flags.is_readonly,
            is_interface: class.flags.is_interface,
            is_enum: class.flags.is_enum,
        },
    };
    Ok(runtime_class)
}

fn new_native_object(
    context: &NativeRequestColdState<'_>,
    owner_unit: Option<usize>,
    class: &php_ir::module::ClassEntry,
) -> Result<php_runtime::api::ObjectRef, String> {
    let entry = native_runtime_class_with_owner(context, owner_unit, class)?;
    Ok(php_runtime::api::ObjectRef::new_with_display_name(
        &entry,
        class.display_name.clone(),
    ))
}

fn native_prepare_runtime_class_constants(
    context: &mut NativeRequestColdState<'_>,
    owner_unit: Option<usize>,
    class: &php_ir::module::ClassEntry,
    source: &php_ir::Instruction,
) -> Result<(), String> {
    fn prepare_constant(
        context: &mut NativeRequestColdState<'_>,
        constant: &php_ir::IrConstant,
        source: &php_ir::Instruction,
    ) -> Result<(), String> {
        match constant {
            php_ir::IrConstant::ClassConstant {
                class_name,
                display_class_name,
                ..
            } => {
                let autoload_name = if display_class_name.is_empty() {
                    class_name
                } else {
                    display_class_name
                };
                native_autoload_class(context, autoload_name, source)
            }
            php_ir::IrConstant::Array(entries) => {
                for entry in entries {
                    if let Some(key) = &entry.key {
                        prepare_constant(context, key, source)?;
                    }
                    prepare_constant(context, &entry.value, source)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    let constants = match owner_unit {
        None => &context.unit.constants,
        Some(unit) => {
            &context
                .dynamic_units
                .get(unit)
                .ok_or_else(|| format!("dynamic native unit {unit} is missing"))?
                .compiled
                .unit()
                .constants
        }
    };
    let defaults = class
        .properties
        .iter()
        .filter_map(|property| {
            property
                .default
                .and_then(|constant| constants.get(constant.index()))
                .cloned()
        })
        .collect::<Vec<_>>();
    for constant in &defaults {
        prepare_constant(context, constant, source)?;
    }
    Ok(())
}

fn encode_native_enum_case(
    context: &mut NativeRequestColdState<'_>,
    class: &php_ir::module::ClassEntry,
    case: &php_ir::module::ClassEnumCaseEntry,
) -> Result<i64, String> {
    let key = (class.name.clone(), case.name.clone());
    if let Some(object) = context.enum_cases.get(&key).cloned() {
        return context.encode_native_object_owner(object);
    }
    let object = new_native_object(context, None, class)?;
    object.set_property(
        "name",
        Value::String(PhpString::from_bytes(case.name.as_bytes().to_vec())),
    );
    if let Some(value) = case
        .value
        .and_then(|value| context.unit.constants.get(value.index()))
        .and_then(|value| ir_constant_value(value).ok())
    {
        object.set_property("value", value);
    }
    context.enum_cases.insert(key, object.clone());
    context.mark_roots_dirty(RootMutationReason::EnumOrStaticObject);
    context.encode_native_object_owner(object)
}

struct NativeStaticPropertyDeclaration {
    owner_unit: Option<usize>,
    owner_name: String,
    owner_display_name: String,
    caller_owns_scope: bool,
    flags: php_ir::module::ClassPropertyFlags,
    default: Option<php_ir::ConstId>,
    has_deferred_default: bool,
    type_: Option<php_ir::IrReturnType>,
}

fn native_static_property_declaration(
    context: &NativeRequestColdState<'_>,
    class_name: &str,
    property: &str,
    caller_function: u32,
) -> Option<NativeStaticPropertyDeclaration> {
    let mut candidate = normalize_class_name(class_name);
    let mut visited = std::collections::BTreeSet::new();
    while visited.insert(candidate.clone()) {
        let (unit, class) = if let Some(class) = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == candidate)
        {
            (None, class)
        } else {
            let (unit, class) = native_external_class_ref(context, &candidate)?;
            (Some(unit), class)
        };
        if let Some(entry) = class
            .properties
            .iter()
            .find(|entry| entry.flags.is_static && entry.name == property)
        {
            return Some(NativeStaticPropertyDeclaration {
                owner_unit: unit,
                owner_name: class.name.clone(),
                owner_display_name: class.display_name.clone(),
                caller_owns_scope: class
                    .methods
                    .iter()
                    .any(|method| method.function.raw() == caller_function),
                flags: entry.flags,
                default: entry.default,
                has_deferred_default: entry.default_class_constant.is_some()
                    || entry.default_named_constant.is_some()
                    || entry.default_expr.is_some(),
                type_: entry.type_.clone(),
            });
        }
        candidate = normalize_class_name(class.parent.as_ref()?);
    }
    None
}

fn native_nested_array_reference(
    value: &mut Value,
    keys: &[php_runtime::api::ArrayKey],
) -> Result<php_runtime::api::ReferenceCell, String> {
    if keys.is_empty() {
        return Ok(match value {
            Value::Reference(reference) => reference.clone(),
            value => {
                let reference = php_runtime::api::ReferenceCell::new(value.clone());
                *value = Value::Reference(reference.clone());
                reference
            }
        });
    }

    if let Value::Reference(reference) = value {
        let mut referenced = reference.get();
        let result = native_nested_array_reference(&mut referenced, keys)?;
        reference.set(referenced);
        return Ok(result);
    }

    if matches!(value, Value::Null | Value::Uninitialized) {
        *value = Value::Array(php_runtime::api::PhpArray::new());
    }
    let Value::Array(array) = value else {
        return Err(format!(
            "Cannot use a value of type {} as an array",
            native_value_type_name(value)
        ));
    };

    let key = keys[0].clone();
    let mut element = array.get(&key).cloned().unwrap_or(Value::Null);
    let reference = native_nested_array_reference(&mut element, &keys[1..])?;
    array.insert(key, element);
    Ok(reference)
}

fn dereference_native_assignment_value(mut value: Value) -> Value {
    for _ in 0..16 {
        let Value::Reference(reference) = value else {
            break;
        };
        value = reference.get();
    }
    value
}

fn execute_native_static_property(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
    caller_function: u32,
) -> Option<Result<i64, String>> {
    if let php_ir::InstructionKind::BindReferenceFromStaticPropertyDim {
        class_name,
        property,
        dims,
        ..
    } = &instruction.kind
    {
        let keys = match arguments
            .iter()
            .map(|argument| {
                context.decode(*argument).and_then(|value| {
                    php_runtime::api::ArrayKey::from_value(&value)
                        .ok_or_else(|| "Illegal offset type".to_owned())
                })
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(keys) if keys.len() == dims.len() => keys,
            Ok(_) => {
                return Some(Err(
                    "static property dimension operands are missing".to_owned()
                ));
            }
            Err(error) => return Some(Err(error)),
        };
        let calling_class = native_calling_class(context, caller_function);
        let resolved_class = match class_name.to_ascii_lowercase().as_str() {
            "self" => calling_class.map_or_else(|| class_name.clone(), |class| class.name.clone()),
            "parent" => calling_class
                .and_then(|class| class.parent.clone())
                .unwrap_or_else(|| class_name.clone()),
            "static" => context
                .called_classes
                .last()
                .map(|class| class.to_string())
                .or_else(|| calling_class.map(|class| class.name.clone()))
                .unwrap_or_else(|| class_name.clone()),
            _ => class_name.clone(),
        };
        let Some(declaration) =
            native_static_property_declaration(context, &resolved_class, property, caller_function)
        else {
            return Some(Err(format!(
                "E_PHP_THROW:Error:Access to undeclared static property {resolved_class}::${property}"
            )));
        };
        let key = (declaration.owner_name, property.clone());
        let current = match context.direct_static_property_value(&key) {
            Some(Ok(value)) => Some(value),
            Some(Err(error)) => return Some(Err(error)),
            None => context.static_properties.get(&key).cloned().or_else(|| {
                declaration
                    .default
                    .and_then(|constant| {
                        if declaration.owner_unit.is_none() {
                            context.unit.constants.get(constant.index())
                        } else {
                            declaration.owner_unit.and_then(|unit| {
                                context.dynamic_units.get(unit).and_then(|package| {
                                    package.compiled.unit().constants.get(constant.index())
                                })
                            })
                        }
                    })
                    .and_then(|constant| ir_constant_value(constant).ok())
            }),
        };
        if keys.is_empty() {
            let reference = match current.unwrap_or(Value::Null) {
                Value::Reference(reference) => reference,
                value => php_runtime::api::ReferenceCell::new(value),
            };
            let replacement = Value::Reference(reference.clone());
            match context.store_direct_static_property_value(&key, replacement.clone()) {
                Some(Ok(())) => {}
                Some(Err(error)) => return Some(Err(error)),
                None => {
                    context.static_properties.insert(key, replacement);
                    context.mark_roots_dirty(RootMutationReason::EnumOrStaticObject);
                }
            }
            return Some(context.encode_native_reference_owner(reference));
        }

        // Binding one dimension must put the leaf ReferenceCell into the
        // authoritative array itself. Wrapping the whole static property in a
        // separate root reference before descending leaves later dimension
        // fetches observing the old array snapshot.
        let mut root = current.unwrap_or(Value::Null);
        let reference = match native_nested_array_reference(&mut root, &keys) {
            Ok(reference) => reference,
            Err(error) => return Some(Err(error)),
        };
        match context.store_direct_static_property_value(&key, root.clone()) {
            Some(Ok(())) => {}
            Some(Err(error)) => return Some(Err(error)),
            None => {
                context.static_properties.insert(key, root);
                context.mark_roots_dirty(RootMutationReason::EnumOrStaticObject);
            }
        }
        return Some(context.encode_native_reference_owner(reference));
    }
    let (class_name, property, assigned, bind_reference) = match &instruction.kind {
        php_ir::InstructionKind::FetchStaticProperty {
            class_name,
            property,
            ..
        } => (class_name.clone(), property.clone(), None, false),
        php_ir::InstructionKind::AssignStaticProperty {
            class_name,
            property,
            ..
        } => {
            let Some(value) = arguments.first() else {
                return Some(Err("static property assignment value is missing".to_owned()));
            };
            (class_name.clone(), property.clone(), Some(*value), false)
        }
        php_ir::InstructionKind::AssignDynamicStaticProperty { property, .. } => {
            let [class_name, value] = arguments else {
                return Some(Err(
                    "dynamic static property assignment operands are missing".to_owned(),
                ));
            };
            let class_name = match context.decode(*class_name) {
                Ok(Value::Reference(reference)) => reference.get(),
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let class_name = match class_name {
                Value::String(class_name) => class_name.to_string_lossy(),
                Value::Object(object) => object.class_name(),
                value => {
                    return Some(Err(format!(
                        "class name must be a valid object or a string, {} given",
                        native_value_type_name(&value)
                    )));
                }
            };
            (class_name, property.clone(), Some(*value), false)
        }
        php_ir::InstructionKind::FetchDynamicStaticProperty { property, .. } => {
            let Some(class_name) = arguments.first() else {
                return Some(Err(
                    "dynamic static property class operand is missing".to_owned()
                ));
            };
            let class_name = match context.decode(*class_name) {
                Ok(Value::Reference(reference)) => reference.get(),
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let class_name = match class_name {
                Value::String(class_name) => class_name.to_string_lossy(),
                Value::Object(object) => object.class_name(),
                value => {
                    return Some(Err(format!(
                        "class name must be a valid object or a string, {} given",
                        native_value_type_name(&value)
                    )));
                }
            };
            (class_name, property.clone(), None, false)
        }
        php_ir::InstructionKind::BindReferenceStaticProperty {
            class_name,
            property,
            ..
        } => {
            let Some(value) = arguments.first() else {
                return Some(Err("static property reference source is missing".to_owned()));
            };
            (class_name.clone(), property.clone(), Some(*value), true)
        }
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
        }
        | php_ir::InstructionKind::UnsetStaticPropertyDim {
            class_name,
            property,
            ..
        } => (class_name.clone(), property.clone(), None, false),
        _ => return None,
    };
    let calling_class = native_calling_class(context, caller_function);
    let resolved_class = match class_name.to_ascii_lowercase().as_str() {
        "self" => calling_class.map_or_else(|| class_name.clone(), |class| class.name.clone()),
        "parent" => calling_class
            .and_then(|class| class.parent.clone())
            .unwrap_or_else(|| class_name.clone()),
        "static" => context
            .called_classes
            .last()
            .map(|class| class.to_string())
            .or_else(|| calling_class.map(|class| class.name.clone()))
            .unwrap_or_else(|| class_name.clone()),
        _ => class_name.clone(),
    };
    let normalized = normalize_class_name(&resolved_class);
    let requested_local_display_name = context
        .unit
        .classes
        .iter()
        .find(|class| class.name == normalized)
        .map(|class| class.display_name.clone());
    if requested_local_display_name.is_none()
        && !native_external_class_exists(context, &resolved_class)
        && context.autoload_in_progress.insert(normalized.clone())
    {
        let callbacks = context.autoload_callbacks.clone();
        for callback in callbacks {
            if let Err(error) = invoke_native_callable_value(
                context,
                callback,
                &[Value::String(PhpString::from_bytes(
                    resolved_class.as_bytes().to_vec(),
                ))],
                instruction,
                None,
            ) {
                context.autoload_in_progress.remove(&normalized);
                return Some(Err(error));
            }
            if native_external_class_exists(context, &resolved_class) {
                break;
            }
        }
        context.autoload_in_progress.remove(&normalized);
    }
    let requested_display_name = requested_local_display_name
        .or_else(|| {
            native_external_class_ref(context, &resolved_class)
                .map(|(_, class)| class.display_name.clone())
        })
        .unwrap_or_else(|| resolved_class.clone());
    let Some(declaration) =
        native_static_property_declaration(context, &resolved_class, &property, caller_function)
    else {
        if matches!(
            instruction.kind,
            php_ir::InstructionKind::IssetStaticProperty { .. }
                | php_ir::InstructionKind::IssetStaticPropertyDim { .. }
        ) {
            return Some(context.encode(Value::Bool(false)));
        }
        if matches!(
            instruction.kind,
            php_ir::InstructionKind::EmptyStaticProperty { .. }
                | php_ir::InstructionKind::EmptyStaticPropertyDim { .. }
        ) {
            return Some(context.encode(Value::Bool(true)));
        }
        return Some(Err(format!(
            "E_PHP_THROW:Error:Access to undeclared static property {requested_display_name}::${property}"
        )));
    };
    let display_name = declaration.owner_display_name;
    if (declaration.flags.is_private || declaration.flags.is_protected)
        && !declaration.caller_owns_scope
    {
        return Some(Err(format!(
            "E_PHP_THROW:Error:Cannot access {} property {}::${property}",
            if declaration.flags.is_private {
                "private"
            } else {
                "protected"
            },
            display_name
        )));
    }
    let key = (declaration.owner_name, property.clone());
    if assigned.is_none()
        && let Some(encoded) = context.direct_static_property_encoded(&key)
    {
        let direct = match &instruction.kind {
            php_ir::InstructionKind::FetchStaticProperty { .. }
            | php_ir::InstructionKind::FetchDynamicStaticProperty { .. } => {
                Some(context.duplicate_dereferenced_native_value(encoded))
            }
            php_ir::InstructionKind::IssetStaticProperty { .. } => context
                .native_encoded_is_set(encoded)
                .map(|value| context.encode(Value::Bool(value))),
            php_ir::InstructionKind::EmptyStaticProperty { .. } => context
                .native_encoded_truthy(encoded)
                .map(|value| context.encode(Value::Bool(!value))),
            php_ir::InstructionKind::IssetStaticPropertyDim { dims, .. }
            | php_ir::InstructionKind::EmptyStaticPropertyDim { dims, .. } => {
                let isset = matches!(
                    instruction.kind,
                    php_ir::InstructionKind::IssetStaticPropertyDim { .. }
                );
                if arguments.len() != dims.len() {
                    None
                } else {
                    match context.direct_dimension_path_encoded(encoded, arguments) {
                        Ok(Some(Some(value))) => {
                            let classified = if isset {
                                context.native_encoded_is_set(value)
                            } else {
                                context.native_encoded_truthy(value).map(|truthy| !truthy)
                            };
                            classified.map(|value| context.encode(Value::Bool(value)))
                        }
                        Ok(Some(None)) => Some(context.encode(Value::Bool(!isset))),
                        Ok(None) | Err(_) => None,
                    }
                }
            }
            _ => None,
        };
        if let Some(result) = direct {
            return Some(result);
        }
    }
    let result = if bind_reference {
        let Some(source) = assigned else {
            return Some(Err("static property reference source is missing".to_owned()));
        };
        let value = match context.decode(source) {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        };
        let reference = match value {
            Value::Reference(reference) => reference,
            value => php_runtime::api::ReferenceCell::new(value),
        };
        let effective = reference.get();
        if let Some(type_) = &declaration.type_
            && !native_value_matches_ir_type_in_context(context, &effective, type_)
        {
            return Some(Err(format!(
                "E_PHP_THROW:TypeError:Cannot assign {} to property {}::${} of type {}",
                native_assignment_type_name(&effective),
                display_name,
                property,
                native_ir_type_name(type_)
            )));
        }
        let replacement = Value::Reference(reference.clone());
        let previous = match context.store_direct_static_property_value(&key, replacement.clone()) {
            Some(Ok(())) => None,
            Some(Err(error)) => return Some(Err(error)),
            None => {
                let previous = context.static_properties.remove(&key);
                if let Err(error) = context.ensure_direct_static_property_encoded(&key, replacement)
                {
                    return Some(Err(error));
                }
                previous
            }
        };
        if let Some(previous) = previous.map(dereference_native_assignment_value)
            && let Value::Object(previous) = previous
            && let Err(error) = context.run_object_destructor(previous)
        {
            return Some(Err(error));
        }
        Value::Reference(reference)
    } else if let Some(assigned) = assigned {
        let mut value = match context.decode(assigned) {
            Ok(value) => dereference_native_assignment_value(value),
            Err(error) => return Some(Err(error)),
        };
        if declaration.owner_unit.is_some() {
            // Closure function ids are unit-local. Preserve the assigning
            // unit when a closure crosses into a class owned by another unit.
            value = native_value_with_owner_unit(value, context.current_dynamic_unit);
        }
        if let Some(type_) = &declaration.type_
            && !native_value_matches_ir_type_in_context(context, &value, type_)
        {
            return Some(Err(format!(
                "E_PHP_THROW:TypeError:Cannot assign {} to property {}::${} of type {}",
                native_assignment_type_name(&value),
                display_name,
                property,
                native_ir_type_name(type_)
            )));
        }
        let direct_current = match context.direct_static_property_value(&key) {
            Some(Ok(value)) => Some(value),
            Some(Err(error)) => return Some(Err(error)),
            None => None,
        };
        let existing_reference = direct_current
            .as_ref()
            .or_else(|| context.static_properties.get(&key))
            .and_then(|current| {
                let Value::Reference(reference) = current else {
                    return None;
                };
                Some(reference.clone())
            });
        let previous = if let Some(reference) = existing_reference {
            let previous = reference.get();
            reference.set(value.clone());
            Some(previous)
        } else if direct_current.is_some() {
            match context.store_direct_static_property_value(&key, value.clone()) {
                // Replacing an authoritative direct slot releases its prior
                // owner inside `store_direct_static_property_value`; that
                // release already performs the exact last-owner destructor
                // transition. Returning the decoded alias here and invoking
                // the destructor again re-entered user code twice for one
                // replacement and could corrupt an in-flight compilation.
                Some(Ok(())) => None,
                Some(Err(error)) => return Some(Err(error)),
                None => unreachable!("direct static value lost its published slot"),
            }
        } else {
            let previous = context.static_properties.remove(&key);
            if let Err(error) = context.ensure_direct_static_property_encoded(&key, value.clone()) {
                return Some(Err(error));
            }
            previous
        };
        context.mark_roots_dirty(RootMutationReason::EnumOrStaticObject);
        if let Some(Value::Object(previous)) = previous
            && !context.object_is_request_rooted(previous.id())
            && let Err(error) = context.run_object_destructor(previous)
        {
            return Some(Err(error));
        }
        value
    } else if let Some(value) = context.direct_static_property_value(&key) {
        match value {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        }
    } else if let Some(value) = context.static_properties.get(&key).cloned() {
        value
    } else {
        let value = declaration.default.and_then(|constant| {
            if declaration.owner_unit.is_none() {
                context.unit.constants.get(constant.index())
            } else {
                declaration.owner_unit.and_then(|unit| {
                    context
                        .dynamic_units
                        .get(unit)
                        .and_then(|package| package.compiled.unit().constants.get(constant.index()))
                })
            }
        });
        let value = value.map_or(Ok(Value::Null), |value| {
            native_runtime_constant_value(context, value)
        });
        match value {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        }
    };
    if assigned.is_none()
        && !bind_reference
        && !matches!(
            instruction.kind,
            php_ir::InstructionKind::UnsetStaticPropertyDim { .. }
        )
    {
        let encoded = match context.ensure_direct_static_property_encoded(&key, result.clone()) {
            Ok(encoded) => encoded,
            Err(error) => return Some(Err(error)),
        };
        let direct = match &instruction.kind {
            php_ir::InstructionKind::FetchStaticProperty { .. }
            | php_ir::InstructionKind::FetchDynamicStaticProperty { .. } => {
                Some(context.duplicate_dereferenced_native_value(encoded))
            }
            php_ir::InstructionKind::IssetStaticProperty { .. } => context
                .native_encoded_is_set(encoded)
                .map(|value| context.encode(Value::Bool(value))),
            php_ir::InstructionKind::EmptyStaticProperty { .. } => context
                .native_encoded_truthy(encoded)
                .map(|value| context.encode(Value::Bool(!value))),
            php_ir::InstructionKind::IssetStaticPropertyDim { dims, .. }
            | php_ir::InstructionKind::EmptyStaticPropertyDim { dims, .. } => {
                let isset = matches!(
                    instruction.kind,
                    php_ir::InstructionKind::IssetStaticPropertyDim { .. }
                );
                if arguments.len() != dims.len() {
                    None
                } else {
                    match context.direct_dimension_path_encoded(encoded, arguments) {
                        Ok(Some(Some(value))) => {
                            let classified = if isset {
                                context.native_encoded_is_set(value)
                            } else {
                                context.native_encoded_truthy(value).map(|truthy| !truthy)
                            };
                            classified.map(|value| context.encode(Value::Bool(value)))
                        }
                        Ok(Some(None)) => Some(context.encode(Value::Bool(!isset))),
                        Ok(None) | Err(_) => None,
                    }
                }
            }
            _ => None,
        };
        if let Some(result) = direct {
            return Some(result);
        }
    }
    if assigned.is_some()
        && let Some(encoded) = context.direct_static_property_encoded(&key)
    {
        // Assignment has already moved the authoritative owner into the
        // native static slot above. Returning by re-encoding the temporary
        // Rust value rebuilt the complete array/object graph a second time.
        // The expression result instead receives one owner from the slot that
        // now contains the PHP-visible value. Reference binding returns the
        // reference identity itself; ordinary assignment returns its value.
        return Some(if bind_reference {
            context.retain(encoded).map(|()| encoded)
        } else {
            context.duplicate_dereferenced_native_value(encoded)
        });
    }
    let result = match &instruction.kind {
        php_ir::InstructionKind::IssetStaticProperty { .. } => {
            Value::Bool(!matches!(result, Value::Null | Value::Uninitialized))
        }
        php_ir::InstructionKind::EmptyStaticProperty { .. } => {
            Value::Bool(!native_property_truthy(&result))
        }
        php_ir::InstructionKind::IssetStaticPropertyDim { dims, .. } => {
            let value = match native_dimension_path_value(
                context,
                Some(result),
                arguments,
                dims.len(),
                instruction,
                NativeDimensionOperation::Fetch { quiet: true },
            ) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            Value::Bool(
                value.is_some_and(|value| !matches!(value, Value::Null | Value::Uninitialized)),
            )
        }
        php_ir::InstructionKind::EmptyStaticPropertyDim { dims, .. } => {
            let value = match native_dimension_path_value(
                context,
                Some(result),
                arguments,
                dims.len(),
                instruction,
                NativeDimensionOperation::Fetch { quiet: true },
            ) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            Value::Bool(value.is_none_or(|value| !native_property_truthy(&value)))
        }
        php_ir::InstructionKind::UnsetStaticPropertyDim { dims, .. } => {
            let keys = arguments
                .iter()
                .take(dims.len())
                .map(|encoded| {
                    context
                        .decode(*encoded)
                        .ok()
                        .and_then(|value| php_runtime::api::ArrayKey::from_value(&value))
                })
                .collect::<Option<Vec<_>>>();
            if let Some(keys) = keys {
                match result {
                    Value::Reference(reference) => {
                        let mut value = reference.get();
                        unset_native_array_dims(&mut value, &keys);
                        reference.set(value);
                        context.mark_roots_dirty(RootMutationReason::EnumOrStaticObject);
                    }
                    mut value => {
                        unset_native_array_dims(&mut value, &keys);
                        match context.store_direct_static_property_value(&key, value.clone()) {
                            Some(Ok(())) => {}
                            Some(Err(error)) => return Some(Err(error)),
                            None => {
                                context.static_properties.insert(key.clone(), value);
                                context.mark_roots_dirty(RootMutationReason::EnumOrStaticObject);
                            }
                        }
                    }
                }
            }
            Value::Null
        }
        php_ir::InstructionKind::FetchStaticProperty { .. }
        | php_ir::InstructionKind::FetchDynamicStaticProperty { .. }
        | php_ir::InstructionKind::AssignStaticProperty { .. }
        | php_ir::InstructionKind::AssignDynamicStaticProperty { .. } => {
            dereference_native_assignment_value(result)
        }
        php_ir::InstructionKind::BindReferenceStaticProperty { .. } => result,
        _ => result,
    };
    Some(context.encode(result))
}

fn native_dimension_path_value(
    context: &mut NativeRequestColdState<'_>,
    mut value: Option<Value>,
    arguments: &[i64],
    dimension_count: usize,
    source: &php_ir::Instruction,
    operation: NativeDimensionOperation,
) -> Result<Option<Value>, String> {
    if arguments.len() != dimension_count {
        return Ok(None);
    }
    for encoded in arguments {
        let Some(mut target) = value else {
            return Ok(None);
        };
        while let Value::Reference(reference) = target {
            target = reference.get();
        }
        let mut key = context.decode(*encoded)?;
        while let Value::Reference(reference) = key {
            key = reference.get();
        }
        emit_native_dimension_conversion_diagnostic(
            context,
            &target,
            &key,
            Some(source),
            operation,
        )?;
        let Some(key) = php_runtime::api::ArrayKey::from_value(&key) else {
            return Ok(None);
        };
        value = match target {
            Value::Array(array) => array.get(&key).cloned(),
            Value::Object(object) => native_simple_xml_dimension(&object, &key),
            _ => None,
        };
    }
    if let Some(mut value) = value {
        while let Value::Reference(reference) = value {
            value = reference.get();
        }
        Ok(Some(value))
    } else {
        Ok(None)
    }
}

fn native_property_truthy(value: &Value) -> bool {
    match value {
        Value::Null | Value::Uninitialized | Value::Bool(false) => false,
        Value::Int(0) => false,
        Value::Float(value) if value.to_f64() == 0.0 => false,
        Value::String(value) if value.as_bytes().is_empty() || value.as_bytes() == b"0" => false,
        Value::Array(value) if value.is_empty() => false,
        Value::Reference(reference) => native_property_truthy(&reference.get()),
        Value::Object(object) if native_simple_xml_empty(object).is_some() => {
            !native_simple_xml_empty(object).unwrap_or(true)
        }
        _ => true,
    }
}

fn native_property_is_set(value: &Value) -> bool {
    match value {
        Value::Null | Value::Uninitialized => false,
        Value::Reference(reference) => native_property_is_set(&reference.get()),
        _ => true,
    }
}

fn unset_native_array_dims(value: &mut Value, keys: &[php_runtime::api::ArrayKey]) {
    if let Value::Reference(reference) = value {
        let mut target = reference.get();
        unset_native_array_dims(&mut target, keys);
        reference.set(target);
        return;
    }
    let Some((key, rest)) = keys.split_first() else {
        return;
    };
    let Value::Array(array) = value else {
        return;
    };
    if rest.is_empty() {
        array.remove(key);
    } else if let Some(mut nested) = array.get_mut(key) {
        unset_native_array_dims(&mut nested, rest);
    }
}

fn assign_native_array_dims(
    value: &mut Value,
    keys: &[php_runtime::api::ArrayKey],
    replacement: Value,
    append: bool,
) {
    if let Value::Reference(reference) = value {
        let mut target = reference.get();
        assign_native_array_dims(&mut target, keys, replacement, append);
        reference.set(target);
        return;
    }
    if !matches!(value, Value::Array(_)) {
        *value = Value::Array(php_runtime::api::PhpArray::new());
    }
    let Value::Array(array) = value else {
        unreachable!("array value was initialized above")
    };
    let Some((key, rest)) = keys.split_first() else {
        if append {
            array.append(replacement);
        }
        return;
    };
    if rest.is_empty() && !append {
        if let Some(Value::Reference(reference)) = array.get(key).cloned() {
            reference.set(replacement);
        } else {
            array.insert(key.clone(), replacement);
        }
    } else {
        let mut nested = array.get(key).cloned().unwrap_or(Value::Null);
        assign_native_array_dims(&mut nested, rest, replacement, append);
        array.insert(key.clone(), nested);
    }
}

fn native_external_method(
    context: &NativeRequestColdState<'_>,
    class_name: &str,
    method: &str,
) -> Option<(NativeDynamicFunction, php_ir::module::ClassMethodEntry)> {
    let (mut unit, mut class) =
        native_external_class_handle(context, class_name).or_else(|| {
            let local = context
                .unit
                .classes
                .iter()
                .find(|class| class.name == normalize_class_name(class_name))?;
            native_external_class_handle(context, local.parent.as_deref()?)
        })?;
    loop {
        if let Some(entry) = class
            .methods
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(method))
            .cloned()
        {
            return Some((
                NativeDynamicFunction {
                    unit,
                    function: entry.function,
                },
                entry,
            ));
        }
        let parent = class.parent.as_deref()?;
        let normalized_parent = normalize_class_name(parent);
        let (parent_unit, parent_class) = context
            .current_dynamic_unit
            .and_then(|unit| {
                context
                    .dynamic_units
                    .get(unit)?
                    .compiled
                    .lookup_unit_class_handle(&normalized_parent)
                    .map(|class| (unit, class))
            })
            .or_else(|| native_external_class_handle(context, parent))?;
        unit = parent_unit;
        class = parent_class;
    }
}

fn create_native_external_object(
    context: &mut NativeRequestColdState<'_>,
    class_name: &str,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    let (unit, class) = native_external_class_handle(context, class_name)
        .ok_or_else(|| format!("E_PHP_VM_UNKNOWN_CLASS: Class {class_name} not found"))?;
    if class.flags.is_abstract
        || class.flags.is_interface
        || class.flags.is_trait
        || class.flags.is_enum
    {
        return Err(format!(
            "Cannot instantiate {} {}",
            class_name, class.display_name
        ));
    }
    native_prepare_runtime_class_constants(context, Some(unit), &class, source)?;
    let object = new_native_object(context, Some(unit), &class)?;
    let receiver = context.encode_native_object_owner(object)?;
    if let Some((constructor, _)) = native_external_method(context, class_name, "__construct") {
        let mut constructor_arguments = Vec::with_capacity(arguments.len() + 1);
        constructor_arguments.push(receiver);
        constructor_arguments.extend_from_slice(arguments);
        let _ = invoke_native_resolved_external_function(
            context,
            constructor,
            &constructor_arguments,
            Some(class.name.clone()),
            context.unit.strict_types,
        )?;
    }
    Ok(receiver)
}

fn native_coerce_call_argument(value: Value, type_: &php_ir::IrReturnType, strict: bool) -> Value {
    use php_ir::IrReturnType as Type;
    if let Value::Reference(reference) = &value {
        return Value::Reference(reference.clone());
    }
    if matches!(type_, Type::Float)
        && let Value::Int(value) = value
    {
        return Value::Float(php_runtime::api::FloatValue::from_f64(value as f64));
    }
    if strict || native_value_matches_ir_type(&value, type_) {
        return value;
    }
    match (type_, value) {
        (Type::Int, Value::String(value)) => value
            .to_string_lossy()
            .trim()
            .parse::<i64>()
            .map(Value::Int)
            .unwrap_or(Value::String(value)),
        (Type::Int, Value::Float(value)) => Value::Int(value.to_f64() as i64),
        (Type::Int, Value::Bool(value)) => Value::Int(i64::from(value)),
        (Type::Float, Value::String(value)) => value
            .to_string_lossy()
            .trim()
            .parse::<f64>()
            .map(|value| Value::Float(php_runtime::api::FloatValue::from_f64(value)))
            .unwrap_or(Value::String(value)),
        (Type::Float, Value::Bool(value)) => {
            Value::Float(php_runtime::api::FloatValue::from_f64(if value {
                1.0
            } else {
                0.0
            }))
        }
        (Type::String, Value::Int(value)) => {
            Value::String(PhpString::from_bytes(value.to_string().into_bytes()))
        }
        (Type::String, Value::Float(value)) => Value::String(PhpString::from_bytes(
            value.to_f64().to_string().into_bytes(),
        )),
        (Type::String, Value::Bool(value)) => Value::String(PhpString::from_bytes(if value {
            b"1".to_vec()
        } else {
            Vec::new()
        })),
        (Type::Bool, value @ (Value::Int(_) | Value::Float(_) | Value::String(_))) => {
            Value::Bool(native_property_truthy(&value))
        }
        (Type::Nullable { inner }, value) => native_coerce_call_argument(value, inner, strict),
        (Type::Union { members }, value) => members
            .iter()
            .map(|member| native_coerce_call_argument(value.clone(), member, strict))
            .find(|candidate| native_value_matches_ir_type(candidate, type_))
            .unwrap_or(value),
        (_, value) => value,
    }
}

fn native_function_has_implicit_closure_this(function: &php_ir::IrFunction) -> bool {
    function.flags.is_closure
        && !function.flags.is_static
        && function.locals.first().is_some_and(|name| name == "this")
        && !function
            .captures
            .iter()
            .any(|capture| capture.local == php_ir::LocalId::new(0))
}

#[cfg(test)]
fn native_backtrace_frame(
    compiled: &crate::compiled_unit::CompiledUnit,
    function: php_ir::FunctionId,
    called_class: Option<Arc<str>>,
    object: Option<php_runtime::api::ObjectRef>,
    arguments: request_state::NativeTraceArguments,
) -> NativeBacktraceFrame {
    let metadata = NativeFunctionMetadataPtr::from_compiled(compiled, function);
    native_backtrace_frame_from_metadata(metadata, called_class, object, arguments)
}

fn native_backtrace_frame_from_metadata(
    metadata: Option<NativeFunctionMetadataPtr>,
    called_class: Option<Arc<str>>,
    object: Option<php_runtime::api::ObjectRef>,
    arguments: request_state::NativeTraceArguments,
) -> NativeBacktraceFrame {
    let class = metadata.as_ref().and_then(|metadata| {
        metadata
            .trace_class
            .as_ref()
            .map(|class| called_class.unwrap_or_else(|| Arc::clone(class)))
    });
    NativeBacktraceFrame {
        metadata,
        class,
        object,
        arguments,
    }
}

fn invoke_native_external_function(
    context: &mut NativeRequestColdState<'_>,
    target: NativeDynamicFunction,
    arguments: &[i64],
    called_class: Option<String>,
    strict: bool,
) -> NativeCallResult {
    invoke_native_external_function_with_metadata(
        context,
        target,
        arguments,
        None,
        called_class,
        strict,
    )
}

fn invoke_native_resolved_external_function(
    context: &mut NativeRequestColdState<'_>,
    target: NativeDynamicFunction,
    arguments: &[i64],
    called_class: Option<String>,
    strict: bool,
) -> NativeCallResult {
    invoke_native_resolved_external_function_with_metadata(
        context,
        target,
        arguments,
        None,
        called_class,
        strict,
    )
}

fn invoke_native_external_function_with_metadata(
    context: &mut NativeRequestColdState<'_>,
    target: NativeDynamicFunction,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    called_class: Option<String>,
    strict: bool,
) -> NativeCallResult {
    invoke_native_external_function_with_metadata_at_tier(
        context,
        target,
        arguments,
        metadata,
        called_class,
        strict,
        false,
        NativeCallableBuiltinPolicy::ExecuteBaseline,
    )
}

fn invoke_native_external_function_with_metadata_policy(
    context: &mut NativeRequestColdState<'_>,
    target: NativeDynamicFunction,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    called_class: Option<String>,
    strict: bool,
    builtin_policy: NativeCallableBuiltinPolicy,
) -> NativeCallResult {
    invoke_native_external_function_with_metadata_at_tier(
        context,
        target,
        arguments,
        metadata,
        called_class,
        strict,
        false,
        builtin_policy,
    )
}

fn invoke_native_resolved_external_function_with_metadata(
    context: &mut NativeRequestColdState<'_>,
    target: NativeDynamicFunction,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    called_class: Option<String>,
    strict: bool,
) -> NativeCallResult {
    invoke_native_external_function_with_metadata_at_tier(
        context,
        target,
        arguments,
        metadata,
        called_class,
        strict,
        true,
        NativeCallableBuiltinPolicy::ExecuteBaseline,
    )
}

fn invoke_native_external_function_with_metadata_at_tier(
    context: &mut NativeRequestColdState<'_>,
    target: NativeDynamicFunction,
    arguments: &[i64],
    metadata: Option<&[php_ir::instruction::IrCallArg]>,
    called_class: Option<String>,
    strict: bool,
    baseline_continuation: bool,
    builtin_policy: NativeCallableBuiltinPolicy,
) -> NativeCallResult {
    prepare_dynamic_native_entry(context, target.unit, target.function)?;
    if builtin_policy == NativeCallableBuiltinPolicy::RequireBaseline
        && !authoritative_native_call_arguments_are_admitted(context, arguments, metadata)
    {
        return Err(NativeCallControl::BaselineRequired);
    }
    let transferred_arguments = arguments
        .iter()
        .map(|argument| {
            let encoded = *argument;
            let unit_local_constant = php_jit::jit_decode_constant(encoded).is_some_and(|index| {
                index != u32::MAX
                    && index != php_jit::JIT_VALUE_UNINITIALIZED
                    && index != php_jit::JIT_VALUE_FALSE
                    && index != php_jit::JIT_VALUE_TRUE
            });
            let direct_array = NativeRequestColdState::direct_value_index(encoded)
                .and_then(|index| context.direct_value_slots.get(index))
                .is_some_and(|slot| {
                    slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY
                });
            if unit_local_constant {
                // Constant indexes are scoped to the caller's IrUnit. Publish
                // scalar/string literals directly into request-wide native
                // storage before switching the active unit.
                context.stabilize_active_unit_constant(
                    php_jit::jit_decode_constant(encoded)
                        .expect("unit-local constant was classified above"),
                )
            } else if direct_array {
                // Do not rebuild the entire native array through Rust
                // `Value`.  Stabilize only embedded unit-local constants in
                // the authoritative slots, then share the same COW handle
                // with the external callee.
                context.stabilize_direct_array_for_cross_unit(encoded)?;
                context.retain(encoded).map(|()| encoded)
            } else {
                let duplicated = context.duplicate_authoritative_native_value(encoded)?;
                match duplicated {
                    Some(value) => Ok(value),
                    None if builtin_policy == NativeCallableBuiltinPolicy::RequireBaseline => Err(
                        "authoritative external call argument unexpectedly disappeared".to_owned(),
                    ),
                    None => context.duplicate_baseline_call_argument(encoded),
                }
            }
        })
        .collect::<Result<smallvec::SmallVec<[i64; 8]>, _>>()?;
    context.with_active_dynamic_unit(target.unit, |context| {
        let pushed_called_class = called_class.is_some();
        if let Some(called_class) = &called_class {
            context
                .called_classes
                .push(Arc::from(called_class.as_str()));
        }
        let result = if baseline_continuation {
            invoke_native_resolved_function_with_metadata_strict(
                context,
                target.function,
                &transferred_arguments,
                metadata,
                strict,
            )
        } else {
            invoke_native_function_with_metadata_strict_at_tier(
                context,
                target.function,
                &transferred_arguments,
                metadata,
                strict,
                false,
                builtin_policy,
            )
        };
        if pushed_called_class {
            context.called_classes.pop();
        }
        match result {
            Ok(encoded) => Ok(context.transfer_external_return(encoded, target.unit)?),
            Err(NativeCallControl::Exit(encoded)) => {
                let encoded = context.transfer_external_return(encoded, target.unit)?;
                Err(NativeCallControl::Exit(encoded))
            }
            Err(control) => Err(control),
        }
    })?
}

fn native_value_with_owner_unit(value: Value, owner_unit: Option<usize>) -> Value {
    match value {
        Value::Callable(callable) => match callable.as_ref() {
            php_runtime::api::CallableValue::Closure(closure)
                if closure.context.owner_unit.is_none() && owner_unit.is_some() =>
            {
                Value::Callable(Box::new(php_runtime::api::CallableValue::Closure(
                    closure.clone().with_owner_unit(owner_unit),
                )))
            }
            _ => Value::Callable(callable),
        },
        value => value,
    }
}

fn invoke_native_method(
    context: &mut NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
) -> NativeCallResult {
    invoke_native_method_with_trace_arguments(context, function, arguments, None)
}

fn invoke_native_method_with_trace_arguments(
    context: &mut NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
    trace_arguments: Option<request_state::NativeTraceArguments>,
) -> NativeCallResult {
    let metadata = NativeFunctionMetadataPtr::from_compiled(&context.compiled, function);
    invoke_native_method_with_prepared_trace_arguments(
        context,
        function,
        arguments,
        trace_arguments,
        metadata,
        false,
    )
}

fn invoke_native_method_with_prepared_trace_arguments(
    context: &mut NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
    arguments: &[i64],
    trace_arguments: Option<request_state::NativeTraceArguments>,
    metadata: Option<NativeFunctionMetadataPtr>,
    baseline_only: bool,
) -> NativeCallResult {
    let function_name = metadata
        .as_ref()
        .map_or("<unknown>", |metadata| metadata.name.as_ref());
    if context.call_frames.len() >= NATIVE_CALL_DEPTH_LIMIT {
        return Err(format!(
            "E_PHP_NATIVE_CALL_DEPTH: maximum native call depth of {NATIVE_CALL_DEPTH_LIMIT} exceeded in {function_name}()"
        )
        .into());
    }
    let handle = if baseline_only {
        // Runtime-resolved targets are the dynamic call boundary. Enter their
        // published baseline-native artifact so deeper guard exits stay inside
        // the callee's native continuation chain. Normal/top-level invocation
        // and statically stable calls keep their optimizing entries.
        ensure_native_baseline_entry(context, function)?
    } else {
        ensure_native_entry(context, function)?
    };
    let instance_method = metadata
        .as_ref()
        .is_some_and(|metadata| metadata.instance_method);
    let object = if instance_method {
        arguments.first().and_then(|receiver| {
            // The receiver's slot-parallel owner already carries the stable
            // object identity and class metadata.  Decoding a direct object
            // here used to demote every authoritative declared slot merely
            // to build a backtrace frame, after which the callee promoted the
            // complete object graph again.  Only non-direct compatibility
            // handles need the cold decoding path.
            context.native_query_object(*receiver).or_else(|| {
                context
                    .decode(*receiver)
                    .ok()
                    .and_then(|receiver| match receiver {
                        Value::Object(object) => Some(object),
                        Value::Reference(reference) => match reference.get() {
                            Value::Object(object) => Some(object),
                            _ => None,
                        },
                        _ => None,
                    })
            })
        })
    } else {
        None
    };
    let called_class = object
        .as_ref()
        .map(php_runtime::api::ObjectRef::class_name_handle)
        .or_else(|| context.called_classes.last().cloned());
    let pushed_called_class = called_class.is_some();
    if let Some(class) = called_class.as_ref() {
        context.called_classes.push(Arc::clone(class));
    }
    let leading = metadata.as_ref().map_or(0, |metadata| {
        metadata.capture_count
            + usize::from(instance_method)
            + usize::from(metadata.implicit_closure_this)
    });
    let frame_arguments = trace_arguments.map_or_else(
        || {
            arguments
                .iter()
                .skip(leading)
                .copied()
                .collect::<request_state::NativeTraceArguments>()
        },
        |arguments| arguments,
    );
    context
        .call_frames
        .push(native_backtrace_frame_from_metadata(
            metadata,
            called_class,
            object,
            frame_arguments,
        ));
    let transition_started_at = context.options.collect_counters.then(|| {
        (
            std::time::Instant::now(),
            context.active_helper_child_time_nanos(),
        )
    });
    context.record_native_direct_calls(&handle);
    let runtime = context.native_runtime_ptr();
    let outcome = handle.invoke_i64_with_native_unwind_runtime(
        arguments,
        php_jit::JIT_RUNTIME_ABI_HASH,
        runtime,
        |types, value| native_catch_matches(context, types, value),
    );
    let outcome = resume_native_optimizing_exit(context, outcome);
    if let Some((started_at, child_time_before)) = transition_started_at {
        let nested_helper_time = context
            .active_helper_child_time_nanos()
            .saturating_sub(child_time_before);
        context.record_native_transition("same_unit", started_at.elapsed(), nested_helper_time);
    }
    let completed_frame = context
        .call_frames
        .pop()
        .expect("native call frame stack underflow");
    if pushed_called_class {
        context.called_classes.pop();
    }
    match outcome {
        Ok(php_jit::JitI64InvokeOutcome::Returned(value)) => {
            let returns_by_ref = context
                .unit
                .functions
                .get(function.index())
                .is_some_and(|function| function.returns_by_ref);
            if returns_by_ref {
                let target = &context.unit.functions[function.index()];
                let span = target
                    .blocks
                    .iter()
                    .filter_map(|block| block.terminator.as_ref())
                    .find(|terminator| {
                        matches!(
                            terminator.kind,
                            php_ir::instruction::TerminatorKind::Return {
                                by_ref_local: None,
                                ..
                            }
                        )
                    })
                    .map_or(target.span, |terminator| terminator.span);
                let path = context
                    .unit
                    .files
                    .get(span.file.index())
                    .map_or("<unknown>", |file| file.path.as_str());
                let line = std::fs::read(path).ok().map_or(1, |bytes| {
                    bytes
                        .iter()
                        .take(span.start as usize)
                        .filter(|byte| **byte == b'\n')
                        .count()
                        + 1
                });
                context.output.write_bytes(format!(
                    "\nNotice: Only variable references should be returned by reference in {path} on line {line}\n"
                ));
                let value = context.decode(value)?;
                return Ok(context
                    .encode_native_reference_owner(php_runtime::api::ReferenceCell::new(value))?);
            }
            Ok(value)
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit { status, value, .. })
            if status == php_jit::JitCallStatus::RETURN_REFERENCE.0 as i32 =>
        {
            Ok(value)
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit {
            status,
            value,
            state,
        }) if status == php_jit::JitCallStatus::THROW.0 as i32 => {
            let throwable = context.decode(value).map_err(|error| {
                let continuation = context
                    .instruction_for_continuation(state.function_id, state.continuation_id)
                    .map(|instruction| format!(" at {:?}", instruction.kind))
                    .unwrap_or_else(|| {
                        format!(
                            " at native continuation {}:{}",
                            state.function_id, state.continuation_id
                        )
                    });
                format!(
                    "native method {function_name} returned an undecodable throwable {value}{continuation}: {error}"
                )
            })?;
            let arguments = completed_frame
                .arguments
                .iter()
                .map(|argument| context.decode(*argument))
                .collect::<Result<Vec<_>, _>>()?;
            context.pending_throwable = Some(native_throwable_with_frame(
                throwable,
                &function_name,
                arguments,
            ));
            context.mark_roots_dirty(RootMutationReason::PendingThrowable);
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
            if context.diagnostic.is_some() {
                // The callee has already published the PHP diagnostic in the
                // shared execution context. Preserve that diagnostic and
                // carry the status through the call trampoline unchanged.
                Err(NativeCallControl::PublishedRuntimeError)
            } else {
                let continuation = context
                    .instruction_for_continuation(state.function_id, state.continuation_id)
                    .map(|instruction| format!(" at {:?}", instruction.kind))
                    .unwrap_or_else(|| {
                        format!(
                            " at native continuation {}:{}",
                            state.function_id, state.continuation_id
                        )
                    });
                Err(NativeCallControl::RuntimeError(format!(
                    "native method {function_name} returned a runtime error{continuation} (control_reserved={:#x}, control_value={}, native_version={}, direct_values={}/{}, direct_array_entries={}/{}, direct_string_bytes={}/{})",
                    state.control_reserved,
                    state.control_value,
                    state.native_version,
                    *context.direct_value_next,
                    context.direct_value_slots.len(),
                    *context.direct_array_next,
                    context.direct_array_entries.len(),
                    *context.direct_string_next,
                    context.direct_string_bytes.len(),
                )))
            }
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit {
            status,
            value,
            state,
        }) if status == php_jit::JitCallStatus::SUSPEND_FIBER.0 as i32
            && context.active_fiber.is_some() =>
        {
            context.pending_fiber_suspension_value = Some(value);
            Err(NativeCallControl::SuspendFiber {
                state: Some(Box::new(state)),
            })
        }
        Ok(php_jit::JitI64InvokeOutcome::SideExit { status, state, .. }) => {
            let continuation = context
                .instruction_for_continuation(state.function_id, state.continuation_id)
                .map(|instruction| format!(" at {:?}", instruction.kind))
                .unwrap_or_else(|| {
                    format!(
                        " at native continuation {}:{}",
                        state.function_id, state.continuation_id
                    )
                });
            let diagnostic = context
                .diagnostic
                .as_ref()
                .map(|diagnostic| format!(": {}", diagnostic.message()))
                .unwrap_or_default();
            Err(NativeCallControl::RuntimeError(format!(
                "native method {function_name} returned status {status}{continuation}{diagnostic}"
            )))
        }
        Err(error) => Err(NativeCallControl::RuntimeError(format!(
            "native method invocation failed: {error:?}"
        ))),
    }
}

pub(super) fn resume_native_optimizing_exit(
    context: &mut NativeRequestColdState<'_>,
    mut outcome: Result<php_jit::JitI64InvokeOutcome, php_jit::JitInvokeError>,
) -> Result<php_jit::JitI64InvokeOutcome, php_jit::JitInvokeError> {
    loop {
        let Ok(php_jit::JitI64InvokeOutcome::SideExit { status, state, .. }) = &outcome else {
            return outcome;
        };
        if *status != php_jit::JitCallStatus::RECOMPILE_REQUESTED.0 as i32 {
            return outcome;
        }
        let transition_instruction =
            context.instruction_for_continuation(state.function_id, state.continuation_id);
        let mut transition_reason = transition_instruction
            .as_ref()
            .map(|instruction| native_optimizing_transition_reason(&instruction.kind))
            .unwrap_or_else(|| std::borrow::Cow::Borrowed("optimizer_unknown"));
        if transition_reason.as_ref() == "optimizer_array:IssetDim" {
            let mut detail = match state.control_reserved {
                php_jit::JIT_OPTIMIZING_EXIT_ARRAY_NOT_TAGGED => "not_tagged",
                php_jit::JIT_OPTIMIZING_EXIT_ARRAY_VIEW_MISSING => "view_missing",
                php_jit::JIT_OPTIMIZING_EXIT_ARRAY_KEY_UNSUPPORTED => "key_unsupported",
                _ => "unknown",
            }
            .to_owned();
            if state.control_reserved == php_jit::JIT_OPTIMIZING_EXIT_ARRAY_NOT_TAGGED
                && let Some(instruction) = transition_instruction.as_ref()
                && let php_ir::InstructionKind::IssetDim { local, .. } = &instruction.kind
                && state.local_initialized(*local)
            {
                detail.push(':');
                detail.push_str(native_transition_value_kind(state.slots[local.index()]));
            }
            transition_reason =
                std::borrow::Cow::Owned(format!("{}:{detail}", transition_reason.as_ref()));
        } else if transition_reason.as_ref() == "optimizer_local:LoadLocal"
            && let Some(instruction) = transition_instruction.as_ref()
            && let php_ir::InstructionKind::LoadLocal { local, .. } = &instruction.kind
            && state.local_initialized(*local)
        {
            let stored = native_transition_stored_value_kind(context, state.slots[local.index()]);
            let next = context
                .instruction_for_continuation(
                    state.function_id,
                    state.continuation_id.saturating_add(1),
                )
                .map(|instruction| {
                    let rendered = format!("{:?}", instruction.kind);
                    rendered
                        .split_once([' ', '{', '('])
                        .map_or(rendered.as_str(), |(name, _)| name)
                        .to_owned()
                })
                .unwrap_or_else(|| "terminal".to_owned());
            transition_reason = std::borrow::Cow::Owned(format!(
                "{}:{stored}:next_{next}",
                transition_reason.as_ref()
            ));
        } else if transition_reason.as_ref() == "optimizer_array:AssignDim"
            && let Some(instruction) = transition_instruction.as_ref()
            && let php_ir::InstructionKind::AssignDim { local, .. } = &instruction.kind
            && state.local_initialized(*local)
        {
            let encoded = state.slots[local.index()];
            let raw = native_transition_value_kind(encoded);
            let stored = native_transition_stored_value_kind(context, encoded);
            let descriptor = php_jit::jit_decode_runtime_value(encoded).map_or_else(
                || "immediate".to_owned(),
                |index| {
                    if index >= php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE {
                        return context
                            .direct_value_slots
                            .get((index - php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE) as usize)
                            .map_or_else(
                                || "direct_missing".to_owned(),
                                |slot| format!("direct_kind_{}_refs_{}", slot.kind, slot.refcount),
                            );
                    }
                    match context.values.get(index as usize).and_then(Option::as_ref) {
                        Some(NativeStoredValue::Php(Value::Reference(reference))) => reference
                            .try_with_value(|value| match value {
                                Value::Array(array) => format!(
                                    "reference_storage_refs_{}",
                                    array.gc_refcount_estimate()
                                ),
                                _ => "reference_non_array".to_owned(),
                            })
                            .unwrap_or_else(|_| "reference_borrowed".to_owned()),
                        _ => "table".to_owned(),
                    }
                },
            );
            transition_reason = std::borrow::Cow::Owned(format!(
                "{}:{raw}:{stored}:{descriptor}",
                transition_reason.as_ref()
            ));
        } else if transition_reason
            .as_ref()
            .starts_with("optimizer_call:CallFunction:")
            && let Some(instruction) = transition_instruction.as_ref()
            && let php_ir::InstructionKind::CallFunction { args, .. } = &instruction.kind
        {
            let values = args
                .iter()
                .take(4)
                .map(|argument| {
                    let encoded = match argument.value {
                        php_ir::Operand::Local(local) if state.local_initialized(local) => {
                            Some(state.slots[local.index()])
                        }
                        php_ir::Operand::Register(register) => (0
                            ..php_jit::JIT_DEOPT_MAX_REGISTERS)
                            .find(|index| {
                                state.initialized_register_mask & (1_u64 << index) != 0
                                    && state.register_ids[*index] == register.raw()
                            })
                            .map(|index| state.registers[index]),
                        php_ir::Operand::Constant(_) | php_ir::Operand::Local(_) => None,
                    };
                    encoded.map_or_else(
                        || "constant_or_unpublished".to_owned(),
                        |encoded| {
                            format!(
                                "{}/{}",
                                native_transition_value_kind(encoded),
                                native_transition_stored_value_kind(context, encoded),
                            )
                        },
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            transition_reason = std::borrow::Cow::Owned(format!(
                "{}:values_{values}:detail_{:#x}",
                transition_reason.as_ref(),
                state.control_reserved,
            ));
        }
        let transition_started = context
            .options
            .collect_counters
            .then(std::time::Instant::now);
        let function = php_ir::FunctionId::new(state.function_id);
        let baseline = ensure_native_baseline_entry(context, function).map_err(|_| {
            php_jit::JitInvokeError::MissingNativeTransition {
                function: state.function_id,
                continuation: state.continuation_id,
            }
        })?;
        let runtime = context.native_runtime_ptr();
        outcome = baseline.invoke_i64_native_transition_with_unwind_runtime(
            state,
            php_jit::JIT_RUNTIME_ABI_HASH,
            runtime,
            |types, value| native_catch_matches(context, types, value),
        );
        if let Some(started) = transition_started {
            context.record_native_transition(transition_reason.as_ref(), started.elapsed(), 0);
        }
    }
}

fn native_transition_value_kind(encoded: i64) -> &'static str {
    let encoded = encoded as u64;
    match encoded & php_jit::JIT_VALUE_RUNTIME_KIND_MASK {
        php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG => "reference",
        php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG => "array",
        php_jit::JIT_VALUE_RUNTIME_OBJECT_TAG => "object",
        php_jit::JIT_VALUE_RUNTIME_STRING_TAG => "string",
        php_jit::JIT_VALUE_RUNTIME_FLOAT_TAG => "float",
        php_jit::JIT_VALUE_RUNTIME_RESOURCE_TAG => "resource",
        php_jit::JIT_VALUE_RUNTIME_CALLABLE_TAG => "callable",
        php_jit::JIT_VALUE_RUNTIME_GENERATOR_TAG => "generator",
        php_jit::JIT_VALUE_RUNTIME_FIBER_TAG => "fiber",
        php_jit::JIT_VALUE_RUNTIME_ITERATOR_TAG => "iterator",
        _ if encoded == php_jit::jit_encode_constant(u32::MAX) as u64 => "null",
        _ if encoded & php_jit::JIT_VALUE_TAG_MASK == php_jit::JIT_VALUE_CONSTANT_TAG => "constant",
        _ => "immediate",
    }
}

fn native_transition_stored_value_kind(
    context: &NativeRequestColdState<'_>,
    encoded: i64,
) -> &'static str {
    if let Some(index) = NativeRequestColdState::direct_value_index(encoded)
        && context.direct_value_slots.get(index).is_some_and(|slot| {
            slot.refcount != 0 && slot.kind == php_jit::JIT_NATIVE_VALUE_VIEW_PREPARED_CALLABLE
        })
    {
        return "prepared_callable";
    }
    let Some(index) = php_jit::jit_decode_runtime_value(encoded) else {
        return native_transition_value_kind(encoded);
    };
    match context.values.get(index as usize).and_then(Option::as_ref) {
        Some(NativeStoredValue::Php(Value::Reference(reference))) => reference
            .try_with_value(native_value_type_name)
            .unwrap_or("borrowed_reference"),
        Some(NativeStoredValue::Php(value)) => native_value_type_name(value),
        Some(NativeStoredValue::GlobalsProxy) => "globals_proxy",
        Some(NativeStoredValue::ArrayIterator(_)) => "array_iterator",
        Some(NativeStoredValue::Iterator(_)) => "iterator",
        Some(NativeStoredValue::GeneratorIterator(_)) => "generator_iterator",
        None => "missing",
    }
}

fn native_optimizing_transition_reason(
    kind: &php_ir::InstructionKind,
) -> std::borrow::Cow<'static, str> {
    use php_ir::InstructionKind;

    let family = match kind {
        InstructionKind::LoadLocal { .. }
        | InstructionKind::StoreLocal { .. }
        | InstructionKind::Discard { .. }
        | InstructionKind::IssetLocal { .. }
        | InstructionKind::EmptyLocal { .. }
        | InstructionKind::UnsetLocal { .. } => "optimizer_local",
        InstructionKind::Unary { .. }
        | InstructionKind::Binary { .. }
        | InstructionKind::Compare { .. }
        | InstructionKind::Cast { .. } => "optimizer_scalar",
        InstructionKind::NewArray { .. }
        | InstructionKind::ArrayInsert { .. }
        | InstructionKind::ArraySpread { .. }
        | InstructionKind::FetchDim { .. }
        | InstructionKind::AssignDim { .. }
        | InstructionKind::AppendDim { .. }
        | InstructionKind::UnsetDim { .. }
        | InstructionKind::IssetDim { .. }
        | InstructionKind::EmptyDim { .. } => "optimizer_array",
        InstructionKind::ForeachInit { .. }
        | InstructionKind::ForeachInitRef { .. }
        | InstructionKind::ForeachNext { .. }
        | InstructionKind::ForeachNextRef { .. }
        | InstructionKind::ForeachCleanup { .. } => "optimizer_foreach",
        InstructionKind::FetchProperty { .. }
        | InstructionKind::AssignProperty { .. }
        | InstructionKind::FetchDynamicStaticProperty { .. }
        | InstructionKind::AssignDynamicStaticProperty { .. }
        | InstructionKind::FetchObjectClassName { .. } => "optimizer_property",
        InstructionKind::BindReference { .. }
        | InstructionKind::BindReferenceDim { .. }
        | InstructionKind::BindReferenceProperty { .. }
        | InstructionKind::BindReferenceFromProperty { .. }
        | InstructionKind::BindReferenceFromPropertyDim { .. }
        | InstructionKind::BindReferencePropertyDim { .. }
        | InstructionKind::BindReferenceDimFromProperty { .. }
        | InstructionKind::BindReferenceFromDim { .. }
        | InstructionKind::BindReferenceFromStaticPropertyDim { .. }
        | InstructionKind::BindReferenceStaticProperty { .. }
        | InstructionKind::BindReferenceFromCall { .. }
        | InstructionKind::BindReferenceFromMethodCall { .. } => "optimizer_reference",
        InstructionKind::CallFunction { .. }
        | InstructionKind::CallMethod { .. }
        | InstructionKind::CallStaticMethod { .. }
        | InstructionKind::CallClosure { .. }
        | InstructionKind::CallCallable { .. }
        | InstructionKind::Pipe { .. }
        | InstructionKind::NewObject { .. }
        | InstructionKind::DynamicNewObject { .. } => "optimizer_call",
        InstructionKind::Include { .. }
        | InstructionKind::Eval { .. }
        | InstructionKind::DeclareFunction { .. }
        | InstructionKind::DeclareClass { .. } => "optimizer_dynamic_code",
        _ => "optimizer_other",
    };
    // This runs only while diagnostic counters are enabled. Preserve the
    // exact IR opcode, but not its operands, so an aggregate family cannot
    // hide the next dominant warm transition after an earlier exit is
    // removed.
    if let InstructionKind::Binary { op, .. } = kind {
        return format!("{family}:Binary:{op:?}").into();
    }
    if let InstructionKind::CallFunction { name, args, .. } = kind {
        let named = args
            .iter()
            .filter(|argument| argument.name.is_some())
            .count();
        let unpacked = args.iter().filter(|argument| argument.unpack).count();
        return format!(
            "{family}:CallFunction:{}:argc{}:named{}:unpack{}",
            name.trim_start_matches('\\').to_ascii_lowercase(),
            args.len(),
            named,
            unpacked,
        )
        .into();
    }
    let debug = format!("{kind:?}");
    let end = debug
        .find(|character: char| matches!(character, ' ' | '{' | '('))
        .unwrap_or(debug.len());
    format!("{family}:{}", &debug[..end]).into()
}

fn invoke_native_property_magic(
    context: &mut NativeRequestColdState<'_>,
    class: &php_ir::module::ClassEntry,
    receiver: i64,
    property: &str,
    magic: &str,
    caller_function: u32,
) -> Result<Option<Value>, String> {
    let Some(method) = class
        .methods
        .iter()
        .find(|method| method.name.eq_ignore_ascii_case(magic))
    else {
        return Ok(None);
    };
    if method.function.raw() == caller_function {
        return Ok(None);
    }
    let name =
        context.encode_native_string_owner(PhpString::from_bytes(property.as_bytes().to_vec()))?;
    let value = invoke_native_method(context, method.function, &[receiver, name])?;
    context.decode(value).map(Some)
}

fn execute_native_property_instruction(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
    caller_function: u32,
    trusted_continuation: Option<u32>,
) -> Option<Result<i64, String>> {
    use php_ir::InstructionKind;
    let (object, property, dynamic_property) = match &instruction.kind {
        InstructionKind::FetchDynamicProperty { .. }
        | InstructionKind::IssetDynamicProperty { .. }
        | InstructionKind::EmptyDynamicProperty { .. }
        | InstructionKind::IssetDynamicPropertyDim { .. }
        | InstructionKind::EmptyDynamicPropertyDim { .. }
        | InstructionKind::AssignDynamicProperty { .. }
        | InstructionKind::UnsetDynamicProperty { .. } => {
            let [object, property, ..] = arguments else {
                return Some(Err("dynamic property operands are missing".to_owned()));
            };
            (*object, String::new(), Some(*property))
        }
        InstructionKind::IssetProperty {
            object: _,
            property,
            ..
        }
        | InstructionKind::EmptyProperty {
            object: _,
            property,
            ..
        }
        | InstructionKind::UnsetProperty {
            object: _,
            property,
            ..
        }
        | InstructionKind::UnsetPropertyDim {
            object: _,
            property,
            ..
        }
        | InstructionKind::AssignPropertyDim {
            object: _,
            property,
            ..
        }
        | InstructionKind::IssetPropertyDim {
            object: _,
            property,
            ..
        }
        | InstructionKind::EmptyPropertyDim {
            object: _,
            property,
            ..
        } => {
            let [object, ..] = arguments else {
                return Some(Err("property object operand is missing".to_owned()));
            };
            (*object, property.clone(), None)
        }
        _ => return None,
    };
    let property = if let Some(property) = dynamic_property {
        match context.decode(property).and_then(native_string) {
            Ok(property) => String::from_utf8_lossy(&property).into_owned(),
            Err(error) => return Some(Err(error)),
        }
    } else {
        property
    };
    let object_encoded = object;
    let direct_query = match &instruction.kind {
        InstructionKind::IssetProperty { .. } | InstructionKind::IssetDynamicProperty { .. } => {
            Some((true, 0usize, 0usize))
        }
        InstructionKind::EmptyProperty { .. } | InstructionKind::EmptyDynamicProperty { .. } => {
            Some((false, 0usize, 0usize))
        }
        InstructionKind::IssetPropertyDim { dims, .. } => Some((true, 1usize, dims.len())),
        InstructionKind::EmptyPropertyDim { dims, .. } => Some((false, 1usize, dims.len())),
        InstructionKind::IssetDynamicPropertyDim { dims, .. } => Some((true, 2usize, dims.len())),
        InstructionKind::EmptyDynamicPropertyDim { dims, .. } => Some((false, 2usize, dims.len())),
        _ => None,
    };
    if let Some((isset, key_offset, key_count)) = direct_query
        && let Some(native_object) = context.native_query_object(object_encoded)
    {
        let normalized_class = normalize_class_name(&native_object.class_name());
        let local_class = native_active_class_handle(context, &normalized_class);
        let (owner_unit, class) = local_class.map_or_else(
            || {
                native_external_class_handle(context, &normalized_class)
                    .map_or((None, None), |(unit, class)| (Some(unit), Some(class)))
            },
            |class| (None, Some(class)),
        );
        let caller_owns_class_scope = owner_unit.is_none()
            && class.as_ref().is_some_and(|class| {
                class
                    .methods
                    .iter()
                    .any(|method| method.function.raw() == caller_function)
            });
        let entry = class
            .as_ref()
            .and_then(|class| class.properties.iter().find(|entry| entry.name == property));
        let accessible = entry.is_some_and(|entry| {
            (!entry.flags.is_private && !entry.flags.is_protected) || caller_owns_class_scope
        });
        if accessible
            && entry.is_none_or(|entry| entry.hooks.get.is_none())
            && let Some(slot) = context.native_declared_property_slot(object_encoded, &property)
        {
            let has_isset_magic = class.as_ref().is_some_and(|class| {
                class.methods.iter().any(|method| {
                    method.name.eq_ignore_ascii_case("__isset")
                        && method.function.raw() != caller_function
                })
            });
            if slot.initialized != 0 || !has_isset_magic {
                let classified = if slot.initialized == 0 {
                    Some(!isset)
                } else if key_count == 0 {
                    if isset {
                        context.native_encoded_is_set(slot.value)
                    } else {
                        context
                            .native_encoded_truthy(slot.value)
                            .map(|truthy| !truthy)
                    }
                } else {
                    let keys = arguments
                        .get(key_offset..key_offset.saturating_add(key_count))
                        .unwrap_or_default();
                    if keys.len() != key_count {
                        None
                    } else {
                        match context.direct_dimension_path_encoded(slot.value, keys) {
                            Ok(Some(Some(value))) => {
                                if isset {
                                    context.native_encoded_is_set(value)
                                } else {
                                    context.native_encoded_truthy(value).map(|truthy| !truthy)
                                }
                            }
                            Ok(Some(None)) => Some(!isset),
                            Ok(None) | Err(_) => None,
                        }
                    }
                };
                if let Some(result) = classified {
                    if let Some(continuation) = trusted_continuation
                        && let Err(error) = context.publish_direct_object_slots(
                            object_encoded,
                            &property,
                            0,
                            i64::from(caller_function),
                            i64::from(continuation),
                            php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_PUBLISHED,
                        )
                    {
                        return Some(Err(error.into()));
                    }
                    return Some(context.encode(Value::Bool(result)));
                }
            }
        }
        if entry.is_none()
            && native_object.has_dynamic_property(&property)
            && let Some(value) = native_object.get_property(&property)
        {
            let classified = if key_count == 0 {
                if isset {
                    Some(native_property_is_set(&value))
                } else {
                    Some(!native_property_truthy(&value))
                }
            } else {
                let keys = arguments
                    .get(key_offset..key_offset.saturating_add(key_count))
                    .unwrap_or_default();
                if keys.len() != key_count {
                    None
                } else {
                    match native_dimension_path_value(
                        context,
                        Some(value),
                        keys,
                        key_count,
                        instruction,
                        NativeDimensionOperation::Fetch { quiet: true },
                    ) {
                        Ok(value) if isset => {
                            Some(value.is_some_and(|value| native_property_is_set(&value)))
                        }
                        Ok(value) => {
                            Some(value.is_none_or(|value| !native_property_truthy(&value)))
                        }
                        Err(error) => return Some(Err(error)),
                    }
                }
            };
            if let Some(result) = classified {
                return Some(context.encode(Value::Bool(result)));
            }
        }
    }
    let closure_operand = context
        .unit
        .functions
        .get(caller_function as usize)
        .and_then(|function| {
            let object_register = match &instruction.kind {
                InstructionKind::AssignDynamicProperty {
                    object: php_ir::Operand::Register(register),
                    ..
                } => Some(*register),
                _ => None,
            }?;
            let local = function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .find_map(|candidate| match candidate.kind {
                    InstructionKind::LoadLocal { dst, local }
                    | InstructionKind::LoadLocalQuiet { dst, local }
                        if dst == object_register =>
                    {
                        Some(local)
                    }
                    _ => None,
                })?;
            function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(|candidate| match candidate.kind {
                    InstructionKind::StoreLocal {
                        local: target,
                        src: php_ir::Operand::Register(source),
                    } if target == local => function
                        .blocks
                        .iter()
                        .flat_map(|block| &block.instructions)
                        .any(|origin| {
                            matches!(origin.kind, InstructionKind::MakeClosure { dst, .. } if dst == source)
                        }),
                    _ => false,
                })
                .then_some(())
        })
        .is_some();
    let mut decoded_object = match context.decode(object) {
        Ok(value) => value,
        Err(error) => return Some(Err(error)),
    };
    for _ in 0..16 {
        let Value::Reference(reference) = decoded_object else {
            break;
        };
        decoded_object = reference.get();
    }
    if !matches!(decoded_object, Value::Object(_)) {
        let quiet_result = match instruction.kind {
            InstructionKind::IssetProperty { .. }
            | InstructionKind::IssetDynamicProperty { .. }
            | InstructionKind::IssetDynamicPropertyDim { .. }
            | InstructionKind::IssetPropertyDim { .. } => Some(false),
            InstructionKind::EmptyProperty { .. }
            | InstructionKind::EmptyDynamicProperty { .. }
            | InstructionKind::EmptyDynamicPropertyDim { .. }
            | InstructionKind::EmptyPropertyDim { .. } => Some(true),
            _ => None,
        };
        if let Some(value) = quiet_result {
            return Some(context.encode(Value::Bool(value)));
        }
    }
    let object = match decoded_object {
        Value::Object(object) => object,
        Value::Callable(_) => {
            return Some(Err(format!(
                "E_PHP_THROW:Error:Cannot create dynamic property Closure::${property}"
            )));
        }
        _ if closure_operand => {
            return Some(Err(format!(
                "E_PHP_THROW:Error:Cannot create dynamic property Closure::${property}"
            )));
        }
        value => {
            return Some(Err(format!(
                "Attempt to access property {property} on {}",
                native_value_type_name(&value)
            )));
        }
    };
    if let Err(error) = context.materialize_direct_object_alias(&object) {
        return Some(Err(error));
    }
    let normalized_class = normalize_class_name(&object.class_name());
    let class = native_active_class_handle(context, &normalized_class);
    let caller_owns_class_scope = class.as_ref().is_some_and(|class| {
        class
            .methods
            .iter()
            .any(|method| method.function.raw() == caller_function)
    });
    let result = match &instruction.kind {
        InstructionKind::FetchDynamicProperty { .. } => {
            if object.get_property(&property).is_none()
                && native_calling_class(context, caller_function).is_some_and(|class| {
                    class.methods.iter().any(|method| {
                        method.function.raw() == caller_function
                            && method.name.eq_ignore_ascii_case("__get")
                    })
                })
            {
                return Some(Err(format!(
                    "Undefined property: {}::${property}",
                    object.display_name()
                )));
            }
            object.get_property(&property).unwrap_or(Value::Null)
        }
        InstructionKind::IssetProperty { .. } | InstructionKind::IssetDynamicProperty { .. } => {
            if object.get_property(&property).is_none()
                && let Some(class) = &class
                && let Some(value) = match invoke_native_property_magic(
                    context,
                    class,
                    object_encoded,
                    &property,
                    "__isset",
                    caller_function,
                ) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                }
            {
                Value::Bool(native_property_truthy(&value))
            } else {
                Value::Bool(
                    object
                        .get_property(&property)
                        .is_some_and(|value| native_property_is_set(&value)),
                )
            }
        }
        InstructionKind::EmptyProperty { .. } | InstructionKind::EmptyDynamicProperty { .. } => {
            if object.get_property(&property).is_none()
                && let Some(class) = &class
                && let Some(isset) = match invoke_native_property_magic(
                    context,
                    class,
                    object_encoded,
                    &property,
                    "__isset",
                    caller_function,
                ) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                }
            {
                if native_property_truthy(&isset) {
                    let value = match invoke_native_property_magic(
                        context,
                        class,
                        object_encoded,
                        &property,
                        "__get",
                        caller_function,
                    ) {
                        Ok(value) => value.unwrap_or(Value::Null),
                        Err(error) => return Some(Err(error)),
                    };
                    Value::Bool(!native_property_truthy(&value))
                } else {
                    Value::Bool(true)
                }
            } else {
                Value::Bool(
                    object
                        .get_property(&property)
                        .is_none_or(|value| !native_property_truthy(&value)),
                )
            }
        }
        InstructionKind::IssetPropertyDim { dims, .. }
        | InstructionKind::EmptyPropertyDim { dims, .. }
        | InstructionKind::IssetDynamicPropertyDim { dims, .. }
        | InstructionKind::EmptyDynamicPropertyDim { dims, .. } => {
            let key_offset = match instruction.kind {
                InstructionKind::IssetDynamicPropertyDim { .. }
                | InstructionKind::EmptyDynamicPropertyDim { .. } => 2,
                _ => 1,
            };
            let value = match native_dimension_path_value(
                context,
                object.get_property(&property),
                &arguments[key_offset..],
                dims.len(),
                instruction,
                NativeDimensionOperation::Fetch { quiet: true },
            ) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            if matches!(
                instruction.kind,
                InstructionKind::IssetPropertyDim { .. }
                    | InstructionKind::IssetDynamicPropertyDim { .. }
            ) {
                Value::Bool(value.is_some_and(|value| native_property_is_set(&value)))
            } else {
                Value::Bool(value.is_none_or(|value| !native_property_truthy(&value)))
            }
        }
        InstructionKind::AssignDynamicProperty { .. } => {
            let Some(value) = arguments.get(2).copied() else {
                return Some(Err(
                    "dynamic property assignment value is missing".to_owned()
                ));
            };
            let value = match context.decode(value) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            if let Some(class) = &class {
                if let Some(entry) = class.properties.iter().find(|entry| entry.name == property) {
                    if let Some(type_) = &entry.type_
                        && !native_value_matches_ir_type_in_context(context, &value, type_)
                    {
                        return Some(Err(format!(
                            "E_PHP_THROW:TypeError:Cannot assign {} to property {}::${} of type {}",
                            native_assignment_type_name(&value),
                            class.display_name,
                            property,
                            native_ir_type_name(type_)
                        )));
                    }
                    if entry.flags.is_private && !caller_owns_class_scope {
                        return Some(Err(format!(
                            "E_PHP_THROW:Error:Cannot access private property {}::${property}",
                            class.display_name
                        )));
                    }
                } else if let Some(method) = class
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("__set"))
                    .filter(|method| method.function.raw() != caller_function)
                {
                    let name = match context.encode_native_string_owner(PhpString::from_bytes(
                        property.as_bytes().to_vec(),
                    )) {
                        Ok(name) => name,
                        Err(error) => return Some(Err(error)),
                    };
                    if let Err(error) = invoke_native_method(
                        context,
                        method.function,
                        &[object_encoded, name, arguments[2]],
                    ) {
                        return Some(Err(error.into()));
                    }
                    return Some(context.encode(value));
                }
            }
            object.set_property(property.clone(), value.clone());
            value
        }
        InstructionKind::UnsetProperty { .. } | InstructionKind::UnsetDynamicProperty { .. } => {
            if let Some(class) = &class {
                if let Some(entry) = class.properties.iter().find(|entry| entry.name == property) {
                    if entry.flags.is_private && !caller_owns_class_scope {
                        return Some(Err(format!(
                            "E_PHP_THROW:Error:Cannot access private property {}::${property}",
                            class.display_name
                        )));
                    }
                } else if let Some(method) = class
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("__unset"))
                    .filter(|method| method.function.raw() != caller_function)
                {
                    let name = match context.encode_native_string_owner(PhpString::from_bytes(
                        property.as_bytes().to_vec(),
                    )) {
                        Ok(name) => name,
                        Err(error) => return Some(Err(error)),
                    };
                    if let Err(error) =
                        invoke_native_method(context, method.function, &[object_encoded, name])
                    {
                        return Some(Err(error.into()));
                    }
                    return Some(context.encode(Value::Null));
                }
            }
            object.unset_property(&property);
            Value::Null
        }
        InstructionKind::UnsetPropertyDim { dims, .. } => {
            let keys = arguments
                .iter()
                .skip(1)
                .take(dims.len())
                .map(|key| {
                    context
                        .decode(*key)
                        .ok()
                        .and_then(|key| php_runtime::api::ArrayKey::from_value(&key))
                })
                .collect::<Option<Vec<_>>>();
            let Some(keys) = keys else {
                let block = context
                    .unit
                    .functions
                    .get(caller_function as usize)
                    .and_then(|function| {
                        function.blocks.iter().find(|block| {
                            block
                                .instructions
                                .iter()
                                .any(|candidate| candidate == instruction)
                        })
                    })
                    .map(|block| {
                        format!(
                            "{:?}",
                            block
                                .instructions
                                .iter()
                                .map(|candidate| &candidate.kind)
                                .collect::<Vec<_>>()
                        )
                    });
                let decoded = arguments
                    .iter()
                    .map(|value| context.decode(*value))
                    .collect::<Vec<_>>();
                return Some(Err(format!(
                    "property dimension key is invalid: instruction={:?} arguments={arguments:?} decoded={:?} block={:?}",
                    instruction.kind, decoded, block,
                )));
            };
            let _ = object.try_modify_property_value(&property, |value| {
                unset_native_array_dims(value, &keys);
            });
            Value::Null
        }
        InstructionKind::AssignPropertyDim { dims, append, .. } => {
            let value_index = 1 + dims.len();
            let Some(replacement) = arguments.get(value_index).copied() else {
                return Some(Err("property dimension value is missing".to_owned()));
            };
            let replacement = match context.decode(replacement) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let keys = arguments
                .iter()
                .skip(1)
                .take(dims.len())
                .map(|key| {
                    context
                        .decode(*key)
                        .ok()
                        .and_then(|key| php_runtime::api::ArrayKey::from_value(&key))
                })
                .collect::<Option<Vec<_>>>();
            let Some(keys) = keys else {
                let block = context
                    .unit
                    .functions
                    .get(caller_function as usize)
                    .and_then(|function| {
                        function.blocks.iter().find(|block| {
                            block
                                .instructions
                                .iter()
                                .any(|candidate| candidate == instruction)
                        })
                    })
                    .map(|block| {
                        format!(
                            "{:?}",
                            block
                                .instructions
                                .iter()
                                .map(|candidate| &candidate.kind)
                                .collect::<Vec<_>>()
                        )
                    });
                let decoded = arguments
                    .iter()
                    .map(|value| context.decode(*value))
                    .collect::<Vec<_>>();
                return Some(Err(format!(
                    "property dimension key is invalid: instruction={:?} arguments={arguments:?} decoded={:?} block={:?}",
                    instruction.kind, decoded, block,
                )));
            };
            if let Some(class) = &class
                && let Some(entry) = class.properties.iter().find(|entry| entry.name == property)
                && entry.flags.is_readonly
            {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot indirectly modify readonly property {}::${property}",
                    class.display_name
                )));
            }
            if let Some(Value::Object(target)) = object.get_property(&property)
                && let Some(target_class) = context.unit.classes.iter().find(|class| {
                    class.name == normalize_class_name(&target.class_name())
                        && class
                            .interfaces
                            .iter()
                            .any(|interface| interface.eq_ignore_ascii_case("ArrayAccess"))
                })
                && let Some(offset_set) = target_class
                    .methods
                    .iter()
                    .find(|method| method.name.eq_ignore_ascii_case("offsetSet"))
                    .map(|method| method.function)
            {
                let key = keys.first().cloned().map_or(Value::Null, |key| match key {
                    php_runtime::api::ArrayKey::Int(value) => Value::Int(value),
                    php_runtime::api::ArrayKey::String(value) => Value::String(value),
                });
                let receiver = match context.encode_native_object_owner(target) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                let key = match context.encode(key) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                let replacement_encoded = match context.encode(replacement.clone()) {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                if let Err(error) =
                    invoke_native_method(context, offset_set, &[receiver, key, replacement_encoded])
                {
                    return Some(Err(error.into()));
                }
                return Some(context.encode(replacement));
            }
            let result = replacement.clone();
            let modified = object.try_modify_property_value(&property, |value| {
                assign_native_array_dims(value, &keys, replacement, *append);
            });
            if !matches!(modified, Ok(Some(()))) {
                let mut value = object.get_property(&property).unwrap_or(Value::Null);
                assign_native_array_dims(&mut value, &keys, result.clone(), *append);
                object.set_property(property.clone(), value);
            }
            result
        }
        _ => return None,
    };
    if let Some(continuation) = trusted_continuation {
        let entry = class
            .as_ref()
            .and_then(|class| class.properties.iter().find(|entry| entry.name == property));
        let accessible = entry.is_some_and(|entry| {
            (!entry.flags.is_private && !entry.flags.is_protected) || caller_owns_class_scope
        });
        let state = match instruction.kind {
            InstructionKind::IssetProperty { .. }
            | InstructionKind::EmptyProperty { .. }
            | InstructionKind::IssetPropertyDim { .. }
            | InstructionKind::EmptyPropertyDim { .. }
                if accessible && entry.is_none_or(|entry| entry.hooks.get.is_none()) =>
            {
                Some(php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_PUBLISHED)
            }
            InstructionKind::AssignPropertyDim { .. }
            | InstructionKind::UnsetPropertyDim { .. }
                if accessible
                    && entry.is_some_and(|entry| {
                        !entry.flags.is_readonly
                            && entry.hooks.get.is_none()
                            && entry.hooks.set.is_none()
                    })
                    && matches!(object.get_property(&property), Some(Value::Array(_))) =>
            {
                Some(php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_DIMENSION_WRITABLE)
            }
            InstructionKind::UnsetProperty { .. }
                if accessible
                    && entry.is_some_and(|entry| {
                        !entry.flags.is_readonly
                            && entry.hooks.get.is_none()
                            && entry.hooks.set.is_none()
                    }) =>
            {
                Some(php_jit::JIT_NATIVE_TRUSTED_PROPERTY_SLOT_WRITABLE)
            }
            _ => None,
        };
        if let Some(state) = state
            && let Err(error) = context.publish_direct_object_slots(
                object_encoded,
                &property,
                0,
                i64::from(caller_function),
                i64::from(continuation),
                state,
            )
        {
            return Some(Err(error));
        }
    }
    Some(context.encode(result))
}

fn cached_native_class_constant(
    context: &NativeRequestColdState<'_>,
    caller_function: u32,
    class: &str,
    constant: &str,
) -> Option<i64> {
    context
        .class_constant_cache
        .get(&(context.current_dynamic_unit, caller_function))
        .and_then(|classes| classes.get(class))
        .and_then(|constants| constants.get(constant))
        .copied()
}

fn encode_and_cache_native_class_constant(
    context: &mut NativeRequestColdState<'_>,
    caller_function: u32,
    class: &str,
    constant: &str,
    value: Value,
) -> Result<i64, String> {
    let encoded = context.encode(value)?;
    // The cache owns one request-lifetime reference; the original encoded
    // owner is returned to the current expression. Subsequent reads duplicate
    // the native handle instead of rebuilding a copied Rust `Value` graph.
    if let Err(error) = context.retain(encoded) {
        let _ = context.release(encoded);
        return Err(error);
    }
    let previous = context
        .class_constant_cache
        .entry((context.current_dynamic_unit, caller_function))
        .or_default()
        .entry(class.to_owned())
        .or_default()
        .insert(constant.to_owned(), encoded);
    if let Some(previous) = previous {
        context.release(previous)?;
    }
    Ok(encoded)
}

fn execute_native_class_constant(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    caller_function: u32,
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::FetchClassConstant {
        class_name,
        constant,
        ..
    } = &instruction.kind
    else {
        return None;
    };
    let resolved_class = match class_name.to_ascii_lowercase().as_str() {
        "self" => {
            native_effective_calling_class(context, caller_function).map(|class| class.name.clone())
        }
        "static" => context
            .called_classes
            .last()
            .map(|class| class.to_string())
            .or_else(|| {
                native_effective_calling_class(context, caller_function)
                    .map(|class| class.name.clone())
            }),
        "parent" => native_effective_calling_class(context, caller_function)
            .and_then(|class| class.parent.clone()),
        _ => Some(normalize_class_name(class_name)),
    };
    let Some(mut resolved_class) = resolved_class else {
        let message = if class_name.eq_ignore_ascii_case("self") {
            "Cannot use \"self\" in the global scope".to_owned()
        } else if class_name.eq_ignore_ascii_case("parent") {
            "Cannot use \"parent\" when no class scope is active".to_owned()
        } else {
            format!("Cannot resolve class {class_name}")
        };
        return Some(Err(format!("E_PHP_THROW:Error:{message}")));
    };
    if let Some(original) = context
        .class_aliases
        .get(&normalize_class_name(&resolved_class))
    {
        resolved_class = original.clone();
    }
    if constant.eq_ignore_ascii_case("class") {
        let display = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == normalize_class_name(&resolved_class))
            .map_or(resolved_class.as_str(), |class| class.display_name.as_str());
        return Some(
            context.encode_native_string_owner(PhpString::from_bytes(display.as_bytes().to_vec())),
        );
    }
    resolved_class = normalize_class_name(&resolved_class);
    if class_name.eq_ignore_ascii_case("ArrayObject")
        && constant.eq_ignore_ascii_case("ARRAY_AS_PROPS")
    {
        return Some(Ok(2));
    }
    if let Some((legacy, modern)) = pdo_mysql_deprecated_constant(&resolved_class, constant)
        && let Err(error) = emit_native_php_diagnostic(
            context,
            php_runtime::api::PHP_E_DEPRECATED,
            &format!(
                "Constant PDO::{legacy} is deprecated since 8.5, use Pdo\\Mysql::{modern} instead"
            ),
            instruction,
            true,
        )
    {
        return Some(Err(error));
    }
    if let Some(encoded) =
        cached_native_class_constant(context, caller_function, &resolved_class, constant)
    {
        return Some(
            context
                .duplicate_authoritative_native_value(encoded)
                .and_then(|native| {
                    native.map_or_else(|| context.duplicate_baseline_call_argument(encoded), Ok)
                }),
        );
    }
    if let Some(value) = native_internal_class_constant(&resolved_class, constant) {
        return Some(encode_and_cache_native_class_constant(
            context,
            caller_function,
            &resolved_class,
            constant,
            value,
        ));
    }
    let mut candidate = resolved_class.clone();
    while let Some(class) = native_active_class_handle(context, &candidate) {
        if let Some(entry) = class
            .constants
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(constant))
        {
            let caller = native_calling_class(context, caller_function);
            if entry.flags.is_private && caller.is_none_or(|caller| caller.name != class.name) {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot access private constant {}::{}",
                    class.display_name, entry.name
                )));
            }
            if entry.flags.is_protected
                && caller
                    .is_none_or(|caller| !native_class_is_a(context, &caller.name, &class.name))
            {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot access protected constant {}::{}",
                    class.display_name, entry.name
                )));
            }
            if let Some(value) = entry
                .value
                .and_then(|value| context.unit.constants.get(value.index()))
            {
                return Some(
                    native_runtime_constant_value(context, value).and_then(|value| {
                        encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        )
                    }),
                );
            }
            if let Some(reference) = &entry.value_named_constant {
                for name in &reference.names {
                    if let Ok(value) = context.lookup_constant(name) {
                        return Some(encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        ));
                    }
                }
            }
            if let Some(reference) = &entry.value_class_constant {
                let value = php_ir::IrConstant::ClassConstant {
                    class_name: reference.class_name.clone(),
                    display_class_name: reference.display_class_name.clone(),
                    constant_name: reference.constant_name.clone(),
                };
                return Some(
                    native_runtime_constant_value(context, &value).and_then(|value| {
                        encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        )
                    }),
                );
            }
        }
        if let Some(case) = class
            .enum_cases
            .iter()
            .find(|case| case.name.eq_ignore_ascii_case(constant))
            .cloned()
        {
            return Some(encode_native_enum_case(context, &class, &case));
        }
        let Some(parent) = class.parent.clone() else {
            break;
        };
        candidate = normalize_class_name(&parent);
    }
    if context
        .unit
        .classes
        .iter()
        .all(|class| class.name != resolved_class)
        && !native_external_class_exists(context, &resolved_class)
    {
        let normalized = resolved_class.clone();
        let autoload_name = if matches!(
            class_name.to_ascii_lowercase().as_str(),
            "self" | "static" | "parent"
        ) {
            resolved_class.as_str()
        } else {
            class_name.as_str()
        };
        if context.autoload_in_progress.insert(normalized.clone()) {
            let callbacks = context.autoload_callbacks.clone();
            for callback in callbacks {
                if let Err(error) = invoke_native_callable_value(
                    context,
                    callback,
                    &[Value::String(PhpString::from_bytes(
                        autoload_name.as_bytes().to_vec(),
                    ))],
                    instruction,
                    None,
                ) {
                    context.autoload_in_progress.remove(&normalized);
                    return Some(Err(error));
                }
                if native_external_class_exists(context, &resolved_class) {
                    break;
                }
            }
            context.autoload_in_progress.remove(&normalized);
        }
    }
    // The late-static class may live in another unit while the requested
    // constant is declared by a parent in the current unit (or vice versa).
    // Walk the combined hierarchy instead of checking only the first external
    // class.
    let mut candidate = resolved_class.clone();
    loop {
        let (owner_unit, class) =
            if let Some(class) = native_active_class_handle(context, &candidate) {
                (None, class)
            } else if let Some((unit, class)) = native_external_class_handle(context, &candidate) {
                (Some(unit), class)
            } else {
                break;
            };
        if let Some(entry) = class
            .constants
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(constant))
        {
            let caller = native_calling_class(context, caller_function);
            if entry.flags.is_private && caller.is_none_or(|caller| caller.name != class.name) {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot access private constant {}::{}",
                    class.display_name, entry.name
                )));
            }
            if entry.flags.is_protected
                && caller
                    .is_none_or(|caller| !native_class_is_a(context, &caller.name, &class.name))
            {
                return Some(Err(format!(
                    "E_PHP_THROW:Error:Cannot access protected constant {}::{}",
                    class.display_name, entry.name
                )));
            }
            if let Some(value) = entry.value.and_then(|value| {
                owner_unit.map_or_else(
                    || context.unit.constants.get(value.index()),
                    |unit| {
                        context.dynamic_units.get(unit).and_then(|package| {
                            package.compiled.unit().constants.get(value.index())
                        })
                    },
                )
            }) {
                return Some(
                    native_runtime_constant_value(context, value).and_then(|value| {
                        encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        )
                    }),
                );
            }
            if let Some(reference) = &entry.value_named_constant {
                for name in &reference.names {
                    if let Ok(value) = context.lookup_constant(name) {
                        return Some(encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        ));
                    }
                }
            }
            if let Some(reference) = &entry.value_class_constant {
                let value = php_ir::IrConstant::ClassConstant {
                    class_name: reference.class_name.clone(),
                    display_class_name: reference.display_class_name.clone(),
                    constant_name: reference.constant_name.clone(),
                };
                return Some(
                    native_runtime_constant_value(context, &value).and_then(|value| {
                        encode_and_cache_native_class_constant(
                            context,
                            caller_function,
                            &resolved_class,
                            constant,
                            value,
                        )
                    }),
                );
            }
        }
        let Some(parent) = class.parent.clone() else {
            break;
        };
        candidate = normalize_class_name(&parent);
    }
    Some(Err(format!(
        "Undefined constant {resolved_class}::{constant}"
    )))
}

fn execute_native_enum_static_method(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::CallStaticMethod {
        class_name, method, ..
    } = &instruction.kind
    else {
        return None;
    };
    let class =
        native_active_class_handle(context, class_name).filter(|class| class.flags.is_enum)?;
    if method.eq_ignore_ascii_case("cases") {
        let mut result = php_runtime::api::PhpArray::new();
        for case in &class.enum_cases {
            let encoded = match encode_native_enum_case(context, &class, case) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let value = match context.decode(encoded) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            result.append(value);
        }
        return Some(context.encode_native_array_owner(result));
    }
    if method.eq_ignore_ascii_case("from") || method.eq_ignore_ascii_case("tryFrom") {
        let Some(argument) = arguments.first() else {
            return Some(Err(format!(
                "{class_name}::{method}() expects exactly 1 argument"
            )));
        };
        let argument = match context.decode(*argument) {
            Ok(Value::Reference(reference)) => reference.get(),
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        };
        let matching = class.enum_cases.iter().find(|case| {
            case.value
                .and_then(|value| context.unit.constants.get(value.index()))
                .and_then(|value| ir_constant_value(value).ok())
                .is_some_and(|value| value == argument)
        });
        if let Some(case) = matching {
            return Some(encode_native_enum_case(context, &class, case));
        }
        if method.eq_ignore_ascii_case("tryFrom") {
            return Some(context.encode(Value::Null));
        }
        return Some(Err(format!(
            "E_PHP_THROW:ValueError:{} is not a valid backing value for enum {}",
            native_value_type_name(&argument),
            class.display_name
        )));
    }
    None
}

fn native_class_is_a(context: &NativeRequestColdState<'_>, class_name: &str, target: &str) -> bool {
    let target = normalize_class_name(target);
    let class_name = normalize_class_name(class_name);
    if class_name == "arrayiterator" && matches!(target.as_str(), "iterator" | "traversable") {
        return true;
    }
    let mut pending = vec![class_name];
    let mut visited = std::collections::BTreeSet::new();
    while let Some(candidate) = pending.pop() {
        if candidate == target {
            return true;
        }
        if !visited.insert(candidate.clone()) {
            continue;
        }
        if let Some(class) = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == candidate)
        {
            if let Some(parent) = &class.parent {
                pending.push(normalize_class_name(parent));
            }
            pending.extend(
                class
                    .interfaces
                    .iter()
                    .map(|interface| normalize_class_name(interface)),
            );
        } else if let Some((_, class)) = native_external_class_ref(context, &candidate) {
            if let Some(parent) = &class.parent {
                pending.push(normalize_class_name(parent));
            }
            pending.extend(
                class
                    .interfaces
                    .iter()
                    .map(|interface| normalize_class_name(interface)),
            );
        }
    }
    false
}

fn native_method_in_hierarchy(
    context: &NativeRequestColdState<'_>,
    class_name: &str,
    method: &str,
) -> Option<php_ir::FunctionId> {
    let mut candidate = normalize_class_name(class_name);
    loop {
        let class = context
            .unit
            .classes
            .iter()
            .find(|class| class.name == candidate)?;
        if let Some(entry) = class
            .methods
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(method))
        {
            return Some(entry.function);
        }
        candidate = normalize_class_name(class.parent.as_ref()?);
    }
}

fn native_function_is_generator(
    context: &NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
) -> bool {
    context
        .unit
        .functions
        .get(function.index())
        .is_some_and(|function| {
            function.flags.is_generator
                || function
                    .blocks
                    .iter()
                    .flat_map(|block| &block.instructions)
                    .any(|instruction| {
                        matches!(
                            instruction.kind,
                            php_ir::InstructionKind::Yield { .. }
                                | php_ir::InstructionKind::YieldFrom { .. }
                        )
                    })
        })
}

fn native_calling_class<'a>(
    context: &'a NativeRequestColdState<'_>,
    function: u32,
) -> Option<&'a php_ir::ClassEntry> {
    context.unit.classes.iter().find(|class| {
        class
            .methods
            .iter()
            .any(|method| method.function.raw() == function)
    })
}

fn native_effective_calling_class<'a>(
    context: &'a NativeRequestColdState<'_>,
    function: u32,
) -> Option<&'a php_ir::ClassEntry> {
    native_calling_class(context, function).or_else(|| {
        let scope = context.lexical_scope_classes.last()?;
        let normalized = normalize_class_name(scope);
        context
            .unit
            .classes
            .iter()
            .find(|class| class.name == normalized)
    })
}

fn native_resolve_scoped_class_name(
    context: &NativeRequestColdState<'_>,
    class_name: &str,
    caller_function: u32,
) -> Result<String, String> {
    match class_name.to_ascii_lowercase().as_str() {
        "self" => native_effective_calling_class(context, caller_function)
            .map(|class| class.display_name.clone())
            .ok_or_else(|| "Cannot use \"self\" in the global scope".to_owned()),
        "static" => context
            .called_classes
            .last()
            .map(|class| class.to_string())
            .or_else(|| {
                native_effective_calling_class(context, caller_function)
                    .map(|class| class.display_name.clone())
            })
            .ok_or_else(|| "Cannot use \"static\" in the global scope".to_owned()),
        "parent" => native_effective_calling_class(context, caller_function)
            .and_then(|class| {
                class
                    .parent_display_name
                    .clone()
                    .or_else(|| class.parent.clone())
            })
            .ok_or_else(|| "Cannot use \"parent\" when no parent scope is active".to_owned()),
        _ => Ok(class_name.to_owned()),
    }
}

fn native_method_access_error(
    context: &NativeRequestColdState<'_>,
    function: php_ir::FunctionId,
    caller_function: u32,
    _late_static_call: bool,
) -> Option<String> {
    let (declaring_class, method) = context.unit.classes.iter().find_map(|class| {
        class
            .methods
            .iter()
            .find(|method| method.function == function)
            .map(|method| (class, method))
    })?;
    if !method.flags.is_private && !method.flags.is_protected {
        return None;
    }
    let caller = native_effective_calling_class(context, caller_function);
    if method.flags.is_private && caller.is_none_or(|caller| caller.name != declaring_class.name) {
        if caller.is_none() {
            return Some(format!(
                "Call to private method {}::{}() from global scope",
                declaring_class.display_name, method.name
            ));
        }
        return Some(format!(
            "Cannot access private method {}::{}()",
            declaring_class.display_name, method.name
        ));
    }
    if method.flags.is_protected
        && caller
            .is_none_or(|caller| !native_class_is_a(context, &caller.name, &declaring_class.name))
    {
        return Some(format!(
            "Cannot access protected method {}::{}()",
            declaring_class.display_name, method.name
        ));
    }
    None
}

fn native_external_method_access_error(
    context: &NativeRequestColdState<'_>,
    target: NativeDynamicFunction,
    caller_function: u32,
    _late_static_call: bool,
) -> Option<String> {
    let unit = context.dynamic_units.get(target.unit)?.compiled.unit();
    let (declaring_class, method) = unit.classes.iter().find_map(|class| {
        class
            .methods
            .iter()
            .find(|method| method.function == target.function)
            .map(|method| (class, method))
    })?;
    if !method.flags.is_private && !method.flags.is_protected {
        return None;
    }
    let caller = native_effective_calling_class(context, caller_function);
    if method.flags.is_private && caller.is_none_or(|caller| caller.name != declaring_class.name) {
        if caller.is_none() {
            return Some(format!(
                "Call to private method {}::{}() from global scope",
                declaring_class.display_name, method.name
            ));
        }
        return Some(format!(
            "Cannot access private method {}::{}()",
            declaring_class.display_name, method.name
        ));
    }
    if method.flags.is_protected
        && caller
            .is_none_or(|caller| !native_class_is_a(context, &caller.name, &declaring_class.name))
    {
        return Some(format!(
            "Cannot access protected method {}::{}()",
            declaring_class.display_name, method.name
        ));
    }
    None
}

/// Packs the already-bound operands for `__call`/`__callStatic` into one
/// authoritative direct array. The generic dispatcher is the explicit
/// baseline-native continuation, so only a legacy operand that reached that
/// tier may use the compatibility duplication branch.
fn encode_native_magic_call_arguments_array(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
) -> Result<i64, String> {
    let mut entries = Vec::<php_jit::JitNativeDirectArrayEntry>::with_capacity(arguments.len());
    for (index, argument) in arguments.iter().enumerate() {
        let value = match context.duplicate_authoritative_native_value(*argument) {
            Ok(Some(value)) => value,
            Ok(None) => match context.duplicate_baseline_call_argument(*argument) {
                Ok(value) => value,
                Err(error) => {
                    for entry in entries {
                        let _ = context.release(entry.key);
                        let _ = context.release(entry.value);
                    }
                    return Err(error);
                }
            },
            Err(error) => {
                for entry in entries {
                    let _ = context.release(entry.key);
                    let _ = context.release(entry.value);
                }
                return Err(error);
            }
        };
        entries.push(php_jit::JitNativeDirectArrayEntry {
            key: i64::try_from(index).unwrap_or(i64::MAX),
            value,
        });
    }
    context.publish_owned_direct_array_entries(entries)
}

fn execute_native_instanceof(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    let (object, static_target) = match &instruction.kind {
        php_ir::InstructionKind::InstanceOf { class_name, .. } => {
            (arguments.first().copied(), Some(class_name.as_str()))
        }
        php_ir::InstructionKind::DynamicInstanceOf { .. } => (arguments.first().copied(), None),
        _ => return None,
    };
    let Some(object) = object else {
        return Some(Err("instanceof receiver is missing".to_owned()));
    };
    let target = if let Some(target) = static_target {
        target.to_owned()
    } else {
        let Some(target) = arguments.get(1) else {
            return Some(Err("instanceof target is missing".to_owned()));
        };
        let direct_target = context.dereference_direct_encoding(*target);
        if let Some(bytes) = context.native_string_name_bytes(direct_target) {
            String::from_utf8_lossy(&bytes).into_owned()
        } else if let Some(object) = context.native_query_object(direct_target) {
            object.class_name()
        } else {
            match context.decode(*target) {
                Ok(Value::String(value)) => value.to_string_lossy(),
                Ok(Value::Object(object)) => object.class_name(),
                Ok(value) => {
                    return Some(Err(format!(
                        "instanceof target must be a class name, {} given",
                        native_value_type_name(&value)
                    )));
                }
                Err(error) => return Some(Err(error)),
            }
        }
    };
    let direct_object = context.dereference_direct_encoding(object);
    let result = match context.native_encoded_value_kind(direct_object) {
        Some(NativeEncodedValueKind::Callable) => target.eq_ignore_ascii_case("Closure"),
        Some(NativeEncodedValueKind::Fiber) => target.eq_ignore_ascii_case("Fiber"),
        Some(NativeEncodedValueKind::Generator) => target.eq_ignore_ascii_case("Generator"),
        Some(NativeEncodedValueKind::Object) => {
            let Some(object) = context.native_query_object(direct_object) else {
                return Some(Err("instanceof receiver lost its native object".to_owned()));
            };
            native_internal_instanceof(&object.class_name(), &target)
                .unwrap_or_else(|| native_class_is_a(context, &object.class_name(), &target))
        }
        _ => match context.decode(object) {
            Ok(Value::Object(object)) => native_internal_instanceof(&object.class_name(), &target)
                .unwrap_or_else(|| native_class_is_a(context, &object.class_name(), &target)),
            Ok(Value::Callable(_)) => target.eq_ignore_ascii_case("Closure"),
            Ok(Value::Fiber(_)) => target.eq_ignore_ascii_case("Fiber"),
            Ok(Value::Generator(_)) => target.eq_ignore_ascii_case("Generator"),
            Ok(Value::Array(array)) => super::native_exception_fields(Value::Array(array))
                .is_some_and(|(class, _, _)| {
                    let normalized = class.to_ascii_lowercase();
                    target.eq_ignore_ascii_case(&class)
                        || target.eq_ignore_ascii_case("Throwable")
                        || (target.eq_ignore_ascii_case("Exception")
                            && normalized.ends_with("exception"))
                        || (target.eq_ignore_ascii_case("Error") && normalized.ends_with("error"))
                }),
            Ok(Value::Reference(reference)) => match reference.get() {
                Value::Object(object) => native_internal_instanceof(&object.class_name(), &target)
                    .unwrap_or_else(|| native_class_is_a(context, &object.class_name(), &target)),
                Value::Callable(_) => target.eq_ignore_ascii_case("Closure"),
                Value::Fiber(_) => target.eq_ignore_ascii_case("Fiber"),
                Value::Generator(_) => target.eq_ignore_ascii_case("Generator"),
                _ => false,
            },
            Ok(_) => false,
            Err(error) => return Some(Err(error)),
        },
    };
    Some(context.encode(Value::Bool(result)))
}

fn execute_native_acquire_callable(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    if !matches!(
        instruction.kind,
        php_ir::InstructionKind::AcquireCallable { .. }
    ) {
        return None;
    }
    let Some(value) = arguments.first() else {
        return Some(Err("callable value is missing".to_owned()));
    };
    let direct = context.dereference_direct_encoding(*value);
    match context.native_encoded_value_kind(direct) {
        Some(NativeEncodedValueKind::Callable)
            if context.prepared_callable_dispatch(direct).is_some() =>
        {
            return Some(context.retain(direct).map(|()| direct));
        }
        Some(NativeEncodedValueKind::String) => {
            let Some(name) = context.native_string_name_bytes(direct) else {
                return Some(Err("callable string has no native bytes".to_owned()));
            };
            return Some(context.encode_prepared_callable(Box::new(
                php_runtime::api::CallableValue::UserFunction {
                    name: String::from_utf8_lossy(&name).into_owned(),
                },
            )));
        }
        Some(NativeEncodedValueKind::Object) => {
            let Some(object) = context.native_query_object(direct) else {
                return Some(Err("callable object has no native owner".to_owned()));
            };
            return Some(context.encode_prepared_callable(Box::new(
                php_runtime::api::CallableValue::BoundMethod {
                    target: php_runtime::api::CallableMethodTarget::Object(object),
                    method: "__invoke".to_owned(),
                    scope: None,
                },
            )));
        }
        Some(NativeEncodedValueKind::Array) => {
            if let Some(entries) = context.direct_array_entries_for(direct) {
                let mut target = None;
                let mut method = None;
                for entry in entries {
                    match context.native_encoded_int(entry.key) {
                        Some(0) => target = Some(context.dereference_direct_encoding(entry.value)),
                        Some(1) => method = Some(context.dereference_direct_encoding(entry.value)),
                        _ => {}
                    }
                }
                let Some(target) = target else {
                    return Some(Err("callable array target is missing".to_owned()));
                };
                let Some(method) = method
                    .and_then(|method| context.native_string_name_bytes(method))
                    .map(|method| String::from_utf8_lossy(&method).into_owned())
                else {
                    return Some(Err("callable array method must be a string".to_owned()));
                };
                let target = if let Some(object) = context.native_query_object(target) {
                    php_runtime::api::CallableMethodTarget::Object(object)
                } else if let Some(class) = context.native_string_name_bytes(target) {
                    php_runtime::api::CallableMethodTarget::Class(
                        String::from_utf8_lossy(&class).into_owned(),
                    )
                } else {
                    return Some(Err(format!(
                        "callable array target must be object or class-string, {} given",
                        context.native_encoded_type_name(target)
                    )));
                };
                return Some(context.encode_prepared_callable(Box::new(
                    php_runtime::api::CallableValue::BoundMethod {
                        target,
                        method,
                        scope: None,
                    },
                )));
            }
        }
        _ => {}
    }
    // Baseline-only compatibility values may still reach acquisition from a
    // materialized ReferenceCell. Direct producers above never decode.
    let value = match context.decode(*value) {
        Ok(value) => dereference_native_callable_value(value),
        Err(error) => return Some(Err(error)),
    };
    let callable = match value {
        Value::Callable(callable) => return Some(context.encode(Value::Callable(callable))),
        Value::String(name) => php_runtime::api::CallableValue::UserFunction {
            name: name.to_string_lossy(),
        },
        Value::Object(object) => php_runtime::api::CallableValue::BoundMethod {
            target: php_runtime::api::CallableMethodTarget::Object(object),
            method: "__invoke".to_owned(),
            scope: None,
        },
        Value::Array(array) => {
            let target = array
                .get(&php_runtime::api::ArrayKey::Int(0))
                .cloned()
                .map(dereference_native_callable_value)
                .ok_or_else(|| "callable array target is missing".to_owned());
            let method = array
                .get(&php_runtime::api::ArrayKey::Int(1))
                .cloned()
                .map(dereference_native_callable_value)
                .ok_or_else(|| "callable array method is missing".to_owned());
            let (target, method) = match (target, method) {
                (Ok(target), Ok(Value::String(method))) => (target, method.to_string_lossy()),
                (Err(error), _) | (_, Err(error)) => return Some(Err(error)),
                _ => return Some(Err("callable array method must be a string".to_owned())),
            };
            let target = match target {
                Value::Object(object) => php_runtime::api::CallableMethodTarget::Object(object),
                Value::String(class) => {
                    php_runtime::api::CallableMethodTarget::Class(class.to_string_lossy())
                }
                value => {
                    return Some(Err(format!(
                        "callable array target must be object or class-string, {} given",
                        native_value_type_name(&value)
                    )));
                }
            };
            php_runtime::api::CallableValue::BoundMethod {
                target,
                method,
                scope: None,
            }
        }
        other => {
            return Some(Err(format!(
                "{} is not callable",
                native_value_type_name(&other)
            )));
        }
    };
    Some(context.encode(Value::Callable(Box::new(callable))))
}

fn execute_native_resolve_callable(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::ResolveCallable { callable, .. } = &instruction.kind else {
        return None;
    };
    let name = match callable {
        php_ir::instruction::CallableKind::FunctionName { name } => name,
        php_ir::instruction::CallableKind::MethodPlaceholder { target }
        | php_ir::instruction::CallableKind::UnresolvedDynamic { target } => {
            return Some(Err(format!("E_PHP_THROW:Error:{target}")));
        }
    };
    let normalized = name.trim_start_matches('\\').to_ascii_lowercase();
    let fallback = normalized
        .rsplit_once('\\')
        .map(|(_, basename)| basename.to_owned());
    let exists = context.function_id(&normalized).is_some()
        || context.external_function(&normalized).is_some()
        || context.visible_function_names.contains(&normalized)
        || php_extensions::BuiltinRegistry::new().contains(&normalized)
        || fallback.as_ref().is_some_and(|fallback| {
            context.function_id(fallback).is_some()
                || context.external_function(fallback).is_some()
                || context.visible_function_names.contains(fallback)
                || php_extensions::BuiltinRegistry::new().contains(fallback)
        });
    if !exists {
        return Some(Err(format!(
            "E_PHP_THROW:Error:Call to undefined function {name}()"
        )));
    }
    Some(context.encode(Value::Callable(Box::new(
        php_runtime::api::CallableValue::UserFunction { name: name.clone() },
    ))))
}

fn native_rebind_closure(
    closure: &php_runtime::api::ClosurePayload,
    new_this: Option<Value>,
    new_scope: Option<Value>,
) -> Result<Value, String> {
    let bound_this = match new_this {
        Some(Value::Object(object)) => Some(object),
        Some(Value::Null) | None => None,
        Some(value) => {
            return Err(format!(
                "Closure::bind(): Argument #2 ($newThis) must be of type ?object, {} given",
                native_value_type_name(&value)
            ));
        }
    };
    let scope: Option<std::sync::Arc<str>> = match new_scope {
        Some(Value::Object(object)) => Some(object.display_name().into()),
        Some(Value::String(class)) => {
            let class = class.to_string_lossy();
            (class != "static").then(|| class.into())
        }
        Some(Value::Null) => None,
        Some(value) => {
            return Err(format!(
                "Closure::bind(): Argument #3 ($newScope) must be of type object|string|null, {} given",
                native_value_type_name(&value)
            ));
        }
        None => bound_this
            .as_ref()
            .map(|object| object.display_name().into()),
    };
    let mut context = closure.context.clone();
    if let Some(scope) = scope {
        context.scope_class = Some(scope.clone());
        context.called_class = Some(scope.clone());
        context.declaring_class = Some(scope);
    }
    Ok(Value::Callable(Box::new(
        php_runtime::api::CallableValue::Closure(
            closure
                .clone()
                .with_bound_this(bound_this)
                .with_context(context),
        ),
    )))
}

fn execute_native_bind_global(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
) -> Option<Result<i64, String>> {
    let php_ir::InstructionKind::BindGlobal { name, .. } = &instruction.kind else {
        return None;
    };
    if let Err(error) = context.materialize_native_request_global(name) {
        return Some(Err(error));
    }
    let current = context
        .inherited_globals
        .get(name)
        .filter(|value| !matches!(value, Value::Uninitialized))
        .cloned()
        .or_else(|| context.options.runtime_context.global_value(name))
        .unwrap_or(Value::Null);
    let reference = match current {
        Value::Reference(reference) => reference,
        value => php_runtime::api::ReferenceCell::new(value),
    };
    context
        .inherited_globals
        .insert(name.clone(), Value::Reference(reference.clone()));
    context.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
    Some(context.encode_native_reference_owner(reference))
}

#[cfg(test)]
mod tests;
