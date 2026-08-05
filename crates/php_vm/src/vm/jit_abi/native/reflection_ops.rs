//! Fixed ReflectionClass services over authoritative native object metadata.

use super::{NativeRequestFastState, PreparedNativeRuntimeClass};

impl NativeRequestFastState {
    /// Publishes the fixed internal ReflectionClass layout used by its exact
    /// constructor and metadata accessors.
    pub(crate) fn prepare_reflection_class_plan(&mut self) {
        if self.internal_class_plans.contains_key("reflectionclass") {
            return;
        }
        let entry = php_runtime::api::ClassEntry {
            name: std::sync::Arc::from("reflectionclass"),
            parent: None,
            interfaces: vec!["reflector".to_owned()],
            methods: Vec::new(),
            properties: vec![php_runtime::api::ClassPropertyEntry {
                name: "name".to_owned(),
                default: php_runtime::api::Value::Uninitialized,
                type_: None,
                flags: php_runtime::api::ClassPropertyFlags::default(),
                hooks: php_runtime::api::ClassPropertyHooks::default(),
                attributes: Vec::new(),
            }],
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: Some(u32::MAX),
            flags: php_runtime::api::ClassFlags::default(),
        };
        let display_name = "ReflectionClass".to_owned();
        let layout_id = php_runtime::api::ObjectRef::prepared_layout_id(&entry, &display_name);
        let prepared = Box::new(PreparedNativeRuntimeClass {
            entry,
            display_name,
            layout_id,
            default_native_slots: vec![php_runtime::api::NativeDeclaredPropertySlot::default()]
                .into_boxed_slice(),
        });
        let plan = Box::new(php_jit::JitNativePreparedClassPlan {
            prepared: std::ptr::from_ref(prepared.as_ref()) as usize as u64,
            display_name_bytes: prepared.display_name.as_ptr() as usize as u64,
            display_name_length: prepared.display_name.len() as u64,
            state: php_jit::JIT_NATIVE_PREPARED_CLASS_ALLOCATABLE,
            flags: php_jit::JIT_NATIVE_PREPARED_CLASS_HAS_CONSTRUCTOR,
        });
        self.internal_class_plans
            .insert("reflectionclass".to_owned(), (prepared, plan));
    }
}

fn reflection_runtime_error() -> php_jit::JitNativeControlResult {
    php_jit::JitNativeControlResult::control(php_jit::JitCallStatus::RUNTIME_ERROR, 0, 0)
}

fn reflection_name_slot(
    object: &php_runtime::api::ObjectRef,
) -> Option<*mut php_runtime::api::NativeDeclaredPropertySlot> {
    let layout = object.class_layout_epoch();
    object
        .native_declared_property_slot_location(layout, "name")
        .or_else(|| {
            object
                .native_dynamic_property_slot_location(layout, "name")
                .flatten()
        })
}

fn reflection_target_exists(fast: &NativeRequestFastState, name: &str) -> bool {
    let normalized = php_ir::module::normalize_class_name(name);
    fast.symbol_query.class_handle(&normalized).is_some()
        || php_std::ExtensionRegistry::standard_library()
            .enabled_class(&normalized)
            .is_some()
        || matches!(
            normalized.as_str(),
            "exception"
                | "error"
                | "typeerror"
                | "valueerror"
                | "argumentcounterror"
                | "fibererror"
        )
}

fn reflection_target_name(fast: &NativeRequestFastState, target: i64) -> Option<Vec<u8>> {
    if let Some(name) = fast.native_string_view(target) {
        return Some(name.to_vec());
    }
    fast.direct_object(target)
        .map(|object| object.class_name().into_bytes())
}

#[allow(unsafe_code)] // Safety: the retained object owns the stable native property cell.
fn reflection_object_name(fast: &NativeRequestFastState, receiver: i64) -> Option<i64> {
    let object = fast.direct_object(receiver)?;
    let slot = reflection_name_slot(object)?;
    // SAFETY: the object owns this stable native property cell, and the bound
    // receiver remains retained for the complete synchronous invocation.
    let slot = unsafe { *slot };
    (slot.initialized != 0).then_some(slot.value)
}

