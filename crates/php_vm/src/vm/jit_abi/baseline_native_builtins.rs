//! Baseline-native PHP builtin semantic interpreter.

use super::*;
use php_runtime::api::PhpString;
use php_runtime::api::Value;
use std::sync::Arc;
include!("baseline_native_builtins/callback_support.rs");

impl NativeRegisteredExtensionRequestState {
    pub(super) fn stream_context_default_options(&self) -> php_runtime::api::PhpArray {
        self.stream_context.default_options()
    }

    pub(super) fn set_stream_context_default_options(
        &mut self,
        options: php_runtime::api::PhpArray,
    ) {
        self.stream_context.set_default_options(options);
    }
}

pub(super) fn normalized_native_builtin_name(name: &str) -> std::borrow::Cow<'_, str> {
    let name = name.trim_start_matches('\\');
    if name.bytes().any(|byte| byte.is_ascii_uppercase()) {
        std::borrow::Cow::Owned(name.to_ascii_lowercase())
    } else {
        std::borrow::Cow::Borrowed(name)
    }
}

fn baseline_native_class_target(
    context: &mut NativeRequestColdState<'_>,
    encoded: i64,
) -> Result<(String, bool), String> {
    match context.baseline_decode_dereferenced_native_value(encoded)? {
        Value::Object(object) => Ok((object.class_name(), true)),
        Value::String(name) => Ok((name.to_string_lossy(), false)),
        value => Err(format!(
            "class query expects object|string, {} given",
            native_value_type_name(&value)
        )),
    }
}

fn baseline_class_interface_names(
    symbols: &NativeSymbolQueryCapability,
    class_name: &str,
) -> Option<Vec<String>> {
    fn visit_interface(
        symbols: &NativeSymbolQueryCapability,
        interface_name: &str,
        depth: usize,
        seen: &mut std::collections::BTreeSet<String>,
        names: &mut Vec<String>,
    ) -> Option<()> {
        if depth >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
            return None;
        }
        let normalized = normalize_class_name(interface_name);
        if !seen.insert(normalized.clone()) {
            return Some(());
        }
        if let Some(interface) = symbols.class_handle(&normalized) {
            names.push(interface.display_name.clone());
            for parent in &interface.interfaces {
                visit_interface(symbols, parent, depth + 1, seen, names)?;
            }
        } else {
            names.push(interface_name.to_owned());
        }
        Some(())
    }

    fn visit_class(
        symbols: &NativeSymbolQueryCapability,
        class_name: &str,
        depth: usize,
        seen: &mut std::collections::BTreeSet<String>,
        names: &mut Vec<String>,
    ) -> Option<()> {
        if depth >= php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY {
            return None;
        }
        let class = symbols.class_handle(class_name)?;
        for interface in &class.interfaces {
            visit_interface(symbols, interface, depth + 1, seen, names)?;
        }
        if let Some(parent) = class.parent.as_deref() {
            visit_class(symbols, parent, depth + 1, seen, names)?;
        }
        Some(())
    }

    let mut names = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    visit_class(symbols, class_name, 0, &mut seen, &mut names)?;
    Some(names)
}

fn execute_native_type_predicate(
    context: &mut NativeRequestColdState<'_>,
    name: &str,
    arguments: &[i64],
) -> Result<Option<i64>, String> {
    let [value] = arguments else {
        return Ok(None);
    };
    let operation = match name {
        "is_int" | "is_integer" | "is_long" => php_jit::JitNativeTypePredicate::Int,
        "is_float" | "is_double" | "is_real" => php_jit::JitNativeTypePredicate::Float,
        "is_string" => php_jit::JitNativeTypePredicate::String,
        "is_bool" => php_jit::JitNativeTypePredicate::Bool,
        "is_null" => php_jit::JitNativeTypePredicate::Null,
        "is_array" => php_jit::JitNativeTypePredicate::Array,
        "is_object" => php_jit::JitNativeTypePredicate::Object,
        "is_resource" => php_jit::JitNativeTypePredicate::Resource,
        "is_scalar" => php_jit::JitNativeTypePredicate::Scalar,
        "is_countable" => php_jit::JitNativeTypePredicate::Countable,
        "is_iterable" => php_jit::JitNativeTypePredicate::Iterable,
        _ => return Ok(None),
    };
    execute_native_type_predicate_operation(context, *value, operation).map(Some)
}

pub(super) fn execute_native_type_predicate_operation(
    context: &mut NativeRequestColdState<'_>,
    value: i64,
    operation: php_jit::JitNativeTypePredicate,
) -> Result<i64, String> {
    let value = context.dereference_direct_encoding(value);
    let kind = context
        .native_encoded_value_kind(value)
        .ok_or_else(|| "native type predicate received an invalid value".to_owned())?;
    let object_is_a = |expected: &str| {
        context
            .native_query_object(value)
            .is_some_and(|object| native_class_is_a(context, &object.class_name(), expected))
    };
    let result = match operation {
        php_jit::JitNativeTypePredicate::Int => kind == NativeEncodedValueKind::Int,
        php_jit::JitNativeTypePredicate::Float => kind == NativeEncodedValueKind::Float,
        php_jit::JitNativeTypePredicate::String => kind == NativeEncodedValueKind::String,
        php_jit::JitNativeTypePredicate::Bool => matches!(kind, NativeEncodedValueKind::Bool(_)),
        php_jit::JitNativeTypePredicate::Null => kind == NativeEncodedValueKind::Null,
        php_jit::JitNativeTypePredicate::Array => kind == NativeEncodedValueKind::Array,
        php_jit::JitNativeTypePredicate::Object => matches!(
            kind,
            NativeEncodedValueKind::Object
                | NativeEncodedValueKind::Fiber
                | NativeEncodedValueKind::Generator
                | NativeEncodedValueKind::Callable
        ),
        php_jit::JitNativeTypePredicate::Resource => kind == NativeEncodedValueKind::Resource,
        php_jit::JitNativeTypePredicate::Scalar => matches!(
            kind,
            NativeEncodedValueKind::Bool(_)
                | NativeEncodedValueKind::Int
                | NativeEncodedValueKind::Float
                | NativeEncodedValueKind::String
        ),
        php_jit::JitNativeTypePredicate::Countable => {
            kind == NativeEncodedValueKind::Array
                || (kind == NativeEncodedValueKind::Object && object_is_a("countable"))
        }
        php_jit::JitNativeTypePredicate::Iterable => {
            matches!(
                kind,
                NativeEncodedValueKind::Array | NativeEncodedValueKind::Generator
            ) || (kind == NativeEncodedValueKind::Object && object_is_a("traversable"))
        }
    };
    Ok(php_jit::jit_encode_constant(if result {
        php_jit::JIT_VALUE_TRUE
    } else {
        php_jit::JIT_VALUE_FALSE
    }))
}

fn execute_native_read_builtin_fast(
    context: &mut NativeRequestColdState<'_>,
    name: &str,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<Option<i64>, String> {
    match (name, arguments) {
        ("array_key_exists" | "key_exists", [key, array]) => {
            if context.direct_array_slot(*array).is_none() {
                return Ok(None);
            }
            let key = match context.decode_baseline_value(*key)? {
                Value::Reference(reference) => reference.get(),
                key => key,
            };
            match &key {
                Value::Null | Value::Uninitialized => emit_native_php_warning(
                    context,
                    php_runtime::api::PHP_E_DEPRECATED,
                    "Using null as the key parameter for array_key_exists() is deprecated, use an empty string instead",
                    source,
                )?,
                Value::Float(key) => {
                    let key = key.to_f64();
                    let label = native_php_float_label(key);
                    if !key.is_finite() {
                        emit_native_php_warning(
                            context,
                            php_runtime::api::PHP_E_WARNING,
                            &format!(
                                "The float {label} is not representable as an int, cast occurred"
                            ),
                            source,
                        )?;
                    }
                    if key.is_nan() || key.fract() != 0.0 {
                        emit_native_php_warning(
                            context,
                            php_runtime::api::PHP_E_DEPRECATED,
                            &format!(
                                "Implicit conversion from float {label} to int loses precision"
                            ),
                            source,
                        )?;
                    }
                }
                _ => {}
            }
            let Some(key) = php_runtime::api::ArrayKey::from_value(&key) else {
                return Ok(None);
            };
            let exists = context.direct_array_find_encoded(*array, &key)?.is_some();
            Ok(Some(php_jit::jit_encode_constant(if exists {
                php_jit::JIT_VALUE_TRUE
            } else {
                php_jit::JIT_VALUE_FALSE
            })))
        }
        ("strlen", [value]) => {
            let strict = context.unit.strict_types_for_span(source.span);
            let Some(value) = context.coerce_native_call_argument_encoded(
                *value,
                &php_ir::IrReturnType::String,
                strict,
            )?
            else {
                return Ok(None);
            };
            let result = context
                .native_string_bytes(value)
                .ok_or_else(|| "strlen() coerced string has no stable native bytes".to_owned())
                .and_then(|bytes| {
                    i64::try_from(bytes.len()).map_err(|_| "strlen() result overflow".to_owned())
                })
                .and_then(|length| context.encode_native_int(length));
            context.release(value)?;
            result.map(Some)
        }
        _ => Ok(None),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeCountArrayIdentity {
    Storage(u64, u64),
    Slot(usize),
}

fn count_native_array_recursive(
    context: &NativeRequestColdState<'_>,
    encoded: i64,
    active_arrays: &mut Vec<NativeCountArrayIdentity>,
    active_references: &mut Vec<u64>,
) -> Result<(usize, usize), String> {
    let encoded = context.dereference_direct_encoding(encoded);
    let slot = NativeRequestColdState::direct_value_index(encoded)
        .ok_or_else(|| "count() direct array has no stable identity".to_owned())?;
    let identity = context
        .baseline_values
        .direct_array_storage_ids
        .get(&slot)
        .copied()
        .map(|(identity, version)| NativeCountArrayIdentity::Storage(identity, version))
        .unwrap_or(NativeCountArrayIdentity::Slot(slot));
    if active_arrays.contains(&identity) {
        return Ok((0, 1));
    }
    let entries = context
        .direct_array_entries_for(encoded)
        .ok_or_else(|| "count() direct array entries are unavailable".to_owned())?;
    active_arrays.push(identity);
    let mut count = entries.len();
    let mut recursion_warnings = 0_usize;
    for entry in entries {
        let child = context.dereference_direct_encoding(entry.value);
        if context.native_encoded_value_kind(child) != Some(NativeEncodedValueKind::Array) {
            continue;
        }
        let reference_identity = context.native_reference_identity(entry.value);
        if reference_identity.is_some_and(|identity| active_references.contains(&identity)) {
            recursion_warnings = recursion_warnings
                .checked_add(1)
                .ok_or_else(|| "count() recursion warning count overflow".to_owned())?;
            continue;
        }
        if let Some(identity) = reference_identity {
            active_references.push(identity);
        }
        let nested_result =
            count_native_array_recursive(context, child, active_arrays, active_references);
        if reference_identity.is_some() {
            active_references.pop();
        }
        let (nested, warnings) = nested_result?;
        count = count
            .checked_add(nested)
            .ok_or_else(|| "count() result overflow".to_owned())?;
        recursion_warnings = recursion_warnings
            .checked_add(warnings)
            .ok_or_else(|| "count() recursion warning count overflow".to_owned())?;
    }
    let popped = active_arrays.pop();
    debug_assert_eq!(popped, Some(identity));
    Ok((count, recursion_warnings))
}

fn execute_native_count_builtin(
    context: &mut NativeRequestColdState<'_>,
    name: &str,
    arguments: &[i64],
    source: &php_ir::Instruction,
    caller_function: Option<u32>,
) -> Result<Option<i64>, String> {
    if !matches!(name, "count" | "sizeof") {
        return Ok(None);
    }
    let [value, rest @ ..] = arguments else {
        return Err("count() expects an argument".to_owned());
    };
    if rest.len() > 1 {
        return Err("count() expects at most 2 arguments".to_owned());
    }
    let strict = context.unit.strict_types_for_span(source.span);
    let mode = match rest.first() {
        Some(mode) => native_builtin_int_argument(context, *mode, strict)?.ok_or_else(|| {
            format!(
                "E_PHP_THROW:TypeError:{name}(): Argument #2 ($mode) must be of type int, {} given",
                context.native_encoded_type_name(*mode)
            )
        })?,
        None => 0,
    };
    if !matches!(mode, 0 | 1) {
        return Err(format!(
            "E_PHP_THROW:ValueError:{name}(): Argument #2 ($mode) must be either COUNT_NORMAL or COUNT_RECURSIVE"
        ));
    }

    let value = context.dereference_direct_encoding(*value);
    let kind = context.native_encoded_value_kind(value);
    match kind {
        Some(NativeEncodedValueKind::Array) => {
            let (count, recursion_warnings) = if mode == 0 {
                (
                    context
                        .direct_array_length(value)
                        .ok_or_else(|| "count() direct array length is unavailable".to_owned())?,
                    0,
                )
            } else {
                count_native_array_recursive(context, value, &mut Vec::new(), &mut Vec::new())?
            };
            for _ in 0..recursion_warnings {
                emit_native_php_warning(
                    context,
                    php_runtime::api::PHP_E_WARNING,
                    &format!("{name}(): Recursion detected"),
                    source,
                )?;
            }
            let count = i64::try_from(count).map_err(|_| "count() result overflow".to_owned())?;
            context.encode_native_int(count).map(Some)
        }
        Some(NativeEncodedValueKind::Object) => {
            let object = context
                .native_query_object(value)
                .ok_or_else(|| "count() direct object owner is unavailable".to_owned())?;
            if object.class_name().eq_ignore_ascii_case("ArrayIterator")
                || native_dom_collection_entries(&object).is_some()
                || native_simple_xml_count(&object).is_some()
            {
                // Extension compatibility objects still expose collection
                // payload through their explicit baseline object facade.
                return Ok(None);
            }
            if !object.is_native_countable() {
                return Err(format!(
                    "E_PHP_THROW:TypeError:{name}(): Argument #1 ($value) must be of type Countable|array, {} given",
                    context.native_encoded_type_name(value)
                ));
            }
            if let Some(function) =
                native_method_in_hierarchy(context, &object.class_name(), "count")
            {
                let receiver = context.encode_native_object_owner(object)?;
                return invoke_native_method(context, function, &[receiver])
                    .map(Some)
                    .map_err(String::from);
            }
            let target = php_runtime::api::CallableMethodTarget::Object(object);
            invoke_baseline_native_bound_method(
                context,
                &target,
                "count",
                &[],
                None,
                strict,
                caller_function,
            )
            .map(Some)
            .map_err(String::from)
        }
        Some(_) => Err(format!(
            "E_PHP_THROW:TypeError:{name}(): Argument #1 ($value) must be of type Countable|array, {} given",
            context.native_encoded_type_name(value)
        )),
        None if mode == 1 => {
            let value = context.decode_baseline_value(value)?;
            let Some((count, recursion_warnings)) =
                php_runtime::api::baseline_count_recursive_value(&value)
            else {
                return Ok(None);
            };
            for _ in 0..recursion_warnings {
                emit_native_php_warning(
                    context,
                    php_runtime::api::PHP_E_WARNING,
                    &format!("{name}(): Recursion detected"),
                    source,
                )?;
            }
            let count = i64::try_from(count).map_err(|_| "count() result overflow".to_owned())?;
            context.encode_native_int(count).map(Some)
        }
        None => Ok(None),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeCallableMethodStatus {
    Callable,
    NonStatic,
    Unavailable,
}

fn native_local_method_entry<'a>(
    context: &'a NativeRequestColdState<'_>,
    class_name: &str,
    method: &str,
) -> Option<&'a php_ir::module::ClassMethodEntry> {
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
            return Some(entry);
        }
        candidate = normalize_class_name(class.parent.as_ref()?);
    }
}

fn native_callable_declared_method_status(
    context: &NativeRequestColdState<'_>,
    class_name: &str,
    method: &str,
    object_target: bool,
    caller_function: u32,
    magic_visibility: bool,
) -> NativeCallableMethodStatus {
    if let Some(entry) = native_local_method_entry(context, class_name, method) {
        if !magic_visibility
            && native_method_access_error(context, entry.function, caller_function, false).is_some()
        {
            return NativeCallableMethodStatus::Unavailable;
        }
        if !object_target && !entry.flags.is_static {
            return NativeCallableMethodStatus::NonStatic;
        }
        return NativeCallableMethodStatus::Callable;
    }
    if let Some((target, entry)) = native_external_method(context, class_name, method) {
        if !magic_visibility
            && native_external_method_access_error(context, target, caller_function, false)
                .is_some()
        {
            return NativeCallableMethodStatus::Unavailable;
        }
        if !object_target && !entry.flags.is_static {
            return NativeCallableMethodStatus::NonStatic;
        }
        return NativeCallableMethodStatus::Callable;
    }
    if let Some(method) =
        php_std::generated::arginfo::method_metadata_in_hierarchy(class_name, method)
    {
        if !object_target && !method.is_static {
            return NativeCallableMethodStatus::NonStatic;
        }
        return NativeCallableMethodStatus::Callable;
    }
    NativeCallableMethodStatus::Unavailable
}

fn native_callable_method_is_available(
    context: &NativeRequestColdState<'_>,
    class_name: &str,
    method: &str,
    object_target: bool,
    caller_function: u32,
) -> bool {
    let ignore_visibility = object_target && method.eq_ignore_ascii_case("__invoke");
    match native_callable_declared_method_status(
        context,
        class_name,
        method,
        object_target,
        caller_function,
        ignore_visibility,
    ) {
        NativeCallableMethodStatus::Callable => true,
        NativeCallableMethodStatus::NonStatic => false,
        NativeCallableMethodStatus::Unavailable if method.eq_ignore_ascii_case("__invoke") => false,
        NativeCallableMethodStatus::Unavailable => {
            let magic = if object_target {
                "__call"
            } else {
                "__callStatic"
            };
            matches!(
                native_callable_declared_method_status(
                    context,
                    class_name,
                    magic,
                    object_target,
                    caller_function,
                    true,
                ),
                NativeCallableMethodStatus::Callable
            )
        }
    }
}

pub(super) fn native_callable_array_parts(
    context: &NativeRequestColdState<'_>,
    encoded: i64,
) -> Option<(i64, i64)> {
    let entries = context.direct_array_entries_for(encoded)?;
    if entries.len() != 2 {
        return None;
    }
    let mut target = None;
    let mut method = None;
    for entry in entries {
        match context.native_encoded_int(entry.key) {
            Some(0) => target = Some(entry.value),
            Some(1) => method = Some(entry.value),
            _ => return None,
        }
    }
    Some((target?, method?))
}

fn native_callable_resolved_class(
    context: &NativeRequestColdState<'_>,
    class_name: &str,
    caller_function: u32,
) -> Option<String> {
    native_resolve_scoped_class_name(context, class_name, caller_function).ok()
}

fn native_callable_name_bytes(
    context: &NativeRequestColdState<'_>,
    encoded: i64,
) -> Result<Vec<u8>, String> {
    let encoded = context.dereference_direct_encoding(encoded);
    let Some(kind) = context.native_encoded_value_kind(encoded) else {
        return Err("is_callable() received an invalid native value".to_owned());
    };
    Ok(match kind {
        NativeEncodedValueKind::Null | NativeEncodedValueKind::Uninitialized => Vec::new(),
        NativeEncodedValueKind::Bool(true) => b"1".to_vec(),
        NativeEncodedValueKind::Bool(false) => Vec::new(),
        NativeEncodedValueKind::Int => context
            .native_encoded_int(encoded)
            .ok_or_else(|| "is_callable() integer payload is unavailable".to_owned())?
            .to_string()
            .into_bytes(),
        NativeEncodedValueKind::Float => php_runtime::api::float_to_php_string(
            context
                .native_encoded_float(encoded)
                .ok_or_else(|| "is_callable() float payload is unavailable".to_owned())?,
        )
        .into_bytes(),
        NativeEncodedValueKind::String => context
            .native_string_name_bytes(encoded)
            .ok_or_else(|| "is_callable() string bytes are unavailable".to_owned())?,
        NativeEncodedValueKind::Array => {
            let Some((target, method)) = native_callable_array_parts(context, encoded) else {
                return Ok(b"Array".to_vec());
            };
            let method = context.dereference_direct_encoding(method);
            if context.native_encoded_value_kind(method) != Some(NativeEncodedValueKind::String) {
                return Ok(b"Array".to_vec());
            }
            let target = context.dereference_direct_encoding(target);
            let mut name = match context.native_encoded_value_kind(target) {
                Some(NativeEncodedValueKind::String) => context
                    .native_string_name_bytes(target)
                    .ok_or_else(|| "is_callable() class bytes are unavailable".to_owned())?,
                Some(NativeEncodedValueKind::Object) => context
                    .native_query_object(target)
                    .map(|object| object.display_name().into_bytes())
                    .ok_or_else(|| "is_callable() object target is unavailable".to_owned())?,
                _ => return Ok(b"Array".to_vec()),
            };
            name.extend_from_slice(b"::");
            name.extend_from_slice(
                context
                    .native_string_bytes(method)
                    .ok_or_else(|| "is_callable() method bytes are unavailable".to_owned())?,
            );
            name
        }
        NativeEncodedValueKind::Object => {
            let object = context
                .native_query_object(encoded)
                .ok_or_else(|| "is_callable() object target is unavailable".to_owned())?;
            format!("{}::__invoke", object.display_name()).into_bytes()
        }
        NativeEncodedValueKind::Callable => match context.prepared_callable_dispatch(encoded) {
            Some(NativePreparedCallableDispatch::Closure) => b"Closure::__invoke".to_vec(),
            Some(NativePreparedCallableDispatch::Named(name)) => name.into_bytes(),
            Some(NativePreparedCallableDispatch::BoundMethod { target, method }) => {
                let class = match target {
                    php_runtime::api::CallableMethodTarget::Object(object) => object.display_name(),
                    php_runtime::api::CallableMethodTarget::Class(class) => class,
                };
                format!("{class}::{method}").into_bytes()
            }
            Some(NativePreparedCallableDispatch::Invalid(target)) => target.into_bytes(),
            None => return Err("is_callable() prepared callable is unavailable".to_owned()),
        },
        NativeEncodedValueKind::Resource => format!(
            "Resource id #{}",
            context
                .native_encoded_resource_id(encoded)
                .ok_or_else(|| "is_callable() resource payload is unavailable".to_owned())?
        )
        .into_bytes(),
        NativeEncodedValueKind::Generator => b"Generator::__invoke".to_vec(),
        NativeEncodedValueKind::Fiber => b"Fiber::__invoke".to_vec(),
        NativeEncodedValueKind::Reference => {
            return Err("is_callable() reference could not be dereferenced".to_owned());
        }
    })
}

pub(super) fn native_encoded_callable_is_valid(
    context: &NativeRequestColdState<'_>,
    encoded: i64,
    syntax_only: bool,
    caller_function: u32,
) -> bool {
    let encoded = context.dereference_direct_encoding(encoded);
    match context.native_encoded_value_kind(encoded) {
        Some(NativeEncodedValueKind::Callable) => true,
        Some(NativeEncodedValueKind::Object) => {
            let Some(object) = context.native_query_object(encoded) else {
                return false;
            };
            native_callable_method_is_available(
                context,
                &object.class_name(),
                "__invoke",
                true,
                caller_function,
            )
        }
        Some(NativeEncodedValueKind::String) if syntax_only => true,
        Some(NativeEncodedValueKind::String) => {
            let Some(bytes) = context.native_string_bytes(encoded) else {
                return false;
            };
            let name = String::from_utf8_lossy(bytes);
            if let Some((class, method)) = name.split_once("::") {
                let Some(class) = native_callable_resolved_class(context, class, caller_function)
                else {
                    return false;
                };
                native_callable_method_is_available(context, &class, method, false, caller_function)
            } else {
                let name = name.trim_start_matches('\\');
                !name.is_empty()
                    && (context.function_id(name).is_some()
                        || context.external_function(name).is_some()
                        || native_php_function_exists(&name.to_ascii_lowercase()))
            }
        }
        Some(NativeEncodedValueKind::Array) => {
            let Some((target, method)) = native_callable_array_parts(context, encoded) else {
                return false;
            };
            let method = context.dereference_direct_encoding(method);
            let Some(method) = context.native_string_bytes(method) else {
                return false;
            };
            let target = context.dereference_direct_encoding(target);
            let (class, object_target) = match context.native_encoded_value_kind(target) {
                Some(NativeEncodedValueKind::String) => {
                    let Some(class) = context.native_string_bytes(target) else {
                        return false;
                    };
                    (String::from_utf8_lossy(class).into_owned(), false)
                }
                Some(NativeEncodedValueKind::Object) => {
                    let Some(object) = context.native_query_object(target) else {
                        return false;
                    };
                    (object.class_name(), true)
                }
                _ => return false,
            };
            if syntax_only {
                return true;
            }
            let Some(class) = native_callable_resolved_class(context, &class, caller_function)
            else {
                return false;
            };
            native_callable_method_is_available(
                context,
                &class,
                &String::from_utf8_lossy(method),
                object_target,
                caller_function,
            )
        }
        _ => false,
    }
}

fn native_callable_autoload_target(
    context: &NativeRequestColdState<'_>,
    encoded: i64,
    caller_function: u32,
) -> Option<String> {
    let encoded = context.dereference_direct_encoding(encoded);
    let class = match context.native_encoded_value_kind(encoded)? {
        NativeEncodedValueKind::String => {
            let name = String::from_utf8_lossy(context.native_string_bytes(encoded)?);
            name.split_once("::")?.0.to_owned()
        }
        NativeEncodedValueKind::Array => {
            let (target, _) = native_callable_array_parts(context, encoded)?;
            let target = context.dereference_direct_encoding(target);
            String::from_utf8_lossy(context.native_string_bytes(target)?).into_owned()
        }
        _ => return None,
    };
    let class = native_callable_resolved_class(context, &class, caller_function)?;
    (!class.is_empty()).then_some(class)
}

fn materialize_native_stream_context_for_baseline(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
) -> Result<Vec<php_runtime::api::ResourceRef>, String> {
    let default = context.native_stream_context.default_options;
    let Value::Array(default) = context.decode_baseline_value(default)? else {
        return Err("native default stream context is not an array".to_owned());
    };
    context
        .registered_extensions
        .set_stream_context_default_options(default);

    let mut resources = Vec::new();
    for argument in arguments {
        let Some(resource) = context.native_resource(*argument) else {
            continue;
        };
        if resource.kind() != php_runtime::api::ResourceKind::StreamContext
            || resources
                .iter()
                .any(|known: &php_runtime::api::ResourceRef| known.id() == resource.id())
        {
            continue;
        }
        if let Some(options) = context
            .native_stream_context
            .resource_options
            .get(&resource.id().get())
            .copied()
        {
            let Value::Array(options) = context.decode_baseline_value(options)? else {
                return Err("native stream context resource options are not an array".to_owned());
            };
            resource
                .replace_context_options(options)
                .map_err(|error| error.message().to_owned())?;
        }
        resources.push(resource);
    }
    Ok(resources)
}

fn republish_native_stream_context_after_baseline(
    context: &mut NativeRequestColdState<'_>,
    argument_resources: Vec<php_runtime::api::ResourceRef>,
    result: &Result<Value, php_runtime::api::BuiltinError>,
) -> Result<(), String> {
    let default = context
        .registered_extensions
        .stream_context_default_options();
    let default = context.encode_native_array_owner(default)?;
    let previous = std::mem::replace(&mut context.native_stream_context.default_options, default);
    context.release(previous)?;

    let mut resources = argument_resources;
    if let Ok(Value::Resource(resource)) = result
        && resource.kind() == php_runtime::api::ResourceKind::StreamContext
        && !resources.iter().any(|known| known.id() == resource.id())
    {
        resources.push(resource.clone());
    }
    for resource in resources {
        let Some(options) = resource.context_options() else {
            continue;
        };
        let options = context.encode_native_array_owner(options)?;
        let previous = context
            .native_stream_context
            .resource_options
            .insert(resource.id().get(), options);
        if let Some(previous) = previous {
            context.release(previous)?;
        }
        resource.clear_context_options();
    }
    Ok(())
}

/// Baseline-native compatibility executor for runtime-backed builtins.
///
/// Optimizing artifacts never import this path. Fixed admitted builtins use
/// their exact typed ABI; uncommon/dynamic call shapes enter this executor
/// only after the caller has selected its baseline continuation.
pub(super) fn execute_baseline_prepared_runtime_builtin(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    source: php_ir::IrSpan,
    prepared: crate::compiled_unit::PreparedNativeBuiltin,
) -> Result<i64, String> {
    let entry = prepared.entry;
    let name = entry.name();
    if native_builtin_is_unavailable_target_function(name) {
        return Err(format!(
            "E_PHP_THROW:Error:Call to undefined function {name}()"
        ));
    }
    if matches!(name, "strftime" | "gmstrftime") && !(1..=2).contains(&arguments.len()) {
        emit_native_php_diagnostic_at_span(
            context,
            php_runtime::api::PHP_E_DEPRECATED,
            &format!(
                "Function {name}() is deprecated since 8.1, use IntlDateFormatter::format() instead"
            ),
            source,
            true,
        )?;
    }
    if !prepared.fixed_arity_validated {
        validate_native_builtin_arity_with_metadata(name, arguments.len(), prepared.metadata)?;
    }
    validate_native_builtin_types(context, name, arguments, source, Some(prepared.type_info))?;
    if name == "array_fill_keys"
        && let Some(result) = execute_exact_native_array_fill_keys(context, arguments)
    {
        return result;
    }
    let stream_context_resources = if name.starts_with("stream_context_") {
        Some(materialize_native_stream_context_for_baseline(
            context, arguments,
        )?)
    } else {
        None
    };
    let mut values = arguments
        .iter()
        .enumerate()
        .map(|(index, argument)| {
            let by_ref = prepared
                .metadata
                .and_then(|function| {
                    function.params.get(index).or_else(|| {
                        function
                            .params
                            .last()
                            .filter(|parameter| parameter.variadic)
                    })
                })
                .is_some_and(|parameter| parameter.by_ref);
            if by_ref {
                context.decode_baseline_value(*argument)
            } else {
                context.baseline_decode_dereferenced_native_value(*argument)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    if name == "shm_put_var" {
        prepare_native_sysvshm_serialization(context, &mut values)?;
    }
    let span = php_runtime::api::RuntimeSourceSpan {
        file: context
            .unit
            .files
            .get(source.file.index())
            .map(|file| file.path.clone()),
        start: source.start,
        end: source.end,
    };
    let lightweight_handler = matches!(
        entry.handler_kind(),
        php_runtime::api::BuiltinHandlerKind::Pure0
            | php_runtime::api::BuiltinHandlerKind::Pure1
            | php_runtime::api::BuiltinHandlerKind::Pure2
            | php_runtime::api::BuiltinHandlerKind::Pure3
            | php_runtime::api::BuiltinHandlerKind::BorrowedN
            | php_runtime::api::BuiltinHandlerKind::Json
            | php_runtime::api::BuiltinHandlerKind::Pcre
    );
    if name.starts_with("session_") {
        context.materialize_native_session_state()?;
    }
    let (result, diagnostics) = if lightweight_handler {
        let mut builtin = borrow_native_builtin_context!(context);
        builtin.set_diagnostic_display(php_runtime::api::PhpDiagnosticDisplayOptions {
            display_errors: false,
            error_reporting: context.error_reporting,
            leading_newline: true,
        });
        let result = (entry.function())(&mut builtin, values, span);
        let diagnostics = builtin.take_diagnostics();
        (result, diagnostics)
    } else {
        let mut builtin = borrow_native_builtin_context!(context);
        builtin.set_diagnostic_display(php_runtime::api::PhpDiagnosticDisplayOptions {
            display_errors: false,
            error_reporting: context.error_reporting,
            leading_newline: true,
        });
        if let php_runtime::api::RuntimeRequestMode::Http(request) =
            &context.options.runtime_context.request_mode
        {
            builtin.set_php_input(Arc::clone(&request.raw_body));
        }
        builtin.set_filter_input_arrays_shared(Rc::clone(
            &context.baseline_values.filter_input_arrays,
        ));
        builtin.set_http_response_state(&mut context.http_response);
        builtin.set_upload_registry(&mut context.upload_registry);
        builtin.set_session_state(
            &mut context.session,
            context.baseline_values.session_global.clone(),
        );
        builtin.set_session_loader(context.options.runtime_context.session_loader.as_ref());
        builtin.set_session_id_generator(
            context
                .options
                .runtime_context
                .session_id_generator
                .as_ref(),
        );
        builtin.sync_session_state_from_global();
        let mut mysql_state = context.mysql_state.borrow_mut();
        builtin.set_mysql_state(&mut mysql_state);
        context.registered_extensions.bind(&mut builtin);
        let result = (entry.function())(&mut builtin, values, span);
        builtin.sync_session_state_from_global();
        let diagnostics = builtin.take_diagnostics();
        (result, diagnostics)
    };
    if let Some(resources) = stream_context_resources {
        republish_native_stream_context_after_baseline(context, resources, &result)?;
    }
    if name.starts_with("session_") {
        context.republish_native_session_commit()?;
        context.mark_roots_dirty(RootMutationReason::Session);
    }
    if !diagnostics.is_empty() {
        let diagnostic_source = php_ir::Instruction {
            id: php_ir::InstrId::new(0),
            span: source,
            kind: php_ir::InstructionKind::Nop,
        };
        for diagnostic in diagnostics {
            let errno = match diagnostic.severity() {
                php_runtime::api::RuntimeSeverity::Notice => php_runtime::api::PHP_E_NOTICE,
                php_runtime::api::RuntimeSeverity::Deprecation => {
                    php_runtime::api::PHP_E_DEPRECATED
                }
                _ => php_runtime::api::PHP_E_WARNING,
            };
            emit_native_php_diagnostic(
                context,
                errno,
                diagnostic.message(),
                &diagnostic_source,
                true,
            )?;
        }
    }
    match result {
        Ok(value) => context.encode_baseline_value(value),
        Err(error) => {
            let id = error.diagnostic_id().to_ascii_uppercase();
            let class = if id.contains("ARITY") || id.contains("ARGUMENT_COUNT") {
                "ArgumentCountError"
            } else if id.contains("VALUE") {
                "ValueError"
            } else if id.contains("TYPE") {
                "TypeError"
            } else {
                "Error"
            };
            Err(format!("E_PHP_THROW:{class}:{}", error.message()))
        }
    }
}

fn execute_exact_native_array_fill_keys(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    let [keys, value] = arguments else {
        return None;
    };
    let source = context.direct_array_entries_for(*keys)?.to_vec();
    let mut normalized = Vec::<php_runtime::api::ArrayKey>::with_capacity(source.len());
    for entry in source {
        let mut key = context.decode_baseline_value(entry.value).ok()?;
        for _ in 0..16 {
            let Value::Reference(reference) = key else {
                break;
            };
            key = reference.get();
        }
        // `array_fill_keys()` stringifies each input value before applying
        // normal PHP string-key normalization. In particular, `3.7` becomes
        // the string key `"3.7"`; ordinary array-key coercion would
        // incorrectly truncate it to the integer key `3`.
        let key = php_runtime::api::to_string(&key).ok()?;
        let key = php_runtime::api::ArrayKey::from_php_string(key);
        if !normalized.contains(&key) {
            normalized.push(key);
        }
    }
    let mut entries = Vec::<php_jit::JitNativeDirectArrayEntry>::with_capacity(normalized.len());
    for key in normalized {
        let key = match key {
            php_runtime::api::ArrayKey::Int(key) => context.encode_baseline_value(Value::Int(key)),
            php_runtime::api::ArrayKey::String(key) => context.encode_native_string_owner(key),
        };
        let key = match key {
            Ok(key) => key,
            Err(error) => {
                for entry in entries {
                    let _ = context.release(entry.key);
                    let _ = context.release(entry.value);
                }
                return Some(Err(error));
            }
        };
        let value = match context.duplicate_dereferenced_native_value(*value) {
            Ok(value) => value,
            Err(error) => {
                let _ = context.release(key);
                for entry in entries {
                    let _ = context.release(entry.key);
                    let _ = context.release(entry.value);
                }
                return Some(Err(error));
            }
        };
        entries.push(php_jit::JitNativeDirectArrayEntry { key, value });
    }
    Some(context.publish_owned_direct_array_entries(entries))
}

fn release_owned_native_array_entries(
    context: &mut NativeRequestColdState<'_>,
    entries: &[php_jit::JitNativeDirectArrayEntry],
) {
    for entry in entries.iter().rev() {
        let _ = context.release(entry.value);
        let _ = context.release(entry.key);
    }
}

fn push_owned_native_named_entry(
    context: &mut NativeRequestColdState<'_>,
    entries: &mut Vec<php_jit::JitNativeDirectArrayEntry>,
    name: &[u8],
    value: i64,
) -> Result<(), String> {
    let key = match context.encode_native_string_bytes_owner(name) {
        Ok(key) => key,
        Err(error) => {
            let _ = context.release(value);
            return Err(error);
        }
    };
    entries.push(php_jit::JitNativeDirectArrayEntry { key, value });
    Ok(())
}

fn encode_native_argument_list(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
) -> Result<i64, String> {
    let mut entries = Vec::<php_jit::JitNativeDirectArrayEntry>::with_capacity(arguments.len());
    for (index, argument) in arguments.iter().copied().enumerate() {
        let key = match i64::try_from(index)
            .map_err(|_| "native argument-list index overflow".to_owned())
            .and_then(|index| context.encode_native_int(index))
        {
            Ok(key) => key,
            Err(error) => {
                release_owned_native_array_entries(context, &entries);
                return Err(error);
            }
        };
        let value = match context.duplicate_authoritative_native_value(argument) {
            Ok(Some(value)) => value,
            Ok(None) => match context.duplicate_baseline_call_argument(argument) {
                Ok(value) => value,
                Err(error) => {
                    let _ = context.release(key);
                    release_owned_native_array_entries(context, &entries);
                    return Err(error);
                }
            },
            Err(error) => {
                let _ = context.release(key);
                release_owned_native_array_entries(context, &entries);
                return Err(error);
            }
        };
        entries.push(php_jit::JitNativeDirectArrayEntry { key, value });
    }
    context.publish_owned_direct_array_entries(entries)
}

fn encode_native_backtrace_frame(
    context: &mut NativeRequestColdState<'_>,
    frame: &request_state::NativeBacktraceFrame,
    options: i64,
) -> Result<i64, String> {
    let metadata = frame.metadata.as_ref();
    let mut entries = Vec::<php_jit::JitNativeDirectArrayEntry>::with_capacity(7);
    let result = (|| -> Result<(), String> {
        if let Some(file) = metadata.and_then(|metadata| metadata.trace_file.as_ref()) {
            let value = context.encode_native_string_bytes_owner(file.as_bytes())?;
            push_owned_native_named_entry(context, &mut entries, b"file", value)?;
        }
        let line = metadata.map_or(0, |metadata| metadata.trace_line);
        if line > 0 {
            let value = context.encode_native_int(line)?;
            push_owned_native_named_entry(context, &mut entries, b"line", value)?;
        }
        let function = metadata.map_or(b"{unknown}".as_slice(), |metadata| {
            metadata.trace_function.as_bytes()
        });
        let value = context.encode_native_string_bytes_owner(function)?;
        push_owned_native_named_entry(context, &mut entries, b"function", value)?;
        if let Some(class) = frame.class.as_ref() {
            let value = context.encode_native_string_bytes_owner(class.as_bytes())?;
            push_owned_native_named_entry(context, &mut entries, b"class", value)?;
        }
        if let Some(call_type) = metadata.and_then(|metadata| metadata.trace_call_type) {
            let value = context.encode_native_string_bytes_owner(call_type.as_bytes())?;
            push_owned_native_named_entry(context, &mut entries, b"type", value)?;
        }
        if options & 1 != 0
            && let Some(object) = frame.object.as_ref()
        {
            let value = context.encode_native_object_owner(object.clone())?;
            push_owned_native_named_entry(context, &mut entries, b"object", value)?;
        }
        if options & 2 == 0 {
            let value = encode_native_argument_list(context, &frame.arguments)?;
            push_owned_native_named_entry(context, &mut entries, b"args", value)?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        release_owned_native_array_entries(context, &entries);
        return Err(error);
    }
    context.publish_owned_direct_array_entries(entries)
}

fn execute_native_debug_backtrace(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
) -> Result<i64, String> {
    let strict = context.unit.strict_types_for_span(source.span);
    let options = arguments.first().map_or(Ok(1), |argument| {
        native_builtin_int_argument(context, *argument, strict)?
            .ok_or_else(|| "debug_backtrace(): argument #1 must be of type int".to_owned())
    })?;
    let limit = arguments.get(1).map_or(Ok(0), |argument| {
        match native_builtin_int_argument(context, *argument, strict)? {
            Some(limit) if limit >= 0 => Ok(limit),
            Some(_) => Err(
                "debug_backtrace(): argument #2 ($limit) must be greater than or equal to 0"
                    .to_owned(),
            ),
            None => Err("debug_backtrace(): argument #2 must be of type int".to_owned()),
        }
    })?;
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    let frames = context
        .call_frames
        .iter()
        .rev()
        .take(if limit == 0 { usize::MAX } else { limit })
        .cloned()
        .collect::<Vec<_>>();
    let mut entries = Vec::<php_jit::JitNativeDirectArrayEntry>::with_capacity(frames.len());
    for (index, frame) in frames.iter().enumerate() {
        let key = match i64::try_from(index)
            .map_err(|_| "debug_backtrace() frame index overflow".to_owned())
            .and_then(|index| context.encode_native_int(index))
        {
            Ok(key) => key,
            Err(error) => {
                release_owned_native_array_entries(context, &entries);
                return Err(error);
            }
        };
        let value = match encode_native_backtrace_frame(context, frame, options) {
            Ok(value) => value,
            Err(error) => {
                let _ = context.release(key);
                release_owned_native_array_entries(context, &entries);
                return Err(error);
            }
        };
        entries.push(php_jit::JitNativeDirectArrayEntry { key, value });
    }
    context.publish_owned_direct_array_entries(entries)
}

fn execute_native_call_user_func_array_control(
    context: &mut NativeRequestColdState<'_>,
    arguments: &[i64],
    source: &php_ir::Instruction,
    caller_locals: Option<(u32, &[php_jit::JitAbiSlot])>,
) -> NativeCallResult {
    let [callback, arguments] = arguments else {
        return Err("call_user_func_array() expects exactly 2 arguments".into());
    };
    let callback_handle = *callback;
    if let Some(result) = execute_native_call_user_func_array_direct(
        context,
        callback_handle,
        *arguments,
        source,
        caller_locals.map(|(function, _)| function),
    ) {
        return result;
    }
    let callback = match context.decode_baseline_value(*callback)? {
        Value::Reference(reference) => reference.get(),
        value => value,
    };
    let arguments = match context.decode_baseline_value(*arguments)? {
        Value::Reference(reference) => reference.get(),
        value => value,
    };
    let Value::Array(arguments) = arguments else {
        return Err("call_user_func_array(): argument #2 must be an array".into());
    };
    if native_callable_has_no_by_ref_parameters(context, &callback) == Some(true) {
        let mut encoded = std::mem::take(&mut context.native_call_encoded_scratch);
        encoded.clear();
        encoded.reserve(arguments.len() + 1);
        encoded.push(callback_handle);
        let result = (|| -> NativeCallResult {
            let mut metadata: Option<Vec<php_ir::instruction::IrCallArg>> = None;
            for (key, value) in arguments.iter() {
                encoded.push(context.encode_baseline_value(value.clone())?);
                let name = match key {
                    php_runtime::api::ArrayKey::Int(_) => None,
                    php_runtime::api::ArrayKey::String(name) => Some(name.to_string_lossy()),
                };
                if name.is_some() && metadata.is_none() {
                    metadata = Some(
                        (0..encoded.len().saturating_sub(2))
                            .map(|_| positional_native_call_argument())
                            .collect(),
                    );
                }
                if let Some(metadata) = metadata.as_mut() {
                    let mut argument = positional_native_call_argument();
                    argument.name = name;
                    metadata.push(argument);
                }
            }
            invoke_native_encoded_callable_value_from(
                context,
                &encoded,
                source,
                metadata,
                caller_locals.map(|(function, _)| function),
            )
        })();
        encoded.clear();
        context.native_call_encoded_scratch = encoded;
        return result;
    }
    let mut values = Vec::with_capacity(arguments.len());
    let mut metadata = Vec::with_capacity(arguments.len());
    for (key, value) in arguments.iter() {
        values.push(value.clone());
        metadata.push(php_ir::instruction::IrCallArg {
            name: match key {
                php_runtime::api::ArrayKey::Int(_) => None,
                php_runtime::api::ArrayKey::String(name) => Some(name.to_string_lossy()),
            },
            value: php_ir::Operand::Register(php_ir::RegId::new(0)),
            unpack: false,
            value_kind: php_ir::instruction::IrCallArgValueKind::Direct,
            by_ref_local: None,
            by_ref_dim: None,
            by_ref_property: None,
            by_ref_property_dim: None,
        });
    }
    if let Value::String(name) = &callback {
        let name = name.to_string_lossy();
        let by_ref_parameters = context
            .function_id(&name)
            .and_then(|function| context.unit.functions.get(function.index()))
            .map(|function| {
                function
                    .params
                    .iter()
                    .enumerate()
                    .filter(|(_, parameter)| parameter.by_ref)
                    .map(|(index, parameter)| {
                        (index, function.name.clone(), parameter.name.clone())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (index, function_name, parameter_name) in by_ref_parameters {
            if values
                .get(index)
                .is_some_and(|value| !matches!(value, Value::Reference(_)))
            {
                emit_native_php_warning(
                    context,
                    php_runtime::api::PHP_E_WARNING,
                    &format!(
                        "{function_name}(): Argument #{} (${}) must be passed by reference, value given",
                        index + 1,
                        parameter_name,
                    ),
                    source,
                )?;
                if let Some(value) = values.get_mut(index) {
                    *value = Value::Reference(php_runtime::api::ReferenceCell::new(value.clone()));
                }
            }
        }
    }
    let callback_label = match &callback {
        Value::String(name) => name.to_string_lossy(),
        Value::Callable(callable) => match callable.as_ref() {
            php_runtime::api::CallableValue::UserFunction { name }
            | php_runtime::api::CallableValue::InternalBuiltin { name } => name.clone(),
            php_runtime::api::CallableValue::BoundMethod { method, .. } => method.clone(),
            php_runtime::api::CallableValue::Closure(_) => "Closure".to_owned(),
            php_runtime::api::CallableValue::MethodPlaceholder { target }
            | php_runtime::api::CallableValue::UnresolvedDynamic { target } => target.clone(),
        },
        _ => "dynamic callable".to_owned(),
    };
    let mut encoded = Vec::with_capacity(values.len() + 1);
    encoded.push(context.encode_baseline_value(callback)?);
    for value in values {
        encoded.push(context.encode_baseline_value(value)?);
    }
    invoke_native_encoded_callable_value_from(
        context,
        &encoded,
        source,
        Some(metadata),
        caller_locals.map(|(function, _)| function),
    )
    .map_err(|control| match control {
        NativeCallControl::RuntimeError(error) if error.starts_with("native runtime value ") => {
            NativeCallControl::RuntimeError(format!(
                "native callback {callback_label} failed: {error}"
            ))
        }
        control => control,
    })
}

pub(super) fn execute_native_callback_builtin_control(
    context: &mut NativeRequestColdState<'_>,
    name: &str,
    arguments: &[i64],
    source: &php_ir::Instruction,
    caller_locals: Option<(u32, &[php_jit::JitAbiSlot])>,
) -> Option<NativeCallResult> {
    match name {
        "call_user_func" | "forward_static_call" => Some(execute_native_call_user_func_encoded(
            context,
            arguments,
            source,
            caller_locals.map(|(function, _)| function),
        )),
        "call_user_func_array" => Some(execute_native_call_user_func_array_control(
            context,
            arguments,
            source,
            caller_locals,
        )),
        _ => None,
    }
}

pub(super) fn execute_baseline_native_builtin_control(
    context: &mut NativeRequestColdState<'_>,
    name: &str,
    arguments: &[i64],
    source: &php_ir::Instruction,
    caller_locals: Option<(u32, &[php_jit::JitAbiSlot])>,
    prepared: Option<crate::compiled_unit::PreparedNativeBuiltin>,
) -> NativeCallResult {
    let normalized = normalized_native_builtin_name(name);
    if matches!(
        normalized.as_ref(),
        "call_user_func" | "call_user_func_array" | "forward_static_call"
    ) && !prepared.is_some_and(|builtin| builtin.fixed_arity_validated)
    {
        let metadata = prepared
            .and_then(|builtin| builtin.metadata)
            .or_else(|| php_std::arginfo::function_metadata_indexed(&normalized));
        validate_native_builtin_arity_with_metadata(&normalized, arguments.len(), metadata)
            .map_err(NativeCallControl::from_baseline_error)?;
    }
    if let Some(outcome) = execute_native_callback_builtin_control(
        context,
        &normalized,
        arguments,
        source,
        caller_locals,
    ) {
        outcome
    } else {
        execute_baseline_native_builtin(context, name, arguments, source, caller_locals, prepared)
            .map_err(NativeCallControl::from_baseline_error)
    }
}

/// Reconstructs the PHP-visible argument vector for the cold compatibility
/// continuation. Direct compiled callers publish the original positional
/// tail in the native runtime view, while fixed parameters are read from the
/// callee's current local slots so assignments made before `func_get_arg(s)`
/// remain observable.
fn baseline_visible_call_arguments(
    context: &NativeRequestColdState<'_>,
    caller_locals: Option<(u32, &[php_jit::JitAbiSlot])>,
) -> Result<Vec<i64>, String> {
    // SAFETY: the fast state is separately allocated for the request and the
    // active linked view, when selected, is owned by the active dynamic-unit
    // package for the complete synchronous call.
    // Safety: the active cold request owns the raw VM state for this synchronous continuation.
    #[allow(unsafe_code)]
    // Safety: the request-owned fast state remains live for this synchronous baseline call.
    let view = unsafe { &*context.fast_state }.header.active_runtime_view();
    let count = usize::try_from(view.active_call_argument_count)
        .map_err(|_| "active native call argument count is invalid".to_owned())?;
    let matching_baseline_frame = caller_locals.and_then(|(function_id, _)| {
        let function = context.unit.functions.get(function_id as usize)?;
        context.call_frames.last().filter(|frame| {
            frame
                .metadata
                .as_ref()
                .is_some_and(|metadata| metadata.name.eq_ignore_ascii_case(&function.name))
        })
    });
    let mut visible = if let Some(frame) = matching_baseline_frame {
        frame.arguments.to_vec()
    } else if view.active_call_tail_arguments != 0 {
        let fixed_count = usize::try_from(view.active_call_fixed_argument_count)
            .map_err(|_| "active native fixed argument count is invalid".to_owned())?;
        let mut values = if fixed_count == 0 {
            Vec::new()
        } else {
            if view.active_call_arguments == 0 {
                return Err("active native fixed argument range is unavailable".to_owned());
            }
            // SAFETY: direct call lowering keeps the fixed stack prefix live
            // until the synchronous callee or its baseline continuation
            // returns.
            // Safety: the active cold request owns the raw VM state for this synchronous continuation.
            #[allow(unsafe_code)]
            // Safety: generated call arguments remain live through the synchronous continuation.
            unsafe {
                std::slice::from_raw_parts(
                    view.active_call_arguments as usize as *const i64,
                    fixed_count,
                )
                .to_vec()
            }
        };
        let tail = context
            .direct_array_entries_for(view.active_call_tail_arguments)
            .ok_or_else(|| "active native tail argument array is unavailable".to_owned())?;
        values.extend(tail.iter().map(|entry| entry.value));
        values
    } else if view.active_call_arguments == 0 {
        context
            .call_frames
            .last()
            .map(|frame| frame.arguments.to_vec())
            .unwrap_or_default()
    } else if count == 0 {
        Vec::new()
    } else {
        // SAFETY: direct call lowering keeps its stack range alive until the
        // baseline continuation returns; ordinary baseline frames publish a
        // request-owned NativeTraceArguments allocation.
        // Safety: the active cold request owns the raw VM state for this synchronous continuation.
        #[allow(unsafe_code)] // Safety: the active call frame owns the published argument range.
        unsafe {
            std::slice::from_raw_parts(view.active_call_arguments as usize as *const i64, count)
                .to_vec()
        }
    };
    let fixed_count = usize::try_from(
        matching_baseline_frame.map_or(view.active_call_fixed_argument_count, |frame| {
            frame.fixed_argument_count
        }),
    )
    .unwrap_or(usize::MAX)
    .min(visible.len());
    let Some((function_id, slots)) = caller_locals else {
        return Ok(visible);
    };
    let function = context
        .unit
        .functions
        .get(function_id as usize)
        .ok_or_else(|| "active native caller function metadata is missing".to_owned())?;
    for (index, parameter) in function.params.iter().take(fixed_count).enumerate() {
        if parameter.variadic {
            break;
        }
        if let Some(slot) = slots.get(parameter.local.index()) {
            visible[index] = slot.payload as i64;
        }
    }
    Ok(visible)
}

/// Baseline-native compatibility executor for builtins without an admitted
/// exact handler. This must not be imported by optimizing artifacts.
pub(super) fn execute_baseline_native_builtin(
    context: &mut NativeRequestColdState<'_>,
    name: &str,
    arguments: &[i64],
    source: &php_ir::Instruction,
    caller_locals: Option<(u32, &[php_jit::JitAbiSlot])>,
    prepared: Option<crate::compiled_unit::PreparedNativeBuiltin>,
) -> Result<i64, String> {
    if let Some(prepared) = prepared
        && matches!(
            prepared.entry.execution_kind(),
            php_runtime::api::BuiltinExecutionKind::Runtime
        )
        && !matches!(
            prepared.entry.name(),
            "set_time_limit" | "strlen" | "count" | "sizeof" | "is_callable"
        )
    {
        return execute_baseline_prepared_runtime_builtin(
            context,
            arguments,
            source.span,
            prepared,
        );
    }
    // A prepared direct callsite owns a canonical static registry name. The
    // generic path still normalizes dynamic names, while warm direct calls do
    // no allocation, case folding, or registry-availability lookup.
    let normalized = prepared.map_or_else(
        || normalized_native_builtin_name(name),
        |builtin| std::borrow::Cow::Borrowed(builtin.entry.name()),
    );
    if native_builtin_is_unavailable_target_function(&normalized) {
        return Err(format!(
            "E_PHP_THROW:Error:Call to undefined function {name}()"
        ));
    }
    if matches!(normalized.as_ref(), "strftime" | "gmstrftime")
        && !(1..=2).contains(&arguments.len())
    {
        emit_native_php_diagnostic(
            context,
            php_runtime::api::PHP_E_DEPRECATED,
            &format!(
                "Function {normalized}() is deprecated since 8.1, use IntlDateFormatter::format() instead"
            ),
            source,
            true,
        )?;
    }
    if !prepared.is_some_and(|builtin| builtin.fixed_arity_validated) {
        let metadata = prepared
            .and_then(|builtin| builtin.metadata)
            .or_else(|| php_std::arginfo::function_metadata_indexed(&normalized));
        validate_native_builtin_arity_with_metadata(&normalized, arguments.len(), metadata)?;
    }
    validate_native_builtin_types(
        context,
        &normalized,
        arguments,
        source.span,
        prepared.map(|builtin| builtin.type_info),
    )?;
    if let Some(result) = execute_native_type_predicate(context, &normalized, arguments)? {
        return Ok(result);
    }
    if let Some(result) = execute_native_count_builtin(
        context,
        &normalized,
        arguments,
        source,
        caller_locals.map(|(function, _)| function),
    )? {
        return Ok(result);
    }
    if let Some(result) = execute_native_read_builtin_fast(context, &normalized, arguments, source)?
    {
        return Ok(result);
    }
    if let Some(result) = execute_native_internal_builtin(context, &normalized, arguments) {
        return result;
    }
    match normalized.as_ref() {
        "is_a" => {
            let [target, expected, rest @ ..] = arguments else {
                return Err("is_a() expects 2 or 3 arguments".to_owned());
            };
            let (target, is_object) = baseline_native_class_target(context, *target)?;
            let expected =
                native_string(context.baseline_decode_dereferenced_native_value(*expected)?)?;
            let expected = String::from_utf8_lossy(&expected);
            let allow_string = if let Some(value) = rest.first() {
                php_runtime::api::to_bool(
                    &context.baseline_decode_dereferenced_native_value(*value)?,
                )?
            } else {
                false
            };
            context.encode_baseline_value(Value::Bool(
                (is_object || allow_string) && native_class_is_a(context, &target, &expected),
            ))
        }
        "class_implements" => {
            let [target, rest @ ..] = arguments else {
                return Err("class_implements() expects 1 or 2 arguments".to_owned());
            };
            let (target, _) = baseline_native_class_target(context, *target)?;
            if let Some(value) = rest.first() {
                let _ = php_runtime::api::to_bool(
                    &context.baseline_decode_dereferenced_native_value(*value)?,
                )?;
            }
            // Safety: the cold request owns and activates this fast-state
            // capability for the complete synchronous baseline call.
            #[allow(unsafe_code)] // Safety: the request owns the active fast state.
            let names = unsafe {
                baseline_class_interface_names(&(*context.fast_state).symbol_query, &target)
            };
            let Some(names) = names else {
                return context.encode_baseline_value(Value::Bool(false));
            };
            let mut result = php_runtime::api::PhpArray::new();
            for name in names {
                result.insert(
                    php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                        name.as_bytes().to_vec(),
                    )),
                    Value::string(name),
                );
            }
            context.encode_native_array_owner(result)
        }
        "get_included_files" | "get_required_files" => {
            let files = context
                .included_files
                .iter()
                .map(|path| Value::string(path.to_string_lossy().into_owned()))
                .collect();
            context.encode_baseline_value(Value::packed_array(files))
        }
        "ob_start" => {
            context.output.start_buffer();
            context.encode_baseline_value(Value::Bool(true))
        }
        "ob_get_clean" => {
            let Some(bytes) = context.output.pop_buffer_clean() else {
                return context.encode_baseline_value(Value::Bool(false));
            };
            context.encode_native_string_owner(PhpString::from_bytes(bytes))
        }
        "ob_get_contents" => {
            let value = context
                .output
                .current_buffer_bytes()
                .map(|bytes| Value::String(PhpString::from_bytes(bytes.to_vec())))
                .unwrap_or(Value::Bool(false));
            context.encode_baseline_value(value)
        }
        "ob_get_level" => {
            context.encode_baseline_value(Value::Int(context.output.buffer_level() as i64))
        }
        "ob_get_length" => context.encode_baseline_value(
            context
                .output
                .current_buffer_len()
                .map_or(Value::Bool(false), |length| Value::Int(length as i64)),
        ),
        "ob_get_flush" => {
            let Some(bytes) = context.output.current_buffer_bytes().map(<[u8]>::to_vec) else {
                emit_native_php_diagnostic(
                    context,
                    php_runtime::api::PHP_E_NOTICE,
                    "ob_get_flush(): Failed to delete and flush buffer. No buffer to delete or flush",
                    source,
                    true,
                )?;
                return context.encode_baseline_value(Value::Bool(false));
            };
            debug_assert!(context.output.pop_buffer_flush().is_some());
            context.encode_native_string_owner(PhpString::from_bytes(bytes))
        }
        "ob_end_flush" => {
            if context.output.pop_buffer_flush().is_none() {
                emit_native_php_diagnostic(
                    context,
                    php_runtime::api::PHP_E_NOTICE,
                    "ob_end_flush(): Failed to delete and flush buffer. No buffer to delete or flush",
                    source,
                    true,
                )?;
                return context.encode_baseline_value(Value::Bool(false));
            }
            context.encode_baseline_value(Value::Bool(true))
        }
        "ob_end_clean" => {
            if context.output.pop_buffer_clean().is_none() {
                emit_native_php_diagnostic(
                    context,
                    php_runtime::api::PHP_E_NOTICE,
                    "ob_end_clean(): Failed to delete buffer. No buffer to delete",
                    source,
                    true,
                )?;
                return context.encode_baseline_value(Value::Bool(false));
            }
            context.encode_baseline_value(Value::Bool(true))
        }
        "array_map" => execute_native_array_map(context, arguments, source),
        "array_filter" => execute_native_array_filter(context, arguments, source),
        "array_reduce" => execute_native_array_reduce(context, arguments, source),
        "array_walk" => execute_native_array_walk(context, arguments, source),
        "array_walk_recursive" => execute_native_array_walk_recursive(context, arguments, source),
        "iterator_to_array" => execute_native_iterator_to_array(context, arguments),
        "array_any" | "array_all" | "array_find" | "array_find_key" => {
            execute_native_array_predicate(context, &normalized, arguments, source)
        }
        "preg_replace_callback" => execute_native_preg_replace_callback(context, arguments, source),
        "preg_replace_callback_array" => {
            if let Some(result) =
                execute_native_preg_replace_callback_array(context, arguments, source)?
            {
                Ok(result)
            } else {
                Err(
                    "E_PHP_THROW:Error:preg_replace_callback_array requires VM callable dispatch for user callbacks"
                        .to_owned(),
                )
            }
        }
        "sort" | "rsort" | "asort" | "arsort" | "natsort" | "natcasesort" => {
            execute_baseline_value_sort(context, &normalized, arguments)
        }
        "array_multisort" => execute_baseline_array_multisort(context, arguments),
        "ksort" => execute_baseline_key_sort(context, arguments, false),
        "krsort" => execute_baseline_key_sort(context, arguments, true),
        "usort" => execute_baseline_callback_sort(context, arguments, source, false, false),
        "uasort" => execute_baseline_callback_sort(context, arguments, source, false, true),
        "uksort" => execute_baseline_callback_sort(context, arguments, source, true, true),
        "func_get_args" => {
            let arguments = baseline_visible_call_arguments(context, caller_locals)?;
            encode_native_argument_list(context, &arguments)
        }
        "get_defined_vars" => {
            let (function_id, slots) = caller_locals.ok_or_else(|| {
                "get_defined_vars() requires the active native caller symbol table".to_owned()
            })?;
            let locals = context
                .unit
                .functions
                .get(function_id as usize)
                .ok_or_else(|| "get_defined_vars() caller function metadata is missing".to_owned())?
                .locals
                .clone();
            let mut result = php_runtime::api::PhpArray::new();
            for (index, name) in locals.iter().enumerate() {
                if php_ir::is_compiler_generated_local_name(name) {
                    continue;
                }
                let Some(slot) = slots.get(index) else {
                    continue;
                };
                let value = context.decode_baseline_value(slot.payload as i64)?;
                if matches!(value, Value::Uninitialized) {
                    continue;
                }
                result.insert(
                    php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                        name.as_bytes().to_vec(),
                    )),
                    value,
                );
            }
            context.encode_native_array_owner(result)
        }
        "compact" => {
            let (function_id, slots) = caller_locals.ok_or_else(|| {
                "compact() requires the active native caller symbol table".to_owned()
            })?;
            let locals = context
                .unit
                .functions
                .get(function_id as usize)
                .ok_or_else(|| "compact() caller function metadata is missing".to_owned())?
                .locals
                .clone();
            let mut names = Vec::new();
            for argument in arguments {
                collect_native_compact_names(
                    context.decode_baseline_value(*argument)?,
                    &mut names,
                )?;
            }
            let mut result = php_runtime::api::PhpArray::new();
            for name in names {
                let Some(index) = locals.iter().position(|local| local == &name) else {
                    emit_native_php_warning(
                        context,
                        2,
                        &format!("compact(): Undefined variable ${name}"),
                        source,
                    )?;
                    continue;
                };
                let Some(slot) = slots.get(index) else {
                    emit_native_php_warning(
                        context,
                        2,
                        &format!("compact(): Undefined variable ${name}"),
                        source,
                    )?;
                    continue;
                };
                // PHP's compact() copies the current value into the result. It
                // never exposes the caller's reference container, even when
                // the source variable was explicitly bound by reference.
                let value = match context.decode_baseline_value(slot.payload as i64)? {
                    Value::Reference(reference) => reference.get(),
                    value => value,
                };
                if matches!(value, Value::Uninitialized) {
                    emit_native_php_warning(
                        context,
                        2,
                        &format!("compact(): Undefined variable ${name}"),
                        source,
                    )?;
                    continue;
                }
                result.insert(
                    php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                        name.as_bytes().to_vec(),
                    )),
                    value,
                );
            }
            context.encode_native_array_owner(result)
        }
        "implode" => {
            let (separator, values) = match arguments {
                [values] => (Vec::new(), *values),
                [separator, values] => (
                    native_string(context.decode_baseline_value(*separator)?)?,
                    *values,
                ),
                _ => return Err("implode() expects 1 or 2 arguments".to_owned()),
            };
            let values = match context.decode_baseline_value(values)? {
                Value::Reference(reference) => reference.get(),
                values => values,
            };
            let Value::Array(values) = values else {
                return Err("implode(): argument #2 must be of type array".to_owned());
            };
            let mut joined = Vec::new();
            for (index, (_, value)) in values.iter().enumerate() {
                if index != 0 {
                    joined.extend_from_slice(&separator);
                }
                let value = match value {
                    Value::Reference(reference) => reference.get(),
                    value => value.clone(),
                };
                joined.extend_from_slice(&native_string(value)?);
            }
            context.encode_native_string_owner(PhpString::from_bytes(joined))
        }
        "define" => {
            let [name, value, ..] = arguments else {
                return Err("define() expects a name and value".to_owned());
            };
            let name =
                String::from_utf8_lossy(&native_string(context.decode_baseline_value(*name)?)?)
                    .into_owned();
            let value = context.decode_baseline_value(*value)?;
            if context
                .baseline_values
                .cold_dynamic_constants
                .contains_key(&name)
                || context.lookup_constant(&name).is_ok()
            {
                let path = context
                    .unit
                    .files
                    .get(source.span.file.index())
                    .map_or("<unknown>", |file| file.path.as_str());
                let line = native_source_line(context, source);
                context.output.write_bytes(format!(
                    "\nWarning: Constant {name} already defined, this will be an error in PHP 9 in {path} on line {line}\n"
                ));
                return context.encode_baseline_value(Value::Bool(false));
            }
            context.insert_dynamic_constant(name, value)?;
            context.mark_roots_dirty(RootMutationReason::GlobalOrStatic);
            context.encode_baseline_value(Value::Bool(true))
        }
        "defined" => {
            let [name] = arguments else {
                return Err("defined() expects exactly 1 argument".to_owned());
            };
            let name =
                String::from_utf8_lossy(&native_string(context.decode_baseline_value(*name)?)?)
                    .into_owned();
            context.encode_baseline_value(Value::Bool(
                context.lookup_constant(&name).is_ok()
                    || native_internal_class_constant_exists(&name),
            ))
        }
        "constant" => {
            let [name] = arguments else {
                return Err("constant() expects exactly 1 argument".to_owned());
            };
            let name =
                String::from_utf8_lossy(&native_string(context.decode_baseline_value(*name)?)?)
                    .into_owned();
            context.encode_baseline_value(context.lookup_constant(&name)?)
        }
        "print" => {
            let [value] = arguments else {
                return Err("print expects exactly 1 argument".to_owned());
            };
            let value = context.decode_baseline_value(*value)?;
            let mut operation = php_runtime::api::NativeOperationContext::default();
            let status = php_runtime::api::native_echo(&mut operation, &mut context.output, &value);
            if status != php_runtime::api::NativeOperationStatus::Ok {
                return Err("print failed to render its argument".to_owned());
            }
            Ok(1)
        }
        "gettype" => {
            let [value] = arguments else {
                return Err("gettype() expects exactly 1 argument".to_owned());
            };
            let mut value = context.decode_baseline_value(*value)?;
            for _ in 0..16 {
                let Value::Reference(reference) = value else {
                    break;
                };
                value = reference.get();
            }
            let type_name = match value {
                Value::Null => "NULL",
                Value::Bool(_) => "boolean",
                Value::Int(_) => "integer",
                Value::Float(_) => "double",
                Value::String(_) => "string",
                Value::Array(_) => "array",
                Value::Object(_) => "object",
                Value::Resource(_) => "resource",
                Value::Uninitialized => "NULL",
                Value::Fiber(_) | Value::Generator(_) | Value::Callable(_) => "object",
                Value::Reference(_) => unreachable!("references were dereferenced above"),
            };
            context.encode_native_string_owner(PhpString::from_bytes(type_name.as_bytes().to_vec()))
        }
        "is_int" | "is_integer" | "is_long" => {
            let [value] = arguments else {
                return Err("is_int() expects exactly 1 argument".to_owned());
            };
            let value = context.decode_baseline_value(*value)?;
            let value = match value {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            context.encode_baseline_value(Value::Bool(matches!(value, Value::Int(_))))
        }
        "is_string" => {
            let [value] = arguments else {
                return Err("is_string() expects exactly 1 argument".to_owned());
            };
            let value = context.decode_baseline_value(*value)?;
            let value = match value {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            context.encode_baseline_value(Value::Bool(matches!(value, Value::String(_))))
        }
        "is_bool" => {
            let [value] = arguments else {
                return Err("is_bool() expects exactly 1 argument".to_owned());
            };
            let value = context.decode_baseline_value(*value)?;
            let value = match value {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            context.encode_baseline_value(Value::Bool(matches!(value, Value::Bool(_))))
        }
        "is_null" => {
            let [value] = arguments else {
                return Err("is_null() expects exactly 1 argument".to_owned());
            };
            let value = context.decode_baseline_value(*value)?;
            let value = match value {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            context.encode_baseline_value(Value::Bool(matches!(value, Value::Null)))
        }
        "is_array" => {
            let [value] = arguments else {
                return Err("is_array() expects exactly 1 argument".to_owned());
            };
            let value = context.decode_baseline_value(*value)?;
            let value = match value {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            context.encode_baseline_value(Value::Bool(matches!(value, Value::Array(_))))
        }
        "strlen" => {
            let [value] = arguments else {
                return Err("strlen() expects exactly 1 argument".to_owned());
            };
            let decoded = context.decode_baseline_value(*value)?;
            let bytes = native_string(decoded.clone()).map_err(|_| {
                format!(
                    "E_PHP_THROW:TypeError:strlen(): Argument #1 ($string) must be of type string, {} given",
                    native_value_type_name(&decoded)
                )
            })?;
            i64::try_from(bytes.len()).map_err(|_| "strlen() result overflow".to_owned())
        }
        "trim" => {
            let [value, ..] = arguments else {
                return Err("trim() expects at least 1 argument".to_owned());
            };
            let bytes = native_string(context.decode_baseline_value(*value)?)?;
            let characters = arguments
                .get(1)
                .map(|value| context.decode_baseline_value(*value))
                .transpose()?
                .map(native_string)
                .transpose()?
                .unwrap_or_else(|| b" \n\r\t\x0b\0".to_vec());
            let start = bytes
                .iter()
                .position(|byte| !characters.contains(byte))
                .unwrap_or(bytes.len());
            let end = bytes
                .iter()
                .rposition(|byte| !characters.contains(byte))
                .map_or(start, |index| index + 1);
            let trimmed = bytes[start..end].to_vec();
            context.encode_native_string_owner(PhpString::from_bytes(trimmed))
        }
        "strtoupper" => {
            let [value] = arguments else {
                return Err(
                    "E_PHP_THROW:ArgumentCountError:strtoupper() expects exactly 1 argument"
                        .to_owned(),
                );
            };
            let mut bytes = native_string(context.decode_baseline_value(*value)?).map_err(|_| {
                "E_PHP_THROW:TypeError:strtoupper(): Argument #1 ($string) must be of type string, array given"
                    .to_owned()
            })?;
            bytes.make_ascii_uppercase();
            context.encode_native_string_owner(PhpString::from_bytes(bytes))
        }
        "count" => {
            let [value, ..] = arguments else {
                return Err("count() expects an argument".to_owned());
            };
            let value = match context.decode_baseline_value(*value)? {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            if let Value::Object(object) = value {
                if object.class_name().eq_ignore_ascii_case("ArrayIterator")
                    && let Some(Value::Array(entries)) = object.get_property("__entries")
                {
                    return context.encode_baseline_value(Value::Int(entries.len() as i64));
                }
                if let Some(entries) = native_dom_collection_entries(&object) {
                    return context.encode_baseline_value(Value::Int(entries.len() as i64));
                }
                if let Some(count) = native_simple_xml_count(&object) {
                    return context.encode_baseline_value(Value::Int(count));
                }
                let function = native_method_in_hierarchy(context, &object.class_name(), "count")
                    .ok_or_else(|| {
                    "count(): argument must be of type Countable|array".to_owned()
                })?;
                let receiver = context.encode_native_object_owner(object)?;
                return Ok(invoke_native_function_with_metadata(
                    context,
                    function,
                    &[receiver],
                    None,
                )?);
            }
            let Value::Array(array) = value else {
                return Err("count(): argument must be an array".to_owned());
            };
            let recursive = arguments
                .get(1)
                .map(|mode| context.decode_baseline_value(*mode))
                .transpose()?
                .is_some_and(|mode| matches!(mode, Value::Int(1)));
            fn count_array(array: &php_runtime::api::PhpArray, recursive: bool) -> usize {
                array.iter().fold(array.len(), |count, (_, value)| {
                    if recursive {
                        match value {
                            Value::Array(nested) => count.saturating_add(count_array(nested, true)),
                            Value::Reference(reference) => match reference.get() {
                                Value::Array(nested) => {
                                    count.saturating_add(count_array(&nested, true))
                                }
                                _ => count,
                            },
                            _ => count,
                        }
                    } else {
                        count
                    }
                })
            }
            i64::try_from(count_array(&array, recursive))
                .map_err(|_| "count() result overflow".to_owned())
        }
        "var_dump" => {
            let mut output = Vec::new();
            for argument in arguments {
                let value = context.decode_baseline_value(*argument)?;
                native_var_dump_with_context(context, &value, 0, &mut output)?;
            }
            context.output.write_bytes(output);
            context.encode_baseline_value(Value::Null)
        }
        "get_class" => {
            let Some(value) = arguments.first() else {
                return Err("get_class() without an object context is unavailable".to_owned());
            };
            let value = match context.decode_baseline_value(*value)? {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            let class = match value {
                Value::Object(object) => object.display_name(),
                Value::Array(exception) => {
                    let key = php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                        b"class".to_vec(),
                    ));
                    match exception.get(&key) {
                        Some(Value::String(class)) => class.to_string_lossy(),
                        _ => return Err("get_class(): argument must be an object".to_owned()),
                    }
                }
                _ => return Err("get_class(): argument must be an object".to_owned()),
            };
            context.encode_native_string_owner(PhpString::from_bytes(class.into_bytes()))
        }
        "get_parent_class" => {
            let Some(value) = arguments.first() else {
                return context.encode_baseline_value(Value::Bool(false));
            };
            let class_name = match context.decode_baseline_value(*value)? {
                Value::Reference(reference) => match reference.get() {
                    Value::Object(object) => object.class_name(),
                    Value::String(name) => name.to_string_lossy(),
                    _ => return context.encode_baseline_value(Value::Bool(false)),
                },
                Value::Object(object) => object.class_name(),
                Value::String(name) => name.to_string_lossy(),
                _ => return context.encode_baseline_value(Value::Bool(false)),
            };
            let Some(parent) =
                native_builtin_class(context, &class_name).and_then(|class| class.parent.clone())
            else {
                return context.encode_baseline_value(Value::Bool(false));
            };
            let display = native_builtin_class(context, &parent)
                .map_or(parent, |class| class.display_name.clone());
            context.encode_native_string_owner(PhpString::from_bytes(display.into_bytes()))
        }
        "is_subclass_of" => {
            let [target, parent, rest @ ..] = arguments else {
                return Err("is_subclass_of() expects 2 or 3 arguments".to_owned());
            };
            let target_value = match context.decode_baseline_value(*target)? {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            let allow_string = rest
                .first()
                .map(|value| context.decode_baseline_value(*value))
                .transpose()?
                .is_none_or(|value| native_property_truthy(&value));
            let class_name = match target_value {
                Value::Object(object) => object.class_name(),
                Value::String(name) if allow_string => name.to_string_lossy(),
                _ => return context.encode_baseline_value(Value::Bool(false)),
            };
            let parent =
                String::from_utf8_lossy(&native_string(context.decode_baseline_value(*parent)?)?)
                    .to_ascii_lowercase();
            context.encode_baseline_value(Value::Bool(native_builtin_is_subclass_of(
                context,
                &class_name,
                &parent,
            )))
        }
        "sys_get_temp_dir" => context.encode_native_string_owner(PhpString::from_bytes(
            std::env::temp_dir().to_string_lossy().as_bytes().to_vec(),
        )),
        "chdir" => {
            let [directory] = arguments else {
                return Err("chdir() expects exactly 1 argument".to_owned());
            };
            let directory = native_string(context.decode_baseline_value(*directory)?)?;
            let directory =
                std::path::PathBuf::from(String::from_utf8_lossy(&directory).into_owned());
            let resolved = if directory.is_absolute() {
                directory
            } else {
                context.cwd.join(directory)
            };
            let resolved = resolved.canonicalize().map_err(|error| error.to_string())?;
            if !resolved.is_dir() {
                return context.encode_baseline_value(Value::Bool(false));
            }
            context.cwd = resolved;
            context.encode_baseline_value(Value::Bool(true))
        }
        "getcwd" => context.encode_native_string_owner(PhpString::from_bytes(
            context.cwd.to_string_lossy().as_bytes().to_vec(),
        )),
        "getenv" => {
            let name = arguments
                .first()
                .map(|name| context.decode_baseline_value(*name))
                .transpose()?;
            if name.as_ref().is_none_or(|name| matches!(name, Value::Null)) {
                let mut values = php_runtime::api::PhpArray::new();
                for (name, value) in context.environment.iter() {
                    values.insert(
                        php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                            name.as_bytes().to_vec(),
                        )),
                        Value::String(PhpString::from_bytes(value.as_bytes().to_vec())),
                    );
                }
                context.encode_native_array_owner(values)
            } else if let Some(name) = name {
                let name = String::from_utf8_lossy(&native_string(name)?).into_owned();
                let value = context
                    .environment
                    .iter()
                    .find(|(candidate, _)| candidate == &name)
                    .map_or(Value::Bool(false), |(_, value)| {
                        Value::String(PhpString::from_bytes(value.as_bytes().to_vec()))
                    });
                context.encode_baseline_value(value)
            } else {
                context.encode_baseline_value(Value::Bool(false))
            }
        }
        "putenv" => {
            let Some(assignment) = arguments.first() else {
                return Err("putenv() expects exactly 1 argument".to_owned());
            };
            let assignment = String::from_utf8_lossy(&native_string(
                context.decode_baseline_value(*assignment)?,
            )?)
            .into_owned();
            if assignment.is_empty() {
                return Err("E_PHP_THROW:ValueError:putenv(): Argument #1 ($assignment) must have a valid syntax".to_owned());
            }
            let (name, value) = assignment
                .split_once('=')
                .map_or((assignment.as_str(), None), |(name, value)| {
                    (name, Some(value.to_owned()))
                });
            if name.is_empty() {
                return Err("E_PHP_THROW:ValueError:putenv(): Argument #1 ($assignment) must have a valid syntax".to_owned());
            }
            let environment = Arc::make_mut(&mut context.environment);
            environment.retain(|(candidate, _)| candidate != name);
            if let Some(value) = value {
                environment.push((name.to_owned(), value));
                environment.sort();
            }
            context.encode_baseline_value(Value::Bool(true))
        }
        "php_sapi_name" => context.encode_native_string_owner(PhpString::from_bytes(
            context
                .options
                .runtime_context
                .sapi_name
                .as_bytes()
                .to_vec(),
        )),
        "php_uname" => {
            let mode = arguments
                .first()
                .map(|mode| context.decode_baseline_value(*mode))
                .transpose()?
                .map(native_string)
                .transpose()?
                .map_or(b'a', |mode| mode.first().copied().unwrap_or(b'a'))
                .to_ascii_lowercase();
            let version = php_source::reference_php_version();
            let value = match mode {
                b's' => "Phrust".to_owned(),
                b'n' => "localhost".to_owned(),
                b'r' => version.to_owned(),
                b'v' => "Stdlib".to_owned(),
                b'm' => "generic".to_owned(),
                _ => format!("Phrust localhost {version} Stdlib generic"),
            };
            context.encode_native_string_owner(PhpString::from_bytes(value.into_bytes()))
        }
        "get_current_user" => {
            context.encode_native_string_owner(PhpString::from_bytes(b"phrust".to_vec()))
        }
        "ignore_user_abort" => {
            if arguments.len() > 1 {
                return Err("ignore_user_abort() expects at most 1 argument".to_owned());
            }
            let previous = context
                .ini_registry
                .get("ignore_user_abort")
                .is_some_and(|value| value != "0" && !value.is_empty());
            if let Some(value) = arguments.first() {
                let enabled = php_runtime::api::to_bool(&context.decode_baseline_value(*value)?)?;
                context
                    .ini_registry
                    .set("ignore_user_abort", if enabled { "1" } else { "0" });
            }
            context.encode_baseline_value(Value::Int(i64::from(previous)))
        }
        "ini_set" | "set_include_path" => {
            let (name, value) = if normalized == "set_include_path" {
                let [value] = arguments else {
                    return Err("set_include_path() expects exactly 1 argument".to_owned());
                };
                (
                    "include_path".to_owned(),
                    context.decode_baseline_value(*value)?,
                )
            } else {
                let [name, value] = arguments else {
                    return Err("ini_set() expects exactly 2 arguments".to_owned());
                };
                (
                    String::from_utf8_lossy(&native_string(context.decode_baseline_value(*name)?)?)
                        .into_owned(),
                    context.decode_baseline_value(*value)?,
                )
            };
            let value = if normalized == "ini_set" {
                php_runtime::api::to_string(&value)
                    .map_err(|error| format!("ini_set(): argument #2: {error}"))?
                    .to_string_lossy()
            } else {
                String::from_utf8_lossy(&native_string(value)?).into_owned()
            };
            let previous = context.ini_registry.set(&name, &value);
            if name.eq_ignore_ascii_case("include_path") && previous.is_some() {
                context.include_path =
                    Arc::new(std::env::split_paths(std::ffi::OsStr::new(&value)).collect());
            }
            if name.eq_ignore_ascii_case("display_errors") && previous.is_some() {
                context.display_errors = context.ini_registry.get("display_errors") == Some("1");
            }
            context.encode_baseline_value(previous.map_or(Value::Bool(false), |previous| {
                Value::String(PhpString::from_bytes(previous.into_bytes()))
            }))
        }
        "ini_get" | "get_include_path" => {
            let name = if normalized == "get_include_path" {
                "include_path".to_owned()
            } else {
                let [name] = arguments else {
                    return Err("ini_get() expects exactly 1 argument".to_owned());
                };
                String::from_utf8_lossy(&native_string(context.decode_baseline_value(*name)?)?)
                    .into_owned()
            };
            context.encode_baseline_value(
                context
                    .ini_registry
                    .get(&name)
                    .map_or(Value::Bool(false), |value| {
                        Value::String(PhpString::from_bytes(value.as_bytes().to_vec()))
                    }),
            )
        }
        "get_cfg_var" => {
            let [name] = arguments else {
                return Err("get_cfg_var() expects exactly 1 argument".to_owned());
            };
            let name =
                String::from_utf8_lossy(&native_string(context.decode_baseline_value(*name)?)?)
                    .into_owned();
            context.encode_baseline_value(
                context
                    .ini_registry
                    .cfg_var(&name)
                    .map_or(Value::Bool(false), |value| {
                        Value::String(PhpString::from_bytes(value.as_bytes().to_vec()))
                    }),
            )
        }
        "ini_get_all" => {
            let extension = arguments
                .first()
                .map(|value| context.decode_baseline_value(*value))
                .transpose()?
                .and_then(|value| match value {
                    Value::Null => None,
                    value => native_string(value)
                        .ok()
                        .map(|value| String::from_utf8_lossy(&value).into_owned()),
                });
            let details = if let Some(value) = arguments.get(1) {
                let value = context.decode_baseline_value(*value)?;
                if matches!(value, Value::Null | Value::Uninitialized) {
                    emit_native_php_warning(
                        context,
                        php_runtime::api::PHP_E_DEPRECATED,
                        "ini_get_all(): Passing null to parameter #2 ($details) of type bool is deprecated",
                        source,
                    )?;
                }
                native_property_truthy(&value)
            } else {
                true
            };
            let entries = extension.as_deref().map_or_else(
                || context.ini_registry.entries(),
                |extension| context.ini_registry.entries_for_extension(extension),
            );
            let mut result = php_runtime::api::PhpArray::new();
            for entry in entries {
                let value = if details {
                    let mut detail = php_runtime::api::PhpArray::new();
                    for (name, value) in [
                        (
                            "global_value",
                            Value::String(PhpString::from_bytes(entry.global_value.into_bytes())),
                        ),
                        (
                            "local_value",
                            Value::String(PhpString::from_bytes(entry.local_value.into_bytes())),
                        ),
                        ("access", Value::Int(entry.access)),
                    ] {
                        detail.insert(
                            php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                                name.as_bytes().to_vec(),
                            )),
                            value,
                        );
                    }
                    Value::Array(detail)
                } else {
                    Value::String(PhpString::from_bytes(entry.local_value.into_bytes()))
                };
                result.insert(
                    php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                        entry.name.as_bytes().to_vec(),
                    )),
                    value,
                );
            }
            context.encode_native_array_owner(result)
        }
        "tempnam" => {
            let [directory, prefix, ..] = arguments else {
                return Err("tempnam() expects a directory and prefix".to_owned());
            };
            let directory = native_string(context.decode_baseline_value(*directory)?)?;
            let prefix = native_string(context.decode_baseline_value(*prefix)?)?;
            let directory =
                std::path::PathBuf::from(String::from_utf8_lossy(&directory).into_owned());
            let directory = if directory.is_absolute() {
                directory
            } else {
                context.cwd.join(directory)
            };
            if !context
                .options
                .runtime_context
                .filesystem
                .allows_path(&directory)
            {
                return context.encode_baseline_value(Value::Bool(false));
            }
            let prefix = String::from_utf8_lossy(&prefix);
            let mut created = None;
            for _ in 0..1_024 {
                let sequence =
                    NATIVE_TEMPNAM_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let path = directory.join(format!("{prefix}{:x}{sequence:x}", std::process::id()));
                match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                {
                    Ok(_) => {
                        created = Some(path);
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.to_string()),
                }
            }
            let path =
                created.ok_or_else(|| "tempnam() could not create a unique file".to_owned())?;
            context.encode_native_string_owner(PhpString::from_bytes(
                path.to_string_lossy().as_bytes().to_vec(),
            ))
        }
        "fopen" => {
            let [path, mode] = arguments else {
                return Err("fopen() expects exactly 2 arguments".to_owned());
            };
            let path = native_encoded_string(context, *path)?;
            let mode = native_encoded_string(context, *mode)?;
            let path_text = String::from_utf8_lossy(&path).into_owned();
            let mode = String::from_utf8_lossy(&mode);
            let resource = php_runtime::api::StreamWrapperRegistry::new()
                .open(
                    &mut context.resources,
                    &path_text,
                    &mode,
                    &context.cwd,
                    &context.options.runtime_context.filesystem,
                    &context.options.runtime_context.stdin,
                )
                .map_err(|error| error.message().to_owned())?;
            context.encode_native_resource_owner(resource)
        }
        "fwrite" => {
            let [resource, data, ..] = arguments else {
                return Err("fwrite() expects at least 2 arguments".to_owned());
            };
            let data = native_encoded_string(context, *data)?;
            if let Some(resource) = context.native_resource(*resource) {
                let written = resource
                    .write_bytes(&data)
                    .map_err(|error| format!("fwrite() failed to write stream resource: {error}"));
                let written = written?;
                match resource.metadata().uri.as_str() {
                    "php://stdout" => context.output.write_bytes(&data[..written]),
                    "php://stderr" => {
                        use std::io::Write as _;
                        std::io::stderr()
                            .lock()
                            .write_all(&data[..written])
                            .map_err(|error| format!("fwrite() failed to write stderr: {error}"))?;
                    }
                    _ => {}
                }
                return Ok(written as i64);
            }
            Err("fwrite() expects a stream resource".to_owned())
        }
        "fclose" => {
            let [resource] = arguments else {
                return Err("fclose() expects exactly 1 argument".to_owned());
            };
            if let Some(resource) = context.native_resource(*resource) {
                return context.encode_baseline_value(Value::Bool(resource.close()));
            }
            Err("fclose() expects a stream resource".to_owned())
        }
        "file_put_contents" => {
            use std::io::Write as _;
            let [path, data, rest @ ..] = arguments else {
                return Err("file_put_contents() expects a path and data".to_owned());
            };
            let path = native_string(context.decode_baseline_value(*path)?)?;
            let data = native_string(context.decode_baseline_value(*data)?)?;
            let flags = rest
                .first()
                .map(|flags| context.decode_baseline_value(*flags))
                .transpose()?
                .and_then(|flags| match flags {
                    Value::Int(flags) => Some(flags),
                    _ => None,
                })
                .unwrap_or(0);
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true);
            if flags & 8 != 0 {
                options.append(true);
            } else {
                options.truncate(true);
            }
            let mut file = options
                .open(String::from_utf8_lossy(&path).as_ref())
                .map_err(|error| error.to_string())?;
            file.write_all(&data).map_err(|error| error.to_string())?;
            i64::try_from(data.len()).map_err(|_| "file_put_contents() result overflow".to_owned())
        }
        "unlink" => {
            let [path, ..] = arguments else {
                return Err("unlink() expects a path".to_owned());
            };
            let path = native_string(context.decode_baseline_value(*path)?)?;
            std::fs::remove_file(String::from_utf8_lossy(&path).as_ref())
                .map_err(|error| error.to_string())?;
            context.encode_baseline_value(Value::Bool(true))
        }
        "call_user_func" | "forward_static_call" => execute_native_callback_builtin_control(
            context,
            normalized.as_ref(),
            arguments,
            source,
            caller_locals,
        )
        .ok_or_else(|| "call_user_func callback arm was not classified".to_owned())?
        .map_err(String::from),
        "spl_autoload_register" => {
            let Some(callback) = arguments.first() else {
                return Err("spl_autoload_register() expects a callback".to_owned());
            };
            let caller_function = caller_locals
                .map(|(function, _)| function)
                .unwrap_or_else(|| context.unit.entry.raw());
            if !native_encoded_callable_is_valid(context, *callback, false, caller_function) {
                return Err(
                    "spl_autoload_register(): Argument #1 ($callback) must be a valid callback"
                        .to_owned(),
                );
            }
            if let Some(throw_on_error) = arguments.get(1) {
                native_builtin_bool_argument(
                    context,
                    *throw_on_error,
                    context.unit.strict_types_for_span(source.span),
                )?
                .ok_or_else(|| {
                    "spl_autoload_register(): Argument #2 ($throw) must be of type bool".to_owned()
                })?;
            }
            let prepend = arguments
                .get(2)
                .map(|prepend| {
                    native_builtin_bool_argument(
                        context,
                        *prepend,
                        context.unit.strict_types_for_span(source.span),
                    )?
                    .ok_or_else(|| {
                        "spl_autoload_register(): Argument #3 ($prepend) must be of type bool"
                            .to_owned()
                    })
                })
                .transpose()?
                .unwrap_or(false);
            let callback = context.duplicate_registered_callback(*callback)?;
            let callback = NativeRegisteredAutoloadCallback {
                callable: callback,
                transient_export: context.include_child,
            };
            if prepend {
                context
                    .registered_callbacks
                    .autoload_callbacks
                    .insert(0, callback);
            } else {
                context
                    .registered_callbacks
                    .autoload_callbacks
                    .push(callback);
            }
            context.mark_roots_dirty(RootMutationReason::CallbackOrHandler);
            Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE))
        }
        "spl_autoload_unregister" => {
            let Some(callback) = arguments.first() else {
                return Err("spl_autoload_unregister() expects a callback".to_owned());
            };
            let callback = context.dereference_direct_encoding(*callback);
            let callbacks = std::mem::take(&mut context.registered_callbacks.autoload_callbacks);
            let mut retained = Vec::with_capacity(callbacks.len());
            let mut removed = Vec::new();
            for candidate in callbacks {
                if context.native_registered_callbacks_equal(candidate.callable, callback) {
                    removed.push(candidate);
                } else {
                    retained.push(candidate);
                }
            }
            context.registered_callbacks.autoload_callbacks = retained;
            let changed = !removed.is_empty();
            for callback in removed {
                context.release_if_live(callback.callable)?;
            }
            if changed {
                context.mark_roots_dirty(RootMutationReason::CallbackOrHandler);
            }
            Ok(php_jit::jit_encode_constant(if changed {
                php_jit::JIT_VALUE_TRUE
            } else {
                php_jit::JIT_VALUE_FALSE
            }))
        }
        "spl_autoload_functions" => {
            let callbacks = context.registered_callbacks.autoload_callbacks.clone();
            let mut entries: Vec<php_jit::JitNativeDirectArrayEntry> =
                Vec::with_capacity(callbacks.len());
            for (index, callback) in callbacks.into_iter().enumerate() {
                let key = context.encode_native_int(index as i64)?;
                let value = match context.duplicate_authoritative_native_value(callback.callable)? {
                    Some(value) => value,
                    None => {
                        context.release_if_live(key)?;
                        for entry in entries {
                            context.release_if_live(entry.key)?;
                            context.release_if_live(entry.value)?;
                        }
                        return Err("autoload callback crossed from baseline storage".to_owned());
                    }
                };
                entries.push(php_jit::JitNativeDirectArrayEntry { key, value });
            }
            context.publish_owned_direct_array_entries(entries)
        }
        "register_shutdown_function" => {
            let Some((callback, arguments)) = arguments.split_first() else {
                return Err("register_shutdown_function() expects a callback".to_owned());
            };
            let callback = context.duplicate_registered_callback(*callback)?;
            let mut owned_arguments = Vec::with_capacity(arguments.len());
            for argument in arguments {
                match context.duplicate_registered_callback_argument(*argument) {
                    Ok(argument) => owned_arguments.push(argument),
                    Err(error) => {
                        context.release_if_live(callback)?;
                        for argument in owned_arguments {
                            context.release_if_live(argument)?;
                        }
                        return Err(error);
                    }
                }
            }
            context.registered_callbacks.shutdown_callbacks.push(
                NativeRegisteredShutdownCallback {
                    callable: callback,
                    arguments: owned_arguments,
                    source: NativeRegisteredCallbackSource::Cold(source.clone()),
                    transient_export: context.include_child,
                },
            );
            context.mark_roots_dirty(RootMutationReason::CallbackOrHandler);
            Ok(php_jit::jit_encode_constant(u32::MAX))
        }
        "class_alias" => {
            let [original, alias, ..] = arguments else {
                return Err("class_alias() expects an original and alias".to_owned());
            };
            let original =
                String::from_utf8_lossy(&native_string(context.decode_baseline_value(*original)?)?)
                    .into_owned();
            let alias =
                String::from_utf8_lossy(&native_string(context.decode_baseline_value(*alias)?)?)
                    .into_owned();
            let normalized_original = normalize_class_name(&original);
            let normalized_alias = normalize_class_name(&alias);
            let exists = context
                .unit
                .classes
                .iter()
                .any(|class| class.name == normalized_original)
                || native_external_class_exists(context, &normalized_original);
            if !exists {
                return context.encode_baseline_value(Value::Bool(false));
            }
            context
                .class_aliases
                .insert(normalized_alias.clone(), normalized_original);
            context.dynamic_classes.insert(normalized_alias);
            context.encode_baseline_value(Value::Bool(true))
        }
        "get_object_vars" | "get_mangled_object_vars" => {
            let Some(object) = arguments.first() else {
                return Err(format!("{normalized}() expects exactly 1 argument"));
            };
            let object = match context.decode_baseline_value(*object)? {
                Value::Reference(reference) => reference.get(),
                value => value,
            };
            let Value::Object(object) = object else {
                return Err(format!(
                    "E_PHP_THROW:TypeError:{normalized}(): Argument #1 ($object) must be of type object"
                ));
            };
            let caller_class = native_builtin_caller_class(context, caller_locals);
            context.encode_native_array_owner(native_object_vars(
                context,
                &object,
                caller_class.as_deref(),
                normalized == "get_mangled_object_vars",
            ))
        }
        "get_class_methods" => {
            let Some(target) = arguments.first() else {
                return Err("get_class_methods() expects exactly 1 argument".to_owned());
            };
            let class_name = match context.decode_baseline_value(*target)? {
                Value::Reference(reference) => match reference.get() {
                    Value::Object(object) => object.class_name(),
                    Value::String(name) => name.to_string_lossy(),
                    _ => return context.encode_baseline_value(Value::Bool(false)),
                },
                Value::Object(object) => object.class_name(),
                Value::String(name) => name.to_string_lossy(),
                _ => return context.encode_baseline_value(Value::Bool(false)),
            };
            let caller_class = native_builtin_caller_class(context, caller_locals);
            let mut seen = std::collections::BTreeSet::new();
            let mut methods = php_runtime::api::PhpArray::new();
            for class in native_builtin_class_lineage(context, &class_name) {
                for method in &class.methods {
                    let visible = native_member_visible_from(
                        context,
                        method.flags.is_private,
                        method.flags.is_protected,
                        &class.name,
                        caller_class.as_deref(),
                    );
                    if visible && seen.insert(method.name.to_ascii_lowercase()) {
                        let display_name = context
                            .unit
                            .functions
                            .get(method.function.index())
                            .and_then(|function| function.name.rsplit_once("::"))
                            .map_or(method.name.as_str(), |(_, name)| name);
                        methods.append(Value::String(PhpString::from_bytes(
                            display_name.as_bytes().to_vec(),
                        )));
                    }
                }
            }
            context.encode_native_array_owner(methods)
        }
        "get_class_vars" => {
            let Some(target) = arguments.first() else {
                return Err("get_class_vars() expects exactly 1 argument".to_owned());
            };
            let class_name =
                String::from_utf8_lossy(&native_string(context.decode_baseline_value(*target)?)?)
                    .into_owned();
            let caller_class = native_builtin_caller_class(context, caller_locals);
            let mut properties = php_runtime::api::PhpArray::new();
            let lineage = native_builtin_class_lineage(context, &class_name);
            let mut seen = std::collections::BTreeSet::new();
            for is_static in [false, true] {
                for class in &lineage {
                    for property in &class.properties {
                        if property.flags.is_static != is_static
                            || !native_property_visible_from(
                                context,
                                property,
                                &class.name,
                                caller_class.as_deref(),
                            )
                            || !seen.insert(property.name.clone())
                        {
                            continue;
                        }
                        let value = property
                            .default
                            .and_then(|constant| context.unit.constants.get(constant.index()))
                            .map(ir_constant_value)
                            .transpose()?
                            .unwrap_or(Value::Null);
                        properties.insert(
                            php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                                property.name.as_bytes().to_vec(),
                            )),
                            value,
                        );
                    }
                }
            }
            context.encode_native_array_owner(properties)
        }
        "function_exists" => {
            let Some(name) = arguments.first() else {
                return Err("function_exists() expects exactly 1 argument".to_owned());
            };
            let name =
                String::from_utf8_lossy(&native_string(context.decode_baseline_value(*name)?)?)
                    .to_ascii_lowercase();
            let exists = context.function_id(&name).is_some()
                || context.external_function(&name).is_some()
                || context.visible_function_names.contains(&name)
                || native_php_function_exists(&name);
            context.encode_baseline_value(Value::Bool(exists))
        }
        "method_exists" | "property_exists" => {
            let [target, member] = arguments else {
                return Err(format!("{normalized}() expects exactly 2 arguments"));
            };
            let target = native_dereference_value(context.decode_baseline_value(*target)?);
            let (class_name, object) = match target {
                Value::Object(object) => (object.class_name(), Some(object)),
                Value::String(class) => (class.to_string_lossy(), None),
                _ => return context.encode_baseline_value(Value::Bool(false)),
            };
            let member =
                String::from_utf8_lossy(&native_string(context.decode_baseline_value(*member)?)?)
                    .into_owned();
            let exists = (normalized == "property_exists"
                && object
                    .as_ref()
                    .is_some_and(|object| object.get_property(&member).is_some()))
                || native_builtin_class_lineage(context, &class_name)
                    .into_iter()
                    .any(|class| {
                        if normalized == "method_exists" {
                            class
                                .methods
                                .iter()
                                .any(|method| method.name.eq_ignore_ascii_case(&member))
                        } else {
                            class
                                .properties
                                .iter()
                                .any(|property| property.name == member)
                        }
                    })
                || (normalized == "method_exists"
                    && php_std::ExtensionRegistry::standard_library()
                        .enabled_class(&class_name)
                        .is_some()
                    && php_std::generated::arginfo::method_metadata_in_hierarchy(
                        &class_name,
                        &member,
                    )
                    .is_some())
                || (normalized == "property_exists"
                    && php_std::ExtensionRegistry::standard_library()
                        .enabled_class(&class_name)
                        .is_some()
                    && php_std::generated::arginfo::property_metadata_in_hierarchy(
                        &class_name,
                        &member,
                    )
                    .is_some());
            context.encode_baseline_value(Value::Bool(exists))
        }
        "class_exists" | "interface_exists" | "trait_exists" | "enum_exists" => {
            let Some(name) = arguments.first() else {
                return Err(format!("{normalized}() expects a class name"));
            };
            let name =
                String::from_utf8_lossy(&native_string(context.decode_baseline_value(*name)?)?)
                    .into_owned();
            let normalized_name = normalize_class_name(&name);
            let matches_kind = |class: &php_ir::ClassEntry| match normalized.as_ref() {
                "interface_exists" => class.flags.is_interface,
                "trait_exists" => class.flags.is_trait,
                "enum_exists" => class.flags.is_enum,
                _ => !class.flags.is_interface && !class.flags.is_trait,
            };
            let matches_internal_kind = |kind: php_std::ClassKind| match normalized.as_ref() {
                "interface_exists" => kind == php_std::ClassKind::Interface,
                "trait_exists" => kind == php_std::ClassKind::Trait,
                "enum_exists" => kind == php_std::ClassKind::Enum,
                _ => matches!(kind, php_std::ClassKind::Class | php_std::ClassKind::Enum),
            };
            let mut exists = context
                .unit
                .classes
                .iter()
                .find(|class| {
                    class.name == normalized_name
                        && (!class.flags.is_conditional || context.class_is_visible(&class.name))
                })
                .is_some_and(matches_kind)
                || native_external_class_ref(context, &normalized_name)
                    .is_some_and(|(_, class)| matches_kind(class))
                || php_std::ExtensionRegistry::standard_library()
                    .enabled_class(&normalized_name)
                    .is_some_and(|class| matches_internal_kind(class.kind()));
            if normalized == "class_exists"
                && matches!(
                    normalized_name.as_str(),
                    "exception"
                        | "error"
                        | "typeerror"
                        | "valueerror"
                        | "argumentcounterror"
                        | "fibererror"
                )
            {
                exists = true;
            }
            let autoload = arguments
                .get(1)
                .map(|value| context.decode_baseline_value(*value))
                .transpose()?
                .is_none_or(|value| native_property_truthy(&value));
            if !exists && autoload && context.autoload_in_progress.insert(normalized_name.clone()) {
                let result = invoke_registered_autoload_callbacks_until(
                    context,
                    name.as_bytes(),
                    source,
                    |context| context.class_is_visible(&normalized_name),
                );
                exists = context.class_is_visible(&normalized_name);
                context.autoload_in_progress.remove(&normalized_name);
                result?;
            }
            context.encode_baseline_value(Value::Bool(exists))
        }
        "call_user_func_array" => execute_native_callback_builtin_control(
            context,
            normalized.as_ref(),
            arguments,
            source,
            caller_locals,
        )
        .ok_or_else(|| "call_user_func_array callback arm was not classified".to_owned())?
        .map_err(String::from),
        "func_num_args" => {
            let count = baseline_visible_call_arguments(context, caller_locals)?.len();
            context.encode_native_int(i64::try_from(count).unwrap_or(i64::MAX))
        }
        "debug_backtrace" => execute_native_debug_backtrace(context, arguments, source),
        "func_get_arg" => {
            let Some(index) = arguments.first() else {
                return Err("func_get_arg() expects exactly 1 argument".to_owned());
            };
            let index = native_builtin_int_argument(
                context,
                *index,
                context.unit.strict_types_for_span(source.span),
            )?
            .ok_or_else(|| "func_get_arg(): argument #1 must be of type int".to_owned())?;
            let visible = baseline_visible_call_arguments(context, caller_locals)?;
            let Some(value) = usize::try_from(index)
                .ok()
                .and_then(|index| visible.get(index))
                .copied()
            else {
                return Err(format!(
                    "E_PHP_THROW:Error:func_get_arg(): argument #{index} not passed to function"
                ));
            };
            context
                .duplicate_authoritative_native_value(value)?
                .map_or_else(|| context.duplicate_baseline_call_argument(value), Ok)
        }
        "is_callable" => {
            let Some(value) = arguments.first() else {
                return Err("is_callable() expects a value".to_owned());
            };
            let caller_function = caller_locals
                .map(|(function, _)| function)
                .unwrap_or_else(|| context.unit.entry.raw());
            let syntax_only = arguments
                .get(1)
                .map(|syntax_only| {
                    native_builtin_bool_argument(
                        context,
                        *syntax_only,
                        context.unit.strict_types_for_span(source.span),
                    )?
                    .ok_or_else(|| {
                        "is_callable(): Argument #2 ($syntax_only) must be of type bool".to_owned()
                    })
                })
                .transpose()?
                .unwrap_or(false);
            if !syntax_only
                && let Some(class) =
                    native_callable_autoload_target(context, *value, caller_function)
            {
                native_autoload_class(context, &class, source)?;
            }
            let callable =
                native_encoded_callable_is_valid(context, *value, syntax_only, caller_function);
            if let Some(target) = arguments.get(2) {
                let name = native_callable_name_bytes(context, *value)?;
                let replacement = context.encode_native_string_bytes_owner(&name)?;
                let stored = context.store_plain_native_reference_payload(*target, replacement)?;
                context.release(replacement)?;
                if !stored {
                    return Err(
                        "is_callable(): Argument #3 ($callable_name) could not be passed by reference"
                            .to_owned(),
                    );
                }
            }
            Ok(php_jit::jit_encode_constant(if callable {
                php_jit::JIT_VALUE_TRUE
            } else {
                php_jit::JIT_VALUE_FALSE
            }))
        }
        "get_defined_functions" => {
            let internal = php_extensions::BuiltinRegistry::new()
                .entries()
                .iter()
                .map(|entry| Value::String(PhpString::from_bytes(entry.name().as_bytes().to_vec())))
                .collect::<Vec<_>>();
            let user = context
                .unit
                .function_table
                .iter()
                .map(|entry| Value::String(PhpString::from_bytes(entry.name.as_bytes().to_vec())))
                .collect::<Vec<_>>();
            let mut functions = php_runtime::api::PhpArray::new();
            functions.insert(
                php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"internal".to_vec())),
                Value::Array(php_runtime::api::PhpArray::from_packed(internal)),
            );
            functions.insert(
                php_runtime::api::ArrayKey::String(PhpString::from_bytes(b"user".to_vec())),
                Value::Array(php_runtime::api::PhpArray::from_packed(user)),
            );
            context.encode_native_array_owner(functions)
        }
        "get_declared_classes" | "get_declared_interfaces" | "get_declared_traits" => {
            let names = context
                .unit
                .classes
                .iter()
                .filter(|class| match normalized.as_ref() {
                    "get_declared_interfaces" => class.flags.is_interface,
                    "get_declared_traits" => class.flags.is_trait,
                    _ => !class.flags.is_interface && !class.flags.is_trait,
                })
                .map(|class| {
                    Value::String(PhpString::from_bytes(
                        class.display_name.as_bytes().to_vec(),
                    ))
                })
                .collect::<Vec<_>>();
            context.encode_native_array_owner(php_runtime::api::PhpArray::from_packed(names))
        }
        "get_defined_constants" => {
            let categorized = arguments
                .first()
                .map(|value| context.decode_baseline_value(*value))
                .transpose()?
                .is_some_and(|value| native_property_truthy(&value));
            let mut core = php_runtime::api::PhpArray::new();
            let standard = php_std::ExtensionRegistry::standard_library().enabled_constants();
            for constant in &standard {
                if let Some(value) = constant.value() {
                    core.insert(
                        php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                            constant.name().as_bytes().to_vec(),
                        )),
                        php_std::constants::constant_to_value(value),
                    );
                }
            }
            let mut user = php_runtime::api::PhpArray::new();
            for (name, value) in context.visible_include_constants()? {
                user.insert(
                    php_runtime::api::ArrayKey::String(PhpString::from_bytes(name.into_bytes())),
                    value,
                );
            }
            if categorized {
                let mut groups = vec![("Core".to_owned(), php_runtime::api::PhpArray::new())];
                for constant in standard {
                    let Some(value) = constant.value() else {
                        continue;
                    };
                    let category = php_constant_category(constant.extension());
                    let index = groups
                        .iter()
                        .position(|(candidate, _)| candidate == category)
                        .unwrap_or_else(|| {
                            groups.push((category.to_owned(), php_runtime::api::PhpArray::new()));
                            groups.len() - 1
                        });
                    groups[index].1.insert(
                        php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                            constant.name().as_bytes().to_vec(),
                        )),
                        php_std::constants::constant_to_value(value),
                    );
                }
                let mut defined = php_runtime::api::PhpArray::new();
                for (key, value) in user.iter() {
                    let php_runtime::api::ArrayKey::String(name) = &key else {
                        continue;
                    };
                    if php_core_runtime_constant(&String::from_utf8_lossy(name.as_bytes())) {
                        groups[0].1.insert(key, value.clone());
                    } else {
                        defined.insert(key, value.clone());
                    }
                }
                if !defined.is_empty() {
                    groups.push(("user".to_owned(), defined));
                }
                let mut result = php_runtime::api::PhpArray::new();
                for (category, values) in groups {
                    if values.is_empty() {
                        continue;
                    }
                    result.insert(
                        php_runtime::api::ArrayKey::String(PhpString::from_bytes(
                            category.into_bytes(),
                        )),
                        Value::Array(values),
                    );
                }
                context.encode_native_array_owner(result)
            } else {
                for (key, value) in user.iter() {
                    core.insert(key, value.clone());
                }
                context.encode_native_array_owner(core)
            }
        }
        "extension_loaded" => {
            let name = arguments
                .first()
                .ok_or_else(|| "extension_loaded() expects exactly 1 argument".to_owned())?;
            let name = native_string(context.decode_baseline_value(*name)?)?;
            let name = String::from_utf8_lossy(&name);
            let loaded = php_std::introspection::extension_loaded(
                php_std::ExtensionRegistry::standard_library(),
                &name,
            );
            context.encode_baseline_value(Value::Bool(loaded))
        }
        "get_loaded_extensions" => {
            if arguments.len() > 1 {
                return Err("get_loaded_extensions() expects at most 1 argument".to_owned());
            }
            let zend_only = arguments
                .first()
                .map(|value| context.decode_baseline_value(*value))
                .transpose()?
                .is_some_and(|value| native_property_truthy(&value));
            let names = if zend_only {
                Vec::new()
            } else {
                php_std::ExtensionRegistry::standard_library()
                    .enabled_extension_names()
                    .into_iter()
                    .map(|name| Value::String(PhpString::from_bytes(name.as_bytes().to_vec())))
                    .collect::<Vec<_>>()
            };
            context.encode_native_array_owner(php_runtime::api::PhpArray::from_packed(names))
        }
        "error_log" => {
            if arguments.is_empty() || arguments.len() > 4 {
                return Err("error_log() expects between 1 and 4 arguments".to_owned());
            }
            let message = native_encoded_string(context, arguments[0])?;
            let message_type = arguments
                .get(1)
                .map(|value| {
                    native_builtin_int_argument(
                        context,
                        *value,
                        context.unit.strict_types_for_span(source.span),
                    )
                })
                .transpose()?
                .flatten()
                .unwrap_or(0);
            let success = match message_type {
                0 | 4 => {
                    eprintln!("{}", String::from_utf8_lossy(&message));
                    true
                }
                3 => {
                    let Some(destination) = arguments.get(2) else {
                        return context.encode_baseline_value(Value::Bool(false));
                    };
                    let destination = native_encoded_string(context, *destination)?;
                    let destination =
                        std::path::PathBuf::from(String::from_utf8_lossy(&destination).as_ref());
                    let destination = if destination.is_absolute() {
                        destination
                    } else {
                        context.cwd.join(destination)
                    };
                    if !context
                        .options
                        .runtime_context
                        .filesystem
                        .allows_path(&destination)
                    {
                        false
                    } else {
                        std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(destination)
                            .and_then(|mut file| std::io::Write::write_all(&mut file, &message))
                            .is_ok()
                    }
                }
                // Message type 1 requires a configured mail transport; type
                // 2 was removed from PHP. Neither may silently become a host
                // process or filesystem capability.
                _ => false,
            };
            context.encode_baseline_value(Value::Bool(success))
        }
        "error_reporting" => {
            let previous = context.error_reporting;
            if let Some(value) = arguments.first() {
                context.error_reporting = native_builtin_int_argument(
                    context,
                    *value,
                    context.unit.strict_types_for_span(source.span),
                )?
                .ok_or_else(|| "error_reporting() expects an int".to_owned())?;
            }
            context.encode_baseline_value(Value::Int(previous))
        }
        "error_get_last" => {
            if !arguments.is_empty() {
                return Err("error_get_last() expects exactly 0 arguments".to_owned());
            }
            context.encode_baseline_value(context.last_error_value())
        }
        "error_clear_last" => {
            if !arguments.is_empty() {
                return Err("error_clear_last() expects exactly 0 arguments".to_owned());
            }
            context.last_error = None;
            context.encode_baseline_value(Value::Null)
        }
        "set_error_handler" => {
            let Some(callback) = arguments.first() else {
                return Err("set_error_handler() expects a callback".to_owned());
            };
            let caller_function = caller_locals
                .map(|(function, _)| function)
                .unwrap_or_else(|| context.unit.entry.raw());
            if !native_encoded_callable_is_valid(context, *callback, false, caller_function) {
                return Err(
                    "set_error_handler(): Argument #1 ($callback) must be a valid callback"
                        .to_owned(),
                );
            }
            let previous = context
                .registered_callbacks
                .error_handlers
                .last()
                .map(|handler| handler.callback);
            let previous = previous
                .map(|previous| context.duplicate_registered_callback(previous))
                .transpose()?
                .unwrap_or_else(|| php_jit::jit_encode_constant(u32::MAX));
            let levels = arguments
                .get(1)
                .and_then(|levels| {
                    context.native_encoded_int(context.dereference_direct_encoding(*levels))
                })
                .unwrap_or(-1);
            let callback = match context.duplicate_registered_callback(*callback) {
                Ok(callback) => callback,
                Err(error) => {
                    context.release_if_live(previous)?;
                    return Err(error);
                }
            };
            context
                .registered_callbacks
                .error_handlers
                .push(NativeRegisteredErrorHandler { callback, levels });
            context.mark_roots_dirty(RootMutationReason::CallbackOrHandler);
            Ok(previous)
        }
        "restore_error_handler" => {
            if let Some(handler) = context.registered_callbacks.error_handlers.pop() {
                context.release_if_live(handler.callback)?;
                context.mark_roots_dirty(RootMutationReason::CallbackOrHandler);
            }
            Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE))
        }
        "set_exception_handler" => {
            let Some(callback) = arguments.first() else {
                return Err("set_exception_handler() expects a callback".to_owned());
            };
            let caller_function = caller_locals
                .map(|(function, _)| function)
                .unwrap_or_else(|| context.unit.entry.raw());
            if !native_encoded_callable_is_valid(context, *callback, false, caller_function) {
                return Err(
                    "set_exception_handler(): Argument #1 ($callback) must be a valid callback"
                        .to_owned(),
                );
            }
            let previous = context
                .registered_callbacks
                .exception_handlers
                .last()
                .copied();
            let previous = previous
                .map(|previous| context.duplicate_registered_callback(previous))
                .transpose()?
                .unwrap_or_else(|| php_jit::jit_encode_constant(u32::MAX));
            let callback = match context.duplicate_registered_callback(*callback) {
                Ok(callback) => callback,
                Err(error) => {
                    context.release_if_live(previous)?;
                    return Err(error);
                }
            };
            context
                .registered_callbacks
                .exception_handlers
                .push(callback);
            context.mark_roots_dirty(RootMutationReason::CallbackOrHandler);
            Ok(previous)
        }
        "restore_exception_handler" => {
            if let Some(handler) = context.registered_callbacks.exception_handlers.pop() {
                context.release_if_live(handler)?;
                context.mark_roots_dirty(RootMutationReason::CallbackOrHandler);
            }
            Ok(php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE))
        }
        "get_exception_handler" => context
            .registered_callbacks
            .exception_handlers
            .last()
            .copied()
            .map(|handler| context.duplicate_registered_callback(handler))
            .transpose()
            .map(|handler| handler.unwrap_or_else(|| php_jit::jit_encode_constant(u32::MAX))),
        "trigger_error" | "user_error" => {
            let Some(message) = arguments.first() else {
                return Err(format!("{normalized}() expects a message"));
            };
            let message =
                String::from_utf8_lossy(&native_string(context.decode_baseline_value(*message)?)?)
                    .into_owned();
            let level = arguments
                .get(1)
                .map(|level| context.decode_baseline_value(*level))
                .transpose()?
                .map_or(1024, |level| match level {
                    Value::Int(level) => level,
                    Value::Reference(reference) => match reference.get() {
                        Value::Int(level) => level,
                        _ => 1024,
                    },
                    _ => 1024,
                });
            if !matches!(level, 256 | 512 | 1024 | 16384) {
                return Err(format!(
                    "E_PHP_THROW:ValueError:{normalized}(): Argument #2 ($error_level) must be one of E_USER_ERROR, E_USER_WARNING, E_USER_NOTICE, or E_USER_DEPRECATED"
                ));
            }
            emit_native_php_warning(context, level, &message, source)?;
            context.encode_baseline_value(Value::Bool(true))
        }
        "settype" => {
            let [target, type_name] = arguments else {
                return Err("settype() expects exactly 2 arguments".to_owned());
            };
            if context.php_handle_is_reference(*target) != Some(true) {
                return Err("settype(): Argument #1 ($var) must be passed by reference".to_owned());
            }
            let direct_target = context.direct_reference_payload(*target).is_some();
            let materialized_target = if direct_target {
                None
            } else {
                let Value::Reference(reference) = context.decode_baseline_value(*target)? else {
                    return Err(
                        "settype(): Argument #1 ($var) must be passed by reference".to_owned()
                    );
                };
                Some(reference)
            };
            let type_name = String::from_utf8_lossy(&native_string(
                context.decode_baseline_value(*type_name)?,
            )?)
            .to_ascii_lowercase();
            let current = if direct_target {
                context.baseline_decode_dereferenced_native_value(*target)?
            } else {
                materialized_target
                    .as_ref()
                    .ok_or_else(|| "settype() materialized target is unavailable".to_owned())?
                    .get()
            };
            let replacement = match type_name.as_str() {
                "null" => Value::Null,
                "bool" | "boolean" => Value::Bool(native_property_truthy(&current)),
                "int" | "integer" => match current {
                    Value::String(value) => {
                        let classified =
                            php_runtime::experimental::numeric_string::classify_php_string(&value);
                        Value::Int(classified.value.map_or(0, |value| value.to_i64()))
                    }
                    Value::Float(value) => {
                        Value::Int(php_runtime::api::php_float_to_int(value.to_f64()))
                    }
                    Value::Bool(value) => Value::Int(i64::from(value)),
                    Value::Null | Value::Uninitialized => Value::Int(0),
                    Value::Int(value) => Value::Int(value),
                    _ => Value::Int(1),
                },
                "float" | "double" | "real" => match current {
                    Value::Float(value) => Value::Float(value),
                    Value::Int(value) => {
                        Value::Float(php_runtime::api::FloatValue::from_f64(value as f64))
                    }
                    Value::String(value) => {
                        let classified =
                            php_runtime::experimental::numeric_string::classify_php_string(&value);
                        Value::Float(php_runtime::api::FloatValue::from_f64(
                            classified.value.map_or(0.0, |value| match value {
                                php_runtime::experimental::numeric_string::NumericStringValue::Int(
                                    value,
                                ) => value as f64,
                                php_runtime::experimental::numeric_string::NumericStringValue::Float(
                                    value,
                                ) => value,
                            }),
                        ))
                    }
                    _ => Value::Float(php_runtime::api::FloatValue::from_f64(0.0)),
                },
                "string" => match current {
                    Value::Array(_) => {
                        emit_native_php_warning(context, 2, "Array to string conversion", source)?;
                        Value::String(PhpString::from_bytes(b"Array".to_vec()))
                    }
                    value => Value::String(PhpString::from_bytes(native_string(value)?)),
                },
                "array" => match current {
                    Value::Array(array) => Value::Array(array),
                    Value::Null | Value::Uninitialized => {
                        Value::Array(php_runtime::api::PhpArray::new())
                    }
                    value => Value::Array(php_runtime::api::PhpArray::from_packed(vec![value])),
                },
                "object" => match current {
                    Value::Object(object) => Value::Object(object),
                    Value::Array(array) => {
                        let object = native_metadata_object("stdClass", std::iter::empty());
                        for (key, value) in array.iter() {
                            let name = match key {
                                php_runtime::api::ArrayKey::Int(key) => key.to_string(),
                                php_runtime::api::ArrayKey::String(key) => key.to_string_lossy(),
                            };
                            object.set_property(name, value.clone());
                        }
                        Value::Object(object)
                    }
                    Value::Null | Value::Uninitialized => {
                        Value::Object(native_metadata_object("stdClass", std::iter::empty()))
                    }
                    value => {
                        let object = native_metadata_object("stdClass", std::iter::empty());
                        object.set_property("scalar", value);
                        Value::Object(object)
                    }
                },
                "resource" => {
                    return Err("E_PHP_THROW:ValueError:Cannot convert to resource type".to_owned());
                }
                _ => {
                    return Err(
                        "E_PHP_THROW:ValueError:settype(): Argument #2 ($type) must be a valid type"
                            .to_owned(),
                    );
                }
            };
            if direct_target {
                let replacement = context.encode_baseline_value(replacement)?;
                let stored = context.store_plain_native_reference_payload(*target, replacement)?;
                context.release(replacement)?;
                if !stored {
                    return Err("settype() direct reference target became unavailable".to_owned());
                }
            } else {
                context.set_native_reference_value(
                    materialized_target
                        .as_ref()
                        .ok_or_else(|| "settype() materialized target is unavailable".to_owned())?,
                    replacement,
                )?;
            }
            context.encode_baseline_value(Value::Bool(true))
        }
        "set_time_limit" => {
            let [seconds] = arguments else {
                return Err("set_time_limit() expects exactly 1 argument".to_owned());
            };
            let seconds = match context.decode_baseline_value(*seconds)? {
                Value::Int(seconds) => seconds,
                Value::Reference(reference) => match reference.get() {
                    Value::Int(seconds) => seconds,
                    _ => return Err("set_time_limit() expects an integer".to_owned()),
                },
                _ => return Err("set_time_limit() expects an integer".to_owned()),
            };
            if seconds < 0 {
                return Err(
                    "E_PHP_THROW:ValueError:set_time_limit(): Argument #1 ($seconds) must be greater than or equal to 0"
                        .to_owned(),
                );
            }
            context.reset_execution_deadline_seconds(seconds as u64);
            context.encode_baseline_value(Value::Bool(true))
        }
        _ => {
            let entry = prepared
                .map(|builtin| builtin.entry)
                .or_else(|| php_extensions::BuiltinRegistry::new().get(&normalized));
            let Some(entry) = entry else {
                return Err(format!(
                    "E_PHP_THROW:Error:Call to undefined function {name}()"
                ));
            };
            let metadata = prepared
                .and_then(|builtin| builtin.metadata)
                .or_else(|| php_std::arginfo::function_metadata_indexed(&normalized));
            let mut values = arguments
                .iter()
                .enumerate()
                .map(|(index, argument)| {
                    let value = context.decode_baseline_value(*argument)?;
                    let by_ref = metadata
                        .and_then(|function| {
                            function.params.get(index).or_else(|| {
                                function
                                    .params
                                    .last()
                                    .filter(|parameter| parameter.variadic)
                            })
                        })
                        .is_some_and(|parameter| parameter.by_ref);
                    Ok::<Value, String>(if by_ref {
                        value
                    } else if let Value::Reference(reference) = value {
                        reference.get()
                    } else {
                        value
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            if normalized == "shm_put_var" {
                prepare_native_sysvshm_serialization(context, &mut values)?;
            }
            if normalized.starts_with("session_") {
                context.materialize_native_session_state()?;
            }
            let span = php_runtime::api::RuntimeSourceSpan {
                file: context
                    .unit
                    .files
                    .get(source.span.file.index())
                    .map(|file| file.path.clone()),
                start: source.span.start,
                end: source.span.end,
            };
            let (result, diagnostics) = {
                let mut builtin = borrow_native_builtin_context!(context);
                builtin.set_diagnostic_display(php_runtime::api::PhpDiagnosticDisplayOptions {
                    // Diagnostics are synchronously routed below so native
                    // set_error_handler callbacks see builtin warnings/notices.
                    display_errors: false,
                    error_reporting: context.error_reporting,
                    leading_newline: true,
                });
                if let php_runtime::api::RuntimeRequestMode::Http(request) =
                    &context.options.runtime_context.request_mode
                {
                    builtin.set_php_input(Arc::clone(&request.raw_body));
                }
                builtin.set_filter_input_arrays_shared(Rc::clone(
                    &context.baseline_values.filter_input_arrays,
                ));
                builtin.set_http_response_state(&mut context.http_response);
                builtin.set_upload_registry(&mut context.upload_registry);
                builtin.set_session_state(
                    &mut context.session,
                    context.baseline_values.session_global.clone(),
                );
                builtin.set_session_loader(context.options.runtime_context.session_loader.as_ref());
                builtin.set_session_id_generator(
                    context
                        .options
                        .runtime_context
                        .session_id_generator
                        .as_ref(),
                );
                builtin.sync_session_state_from_global();
                let mut mysql_state = context.mysql_state.borrow_mut();
                builtin.set_mysql_state(&mut mysql_state);
                context.registered_extensions.bind(&mut builtin);
                let result = (entry.function())(&mut builtin, values, span);
                builtin.sync_session_state_from_global();
                let diagnostics = builtin.take_diagnostics();
                (result, diagnostics)
            };
            if normalized.starts_with("session_") {
                context.republish_native_session_commit()?;
                context.mark_roots_dirty(RootMutationReason::Session);
            }
            for diagnostic in diagnostics {
                let errno = match diagnostic.severity() {
                    php_runtime::api::RuntimeSeverity::Notice => php_runtime::api::PHP_E_NOTICE,
                    php_runtime::api::RuntimeSeverity::Deprecation => {
                        php_runtime::api::PHP_E_DEPRECATED
                    }
                    _ => php_runtime::api::PHP_E_WARNING,
                };
                emit_native_php_diagnostic(context, errno, diagnostic.message(), source, true)?;
            }
            match result {
                Ok(value) => context.encode_baseline_value(value),
                Err(error) => {
                    let id = error.diagnostic_id().to_ascii_uppercase();
                    let class = if id.contains("ARITY") || id.contains("ARGUMENT_COUNT") {
                        "ArgumentCountError"
                    } else if id.contains("VALUE") {
                        "ValueError"
                    } else if id.contains("TYPE") {
                        "TypeError"
                    } else {
                        "Error"
                    };
                    Err(format!("E_PHP_THROW:{class}:{}", error.message()))
                }
            }
        }
    }
}

#[cfg(test)]
fn validate_native_builtin_arity(name: &str, argument_count: usize) -> Result<(), String> {
    validate_native_builtin_arity_with_metadata(
        name,
        argument_count,
        php_std::arginfo::function_metadata_indexed(name),
    )
}

fn validate_native_builtin_arity_with_metadata(
    name: &str,
    argument_count: usize,
    function: Option<&php_std::generated::arginfo::GeneratedFunctionMetadata>,
) -> Result<(), String> {
    let Some(function) = function else {
        return Ok(());
    };
    let required = function
        .params
        .iter()
        .filter(|parameter| {
            !parameter.optional && parameter.default_value.is_none() && !parameter.variadic
        })
        .count();
    // These callback-tail APIs encode a PHP overload in a single variadic
    // stub. The callback(s) inside `...$rest` are still mandatory, so their
    // runtime minimum cannot be inferred by counting fixed parameters.
    let required = match name {
        "array_intersect_uassoc" | "array_intersect_ukey" | "array_uintersect" => 2,
        "array_uintersect_uassoc" => 3,
        _ => required,
    };
    let variadic = function
        .params
        .last()
        .is_some_and(|parameter| parameter.variadic);
    let plural = |count: usize| if count == 1 { "" } else { "s" };
    if argument_count < required {
        let expectation = if name == "strtr" {
            "exactly 2 arguments".to_owned()
        } else if !variadic && required == function.params.len() {
            format!("exactly {required} argument{}", plural(required))
        } else {
            format!("at least {required} argument{}", plural(required))
        };
        return Err(format!(
            "E_PHP_THROW:ArgumentCountError:{}() expects {expectation}, {argument_count} given",
            function.name,
        ));
    }
    if !variadic && argument_count > function.params.len() {
        let maximum = function.params.len();
        let expectation = if name == "strtr" {
            "exactly 3 arguments".to_owned()
        } else if required == maximum {
            format!("exactly {maximum} argument{}", plural(maximum))
        } else {
            format!("at most {maximum} argument{}", plural(maximum))
        };
        return Err(format!(
            "E_PHP_THROW:ArgumentCountError:{}() expects {expectation}, {argument_count} given",
            function.name,
        ));
    }
    Ok(())
}

pub(super) fn native_php_function_exists(name: &str) -> bool {
    // `print` is a language construct, while the mhash compatibility symbols
    // are conditional on a libmhash-enabled PHP build. Both have internal
    // implementation entries but are absent from the pinned PHP 8.5.7 target
    // function table.
    if matches!(
        name,
        "print"
            | "mhash"
            | "mhash_count"
            | "mhash_get_block_size"
            | "mhash_get_hash_name"
            | "mhash_keygen_s2k"
    ) {
        return false;
    }
    php_std::introspection::function_exists(php_std::ExtensionRegistry::standard_library(), name)
        || php_extensions::BuiltinRegistry::new().contains(name)
}

pub(super) fn native_internal_class_constant_exists(name: &str) -> bool {
    let Some((class_name, constant_name)) = name.rsplit_once("::") else {
        return false;
    };
    php_std::ExtensionRegistry::standard_library()
        .enabled_class(class_name)
        .is_some()
        && php_std::generated::arginfo::constant_metadata_in_hierarchy(class_name, constant_name)
            .is_some()
}

pub(super) fn native_builtin_is_unavailable_target_function(name: &str) -> bool {
    let name = name.trim_start_matches('\\');
    [
        "mhash",
        "mhash_count",
        "mhash_get_block_size",
        "mhash_get_hash_name",
        "mhash_keygen_s2k",
    ]
    .iter()
    .any(|unavailable| name.eq_ignore_ascii_case(unavailable))
}

fn validate_native_builtin_types(
    context: &mut NativeRequestColdState<'_>,
    name: &str,
    arguments: &[i64],
    source: php_ir::IrSpan,
    prepared_info: Option<Option<&php_std::arginfo::FunctionArgInfo>>,
) -> Result<(), String> {
    if matches!(
        name.to_ascii_lowercase().as_str(),
        "is_callable" | "strlen" | "count" | "sizeof"
    ) {
        // Exact native handlers validate/coerce their scalar option directly
        // and keep mixed values plus by-reference output in native storage.
        return Ok(());
    }
    if let Some(info) = prepared_info {
        return info.map_or(Ok(()), |info| {
            validate_native_builtin_types_with_info(context, info, arguments, source)
        });
    }
    let Some(metadata) = php_std::arginfo::function_metadata_indexed(name) else {
        return Ok(());
    };
    if !matches!(metadata.extension, "hash" | "json" | "pcre" | "tokenizer") {
        return Ok(());
    }
    if metadata.params.iter().any(|parameter| {
        parameter
            .type_decl
            .split('|')
            .any(|atom| atom.trim() == "callable")
    }) {
        // Runtime callable validation must accept PHP's array callback form
        // and resolve visibility; the scalar arginfo validator intentionally
        // has no class-table context for that job.
        return Ok(());
    }
    let Some(info) = php_std::arginfo::function_arginfo_indexed(name) else {
        return Ok(());
    };
    validate_native_builtin_types_with_info(context, info, arguments, source)
}

fn validate_native_builtin_types_with_info(
    context: &mut NativeRequestColdState<'_>,
    info: &php_std::arginfo::FunctionArgInfo,
    arguments: &[i64],
    source: php_ir::IrSpan,
) -> Result<(), String> {
    let values = arguments
        .iter()
        .map(|argument| {
            context
                .decode_baseline_value(*argument)
                .map(|value| match value {
                    Value::Reference(reference) => reference.get(),
                    value => value,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mode = if context.unit.strict_types_for_span(source) {
        php_std::arginfo::CoercionMode::Strict
    } else {
        php_std::arginfo::CoercionMode::Weak
    };
    let span = php_runtime::api::RuntimeSourceSpan {
        file: context
            .unit
            .files
            .get(source.file.index())
            .map(|file| file.path.clone()),
        start: source.start,
        end: source.end,
    };
    php_std::arginfo::ArgumentValidator::new(mode)
        .validate(info, &values, span)
        .map(|_| ())
        .map_err(|error| {
            let class = match error.class() {
                php_std::arginfo::ArginfoErrorClass::TypeError => "TypeError",
                php_std::arginfo::ArginfoErrorClass::ValueError => "ValueError",
            };
            format!("E_PHP_THROW:{class}:{}", error.diagnostic().message())
        })
}

#[cfg(test)]
mod arity_tests {
    use super::{
        native_builtin_is_unavailable_target_function, native_php_function_exists,
        validate_native_builtin_arity,
    };

    #[test]
    fn generated_builtin_arity_uses_php_argument_count_diagnostics() {
        assert_eq!(
            validate_native_builtin_arity("abs", 0),
            Err(
                "E_PHP_THROW:ArgumentCountError:abs() expects exactly 1 argument, 0 given"
                    .to_owned()
            )
        );
        assert_eq!(
            validate_native_builtin_arity("array_chunk", 0),
            Err(
                "E_PHP_THROW:ArgumentCountError:array_chunk() expects at least 2 arguments, 0 given"
                    .to_owned()
            )
        );
        assert!(validate_native_builtin_arity("printf", 4).is_ok());
        assert_eq!(
            validate_native_builtin_arity("array_uintersect_uassoc", 0),
            Err(
                "E_PHP_THROW:ArgumentCountError:array_uintersect_uassoc() expects at least 3 arguments, 0 given"
                    .to_owned()
            )
        );
        assert_eq!(
            validate_native_builtin_arity("strtr", 0),
            Err(
                "E_PHP_THROW:ArgumentCountError:strtr() expects exactly 2 arguments, 0 given"
                    .to_owned()
            )
        );
    }

    #[test]
    fn function_exists_uses_the_php_visible_target_surface() {
        assert!(native_php_function_exists("class_alias"));
        assert!(!native_php_function_exists("print"));
        assert!(!native_php_function_exists("mhash"));
        assert!(native_builtin_is_unavailable_target_function("\\MHASH"));
        assert!(!native_builtin_is_unavailable_target_function("hash"));
    }
}