/// Initializes the fixed native `ReflectionClass::__construct` receiver.
#[allow(unsafe_code)] // Safety: the compiled ABI supplies request-owned pointers for this call.
pub(crate) unsafe extern "C" fn jit_native_reflection_class_construct_php_entry(
    runtime: *mut std::ffi::c_void,
    arguments: *const i64,
    _transition_out: *mut php_jit::JitDeoptState,
    _resume_id: i32,
    _resume_state: *const php_jit::JitDeoptState,
) -> php_jit::JitNativeControlResult {
    if runtime.is_null() || arguments.is_null() {
        return reflection_runtime_error();
    }
    let fast = unsafe { &mut *runtime.cast::<NativeRequestFastState>() };
    let receiver = unsafe { *arguments };
    let target = unsafe { *arguments.add(1) };
    let Some(name) = reflection_target_name(fast, target) else {
        return reflection_runtime_error();
    };
    let Ok(name_text) = std::str::from_utf8(&name) else {
        return reflection_runtime_error();
    };
    if !reflection_target_exists(fast, name_text) {
        return reflection_runtime_error();
    }
    let Some(object) = fast.direct_object(receiver).cloned() else {
        return reflection_runtime_error();
    };
    let name_value = match fast.publish_direct_string_bytes(&name) {
        Ok(value) => value,
        Err(_) => return reflection_runtime_error(),
    };
    let layout = object.class_layout_epoch();
    let slot = if let Some(slot) = reflection_name_slot(&object) {
        slot
    } else {
        let published = php_runtime::api::NativeDeclaredPropertySlot {
            initialized: 1,
            reserved: 0,
            value: name_value,
        };
        if object
            .set_native_dynamic_property(layout, "name".to_owned(), published)
            .is_err()
        {
            let _ = fast.discard_owned_direct_value(name_value);
            return reflection_runtime_error();
        }
        return php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX));
    };
    // SAFETY: `reflection_name_slot` returns an object-owned stable cell and
    // this invocation retains the bound receiver until generated cleanup.
    let previous = unsafe { *slot };
    unsafe {
        *slot = php_runtime::api::NativeDeclaredPropertySlot {
            initialized: 1,
            reserved: 0,
            value: name_value,
        };
    }
    if previous.initialized != 0 {
        let _ = fast.discard_owned_direct_value(previous.value);
    }
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX))
}

/// Returns the authoritative name stored by the fixed constructor.
#[allow(unsafe_code)] // Safety: the compiled ABI supplies request-owned pointers for this call.
pub(crate) unsafe extern "C" fn jit_native_reflection_class_get_name_php_entry(
    runtime: *mut std::ffi::c_void,
    arguments: *const i64,
    _transition_out: *mut php_jit::JitDeoptState,
    _resume_id: i32,
    _resume_state: *const php_jit::JitDeoptState,
) -> php_jit::JitNativeControlResult {
    if runtime.is_null() || arguments.is_null() {
        return reflection_runtime_error();
    }
    let fast = unsafe { &mut *runtime.cast::<NativeRequestFastState>() };
    let receiver = unsafe { *arguments };
    let Some(name) = reflection_object_name(fast, receiver) else {
        return reflection_runtime_error();
    };
    match fast.retain_direct_encoded(name) {
        Ok(()) => php_jit::JitNativeControlResult::returning(name),
        Err(_) => reflection_runtime_error(),
    }
}

/// Implements the fixed `ReflectionClass::hasProperty` metadata query.
#[allow(unsafe_code)] // Safety: the compiled ABI supplies request-owned pointers for this call.
pub(crate) unsafe extern "C" fn jit_native_reflection_class_has_property_php_entry(
    runtime: *mut std::ffi::c_void,
    arguments: *const i64,
    _transition_out: *mut php_jit::JitDeoptState,
    _resume_id: i32,
    _resume_state: *const php_jit::JitDeoptState,
) -> php_jit::JitNativeControlResult {
    if runtime.is_null() || arguments.is_null() {
        return reflection_runtime_error();
    }
    let fast = unsafe { &mut *runtime.cast::<NativeRequestFastState>() };
    let receiver = unsafe { *arguments };
    let member = unsafe { *arguments.add(1) };
    let Some(name) = reflection_object_name(fast, receiver)
        .and_then(|name| fast.native_string_view(name))
        .and_then(|name| std::str::from_utf8(name).ok())
        .map(str::to_owned)
    else {
        return reflection_runtime_error();
    };
    let Some(member) = fast
        .native_string_view(member)
        .and_then(|member| std::str::from_utf8(member).ok())
        .map(str::to_owned)
    else {
        return reflection_runtime_error();
    };
    let exists = fast.symbol_query.class_lineage_any(&name, &mut |class| {
        class
            .properties
            .iter()
            .any(|property| property.name == member)
    }) || php_std::generated::arginfo::property_metadata_in_hierarchy(&name, &member)
        .is_some();
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(if exists {
        php_jit::JIT_VALUE_TRUE
    } else {
        php_jit::JIT_VALUE_FALSE
    }))
}
