fn exact_query_return_bool(value: bool) -> php_jit::JitNativeControlResult {
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(if value {
        php_jit::JIT_VALUE_TRUE
    } else {
        php_jit::JIT_VALUE_FALSE
    }))
}

fn exact_query_class_name(fast: &NativeRequestFastState, encoded: i64) -> Option<String> {
    fast.native_string_view(encoded)
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
}

fn exact_query_autoload(fast: &NativeRequestFastState, encoded: i64) -> Option<bool> {
    if encoded == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        return Some(true);
    }
    fast.native_comparison_value(encoded)
        .map(native_comparison_truthy)
}

fn exact_class_kind_exists<const KIND: u8>(
    fast: &mut NativeRequestFastState,
    arguments: [i64; 3],
) -> php_jit::JitNativeControlResult {
    let Some(name) = exact_query_class_name(fast, arguments[0]) else {
        return exact_query_contract_violation();
    };
    let Some(autoload) = exact_query_autoload(fast, arguments[1]) else {
        return exact_query_contract_violation();
    };
    let callback_completed = arguments[2] != 0;
    let normalized_name = normalize_class_name(&name);
    if fast
        .complete_direct_class_autoload_callback(
            normalized_name.as_bytes(),
            callback_completed,
        )
        .is_err()
    {
        return exact_query_contract_violation();
    }
    let symbols = &fast.symbol_query;
    let matches_kind = |class: &php_ir::ClassEntry| match KIND {
        1 => class.flags.is_interface,
        2 => class.flags.is_trait,
        3 => class.flags.is_enum,
        _ => !class.flags.is_interface && !class.flags.is_trait,
    };
    let matches_internal_kind = |kind: php_std::ClassKind| match KIND {
        1 => kind == php_std::ClassKind::Interface,
        2 => kind == php_std::ClassKind::Trait,
        3 => kind == php_std::ClassKind::Enum,
        _ => matches!(kind, php_std::ClassKind::Class | php_std::ClassKind::Enum),
    };
    let mut exists = symbols.active_compiled().is_some_and(|compiled| {
        compiled
            .unit()
            .classes
            .iter()
            .find(|class| {
                class.name == normalized_name
                    && (!class.flags.is_conditional || symbols.class_is_visible(&class.name))
            })
            .is_some_and(matches_kind)
    }) || symbols
        .external_class_handle(&normalized_name)
        .is_some_and(|class| matches_kind(&class))
        || php_std::ExtensionRegistry::standard_library()
            .enabled_class(&normalized_name)
            .is_some_and(|class| matches_internal_kind(class.kind()));
    if KIND == 0
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
    if exists {
        fast.discard_direct_class_autoload_action(normalized_name.as_bytes());
        return exact_query_return_bool(true);
    }
    if !autoload {
        return exact_query_return_bool(false);
    }
    match fast.next_direct_class_autoload_callback(
        Box::<[u8]>::from(normalized_name.as_bytes()),
    ) {
        Ok(Some(callback)) => php_jit::JitNativeControlResult::control(
            php_jit::JitCallStatus::INVOKE_USER_CALLBACK,
            0,
            callback,
        ),
        Ok(None) => exact_query_return_bool(false),
        Err(_) => exact_query_runtime_error(),
    }
}

fn exact_member_exists<const METHOD: bool>(
    fast: &NativeRequestFastState,
    symbols: &NativeSymbolQueryCapability,
    arguments: [i64; 2],
) -> php_jit::JitNativeControlResult {
    let (class_name, object) = if let Some(object) = fast.native_query_object(arguments[0]) {
        (object.class_name_handle(), Some(object))
    } else if let Some(name) = exact_query_class_name(fast, arguments[0]) {
        (Arc::<str>::from(name), None)
    } else {
        return exact_query_contract_violation();
    };
    let Some(member) = exact_query_class_name(fast, arguments[1]) else {
        return exact_query_contract_violation();
    };
    let exists = (!METHOD
        && object
            .as_ref()
            .is_some_and(|object| object.has_dynamic_property(&member)))
        || symbols.class_lineage_any(&class_name, &mut |class| {
            if METHOD {
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
        || (METHOD
            && php_std::ExtensionRegistry::standard_library()
                .enabled_class(&class_name)
                .is_some()
            && php_std::generated::arginfo::method_metadata_in_hierarchy(&class_name, &member)
                .is_some())
        || (!METHOD
            && php_std::ExtensionRegistry::standard_library()
                .enabled_class(&class_name)
                .is_some()
            && php_std::generated::arginfo::property_metadata_in_hierarchy(&class_name, &member)
                .is_some());
    exact_query_return_bool(exists)
}

macro_rules! exact_symbol_query_abi {
    (
        $abi:ident,
        $fast:ident,
        $symbols:ident,
        $arguments:ident,
        $body:block
    ) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
        ) -> php_jit::JitNativeControlResult {
            debug_assert!(!runtime.is_null());
            // SAFETY: optimizing publication passes the stable request-owned
            // FastState pointer. The exact query reads only its native values
            // and the narrow live symbol capability published at activation.
            #[allow(unsafe_code)] // Safety: the generated callback preserves the request state.
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            let $fast = unsafe { &*runtime };
            let $symbols = &$fast.symbol_query;
            let $arguments = [argument_0, argument_1];
            $body
        }
    };
}

pub(crate) extern "C" fn jit_native_define_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    file: i64,
    start: i64,
) -> php_jit::JitNativeControlResult {
    debug_assert!(!runtime.is_null());
    // SAFETY: exact-handler publication passes the request-owned FastState
    // and the call remains synchronous.
    #[allow(unsafe_code)] // Safety: the generated site publishes this immutable exception plan.
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(name) = exact_query_class_name(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    if fast.symbol_query.constant_exists(&name) {
        if super::exact_runtime_ops::emit_exact_native_structured_warning(
            fast,
            "E_PHP_RUNTIME_CONSTANT_ALREADY_DEFINED_WARNING",
            format!("Constant {name} already defined"),
            file,
            start,
        ) != 0
        {
            return exact_query_runtime_error();
        }
        return exact_query_return_bool(false);
    }
    if !fast.publish_native_dynamic_constant(name, argument_1) {
        return exact_query_contract_violation();
    }
    exact_query_return_bool(true)
}

exact_symbol_query_abi!(jit_native_defined_abi, fast, symbols, arguments, {
    if let Some(name) = exact_query_class_name(fast, arguments[0]) {
        exact_query_return_bool(symbols.constant_exists(&name))
    } else {
        exact_query_contract_violation()
    }
});

pub(crate) extern "C" fn jit_native_constant_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    _argument_1: i64,
) -> php_jit::JitNativeControlResult {
    debug_assert!(!runtime.is_null());
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)] // Safety: generated code passes the active request-owned fast state.
    let fast = unsafe { &mut *runtime };
    let Some(name) = exact_query_class_name(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let source = if let Some(value) = fast
        .symbol_query
        .native_constants()
        .and_then(|constants| constants.get(&name))
        .copied()
    {
        ExactConstantInventorySource::BorrowedEncoded(value)
    } else if let Some(value) = fast
        .symbol_query
        .active_compiled()
        .and_then(|compiled| {
            compiled
                .unit()
                .constant_table
                .iter()
                .find(|constant| constant.name == name)
                .and_then(|constant| compiled.unit().constants.get(constant.value.index()))
        })
        .map(std::ptr::from_ref)
    {
        ExactConstantInventorySource::Ir(value)
    } else if let Some(value) = php_std::ExtensionRegistry::standard_library()
        .enabled_constant(&name)
        .and_then(php_std::ConstantDescriptor::value)
    {
        ExactConstantInventorySource::Standard(value)
    } else {
        // Publication excludes source-aware undefined-constant diagnostics.
        return exact_query_contract_violation();
    };
    let value = match source {
        ExactConstantInventorySource::Standard(value) => {
            publish_exact_standard_constant(fast, value)
        }
        ExactConstantInventorySource::Ir(value) => {
            // SAFETY: the IR constant belongs to the publication-stable
            // active compiled unit for this synchronous exact call.
            #[allow(unsafe_code)]
            publish_exact_resolved_ir_constant(fast, unsafe { &*value }, 0)
        }
        ExactConstantInventorySource::BorrowedEncoded(value) => {
            fast.retain_direct_encoded(value).ok().map(|()| value)
        }
    };
    value.map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

exact_symbol_query_abi!(jit_native_function_exists_abi, fast, symbols, arguments, {
    if let Some(name) = exact_query_class_name(fast, arguments[0]) {
        exact_query_return_bool(symbols.function_exists(&name))
    } else {
        exact_query_contract_violation()
    }
});
macro_rules! exact_class_query_abi {
    ($abi:ident, $kind:expr) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
            callback_completed: i64,
        ) -> php_jit::JitNativeControlResult {
            debug_assert!(!runtime.is_null());
            // SAFETY: generated code passes the request-owned fast state and
            // re-enters this leaf only after its generated callback returns.
            #[allow(unsafe_code)]
            let fast = unsafe { &mut *runtime };
            exact_class_kind_exists::<$kind>(
                fast,
                [argument_0, argument_1, callback_completed],
            )
        }
    };
}

exact_class_query_abi!(jit_native_class_exists_abi, 0);
exact_class_query_abi!(jit_native_interface_exists_abi, 1);
exact_class_query_abi!(jit_native_trait_exists_abi, 2);
exact_class_query_abi!(jit_native_enum_exists_abi, 3);
exact_symbol_query_abi!(jit_native_method_exists_abi, fast, symbols, arguments, {
    exact_member_exists::<true>(fast, symbols, arguments)
});
exact_symbol_query_abi!(jit_native_property_exists_abi, fast, symbols, arguments, {
    exact_member_exists::<false>(fast, symbols, arguments)
});

pub(crate) extern "C" fn jit_native_preg_match_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
    argument_4: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)] // Safety: the generated site publishes this immutable exception plan.
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let flags = if argument_3 != missing {
        match fast.native_printf_scalar(argument_3) {
            Some(php_runtime::api::NativePrintfScalar::Int(flags)) => flags,
            _ => return exact_query_contract_violation(),
        }
    } else {
        0
    };
    let offset = if argument_4 != missing {
        match fast.native_printf_scalar(argument_4) {
            Some(php_runtime::api::NativePrintfScalar::Int(offset)) => offset,
            _ => return exact_query_contract_violation(),
        }
    } else {
        0
    };
    if argument_2 != missing && !fast.direct_reference_accepts_native_replace(argument_2) {
        return exact_query_contract_violation();
    }
    let Some(result) = fast.native_preg_match_direct(
        argument_0,
        argument_1,
        flags,
        offset,
        argument_2 != missing,
    ) else {
        return exact_query_contract_violation();
    };
    let result = match result {
        Ok(Some(result)) => result,
        Ok(None) => return exact_query_contract_violation(),
        Err(_) => return exact_query_contract_violation(),
    };
    if argument_2 != missing {
        let Some(captures) = result.captures else {
            return exact_query_contract_violation();
        };
        if !fast.replace_direct_reference(argument_2, captures) {
            return exact_query_contract_violation();
        }
    }
    php_jit::JitNativeControlResult::returning(i64::from(result.matched))
}
pub(crate) extern "C" fn jit_native_preg_match_all_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
    argument_4: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let flags = if argument_3 != missing {
        match fast.native_printf_scalar(argument_3) {
            Some(php_runtime::api::NativePrintfScalar::Int(flags)) => flags,
            _ => return exact_query_contract_violation(),
        }
    } else {
        php_runtime::api::PREG_PATTERN_ORDER
    };
    let offset = if argument_4 != missing {
        match fast.native_printf_scalar(argument_4) {
            Some(php_runtime::api::NativePrintfScalar::Int(offset)) => offset,
            _ => return exact_query_contract_violation(),
        }
    } else {
        0
    };
    if argument_2 != missing && !fast.direct_reference_accepts_native_replace(argument_2) {
        return exact_query_contract_violation();
    }
    let Some(result) = fast.native_preg_match_all_direct(
        argument_0,
        argument_1,
        flags,
        offset,
        argument_2 != missing,
    ) else {
        return exact_query_contract_violation();
    };
    let result = match result {
        Ok(Some(result)) => result,
        Ok(None) => return exact_query_contract_violation(),
        Err(_) => return exact_query_contract_violation(),
    };
    if argument_2 != missing {
        let Some(captures) = result.captures else {
            return exact_query_contract_violation();
        };
        if !fast.replace_direct_reference(argument_2, captures) {
            return exact_query_contract_violation();
        }
    }
    php_jit::JitNativeControlResult::returning(result.count)
}

/// Prepares scalar callback replacement rows in authoritative native arrays.
/// The generated caller invokes its already prepared callback target directly.
pub(crate) extern "C" fn jit_native_preg_callback_plan_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
    effects_committed: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: the compiled ABI passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let (
        Some(php_runtime::api::NativePrintfScalar::Int(limit)),
        Some(php_runtime::api::NativePrintfScalar::Int(flags)),
    ) = (
        fast.native_printf_scalar(argument_2),
        fast.native_printf_scalar(argument_3),
    )
    else {
        return exact_query_contract_violation();
    };
    match fast.native_preg_callback_plan_direct(argument_0, argument_1, limit, flags) {
        php_runtime::api::NativePregCallbackPlanResult::Plan(plan) => {
            php_jit::JitNativeControlResult::returning(plan)
        }
        php_runtime::api::NativePregCallbackPlanResult::SemanticFailure => {
            php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX))
        }
        php_runtime::api::NativePregCallbackPlanResult::Unsupported
            if effects_committed == 0 =>
        {
            exact_query_contract_violation()
        }
        php_runtime::api::NativePregCallbackPlanResult::Unsupported => {
            exact_query_contract_violation()
        }
    }
}

/// Joins the direct string results of already executed callbacks with the
/// immutable subject spans from the native PCRE plan.
pub(crate) extern "C" fn jit_native_preg_callback_assemble_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: the compiled ABI passes the active request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    match fast.native_preg_callback_assemble_direct(argument_0, argument_1, argument_2) {
        Ok(result) => php_jit::JitNativeControlResult::returning(result),
        Err(_) => exact_query_contract_violation(),
    }
}

macro_rules! exact_native_preg_replace_abi {
    ($abi:ident, $filter:literal) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
            argument_2: i64,
            argument_3: i64,
            argument_4: i64,
        ) -> php_jit::JitNativeControlResult {
            #[allow(unsafe_code)]
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            let fast = unsafe { &mut *runtime };
            if fast.native_string_view(argument_0).is_none()
                || fast.native_string_view(argument_1).is_none()
            {
                return exact_query_contract_violation();
            }
            let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
            let limit = if argument_3 != missing {
                match fast.native_printf_scalar(argument_3) {
                    Some(php_runtime::api::NativePrintfScalar::Int(limit)) => limit,
                    _ => return exact_query_contract_violation(),
                }
            } else {
                -1
            };
            if argument_4 != missing {
                if !fast.direct_reference_accepts_native_replace(argument_4) {
                    return exact_query_contract_violation();
                }
            }
            let (value, count) = if fast.native_string_view(argument_2).is_some() {
                let Some(result) = fast
                    .native_preg_replace_scalar(argument_0, argument_1, argument_2, limit, $filter)
                else {
                    return exact_query_contract_violation();
                };
                let value = if let Some(bytes) = result.bytes {
                    match fast.publish_direct_string_bytes(&bytes) {
                        Ok(value) => value,
                        Err(_) => return exact_query_contract_violation(),
                    }
                } else {
                    php_jit::jit_encode_constant(u32::MAX)
                };
                (value, result.count)
            } else {
                let Some((value, count)) = fast.native_preg_replace_many_direct(
                    argument_0,
                    argument_1,
                    argument_2,
                    limit,
                    $filter,
                )
                else {
                    return exact_query_contract_violation();
                };
                (value, count)
            };
            if argument_4 != missing && !fast.replace_direct_reference(argument_4, count) {
                return exact_query_contract_violation();
            }
            php_jit::JitNativeControlResult::returning(value)
        }
    };
}

exact_native_preg_replace_abi!(jit_native_preg_replace_abi, false);
exact_native_preg_replace_abi!(jit_native_preg_filter_abi, true);

pub(crate) extern "C" fn jit_native_preg_split_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let limit = if argument_2 != missing {
        match fast.native_printf_scalar(argument_2) {
            Some(php_runtime::api::NativePrintfScalar::Int(limit)) => limit,
            _ => return exact_query_contract_violation(),
        }
    } else {
        -1
    };
    let flags = if argument_3 != missing {
        match fast.native_printf_scalar(argument_3) {
            Some(php_runtime::api::NativePrintfScalar::Int(flags)) => flags,
            _ => return exact_query_contract_violation(),
        }
    } else {
        0
    };
    let Some(result) = fast.native_preg_split_direct(argument_0, argument_1, limit, flags) else {
        return exact_query_contract_violation();
    };
    php_jit::JitNativeControlResult::returning(result)
}

pub(crate) extern "C" fn jit_native_preg_grep_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let flags = if argument_2 != missing {
        match fast.native_printf_scalar(argument_2) {
            Some(php_runtime::api::NativePrintfScalar::Int(flags)) => flags,
            _ => return exact_query_contract_violation(),
        }
    } else {
        0
    };
    let Some(published) = fast.native_preg_grep_direct(argument_0, argument_1, flags) else {
        return exact_query_contract_violation();
    };
    php_jit::JitNativeControlResult::returning(published)
}

pub(crate) extern "C" fn jit_native_preg_quote_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(text) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let delimiter = if argument_1 != missing {
        let Some(delimiter) = fast.native_string_view(argument_1) else {
            return exact_query_contract_violation();
        };
        delimiter.first().copied()
    } else {
        None
    };
    let quoted = php_runtime::api::preg_quote(text, delimiter);
    match fast.publish_direct_string_bytes(&quoted) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_preg_last_error_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &*runtime };
    fast.native_pcre_last_error()
        .map_or_else(exact_query_contract_violation, |(code, _)| {
            php_jit::JitNativeControlResult::returning(code)
        })
}

pub(crate) extern "C" fn jit_native_preg_last_error_msg_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &*runtime };
    let Some((_, message)) = fast.native_pcre_last_error() else {
        return exact_query_contract_violation();
    };
    let source = (message.as_ptr(), message.len());
    #[allow(unsafe_code)]
    // Safety: the request-owned PCRE state and native string arena are
    // disjoint and stable during this synchronous copy.
    match unsafe { &mut *runtime }.publish_direct_string_with(source.1, |output| {
        if source.1 != 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(source.0, output.as_mut_ptr(), source.1);
            }
        }
    }) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_json_encode_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    // SAFETY: the exact handler executes synchronously with the active
    // request's stable FastState. It consumes only native value descriptors
    // and the dedicated JSON-state capability.
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let flags = if argument_1 != missing {
        match fast.native_printf_scalar(argument_1) {
            Some(php_runtime::api::NativePrintfScalar::Int(flags)) => flags,
            _ => return exact_query_contract_violation(),
        }
    } else {
        0
    };
    if flags & !php_runtime::api::NATIVE_JSON_DIRECT_ENCODE_FLAGS != 0 {
        return exact_query_contract_violation();
    }
    let depth = if argument_2 != missing {
        match fast.native_printf_scalar(argument_2) {
            Some(php_runtime::api::NativePrintfScalar::Int(depth))
                if depth >= 0 && depth <= i64::from(i32::MAX) =>
            {
                depth as usize
            }
            _ => return exact_query_contract_violation(),
        }
    } else {
        512
    };
    let Some(output_length) = fast.native_json_output_length(argument_0, depth, flags) else {
        return exact_query_contract_violation();
    };
    let encoded = match fast.try_publish_direct_string_with(output_length, |output| {
        // SAFETY: direct string publication mutates only the disjoint byte
        // arena; the authoritative source graph remains stable for this
        // synchronous second JSON pass.
        #[allow(unsafe_code)]
        let fast = unsafe { &*runtime };
        fast.native_json_into(argument_0, depth, flags, output)
            .then_some(())
            .ok_or("native JSON changed after its length pass")
    }) {
        Ok(encoded) => encoded,
        Err(_) => return exact_query_contract_violation(),
    };
    if fast.clear_json_error().is_err() {
        let _ = fast.discard_owned_direct_value(encoded);
        return exact_query_contract_violation();
    }
    php_jit::JitNativeControlResult::returning(encoded)
}

fn exact_json_decode_error(
    fast: &mut NativeRequestFastState,
    prepared_error: u64,
    value_error: bool,
    message: &[u8],
) -> php_jit::JitNativeControlResult {
    if prepared_error == 0 {
        return exact_query_contract_violation();
    }
    // SAFETY: generated code loads this pointer from the immutable
    // per-continuation exception-plan table.
    #[allow(unsafe_code)]
    let prepared =
        unsafe { &*(prepared_error as usize as *const PreparedNativeCountThrowableSites) };
    let prepared = if value_error {
        &prepared.value_error
    } else {
        &prepared.type_error
    };
    let throwable = super::exact_runtime_ops::publish_prepared_exception(
        fast,
        prepared,
        message,
        0,
        php_jit::jit_encode_constant(u32::MAX),
    );
    if throwable.status == php_jit::JitCallStatus::RETURN {
        php_jit::JitNativeControlResult::control(
            php_jit::JitCallStatus::THROW,
            0,
            throwable.value,
        )
    } else {
        throwable
    }
}

fn exact_json_decode_failure(
    fast: &mut NativeRequestFastState,
    prepared_error: u64,
    code: i64,
    message: &[u8],
) -> php_jit::JitNativeControlResult {
    if prepared_error == 0 {
        return exact_query_contract_violation();
    }
    // SAFETY: generated code loads this pointer from the immutable
    // per-continuation exception-plan table.
    #[allow(unsafe_code)]
    let prepared =
        unsafe { &*(prepared_error as usize as *const PreparedNativeCountThrowableSites) };
    let throwable = super::exact_runtime_ops::publish_prepared_exception(
        fast,
        &prepared.json_exception,
        message,
        code,
        php_jit::jit_encode_constant(u32::MAX),
    );
    if throwable.status == php_jit::JitCallStatus::RETURN {
        php_jit::JitNativeControlResult::control(
            php_jit::JitCallStatus::THROW,
            0,
            throwable.value,
        )
    } else {
        throwable
    }
}

pub(crate) extern "C" fn jit_native_json_decode_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
    prepared_error: u64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let coerced_input = if fast.native_string_view(argument_0).is_some() {
        None
    } else if let Some(bytes) = fast.native_scalar_bytes(argument_0) {
        Some(bytes.as_bytes().to_vec())
    } else {
        let Some(actual) = fast.exact_type_name(argument_0, true) else {
            return exact_query_contract_violation();
        };
        let message = format!(
            "json_decode(): Argument #1 ($json) must be of type string, {} given",
            String::from_utf8_lossy(&actual)
        );
        return exact_json_decode_error(fast, prepared_error, false, message.as_bytes());
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let associative = if argument_1 == missing {
        false
    } else {
        match fast.native_printf_scalar(argument_1) {
            Some(php_runtime::api::NativePrintfScalar::Null) => false,
            Some(php_runtime::api::NativePrintfScalar::Bool(value)) => value,
            Some(value) => match value {
                php_runtime::api::NativePrintfScalar::Null => false,
                php_runtime::api::NativePrintfScalar::Bool(value) => value,
                php_runtime::api::NativePrintfScalar::Int(value) => value != 0,
                php_runtime::api::NativePrintfScalar::Float(value) => value != 0.0,
                php_runtime::api::NativePrintfScalar::String(value) => {
                    !value.is_empty() && value != b"0"
                }
            },
            None => {
                let Some(actual) = fast.exact_type_name(argument_1, true) else {
                    return exact_query_contract_violation();
                };
                let message = format!(
                    "json_decode(): Argument #2 ($associative) must be of type ?bool, {} given",
                    String::from_utf8_lossy(&actual)
                );
                return exact_json_decode_error(
                    fast,
                    prepared_error,
                    false,
                    message.as_bytes(),
                );
            }
        }
    };
    let depth = if argument_2 != missing {
        let Some(depth) = exact_native_weak_integer(fast, argument_2) else {
            let Some(actual) = fast.exact_type_name(argument_2, true) else {
                return exact_query_contract_violation();
            };
            let message = format!(
                "json_decode(): Argument #3 ($depth) must be of type int, {} given",
                String::from_utf8_lossy(&actual)
            );
            return exact_json_decode_error(fast, prepared_error, false, message.as_bytes());
        };
        if depth <= 0 {
            return exact_json_decode_error(
                fast,
                prepared_error,
                true,
                b"json_decode(): Argument #3 ($depth) must be greater than 0",
            );
        }
        if depth > i64::from(i32::MAX) {
            return exact_json_decode_error(
                fast,
                prepared_error,
                true,
                b"json_decode(): Argument #3 ($depth) must be less than 2147483647",
            );
        }
        depth
    } else {
        512
    };
    let flags = if argument_3 != missing {
        let Some(flags) = exact_native_weak_integer(fast, argument_3) else {
            let Some(actual) = fast.exact_type_name(argument_3, true) else {
                return exact_query_contract_violation();
            };
            let message = format!(
                "json_decode(): Argument #4 ($flags) must be of type int, {} given",
                String::from_utf8_lossy(&actual)
            );
            return exact_json_decode_error(fast, prepared_error, false, message.as_bytes());
        };
        flags
    } else {
        0
    };
    const JSON_OBJECT_AS_ARRAY: i64 = 1;
    const JSON_THROW_ON_ERROR: i64 = 1 << 22;
    let associative = associative || flags & JSON_OBJECT_AS_ARRAY != 0;
    let (input, owned_input) = if let Some(bytes) = coerced_input {
        let Ok(input) = fast.publish_direct_string_bytes(&bytes) else {
            return exact_query_contract_violation();
        };
        (input, Some(input))
    } else {
        (argument_0, None)
    };
    let previous_json_error = fast.native_json_last_error().map(|(code, _)| code);
    let result = decode_native_json_direct(fast, input, depth, associative, flags);
    if let Some(input) = owned_input {
        let _ = fast.discard_owned_direct_value(input);
    }
    let Some(result) = result else {
        return exact_query_contract_violation();
    };
    match result {
        Ok(Some(value)) => {
            let Some((code, message)) = fast.native_json_last_error() else {
                return exact_query_contract_violation();
            };
            let message = message.as_bytes().to_vec();
            if flags & JSON_THROW_ON_ERROR != 0 {
                let Some(previous_json_error) = previous_json_error else {
                    return exact_query_contract_violation();
                };
                if fast.set_json_error(previous_json_error).is_err() {
                    return exact_query_contract_violation();
                }
                if code != 0 {
                    return exact_json_decode_failure(
                        fast,
                        prepared_error,
                        code,
                        &message,
                    );
                }
            }
            php_jit::JitNativeControlResult::returning(value)
        }
        Ok(None) | Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_json_validate_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let depth = if argument_1 != missing {
        match fast.native_printf_scalar(argument_1) {
            Some(php_runtime::api::NativePrintfScalar::Int(depth)) => depth,
            _ => return exact_query_contract_violation(),
        }
    } else {
        512
    };
    let flags = if argument_2 != missing {
        match fast.native_printf_scalar(argument_2) {
            Some(php_runtime::api::NativePrintfScalar::Int(flags)) => flags,
            _ => return exact_query_contract_violation(),
        }
    } else {
        0
    };
    let Some(result) = fast.validate_native_json(argument_0, depth, flags) else {
        return exact_query_contract_violation();
    };
    match result {
        Ok(valid) => exact_query_return_bool(valid),
        Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_json_last_error_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &*runtime };
    fast.native_json_last_error()
        .map_or_else(exact_query_contract_violation, |(code, _)| {
            php_jit::JitNativeControlResult::returning(code)
        })
}

pub(crate) extern "C" fn jit_native_json_last_error_msg_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &*runtime };
    let Some((_, message)) = fast.native_json_last_error() else {
        return exact_query_contract_violation();
    };
    let source = (message.as_ptr(), message.len());
    #[allow(unsafe_code)]
    // Safety: the request-owned JSON state and native string arena are
    // disjoint and stable during this synchronous copy.
    match unsafe { &mut *runtime }.publish_direct_string_with(source.1, |output| {
        if source.1 != 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(source.0, output.as_mut_ptr(), source.1);
            }
        }
    }) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => exact_query_contract_violation(),
    }
}

#[derive(Clone, Copy)]
enum ExactNativeFormatArguments {
    Direct(*const i64, usize),
    Array(*const php_jit::JitNativeDirectArrayEntry, usize),
}

impl ExactNativeFormatArguments {
    fn len(self) -> usize {
        match self {
            Self::Direct(_, length) | Self::Array(_, length) => length,
        }
    }

    #[allow(unsafe_code)] // Safety: construction validates the synchronous source range.
    unsafe fn encoded(self, index: usize) -> Option<i64> {
        if index >= self.len() {
            return None;
        }
        match self {
            Self::Direct(values, _) => Some(unsafe { *values.add(index) }),
            Self::Array(entries, _) => Some(unsafe { (*entries.add(index)).value }),
        }
    }
}

#[allow(unsafe_code)] // Safety: all pointers are request-stable for this synchronous exact call.
unsafe fn exact_native_visit_format(
    runtime: *mut NativeRequestFastState,
    name: &'static str,
    format: *const u8,
    format_length: usize,
    arguments: ExactNativeFormatArguments,
    emit: impl FnMut(&[u8]) -> Option<()>,
) -> Option<usize> {
    let format = unsafe { std::slice::from_raw_parts(format, format_length) };
    php_runtime::api::visit_native_printf_scalars(
        name,
        format,
        arguments.len(),
        |index| {
            let encoded = unsafe { arguments.encoded(index) }?;
            unsafe { &*runtime }.native_printf_scalar(encoded)
        },
        emit,
    )
}

fn exact_native_format<const VECTOR: bool, const OUTPUT: bool>(
    runtime: *mut NativeRequestFastState,
    name: &'static str,
    arguments: &[i64],
) -> php_jit::JitNativeControlResult {
    debug_assert!((VECTOR && arguments.len() == 2) || (!VECTOR && !arguments.is_empty()));
    let (format, format_length, values) = {
        #[allow(unsafe_code)]
        // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
        let fast = unsafe { &*runtime };
        let Some(format) = fast.native_string_view(arguments[0]) else {
            return exact_query_contract_violation();
        };
        let values = if VECTOR {
            let Some(entries) = fast.native_printf_array_entries(arguments[1]) else {
                return exact_query_contract_violation();
            };
            ExactNativeFormatArguments::Array(entries.as_ptr(), entries.len())
        } else {
            ExactNativeFormatArguments::Direct(arguments[1..].as_ptr(), arguments.len() - 1)
        };
        (format.as_ptr(), format.len(), values)
    };

    // Pass one validates the complete parser/conversion plan before an output
    // effect or native arena reservation becomes visible.
    #[allow(unsafe_code)]
    let Some(output_length) = (unsafe {
        exact_native_visit_format(
            runtime,
            name,
            format,
            format_length,
            values,
            |_| Some(()),
        )
    }) else {
        return exact_query_contract_violation();
    };

    if OUTPUT {
        #[allow(unsafe_code)]
        // Safety: pass one proved that each accessor/formatter succeeds. The
        // request output pointer is stable for this synchronous second pass.
        let written = unsafe {
            exact_native_visit_format(
                runtime,
                name,
                format,
                format_length,
                values,
                |bytes| (&*runtime).write_output_slice(bytes).ok(),
            )
        };
        if written == Some(output_length) {
            php_jit::JitNativeControlResult::returning(
                i64::try_from(output_length).unwrap_or(i64::MAX),
            )
        } else {
            exact_query_contract_violation()
        }
    } else {
        #[allow(unsafe_code)]
        // Safety: the direct string arena is disjoint from every immutable
        // argument source and remains stable during the second pass.
        match unsafe { &mut *runtime }.try_publish_direct_string_with(
            output_length,
            |output| {
                let mut cursor = 0_usize;
                let written = unsafe {
                    exact_native_visit_format(
                        runtime,
                        name,
                        format,
                        format_length,
                        values,
                        |bytes| {
                            let end = cursor.checked_add(bytes.len())?;
                            output.get_mut(cursor..end)?.copy_from_slice(bytes);
                            cursor = end;
                            Some(())
                        },
                    )
                };
                (written == Some(output_length) && cursor == output.len())
                    .then_some(())
                    .ok_or("native printf output changed after its validation pass")
            },
        ) {
            Ok(value) => php_jit::JitNativeControlResult::returning(value),
            Err(_) => exact_query_contract_violation(),
        }
    }
}

macro_rules! exact_native_variadic_format_abi {
    ($abi:ident, $name:literal, $output:literal) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_count: u32,
            arguments: *const i64,
        ) -> php_jit::JitNativeControlResult {
            let Ok(count) = usize::try_from(argument_count) else {
                return exact_query_contract_violation();
            };
            debug_assert!(count >= 1);
            if count == 0 || arguments.is_null() {
                return exact_query_contract_violation();
            }
            // SAFETY: optimizing code passes a synchronous stack slice with
            // exactly `argument_count` authoritative native encodings.
            #[allow(unsafe_code)]
            // Safety: generated code keeps the exact argument slice live for this call.
            let arguments = unsafe { std::slice::from_raw_parts(arguments, count) };
            exact_native_format::<false, $output>(runtime, $name, arguments)
        }
    };
}

macro_rules! exact_native_vector_format_abi {
    ($abi:ident, $name:literal, $output:literal) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
        ) -> php_jit::JitNativeControlResult {
            exact_native_format::<true, $output>(runtime, $name, &[argument_0, argument_1])
        }
    };
}

exact_native_variadic_format_abi!(jit_native_sprintf_abi, "sprintf", false);
exact_native_variadic_format_abi!(jit_native_printf_abi, "printf", true);
exact_native_vector_format_abi!(jit_native_vsprintf_abi, "vsprintf", false);
exact_native_vector_format_abi!(jit_native_vprintf_abi, "vprintf", true);

fn exact_native_number_format_value(
    fast: &NativeRequestFastState,
    encoded: i64,
) -> Option<php_runtime::api::NativePrintfScalar<'_>> {
    match fast.native_printf_scalar(encoded)? {
        value @ (php_runtime::api::NativePrintfScalar::Int(_)
        | php_runtime::api::NativePrintfScalar::Float(_)) => Some(value),
        php_runtime::api::NativePrintfScalar::String(bytes) => {
            match php_runtime::api::native_bytes_to_number(bytes).ok()? {
                php_runtime::api::NumericValue::Int(value) => {
                    Some(php_runtime::api::NativePrintfScalar::Int(value))
                }
                php_runtime::api::NumericValue::Float(value) => {
                    Some(php_runtime::api::NativePrintfScalar::Float(value))
                }
            }
        }
        php_runtime::api::NativePrintfScalar::Null
        | php_runtime::api::NativePrintfScalar::Bool(_) => None,
    }
}

fn exact_native_number_format_separator<'a>(
    fast: &'a NativeRequestFastState,
    encoded: i64,
    default: &'static [u8],
) -> Option<&'a [u8]> {
    if encoded == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        return Some(default);
    }
    match fast.native_printf_scalar(encoded)? {
        php_runtime::api::NativePrintfScalar::Null => Some(default),
        php_runtime::api::NativePrintfScalar::String(bytes) => Some(bytes),
        php_runtime::api::NativePrintfScalar::Bool(_)
        | php_runtime::api::NativePrintfScalar::Int(_)
        | php_runtime::api::NativePrintfScalar::Float(_) => None,
    }
}

/// Exact `number_format()` over the authoritative scalar/string plane.
/// Weak separator coercions and warning-producing non-numeric shapes are
/// excluded by publication before any result is published.
pub(crate) extern "C" fn jit_native_number_format_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(number) = exact_native_number_format_value(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let decimals = if argument_1
        == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING)
    {
        0
    } else {
        let Some(decimals) = exact_native_integer(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        decimals
    };
    let Some(decimal_separator) =
        exact_native_number_format_separator(fast, argument_2, b".")
    else {
        return exact_query_contract_violation();
    };
    let Some(thousands_separator) =
        exact_native_number_format_separator(fast, argument_3, b",")
    else {
        return exact_query_contract_violation();
    };
    let Some(formatted) = php_runtime::api::native_number_format(
        number,
        decimals,
        decimal_separator,
        thousands_separator,
    ) else {
        return exact_query_contract_violation();
    };
    fast.publish_direct_string_bytes(&formatted).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

#[derive(Clone, Copy)]
struct ExactNativePackArguments {
    values: *const i64,
    length: usize,
}

impl ExactNativePackArguments {
    #[allow(unsafe_code)] // Safety: construction validates the synchronous argument range.
    unsafe fn encoded(self, index: usize) -> Option<i64> {
        (index < self.length).then(|| unsafe { *self.values.add(index) })
    }
}

#[allow(unsafe_code)] // Safety: sources are request-stable for this synchronous exact call.
unsafe fn exact_native_visit_pack(
    runtime: *mut NativeRequestFastState,
    format: *const u8,
    format_length: usize,
    arguments: ExactNativePackArguments,
    emit: impl FnMut(&[u8]) -> Option<()>,
) -> Option<usize> {
    let format = unsafe { std::slice::from_raw_parts(format, format_length) };
    php_runtime::api::visit_native_pack(
        format,
        arguments.length,
        |index| {
            let encoded = unsafe { arguments.encoded(index) }?;
            match unsafe { &*runtime }.native_printf_scalar(encoded)? {
                php_runtime::api::NativePrintfScalar::Int(value) => {
                    Some(php_runtime::api::NativePackArgument::Int(value))
                }
                php_runtime::api::NativePrintfScalar::String(value) => {
                    Some(php_runtime::api::NativePackArgument::String(value))
                }
                php_runtime::api::NativePrintfScalar::Null
                | php_runtime::api::NativePrintfScalar::Bool(_)
                | php_runtime::api::NativePrintfScalar::Float(_) => None,
            }
        },
        emit,
    )
}

/// Exact variadic `pack()` over authoritative native scalars and byte strings.
pub(crate) extern "C" fn jit_native_pack_abi(
    runtime: *mut NativeRequestFastState,
    argument_count: i32,
    arguments: *const i64,
) -> php_jit::JitNativeControlResult {
    let Ok(argument_count) = usize::try_from(argument_count) else {
        return exact_query_contract_violation();
    };
    if argument_count == 0 || arguments.is_null() {
        return exact_query_contract_violation();
    }
    let (format, format_length, values) = {
        // SAFETY: the compiled slice ABI keeps all arguments live for this call.
        #[allow(unsafe_code)]
        let encoded_format = unsafe { *arguments };
        // SAFETY: the compiled ABI passes the request-owned fast state.
        #[allow(unsafe_code)]
        let fast = unsafe { &*runtime };
        let Some(format) = fast.native_string_view(encoded_format) else {
            return exact_query_contract_violation();
        };
        // SAFETY: argument_count is nonzero and the generated slice ABI owns
        // the complete argument range for this synchronous call.
        #[allow(unsafe_code)]
        let values = unsafe { arguments.add(1) };
        (
            format.as_ptr(),
            format.len(),
            ExactNativePackArguments {
                values,
                length: argument_count - 1,
            },
        )
    };
    // Pass one validates every format code and operand before reserving a
    // visible native string.
    #[allow(unsafe_code)]
    let Some(output_length) =
        (unsafe { exact_native_visit_pack(runtime, format, format_length, values, |_| Some(())) })
    else {
        return exact_query_contract_violation();
    };
    // SAFETY: the request state and reserved output range are disjoint from
    // the stable source strings for this synchronous second pass.
    #[allow(unsafe_code)]
    match unsafe { &mut *runtime }.publish_direct_string_with(output_length, |output| {
        let mut cursor = 0usize;
        let written = unsafe {
            exact_native_visit_pack(runtime, format, format_length, values, |bytes| {
                let end = cursor.checked_add(bytes.len())?;
                output.get_mut(cursor..end)?.copy_from_slice(bytes);
                cursor = end;
                Some(())
            })
        };
        debug_assert_eq!(written, Some(output_length));
        debug_assert_eq!(cursor, output_length);
    }) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => exact_query_contract_violation(),
    }
}

fn exact_native_unpack_key_bytes<'a>(
    key: php_runtime::api::NativeUnpackKey<'a>,
) -> (Option<i64>, std::borrow::Cow<'a, [u8]>) {
    match key {
        php_runtime::api::NativeUnpackKey::Int(value) => {
            (Some(value), std::borrow::Cow::Borrowed(&[]))
        }
        php_runtime::api::NativeUnpackKey::String(value) => {
            (None, std::borrow::Cow::Borrowed(value))
        }
        php_runtime::api::NativeUnpackKey::IndexedString(prefix, index) => {
            let mut value = Vec::with_capacity(prefix.len().saturating_add(20));
            value.extend_from_slice(prefix);
            value.extend_from_slice(index.to_string().as_bytes());
            (None, std::borrow::Cow::Owned(value))
        }
    }
}

fn exact_native_unpack_value(
    fast: &mut NativeRequestFastState,
    value: php_runtime::api::NativeUnpackValue<'_>,
) -> Option<i64> {
    match value {
        php_runtime::api::NativeUnpackValue::Int(value) => fast.publish_direct_int(value).ok(),
        php_runtime::api::NativeUnpackValue::Hex { code, input, count } => fast
            .publish_direct_string_with(count, |output| {
                debug_assert!(php_runtime::api::native_unpack_hex_into(
                    code, input, count, output,
                ));
            })
            .ok(),
    }
}

/// Exact `unpack()` publishes every result directly into authoritative native
/// array/string storage. Validation completes before the unpublished writer is
/// allocated, so an unsupported format has no partial PHP-visible effect.
pub(crate) extern "C" fn jit_native_unpack_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    let (format, format_length, data, data_length, offset) = {
        // SAFETY: the compiled ABI passes the request-owned fast state.
        #[allow(unsafe_code)]
        let fast = unsafe { &*runtime };
        let (Some(format), Some(data)) = (
            fast.native_string_view(argument_0),
            fast.native_string_view(argument_1),
        ) else {
            return exact_query_contract_violation();
        };
        let offset = if argument_2
            == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING)
        {
            0
        } else {
            let Some(offset) = exact_native_integer(fast, argument_2)
                .and_then(|offset| usize::try_from(offset).ok())
            else {
                return exact_query_contract_violation();
            };
            offset
        };
        (
            format.as_ptr(),
            format.len(),
            data.as_ptr(),
            data.len(),
            offset,
        )
    };
    // SAFETY: both source ranges remain request-stable for this call.
    #[allow(unsafe_code)]
    let (format_bytes, data_bytes) = unsafe {
        (
            std::slice::from_raw_parts(format, format_length),
            std::slice::from_raw_parts(data, data_length),
        )
    };
    let Some(entry_count) =
        php_runtime::api::visit_native_unpack(format_bytes, data_bytes, offset, |_, _| Some(()))
    else {
        return exact_query_contract_violation();
    };
    // SAFETY: the compiled ABI passes the request-owned fast state.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(mut writer) = fast
        .begin_owned_direct_array(entry_count.min(4), entry_count)
        .ok()
    else {
        return exact_query_contract_violation();
    };
    let fast = fast as *mut NativeRequestFastState;
    let published =
        php_runtime::api::visit_native_unpack(format_bytes, data_bytes, offset, |key, value| {
            // SAFETY: the unpublished writer and request state are exclusive
            // to this synchronous closure.
            #[allow(unsafe_code)]
            let fast = unsafe { &mut *fast };
            let value = exact_native_unpack_value(fast, value)?;
            let (integer_key, string_key) = exact_native_unpack_key_bytes(key);
            for index in 0..writer.len() {
                let Some(entry) = writer.get(index) else {
                    let _ = fast.discard_owned_direct_value(value);
                    return None;
                };
                let matches = if let Some(integer_key) = integer_key {
                    let Some(NativeComparisonValue::Int(existing)) =
                        fast.native_comparison_value(entry.key)
                    else {
                        let _ = fast.discard_owned_direct_value(value);
                        return None;
                    };
                    existing == integer_key
                } else {
                    let Some((existing, length)) = fast.stable_native_string_range(entry.key)
                    else {
                        let _ = fast.discard_owned_direct_value(value);
                        return None;
                    };
                    // SAFETY: the unpublished key remains live in writer.
                    #[allow(unsafe_code)]
                    let existing = unsafe { std::slice::from_raw_parts(existing, length) };
                    existing == string_key.as_ref()
                };
                if matches {
                    let Some(previous) = writer.replace_owned(
                        index,
                        php_jit::JitNativeDirectArrayEntry {
                            key: entry.key,
                            value,
                        },
                    ) else {
                        let _ = fast.discard_owned_direct_value(value);
                        return None;
                    };
                    let _ = fast.discard_owned_direct_value(previous.value);
                    return Some(());
                }
            }
            let key = if let Some(key) = integer_key {
                match fast.publish_direct_int(key) {
                    Ok(key) => key,
                    Err(_) => {
                        let _ = fast.discard_owned_direct_value(value);
                        return None;
                    }
                }
            } else {
                match fast.publish_direct_string_bytes(string_key.as_ref()) {
                    Ok(key) => key,
                    Err(_) => {
                        let _ = fast.discard_owned_direct_value(value);
                        return None;
                    }
                }
            };
            if fast
                .push_owned_direct_array_entry(
                    &mut writer,
                    php_jit::JitNativeDirectArrayEntry { key, value },
                )
                .is_err()
            {
                let _ = fast.discard_owned_direct_value(value);
                let _ = fast.discard_owned_direct_value(key);
                return None;
            }
            Some(())
        });
    if published.is_none() {
        // SAFETY: writer is still unpublished and exclusively owned here.
        #[allow(unsafe_code)]
        unsafe { &mut *fast }.abort_owned_direct_array(writer);
        return exact_query_contract_violation();
    }
    // SAFETY: writer is complete and exclusively owned here.
    #[allow(unsafe_code)]
    unsafe { &mut *fast }
        .finish_owned_direct_array(writer)
        .map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        )
}

fn exact_native_boolean_flag(fast: &NativeRequestFastState, value: i64) -> Option<bool> {
    match fast.native_printf_scalar(value)? {
        php_runtime::api::NativePrintfScalar::Null => Some(false),
        php_runtime::api::NativePrintfScalar::Bool(value) => Some(value),
        php_runtime::api::NativePrintfScalar::Int(value) => Some(value != 0),
        php_runtime::api::NativePrintfScalar::Float(value) => Some(value != 0.0),
        php_runtime::api::NativePrintfScalar::String(value) => {
            Some(!value.is_empty() && value != b"0")
        }
    }
}

fn exact_native_integer(fast: &NativeRequestFastState, value: i64) -> Option<i64> {
    match fast.native_printf_scalar(value)? {
        php_runtime::api::NativePrintfScalar::Int(value) => Some(value),
        _ => None,
    }
}

fn exact_native_weak_integer(fast: &NativeRequestFastState, value: i64) -> Option<i64> {
    match fast.native_printf_scalar(value)? {
        php_runtime::api::NativePrintfScalar::Null
        | php_runtime::api::NativePrintfScalar::Bool(false) => Some(0),
        php_runtime::api::NativePrintfScalar::Bool(true) => Some(1),
        php_runtime::api::NativePrintfScalar::Int(value) => Some(value),
        php_runtime::api::NativePrintfScalar::Float(value) => {
            Some(php_runtime::api::php_float_to_int(value))
        }
        php_runtime::api::NativePrintfScalar::String(value) => {
            php_runtime::api::native_bytes_to_weak_int_parameter(value)
        }
    }
}

fn exact_native_intval_value(value: NativeComparisonValue<'_>, base: i64) -> Option<i64> {
    match value {
        NativeComparisonValue::Null => Some(0),
        NativeComparisonValue::Bool(value) => Some(i64::from(value)),
        NativeComparisonValue::Int(value) => Some(value),
        NativeComparisonValue::Float(value) => Some(php_runtime::api::php_float_to_int(value)),
        NativeComparisonValue::String(value) if base != 10 => Some(
            php_runtime::api::native_parse_intval_string_base(value, base),
        ),
        NativeComparisonValue::String(value) => Some(
            php_runtime::api::native_bytes_to_number(value).map_or(0, |number| match number {
                php_runtime::api::NumericValue::Int(value) => value,
                php_runtime::api::NumericValue::Float(value) => value as i64,
            }),
        ),
        NativeComparisonValue::Array { entries, .. } => Some(i64::from(!entries.is_empty())),
        NativeComparisonValue::Resource(value) => i64::try_from(value).ok(),
        NativeComparisonValue::Object(_) | NativeComparisonValue::OpaqueIdentity(_) => None,
    }
}

/// Exact two-argument `intval` over authoritative native values. The builtin
/// identity and arity are fixed by the imported symbol; unsupported object or
/// callable semantics are rejected before optimizer entry.
pub(crate) extern "C" fn jit_native_intval_base_abi(
    runtime: *mut NativeRequestFastState,
    source: i64,
    base: i64,
) -> php_jit::JitNativeControlResult {
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    #[allow(unsafe_code)]
    let fast = unsafe { &mut *runtime };
    let Some(base) = exact_native_integer(fast, base) else {
        return exact_query_contract_violation();
    };
    let Some(source) = fast.native_comparison_value(source) else {
        return exact_query_contract_violation();
    };
    let Some(value) = exact_native_intval_value(source, base) else {
        return exact_query_contract_violation();
    };
    fast.publish_direct_int(value).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

macro_rules! exact_native_fixed_digest_abi {
    ($abi:ident, $output_length:path, $digest_into:path) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
        ) -> php_jit::JitNativeControlResult {
            #[allow(unsafe_code)]
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            let fast = unsafe { &mut *runtime };
            let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
            let raw = if argument_1 != missing {
                let Some(raw) = exact_native_boolean_flag(fast, argument_1) else {
                    return exact_query_contract_violation();
                };
                raw
            } else {
                false
            };
            fast.publish_direct_string_transform(
                argument_0,
                |_| Some($output_length(raw)),
                |input, output| $digest_into(input, raw, output),
            )
            .map_or_else(
                exact_query_contract_violation,
                php_jit::JitNativeControlResult::returning,
            )
        }
    };
}

exact_native_fixed_digest_abi!(
    jit_native_md5_abi,
    php_runtime::api::native_md5_output_length,
    php_runtime::api::native_md5_into
);
exact_native_fixed_digest_abi!(
    jit_native_sha1_abi,
    php_runtime::api::native_sha1_output_length,
    php_runtime::api::native_sha1_into
);

pub(crate) extern "C" fn jit_native_crc32_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &*runtime };
    let Some(input) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    php_jit::JitNativeControlResult::returning(php_runtime::api::native_crc32(input))
}

pub(crate) extern "C" fn jit_native_hash_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
) -> php_jit::JitNativeControlResult {
    // Hash options carry algorithm-specific arrays and remain outside
    // optimizer admission until that representation is published as metadata.
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    if argument_3 != missing {
        return exact_query_contract_violation();
    }
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let binary = if argument_2 != missing {
        let Some(binary) = exact_native_boolean_flag(fast, argument_2) else {
            return exact_query_contract_violation();
        };
        binary
    } else {
        false
    };
    fast.publish_direct_string_transform2(
        argument_0,
        argument_1,
        |algorithm, _| php_runtime::api::native_hash_output_length(algorithm, binary),
        |algorithm, input, output| {
            php_runtime::api::native_hash_into(algorithm, input, binary, output)
        },
    )
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_hash_hmac_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let binary = if argument_3 != missing {
        let Some(binary) = exact_native_boolean_flag(fast, argument_3) else {
            return exact_query_contract_violation();
        };
        binary
    } else {
        false
    };
    fast.publish_direct_string_transform3(
        argument_0,
        argument_1,
        argument_2,
        |algorithm, _, _| php_runtime::api::native_hash_hmac_output_length(algorithm, binary),
        |algorithm, input, key, output| {
            php_runtime::api::native_hash_hmac_into(algorithm, input, key, binary, output)
        },
    )
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_hash_equals_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &*runtime };
    let (Some(known), Some(user)) = (
        fast.native_string_view(argument_0),
        fast.native_string_view(argument_1),
    ) else {
        return exact_query_contract_violation();
    };
    if known.len() != user.len() {
        return exact_query_return_bool(false);
    }
    let difference = known
        .iter()
        .zip(user)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        });
    exact_query_return_bool(difference == 0)
}

pub(crate) extern "C" fn jit_native_sodium_crypto_generichash_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let length = if argument_2 == missing {
        32
    } else {
        let Some(length) = exact_native_integer(fast, argument_2) else {
            return exact_query_contract_violation();
        };
        length
    };
    let Ok(length) = usize::try_from(length) else {
        return exact_query_contract_violation();
    };
    if !(16..=64).contains(&length) {
        return exact_query_contract_violation();
    }
    let output = {
        let Some(message) = fast.native_string_view(argument_0) else {
            return exact_query_contract_violation();
        };
        let key = if argument_1 == missing {
            &[][..]
        } else {
            let Some(key) = fast.native_string_view(argument_1) else {
                return exact_query_contract_violation();
            };
            key
        };
        if !key.is_empty() && !(16..=64).contains(&key.len()) {
            return exact_query_contract_violation();
        }
        php_runtime::api::native_sodium_crypto_generichash(message, key, length)
    };
    fast.publish_direct_string_bytes(&output).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_sodium_bin2base64_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(variant) = exact_native_integer(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    let output = {
        let Some(input) = fast.native_string_view(argument_0) else {
            return exact_query_contract_violation();
        };
        let Some(output) = php_runtime::api::native_sodium_bin2base64(input, variant) else {
            return exact_query_contract_violation();
        };
        output
    };
    fast.publish_direct_string_bytes(&output).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

macro_rules! exact_native_direct_byte_transform_abi {
    ($abi:ident, $output_length:expr, $transform:expr) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
        ) -> php_jit::JitNativeControlResult {
            #[allow(unsafe_code)]
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            let fast = unsafe { &mut *runtime };
            fast.publish_direct_string_transform(argument_0, $output_length, $transform)
                .map_or_else(
                    exact_query_contract_violation,
                    php_jit::JitNativeControlResult::returning,
                )
        }
    };
}

exact_native_direct_byte_transform_abi!(
    jit_native_base64_encode_abi,
    php_runtime::api::native_base64_encode_output_length,
    php_runtime::api::native_base64_encode_into
);
exact_native_direct_byte_transform_abi!(
    jit_native_bin2hex_abi,
    php_runtime::api::native_bin2hex_output_length,
    php_runtime::api::native_bin2hex_into
);
exact_native_direct_byte_transform_abi!(
    jit_native_hex2bin_abi,
    php_runtime::api::native_hex2bin_output_length,
    php_runtime::api::native_hex2bin_into
);
exact_native_direct_byte_transform_abi!(
    jit_native_quoted_printable_decode_abi,
    |input| Some(php_runtime::api::native_quoted_printable_decode_output_length(input)),
    php_runtime::api::native_quoted_printable_decode_into
);
exact_native_direct_byte_transform_abi!(
    jit_native_urlencode_abi,
    |input| php_runtime::api::native_url_encode_output_length(input, false),
    |input, output| php_runtime::api::native_url_encode_into(input, false, output)
);
exact_native_direct_byte_transform_abi!(
    jit_native_rawurlencode_abi,
    |input| php_runtime::api::native_url_encode_output_length(input, true),
    |input, output| php_runtime::api::native_url_encode_into(input, true, output)
);
exact_native_direct_byte_transform_abi!(
    jit_native_urldecode_abi,
    |input| Some(php_runtime::api::native_url_decode_output_length(input, false)),
    |input, output| php_runtime::api::native_url_decode_into(input, false, output)
);
exact_native_direct_byte_transform_abi!(
    jit_native_rawurldecode_abi,
    |input| Some(php_runtime::api::native_url_decode_output_length(input, true)),
    |input, output| php_runtime::api::native_url_decode_into(input, true, output)
);
exact_native_direct_byte_transform_abi!(
    jit_native_convert_uuencode_abi,
    php_runtime::api::native_convert_uuencode_output_length,
    php_runtime::api::native_convert_uuencode_into
);
exact_native_direct_byte_transform_abi!(
    jit_native_convert_uudecode_abi,
    php_runtime::api::native_convert_uudecode_output_length,
    php_runtime::api::native_convert_uudecode_into
);
exact_native_direct_byte_transform_abi!(
    jit_native_stripcslashes_abi,
    |input| Some(php_runtime::api::native_stripcslashes_output_length(input)),
    php_runtime::api::native_stripcslashes_into
);
exact_native_direct_byte_transform_abi!(
    jit_native_stripslashes_abi,
    |input| Some(php_runtime::api::native_stripslashes_output_length(input)),
    php_runtime::api::native_stripslashes_into
);
exact_native_direct_byte_transform_abi!(
    jit_native_quotemeta_abi,
    php_runtime::api::native_quotemeta_output_length,
    php_runtime::api::native_quotemeta_into
);

pub(crate) extern "C" fn jit_native_addcslashes_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    fast.publish_direct_string_transform2(
        argument_0,
        argument_1,
        php_runtime::api::native_addcslashes_output_length,
        php_runtime::api::native_addcslashes_into,
    )
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

fn exact_native_optional_string(
    fast: &mut NativeRequestFastState,
    output: Option<(*const u8, usize)>,
) -> php_jit::JitNativeControlResult {
    output.map_or_else(
        || exact_query_return_bool(false),
        |(source, length)| {
            fast.publish_direct_string_with(length, |output| {
                if length == 0 {
                    return;
                }
                // SAFETY: the source is a request-stable subrange of an
                // authoritative input string and the destination belongs to
                // the disjoint native string reservation.
                #[allow(unsafe_code)]
                unsafe {
                    std::ptr::copy_nonoverlapping(source, output.as_mut_ptr(), length);
                }
            })
            .map_or_else(
                |_| exact_query_contract_violation(),
                php_jit::JitNativeControlResult::returning,
            )
        },
    )
}

macro_rules! exact_native_string_search_slice_abi {
    ($abi:ident, $case_insensitive:literal) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
            argument_2: i64,
        ) -> php_jit::JitNativeControlResult {
            #[allow(unsafe_code)]
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            let fast = unsafe { &mut *runtime };
            let before_needle = if argument_2
                != php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING)
            {
                let Some(before_needle) = exact_native_boolean_flag(fast, argument_2) else {
                    return exact_query_contract_violation();
                };
                before_needle
            } else {
                false
            };
            let output = {
                let (Some(haystack), Some(needle)) = (
                    fast.native_string_view(argument_0),
                    fast.native_string_view(argument_1),
                ) else {
                    return exact_query_contract_violation();
                };
                php_runtime::api::native_string_search_slice(
                    haystack,
                    needle,
                    $case_insensitive,
                    before_needle,
                )
                .map(|bytes| (bytes.as_ptr(), bytes.len()))
            };
            exact_native_optional_string(fast, output)
        }
    };
}

exact_native_string_search_slice_abi!(jit_native_strstr_abi, false);
exact_native_string_search_slice_abi!(jit_native_stristr_abi, true);

pub(crate) extern "C" fn jit_native_strrchr_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let before_needle =
        if argument_2 != php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
            let Some(before_needle) = exact_native_boolean_flag(fast, argument_2) else {
                return exact_query_contract_violation();
            };
            before_needle
        } else {
            false
        };
    let output = {
        let (Some(haystack), Some(needle)) = (
            fast.native_string_view(argument_0),
            fast.native_string_view(argument_1),
        ) else {
            return exact_query_contract_violation();
        };
        let needle = needle.first().copied().unwrap_or(0);
        php_runtime::api::native_strrchr(haystack, needle, before_needle)
            .map(|bytes| (bytes.as_ptr(), bytes.len()))
    };
    exact_native_optional_string(fast, output)
}

pub(crate) extern "C" fn jit_native_strpbrk_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let (Some(haystack), Some(characters)) = (
        fast.native_string_view(argument_0),
        fast.native_string_view(argument_1),
    ) else {
        return exact_query_contract_violation();
    };
    if characters.is_empty() {
        return exact_query_contract_violation();
    }
    let output = php_runtime::api::native_strpbrk(haystack, characters)
        .map(|bytes| (bytes.as_ptr(), bytes.len()));
    exact_native_optional_string(fast, output)
}

pub(crate) extern "C" fn jit_native_substr_compare_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
    argument_4: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(offset) = exact_native_integer(fast, argument_2) else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let length = if argument_3 != missing && argument_3 != php_jit::jit_encode_constant(u32::MAX) {
        let Some(length) = exact_native_integer(fast, argument_3) else {
            return exact_query_contract_violation();
        };
        let Ok(length) = usize::try_from(length) else {
            return exact_query_contract_violation();
        };
        Some(length)
    } else {
        None
    };
    let case_insensitive = if argument_4 != missing {
        let Some(case_insensitive) = exact_native_boolean_flag(fast, argument_4) else {
            return exact_query_contract_violation();
        };
        case_insensitive
    } else {
        false
    };
    let output = {
        let (Some(main), Some(other)) = (
            fast.native_string_view(argument_0),
            fast.native_string_view(argument_1),
        ) else {
            return exact_query_contract_violation();
        };
        php_runtime::api::native_substr_compare(main, other, offset, length, case_insensitive)
    };
    output.map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

macro_rules! exact_native_natural_compare_abi {
    ($abi:ident, $case_insensitive:literal) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
        ) -> php_jit::JitNativeControlResult {
            #[allow(unsafe_code)]
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            let fast = unsafe { &*runtime };
            let (Some(left), Some(right)) = (
                fast.native_string_view(argument_0),
                fast.native_string_view(argument_1),
            ) else {
                return exact_query_contract_violation();
            };
            php_jit::JitNativeControlResult::returning(php_runtime::api::native_natural_compare(
                left,
                right,
                $case_insensitive,
            ))
        }
    };
}

exact_native_natural_compare_abi!(jit_native_strnatcmp_abi, false);
exact_native_natural_compare_abi!(jit_native_strnatcasecmp_abi, true);

pub(crate) extern "C" fn jit_native_ucwords_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    if argument_1 == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        return fast
            .publish_direct_string_transform(
                argument_0,
                |input| Some(input.len()),
                |input, output| php_runtime::api::native_ucwords_into(input, None, output),
            )
            .map_or_else(
                exact_query_contract_violation,
                php_jit::JitNativeControlResult::returning,
            );
    }
    fast.publish_direct_string_transform2(
        argument_0,
        argument_1,
        |input, _| Some(input.len()),
        |input, delimiters, output| {
            php_runtime::api::native_ucwords_into(input, Some(delimiters), output)
        },
    )
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_str_pad_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(length) = exact_native_integer(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    let Ok(target) = usize::try_from(length) else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let pad_type = if argument_3 != missing {
        let Some(pad_type) = exact_native_integer(fast, argument_3) else {
            return exact_query_contract_violation();
        };
        pad_type
    } else {
        1
    };
    if argument_2 == missing {
        return fast
            .publish_direct_string_transform(
                argument_0,
                |input| php_runtime::api::native_str_pad_output_length(input, target, b" "),
                |input, output| {
                    php_runtime::api::native_str_pad_into(input, target, b" ", pad_type, output)
                },
            )
            .map_or_else(
                exact_query_contract_violation,
                php_jit::JitNativeControlResult::returning,
            );
    }
    fast.publish_direct_string_transform2(
        argument_0,
        argument_2,
        |input, pad| php_runtime::api::native_str_pad_output_length(input, target, pad),
        |input, pad, output| {
            php_runtime::api::native_str_pad_into(input, target, pad, pad_type, output)
        },
    )
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_strtr_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    if argument_2 == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        return fast
            .publish_direct_strtr_map(argument_0, argument_1)
            .map_or_else(
                exact_query_contract_violation,
                php_jit::JitNativeControlResult::returning,
            );
    }
    fast.publish_direct_string_transform3(
        argument_0,
        argument_1,
        argument_2,
        |subject, _, _| Some(subject.len()),
        php_runtime::api::native_strtr_into,
    )
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_strip_tags_abi(
    _runtime: *mut NativeRequestFastState,
    _argument_0: i64,
    _argument_1: i64,
) -> php_jit::JitNativeControlResult {
    exact_query_contract_violation()
}

pub(crate) extern "C" fn jit_native_substr_replace_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(offset) = exact_native_integer(fast, argument_2) else {
        return exact_query_contract_violation();
    };
    let length = if argument_3 != php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING)
        && argument_3 != php_jit::jit_encode_constant(u32::MAX)
    {
        let Some(length) = exact_native_integer(fast, argument_3) else {
            return exact_query_contract_violation();
        };
        Some(length)
    } else {
        None
    };
    fast.publish_direct_string_transform2(
        argument_0,
        argument_1,
        |subject, replacement| {
            php_runtime::api::native_substr_replace_output_length(
                subject,
                replacement,
                offset,
                length,
            )
        },
        |subject, replacement, output| {
            php_runtime::api::native_substr_replace_into(
                subject,
                replacement,
                offset,
                length,
                output,
            )
        },
    )
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_str_split_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let length = if argument_1 != php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING)
    {
        let Some(length) = exact_native_integer(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        let Ok(length) = usize::try_from(length) else {
            return exact_query_contract_violation();
        };
        if length == 0 {
            return exact_query_contract_violation();
        }
        length
    } else {
        1
    };
    let Some(input) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let input_bytes = input.as_ptr();
    let input_length = input.len();
    let chunk_count = input_length.div_ceil(length);
    fast.publish_owned_direct_array_with(chunk_count, |fast, index| {
        let start = index * length;
        let chunk_length = length.min(input_length - start);
        // SAFETY: direct strings and trusted constants have request-stable
        // backing ranges. `start` and `chunk_length` partition that range.
        #[allow(unsafe_code)]
        let chunk = unsafe { std::slice::from_raw_parts(input_bytes.add(start), chunk_length) };
        let value = fast
            .publish_direct_string_bytes(chunk)
            .map_err(|_| "native str_split chunk publication failed")?;
        Ok(php_jit::JitNativeDirectArrayEntry {
            key: i64::try_from(index).unwrap_or(i64::MAX),
            value,
        })
    })
    .ok()
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_version_compare_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &*runtime };
    let (Some(left), Some(right)) = (
        fast.native_string_view(argument_0),
        fast.native_string_view(argument_1),
    ) else {
        return exact_query_contract_violation();
    };
    let comparison = php_runtime::api::native_version_compare(left, right);
    if argument_2 == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        return php_jit::JitNativeControlResult::returning(comparison);
    }
    let Some(operator) = fast.native_string_view(argument_2) else {
        return exact_query_contract_violation();
    };
    php_runtime::api::native_version_operator_matches(operator, comparison)
        .map_or_else(exact_query_contract_violation, exact_query_return_bool)
}

pub(crate) extern "C" fn jit_native_array_sum_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(entries) = fast.native_direct_array_entries(argument_0) else {
        return exact_query_contract_violation();
    };
    let mut integer_total = 0_i64;
    let mut float_total = 0.0_f64;
    let mut use_float = false;
    for entry in entries {
        let number = match fast.native_printf_scalar(entry.value) {
            Some(php_runtime::api::NativePrintfScalar::Null) => {
                php_runtime::api::NumericValue::Int(0)
            }
            Some(php_runtime::api::NativePrintfScalar::Bool(value)) => {
                php_runtime::api::NumericValue::Int(i64::from(value))
            }
            Some(php_runtime::api::NativePrintfScalar::Int(value)) => {
                php_runtime::api::NumericValue::Int(value)
            }
            Some(php_runtime::api::NativePrintfScalar::Float(value)) => {
                php_runtime::api::NumericValue::Float(value)
            }
            Some(php_runtime::api::NativePrintfScalar::String(value)) => {
                let Ok(number) = php_runtime::api::native_bytes_to_number(value) else {
                    return exact_query_contract_violation();
                };
                number
            }
            None => return exact_query_contract_violation(),
        };
        match number {
            php_runtime::api::NumericValue::Int(value) if !use_float => {
                if let Some(total) = integer_total.checked_add(value) {
                    integer_total = total;
                } else {
                    use_float = true;
                    float_total = integer_total as f64 + value as f64;
                }
            }
            php_runtime::api::NumericValue::Int(value) => {
                float_total += value as f64;
            }
            php_runtime::api::NumericValue::Float(value) if !use_float => {
                use_float = true;
                float_total = integer_total as f64 + value;
            }
            php_runtime::api::NumericValue::Float(value) => {
                float_total += value;
            }
        }
    }
    let result = if use_float {
        fast.publish_direct_float(float_total)
    } else {
        fast.publish_direct_int(integer_total)
    };
    result.map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_random_bytes_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(length) =
        exact_native_integer(fast, argument_0).and_then(|value| usize::try_from(value).ok())
    else {
        return exact_query_contract_violation();
    };
    if length == 0 || length > php_jit::JIT_NATIVE_DIRECT_STRING_BYTE_CAPACITY {
        return exact_query_contract_violation();
    }
    let Some(fill_random) = fast.random.fill else {
        return exact_query_contract_violation();
    };
    fast.try_publish_direct_string_with(length, |output| {
        fill_random(output)
            .then_some(())
            .ok_or("native random source rejected the output range")
    })
    .map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

fn exact_native_random_range(
    fast: &NativeRequestFastState,
    minimum: i64,
    maximum: i64,
) -> php_jit::JitNativeControlResult {
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let (minimum, maximum) = if minimum == missing && maximum == missing {
        (0, i64::from(i32::MAX))
    } else {
        let (Some(minimum), Some(maximum)) = (
            exact_native_integer(fast, minimum),
            exact_native_integer(fast, maximum),
        ) else {
            return exact_query_contract_violation();
        };
        (minimum, maximum)
    };
    fast.random_int_inclusive(minimum, maximum).map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_random_int_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &*runtime };
    exact_native_random_range(fast, argument_0, argument_1)
}

macro_rules! exact_native_legacy_random_abi {
    ($abi:ident) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
        ) -> php_jit::JitNativeControlResult {
            #[allow(unsafe_code)]
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            let fast = unsafe { &*runtime };
            exact_native_random_range(fast, argument_0, argument_1)
        }
    };
}

exact_native_legacy_random_abi!(jit_native_rand_abi);
exact_native_legacy_random_abi!(jit_native_mt_rand_abi);

macro_rules! exact_native_random_max_abi {
    ($abi:ident) => {
        pub(crate) extern "C" fn $abi(
            _runtime: *mut NativeRequestFastState,
        ) -> php_jit::JitNativeControlResult {
            php_jit::JitNativeControlResult::returning(i64::from(i32::MAX))
        }
    };
}

exact_native_random_max_abi!(jit_native_getrandmax_abi);
exact_native_random_max_abi!(jit_native_mt_getrandmax_abi);

pub(crate) extern "C" fn jit_native_array_rand_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let requested =
        if argument_1 != php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
            let Some(requested) = exact_native_integer(fast, argument_1)
                .and_then(|value| usize::try_from(value).ok())
            else {
                return exact_query_contract_violation();
            };
            requested
        } else {
            1
        };
    let Some((source_entries, length)) = fast.stable_native_array_range(argument_0) else {
        return exact_query_contract_violation();
    };
    if requested == 0 || requested > length {
        return exact_query_contract_violation();
    }
    if requested == 1 {
        let Some(index) = fast.random_bounded_usize(length) else {
            return exact_query_contract_violation();
        };
        // SAFETY: the input owner keeps this stable array range live for the
        // complete synchronous exact call.
        #[allow(unsafe_code)]
        let key = unsafe { (*source_entries.add(index)).key };
        return fast.retain_direct_encoded(key).map_or_else(
            |_| exact_query_contract_violation(),
            |_| php_jit::JitNativeControlResult::returning(key),
        );
    }
    let output = match fast.publish_owned_direct_array_with(length, |_, index| {
        let encoded = i64::try_from(index).map_err(|_| "native array_rand index overflow")?;
        Ok(php_jit::JitNativeDirectArrayEntry {
            key: encoded,
            value: encoded,
        })
    }) {
        Ok(output) => output,
        Err(_) => return exact_query_contract_violation(),
    };
    let selected = fast.mutate_owned_direct_array(output, |fast, writer| {
        for index in 0..requested {
            let offset = fast
                .random_bounded_usize(length - index)
                .ok_or("native array_rand random source failed")?;
            let target = index + offset;
            if target == index {
                continue;
            }
            let left = writer
                .get(index)
                .ok_or("native array_rand left permutation entry disappeared")?;
            let right = writer
                .get(target)
                .ok_or("native array_rand right permutation entry disappeared")?;
            writer
                .replace_owned(
                    index,
                    php_jit::JitNativeDirectArrayEntry {
                        key: left.key,
                        value: right.value,
                    },
                )
                .ok_or("native array_rand left permutation replacement failed")?;
            writer
                .replace_owned(
                    target,
                    php_jit::JitNativeDirectArrayEntry {
                        key: right.key,
                        value: left.value,
                    },
                )
                .ok_or("native array_rand right permutation replacement failed")?;
        }
        for index in 0..requested {
            let entry = writer
                .get(index)
                .ok_or("native array_rand selected entry disappeared")?;
            let source_index = usize::try_from(entry.value)
                .map_err(|_| "native array_rand selected index overflow")?;
            if source_index >= length {
                return Err("native array_rand selected index is out of bounds");
            }
            // SAFETY: the input owner keeps the stable source range live while
            // the disjoint result arena mutates.
            #[allow(unsafe_code)]
            let value = unsafe { (*source_entries.add(source_index)).key };
            fast.retain_direct_encoded(value)?;
            if writer
                .replace_owned(
                    index,
                    php_jit::JitNativeDirectArrayEntry {
                        key: entry.key,
                        value,
                    },
                )
                .is_none()
            {
                let _ = fast.discard_owned_direct_value(value);
                return Err("native array_rand selected key replacement failed");
            }
        }
        writer.length = requested;
        Ok(())
    });
    if selected.is_err() || fast.shrink_owned_direct_array_to_fit(output).is_err() {
        let _ = fast.discard_owned_direct_value(output);
        return exact_query_contract_violation();
    }
    php_jit::JitNativeControlResult::returning(output)
}

pub(crate) extern "C" fn jit_native_shuffle_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    if fast.native_shuffle(argument_0).is_none() {
        return exact_query_contract_violation();
    }
    exact_query_return_bool(true)
}

macro_rules! exact_native_html_encode_abi {
    ($abi:ident, $all_entities:expr) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
            argument_2: i64,
            argument_3: i64,
        ) -> php_jit::JitNativeControlResult {
            #[allow(unsafe_code)]
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            let fast = unsafe { &mut *runtime };
            let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
            let flags = if argument_1 != missing {
                let Some(flags) = exact_native_integer(fast, argument_1) else {
                    return exact_query_contract_violation();
                };
                flags
            } else {
                php_runtime::api::NATIVE_HTML_ESCAPE_DEFAULT_FLAGS
            };
            if argument_2 != missing
                && argument_2 != php_jit::jit_encode_constant(u32::MAX)
                && fast.native_string_view(argument_2).is_none()
            {
                return exact_query_contract_violation();
            }
            let double_encode = if argument_3 != missing {
                let Some(double_encode) = exact_native_boolean_flag(fast, argument_3) else {
                    return exact_query_contract_violation();
                };
                double_encode
            } else {
                true
            };
            fast.publish_direct_string_transform(
                argument_0,
                |input| {
                    php_runtime::api::native_html_escape_output_length(
                        input,
                        flags,
                        double_encode,
                        $all_entities,
                    )
                },
                |input, output| {
                    php_runtime::api::native_html_escape_into(
                        input,
                        flags,
                        double_encode,
                        $all_entities,
                        output,
                    )
                },
            )
            .map_or_else(
                exact_query_contract_violation,
                php_jit::JitNativeControlResult::returning,
            )
        }
    };
}

exact_native_html_encode_abi!(jit_native_htmlspecialchars_abi, false);
exact_native_html_encode_abi!(jit_native_htmlentities_abi, true);

pub(crate) extern "C" fn jit_native_html_entity_decode_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let flags = if argument_1 != missing {
        let Some(flags) = exact_native_integer(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        flags
    } else {
        php_runtime::api::NATIVE_HTML_ESCAPE_DEFAULT_FLAGS
    };
    if argument_2 != missing
        && argument_2 != php_jit::jit_encode_constant(u32::MAX)
        && fast.native_string_view(argument_2).is_none()
    {
        return exact_query_contract_violation();
    }
    fast.publish_direct_string_transform(
        argument_0,
        |input| php_runtime::api::native_html_entity_decode_output_length(input, flags, false),
        |input, output| {
            php_runtime::api::native_html_entity_decode_into(input, flags, false, output)
        },
    )
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_htmlspecialchars_decode_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let flags = if argument_1 != php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        let Some(flags) = exact_native_integer(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        flags
    } else {
        php_runtime::api::NATIVE_HTML_ESCAPE_DEFAULT_FLAGS
    };
    fast.publish_direct_string_transform(
        argument_0,
        |input| php_runtime::api::native_html_entity_decode_output_length(input, flags, true),
        |input, output| {
            php_runtime::api::native_html_entity_decode_into(input, flags, true, output)
        },
    )
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_get_html_translation_table_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let table = if argument_0 == missing {
        0
    } else {
        let Some(table) = exact_native_integer(fast, argument_0) else {
            return exact_query_contract_violation();
        };
        table
    };
    let flags = if argument_1 == missing {
        php_runtime::api::NATIVE_HTML_ESCAPE_DEFAULT_FLAGS
    } else {
        let Some(flags) = exact_native_integer(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        flags
    };
    let encoding = if argument_2 == missing {
        None
    } else {
        let Some(encoding) = exact_native_date_string(fast, argument_2) else {
            return exact_query_contract_violation();
        };
        Some(encoding)
    };
    let entries = php_runtime::api::native_html_translation_entries(
        table,
        flags,
        encoding.as_deref().map(str::as_bytes),
    );
    let Some(mut writer) = fast
        .begin_owned_direct_array(entries.len(), entries.len())
        .ok()
    else {
        return exact_query_contract_violation();
    };
    for (key_bytes, value_bytes) in entries {
        let key = match fast.publish_direct_string_bytes(key_bytes) {
            Ok(key) => key,
            Err(_) => {
                fast.abort_owned_direct_array(writer);
                return exact_query_contract_violation();
            }
        };
        let value = match fast.publish_direct_string_bytes(value_bytes) {
            Ok(value) => value,
            Err(_) => {
                let _ = fast.discard_owned_direct_value(key);
                fast.abort_owned_direct_array(writer);
                return exact_query_contract_violation();
            }
        };
        if fast
            .push_owned_direct_array_entry(
                &mut writer,
                php_jit::JitNativeDirectArrayEntry { key, value },
            )
            .is_err()
        {
            let _ = fast.discard_owned_direct_value(value);
            let _ = fast.discard_owned_direct_value(key);
            fast.abort_owned_direct_array(writer);
            return exact_query_contract_violation();
        }
    }
    fast.finish_owned_direct_array(writer).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_http_build_query_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let output = {
        let numeric_prefix = if argument_1 != missing {
            let Some(prefix) = fast.native_string_view(argument_1) else {
                return exact_query_contract_violation();
            };
            Some((prefix.as_ptr(), prefix.len()))
        } else {
            None
        };
        let separator =
            if argument_2 == missing || argument_2 == php_jit::jit_encode_constant(u32::MAX) {
                let Some(separator) = fast.native_arg_separator_output() else {
                    return exact_query_contract_violation();
                };
                (separator.as_ptr(), separator.len())
            } else {
                let Some(separator) = fast.native_string_view(argument_2) else {
                    return exact_query_contract_violation();
                };
                (separator.as_ptr(), separator.len())
            };
        let raw_encoding = if argument_3 != missing {
            let Some(encoding) = exact_native_integer(fast, argument_3) else {
                return exact_query_contract_violation();
            };
            encoding == php_runtime::api::NATIVE_PHP_QUERY_RFC3986
        } else {
            false
        };
        // SAFETY: both optional configuration bytes and native string ranges
        // remain stable for this synchronous exact call.
        #[allow(unsafe_code)]
        let numeric_prefix = numeric_prefix
            .map(|(bytes, length)| unsafe { std::slice::from_raw_parts(bytes, length) });
        #[allow(unsafe_code)]
        let separator = unsafe { std::slice::from_raw_parts(separator.0, separator.1) };
        fast.native_http_build_query(argument_0, numeric_prefix, separator, raw_encoding)
    };
    output.map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_parse_url_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let component =
        if argument_1 != php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
            let Some(component) = exact_native_integer(fast, argument_1) else {
                return exact_query_contract_violation();
            };
            Some(component)
        } else {
            None
        };
    match fast.native_parse_url_direct(argument_0, component) {
        Some(Ok((true, Some(value)))) => php_jit::JitNativeControlResult::returning(value),
        Some(Ok((false, None))) => exact_query_return_bool(false),
        Some(Ok((true, None) | (false, Some(_)))) => exact_query_contract_violation(),
        Some(Err(_)) | None => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_parse_str_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    if !fast.direct_reference_accepts_native_replace(argument_1) {
        return exact_query_contract_violation();
    }
    let Some(array) = fast.native_parse_str_direct(argument_0) else {
        return exact_query_contract_violation();
    };
    if !fast.replace_direct_reference(argument_1, array) {
        return exact_query_contract_violation();
    }
    php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX))
}

macro_rules! exact_native_sort_abi {
    (
        $abi:ident,
        compare_keys = $compare_keys:literal,
        reverse = $reverse:literal,
        fixed_natural = $fixed_natural:literal,
        force_case_insensitive = $force_case_insensitive:literal,
        preserve_keys = $preserve_keys:literal,
        fixed_arity = $fixed_arity:literal
    ) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
        ) -> php_jit::JitNativeControlResult {
            #[allow(unsafe_code)]
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            let fast = unsafe { &mut *runtime };
            let flags = if argument_1
                != php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING)
            {
                let Some(flags) = exact_native_integer(fast, argument_1) else {
                    return exact_query_contract_violation();
                };
                flags
            } else {
                0
            };
            if fast
                                                        .native_sort::<
                                                            $compare_keys,
                                                            $reverse,
                                                            $fixed_natural,
                                                            $force_case_insensitive,
                                                            $preserve_keys,
                                                        >(argument_0, flags)
                                                        .is_none()
                                                    {
                                                        return exact_query_contract_violation();
                                                    }
            exact_query_return_bool(true)
        }
    };
}

exact_native_sort_abi!(
    jit_native_asort_abi,
    compare_keys = false,
    reverse = false,
    fixed_natural = false,
    force_case_insensitive = false,
    preserve_keys = true,
    fixed_arity = false
);
exact_native_sort_abi!(
    jit_native_arsort_abi,
    compare_keys = false,
    reverse = true,
    fixed_natural = false,
    force_case_insensitive = false,
    preserve_keys = true,
    fixed_arity = false
);
exact_native_sort_abi!(
    jit_native_ksort_abi,
    compare_keys = true,
    reverse = false,
    fixed_natural = false,
    force_case_insensitive = false,
    preserve_keys = true,
    fixed_arity = false
);
exact_native_sort_abi!(
    jit_native_krsort_abi,
    compare_keys = true,
    reverse = true,
    fixed_natural = false,
    force_case_insensitive = false,
    preserve_keys = true,
    fixed_arity = false
);
exact_native_sort_abi!(
    jit_native_natsort_abi,
    compare_keys = false,
    reverse = false,
    fixed_natural = true,
    force_case_insensitive = false,
    preserve_keys = true,
    fixed_arity = true
);
exact_native_sort_abi!(
    jit_native_natcasesort_abi,
    compare_keys = false,
    reverse = false,
    fixed_natural = true,
    force_case_insensitive = true,
    preserve_keys = true,
    fixed_arity = true
);
exact_native_sort_abi!(
    jit_native_sort_abi,
    compare_keys = false,
    reverse = false,
    fixed_natural = false,
    force_case_insensitive = false,
    preserve_keys = false,
    fixed_arity = false
);
exact_native_sort_abi!(
    jit_native_rsort_abi,
    compare_keys = false,
    reverse = true,
    fixed_natural = false,
    force_case_insensitive = false,
    preserve_keys = false,
    fixed_arity = false
);

pub(crate) extern "C" fn jit_native_spl_object_hash_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(identity) = fast.native_object_identity(argument_0) else {
        return exact_query_contract_violation();
    };
    let mut hash = [b'0'; 32];
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for (index, byte) in hash.iter_mut().enumerate().skip(16) {
        let shift = (31 - index) * 4;
        *byte = HEX[((identity >> shift) & 0xf) as usize];
    }
    match fast.publish_direct_string_bytes(&hash) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_spl_object_id_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(identity) = fast.native_object_identity(argument_0) else {
        return exact_query_contract_violation();
    };
    match fast.publish_direct_int(identity as i64) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_serialize_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(output_length) = fast.native_serialize_output_length(argument_0) else {
        return exact_query_contract_violation();
    };
    fast.try_publish_direct_string_with(output_length, |output| {
        // SAFETY: direct string publication mutates only the disjoint byte
        // arena; the authoritative source graph remains stable for this
        // synchronous second serialization pass.
        #[allow(unsafe_code)]
        let fast = unsafe { &*runtime };
        fast.native_serialize_into(argument_0, output)
            .then_some(())
            .ok_or("native serialization changed after its length pass")
    })
    .map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_unserialize_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(value) = fast.native_unserialize(argument_0) else {
        return exact_query_contract_violation();
    };
    php_jit::JitNativeControlResult::returning(value)
}

pub(crate) extern "C" fn jit_native_token_get_all_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    if argument_1 != php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING)
        && exact_native_integer(fast, argument_1) != Some(0)
    {
        return exact_query_contract_violation();
    }
    let Some((source, source_length)) = fast.stable_native_string_range(argument_0) else {
        return exact_query_contract_violation();
    };
    #[allow(unsafe_code)]
    // Safety: the encoded string owner remains live for this synchronous call,
    // and native arena publication does not relocate its backing range.
    let source = unsafe { std::slice::from_raw_parts(source, source_length) };
    match php_runtime::api::native_tokenize_default_into(source, fast) {
        Some(value) => php_jit::JitNativeControlResult::returning(value),
        None => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_token_name_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(id) = exact_native_integer(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let name = php_runtime::api::token_name_for_id(id).unwrap_or("UNKNOWN");
    match fast.publish_direct_string_bytes(name.as_bytes()) {
        Ok(value) => php_jit::JitNativeControlResult::returning(value),
        Err(_) => exact_query_contract_violation(),
    }
}

fn exact_mb_current_encoding(fast: &NativeRequestFastState) -> Option<&'static str> {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    unsafe {
        fast.mbstring
            .internal_encoding
            .as_ref()
            .and_then(|encoding| {
                php_runtime::api::native_mb_canonical_encoding(encoding.as_bytes())
            })
    }
}

fn exact_mb_encoding_argument(fast: &NativeRequestFastState, encoded: i64) -> Option<&'static str> {
    if encoded == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        return exact_mb_current_encoding(fast);
    }
    let bytes = fast.native_string_view(encoded)?;
    php_runtime::api::native_mb_canonical_encoding(bytes)
}

fn exact_mb_publish_bytes(
    fast: &mut NativeRequestFastState,
    bytes: &[u8],
) -> php_jit::JitNativeControlResult {
    fast.publish_direct_string_bytes(bytes).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_iconv_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    file: i64,
    start: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(from) = fast.native_string_view(argument_0).map(<[u8]>::to_vec) else {
        return exact_query_contract_violation();
    };
    let Some(to) = fast.native_string_view(argument_1).map(<[u8]>::to_vec) else {
        return exact_query_contract_violation();
    };
    let Some(input) = fast.native_string_view(argument_2).map(<[u8]>::to_vec) else {
        return exact_query_contract_violation();
    };
    match php_runtime::api::native_iconv_convert(&from, &to, &input) {
        php_runtime::api::NativeIconvConversion::Converted(output) => {
            exact_mb_publish_bytes(fast, &output)
        }
        php_runtime::api::NativeIconvConversion::EncodingTooLong => {
            if super::exact_runtime_ops::emit_exact_native_structured_warning(
                fast,
                "E_PHP_RUNTIME_ICONV_ENCODING_TOO_LONG",
                "iconv(): Encoding parameter exceeds the maximum allowed length of 64 characters"
                    .to_owned(),
                file,
                start,
            ) != 0
            {
                return exact_query_runtime_error();
            }
            exact_query_return_bool(false)
        }
        php_runtime::api::NativeIconvConversion::WrongEncoding { from, to } => {
            if super::exact_runtime_ops::emit_exact_native_structured_warning(
                fast,
                "E_PHP_RUNTIME_ICONV_WRONG_ENCODING",
                format!(
                    "iconv(): Wrong encoding, conversion from \"{from}\" to \"{to}\" is not allowed"
                ),
                file,
                start,
            ) != 0
            {
                return exact_query_runtime_error();
            }
            exact_query_return_bool(false)
        }
        php_runtime::api::NativeIconvConversion::InvalidInput => {
            if super::exact_runtime_ops::emit_exact_native_structured_warning(
                fast,
                "E_PHP_RUNTIME_ICONV_INVALID_INPUT",
                "iconv(): Detected an illegal character in input string".to_owned(),
                file,
                start,
            ) != 0
            {
                return exact_query_runtime_error();
            }
            exact_query_return_bool(false)
        }
        php_runtime::api::NativeIconvConversion::Unrepresentable => {
            exact_query_return_bool(false)
        }
    }
}

fn exact_normalizer_input(
    fast: &mut NativeRequestFastState,
    encoded: i64,
    prepared_error: u64,
    name: &'static str,
) -> Result<String, php_jit::JitNativeControlResult> {
    let Some(bytes) = fast.native_string_view(encoded) else {
        return Err(exact_query_contract_violation());
    };
    std::str::from_utf8(bytes).map(str::to_owned).map_err(|_| {
        exact_json_decode_error(
            fast,
            prepared_error,
            true,
            format!("{name}(): input must be valid UTF-8").as_bytes(),
        )
    })
}

fn exact_normalizer_form(
    fast: &NativeRequestFastState,
    encoded: i64,
) -> Option<i64> {
    if encoded == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        Some(php_runtime::api::NORMALIZER_FORM_C)
    } else {
        exact_native_integer(fast, encoded)
    }
}

pub(crate) extern "C" fn jit_native_normalizer_normalize_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    prepared_error: u64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let input = match exact_normalizer_input(
        fast,
        argument_0,
        prepared_error,
        "normalizer_normalize",
    ) {
        Ok(input) => input,
        Err(error) => return error,
    };
    let Some(form) = exact_normalizer_form(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    match php_runtime::api::normalize_string(&input, form) {
        Some(output) => exact_mb_publish_bytes(fast, output.as_bytes()),
        None => exact_json_decode_error(
            fast,
            prepared_error,
            true,
            b"normalizer_normalize(): Argument #2 ($form) must be a valid normalization form",
        ),
    }
}

pub(crate) extern "C" fn jit_native_normalizer_is_normalized_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    prepared_error: u64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let input = match exact_normalizer_input(
        fast,
        argument_0,
        prepared_error,
        "normalizer_is_normalized",
    ) {
        Ok(input) => input,
        Err(error) => return error,
    };
    let Some(form) = exact_normalizer_form(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    match php_runtime::api::is_normalized_string(&input, form) {
        Some(normalized) => exact_query_return_bool(normalized),
        None => exact_json_decode_error(
            fast,
            prepared_error,
            true,
            b"normalizer_is_normalized(): Argument #2 ($form) must be a valid normalization form",
        ),
    }
}

fn exact_mb_publish_string_list<I>(
    fast: &mut NativeRequestFastState,
    values: I,
) -> php_jit::JitNativeControlResult
where
    I: IntoIterator,
    I::IntoIter: ExactSizeIterator,
    I::Item: AsRef<str>,
{
    let mut values = values.into_iter();
    let length = values.len();
    fast.publish_owned_direct_array_with(length, |fast, index| {
        let value = values.next().ok_or("native mbstring list truncated")?;
        let value = fast
            .publish_direct_string_bytes(value.as_ref().as_bytes())
            .map_err(|_| "native mbstring list value publication failed")?;
        Ok(php_jit::JitNativeDirectArrayEntry {
            key: i64::try_from(index).unwrap_or(i64::MAX),
            value,
        })
    })
    .ok()
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_mb_detect_encoding_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(input) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    if argument_2 != missing {
        // Strict detection has distinct invalid-sequence semantics and remains
        // outside optimizer admission until that mode is native.
        return exact_query_contract_violation();
    }
    let detected = if argument_1 == missing {
        let Some(current) = exact_mb_current_encoding(fast) else {
            return exact_query_contract_violation();
        };
        php_runtime::api::native_mb_detect_encoding(input, std::iter::once(current))
    } else if let Some(bytes) = fast.native_string_view(argument_1) {
        let Ok(list) = std::str::from_utf8(bytes) else {
            return exact_query_contract_violation();
        };
        php_runtime::api::native_mb_detect_encoding(
            input,
            list.split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty()),
        )
    } else if let Some((entries, length)) = fast.stable_native_array_range(argument_1) {
        for index in 0..length {
            // SAFETY: the encoded argument keeps this stable native array
            // range live throughout the synchronous detection call.
            #[allow(unsafe_code)]
            let entry = unsafe { *entries.add(index) };
            let Some(bytes) = fast.native_string_view(entry.value) else {
                return exact_query_contract_violation();
            };
            if std::str::from_utf8(bytes).is_err() {
                return exact_query_contract_violation();
            }
        }
        php_runtime::api::native_mb_detect_encoding(input, (0..length).map(|index| {
            // SAFETY: every entry and UTF-8 string was validated above and
            // remains stable while the iterator is consumed synchronously.
            #[allow(unsafe_code)]
            let entry = unsafe { *entries.add(index) };
            std::str::from_utf8(
                fast.native_string_view(entry.value)
                    .expect("validated native mbstring candidate disappeared"),
            )
            .expect("validated native mbstring candidate stopped being UTF-8")
        }))
    } else {
        return exact_query_contract_violation();
    };
    match detected {
        Some(Some(encoding)) => exact_mb_publish_bytes(fast, encoding.as_bytes()),
        Some(None) => exact_query_return_bool(false),
        None => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_mb_check_encoding_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    if argument_0 == missing {
        return exact_query_return_bool(true);
    }
    let Some(input) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(encoding) = exact_mb_encoding_argument(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    php_runtime::api::native_mb_check_encoding(input, encoding)
        .map_or_else(exact_query_contract_violation, exact_query_return_bool)
}

pub(crate) extern "C" fn jit_native_mb_convert_encoding_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(input) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(to_encoding) = fast
        .native_string_view(argument_1)
        .and_then(php_runtime::api::native_mb_canonical_encoding)
    else {
        return exact_query_contract_violation();
    };
    let Some(from_encoding) = exact_mb_encoding_argument(fast, argument_2) else {
        return exact_query_contract_violation();
    };
    let Some(output) =
        php_runtime::api::native_mb_convert_encoding(input, to_encoding, from_encoding)
    else {
        return exact_query_contract_violation();
    };
    exact_mb_publish_bytes(fast, &output)
}

pub(crate) extern "C" fn jit_native_mb_internal_encoding_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    if argument_0 == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        let Some(encoding) = exact_mb_current_encoding(fast) else {
            return exact_query_contract_violation();
        };
        return exact_mb_publish_bytes(fast, encoding.as_bytes());
    }
    let Some(canonical) = fast
        .native_string_view(argument_0)
        .and_then(php_runtime::api::native_mb_canonical_encoding)
    else {
        return exact_query_contract_violation();
    };
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let Some(encoding) = (unsafe { fast.mbstring.internal_encoding.as_mut() }) else {
        return exact_query_contract_violation();
    };
    canonical.clone_into(encoding);
    exact_query_return_bool(true)
}

pub(crate) extern "C" fn jit_native_mb_list_encodings_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    exact_mb_publish_string_list(fast, php_runtime::api::native_mb_encoding_names())
}

pub(crate) extern "C" fn jit_native_mb_encoding_aliases_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(encoding) = fast
        .native_string_view(argument_0)
        .and_then(php_runtime::api::native_mb_canonical_encoding)
    else {
        return exact_query_contract_violation();
    };
    let Some(aliases) = php_runtime::api::native_mb_encoding_aliases(encoding) else {
        return exact_query_contract_violation();
    };
    exact_mb_publish_string_list(fast, aliases.iter().copied())
}

pub(crate) extern "C" fn jit_native_mb_substitute_character_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let Some(state) = (unsafe { fast.mbstring.substitute_character.as_mut() }) else {
        return exact_query_contract_violation();
    };
    if argument_0 == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        return match state {
            php_runtime::api::MbSubstituteCharacter::Codepoint(value) => {
                fast.publish_direct_int(*value).map_or_else(
                    |_| exact_query_contract_violation(),
                    php_jit::JitNativeControlResult::returning,
                )
            }
            php_runtime::api::MbSubstituteCharacter::Mode(mode) => {
                exact_mb_publish_bytes(fast, mode.as_bytes())
            }
        };
    }
    let replacement = match fast.native_printf_scalar(argument_0) {
        Some(php_runtime::api::NativePrintfScalar::Int(value))
            if char::from_u32(value as u32).is_some() =>
        {
            php_runtime::api::MbSubstituteCharacter::Codepoint(value)
        }
        Some(php_runtime::api::NativePrintfScalar::String(value)) => {
            let Ok(mode) = std::str::from_utf8(value) else {
                return exact_query_contract_violation();
            };
            let mode = match mode.to_ascii_lowercase().as_str() {
                "none" => "none",
                "long" => "long",
                "entity" => "entity",
                _ => return exact_query_contract_violation(),
            };
            php_runtime::api::MbSubstituteCharacter::Mode(mode)
        }
        _ => return exact_query_contract_violation(),
    };
    *state = replacement;
    exact_query_return_bool(true)
}

pub(crate) extern "C" fn jit_native_mb_strlen_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(input) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(encoding) = exact_mb_encoding_argument(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    let Some(length) = php_runtime::api::native_mb_strlen(input, encoding) else {
        return exact_query_contract_violation();
    };
    fast.publish_direct_int(length).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

macro_rules! exact_mb_simple_case_abi {
    ($abi:ident, $uppercase:literal) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
        ) -> php_jit::JitNativeControlResult {
            #[allow(unsafe_code)]
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            let fast = unsafe { &mut *runtime };
            let Some(input) = fast.native_string_view(argument_0) else {
                return exact_query_contract_violation();
            };
            let Some(encoding) = exact_mb_encoding_argument(fast, argument_1) else {
                return exact_query_contract_violation();
            };
            let Some(output) =
                php_runtime::api::native_mb_convert_simple_case(input, encoding, $uppercase)
            else {
                return exact_query_contract_violation();
            };
            exact_mb_publish_bytes(fast, &output)
        }
    };
}

exact_mb_simple_case_abi!(jit_native_mb_strtolower_abi, false);
exact_mb_simple_case_abi!(jit_native_mb_strtoupper_abi, true);

macro_rules! exact_mb_position_abi {
    ($abi:ident, $insensitive:literal, $reverse:literal) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
            argument_2: i64,
            argument_3: i64,
        ) -> php_jit::JitNativeControlResult {
            #[allow(unsafe_code)]
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            let fast = unsafe { &mut *runtime };
            let Some(haystack) = fast.native_string_view(argument_0) else {
                return exact_query_contract_violation();
            };
            let Some(needle) = fast.native_string_view(argument_1) else {
                return exact_query_contract_violation();
            };
            let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
            let offset = if argument_2 != missing {
                let Some(offset) = exact_native_integer(fast, argument_2) else {
                    return exact_query_contract_violation();
                };
                offset
            } else {
                0
            };
            let Some(encoding) = exact_mb_encoding_argument(fast, argument_3) else {
                return exact_query_contract_violation();
            };
            match php_runtime::api::native_mb_position(
                haystack,
                needle,
                offset,
                encoding,
                $insensitive,
                $reverse,
            ) {
                Some(Some(position)) => fast.publish_direct_int(position).map_or_else(
                    |_| exact_query_contract_violation(),
                    php_jit::JitNativeControlResult::returning,
                ),
                Some(None) => exact_query_return_bool(false),
                None => exact_query_contract_violation(),
            }
        }
    };
}

exact_mb_position_abi!(jit_native_mb_stripos_abi, true, false);
exact_mb_position_abi!(jit_native_mb_strpos_abi, false, false);
exact_mb_position_abi!(jit_native_mb_strripos_abi, true, true);
exact_mb_position_abi!(jit_native_mb_strrpos_abi, false, true);

pub(crate) extern "C" fn jit_native_mb_substr_count_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(haystack) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(needle) = fast.native_string_view(argument_1) else {
        return exact_query_contract_violation();
    };
    let Some(encoding) = exact_mb_encoding_argument(fast, argument_2) else {
        return exact_query_contract_violation();
    };
    let Some(count) = php_runtime::api::native_mb_substr_count(haystack, needle, encoding) else {
        return exact_query_contract_violation();
    };
    fast.publish_direct_int(count).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

fn exact_mb_optional_length(fast: &NativeRequestFastState, encoded: i64) -> Option<Option<i64>> {
    if encoded == php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        return Some(None);
    }
    match fast.native_printf_scalar(encoded) {
        Some(php_runtime::api::NativePrintfScalar::Null) => Some(None),
        Some(php_runtime::api::NativePrintfScalar::Int(value)) => Some(Some(value)),
        _ => None,
    }
}

macro_rules! exact_mb_substring_abi {
    ($abi:ident, $native:path) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
            argument_2: i64,
            argument_3: i64,
        ) -> php_jit::JitNativeControlResult {
            #[allow(unsafe_code)]
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            let fast = unsafe { &mut *runtime };
            let Some(input) = fast.native_string_view(argument_0) else {
                return exact_query_contract_violation();
            };
            let Some(start) = exact_native_integer(fast, argument_1) else {
                return exact_query_contract_violation();
            };
            let Some(length) = exact_mb_optional_length(fast, argument_2) else {
                return exact_query_contract_violation();
            };
            let Some(encoding) = exact_mb_encoding_argument(fast, argument_3) else {
                return exact_query_contract_violation();
            };
            let Some(output) = $native(input, start, length, encoding) else {
                return exact_query_contract_violation();
            };
            exact_mb_publish_bytes(fast, &output)
        }
    };
}

exact_mb_substring_abi!(jit_native_mb_substr_abi, php_runtime::api::native_mb_substr);
exact_mb_substring_abi!(jit_native_mb_strcut_abi, php_runtime::api::native_mb_strcut);

pub(crate) extern "C" fn jit_native_mb_strwidth_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(input) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(encoding) = exact_mb_encoding_argument(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    let Some(width) = php_runtime::api::native_mb_strwidth(input, encoding) else {
        return exact_query_contract_violation();
    };
    fast.publish_direct_int(width).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_mb_strimwidth_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
    argument_4: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(input) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let (Some(start), Some(width)) = (
        exact_native_integer(fast, argument_1),
        exact_native_integer(fast, argument_2),
    ) else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let marker = if argument_3 != missing {
        let Some(marker) = fast.native_string_view(argument_3) else {
            return exact_query_contract_violation();
        };
        marker
    } else {
        &[]
    };
    let Some(encoding) = exact_mb_encoding_argument(fast, argument_4) else {
        return exact_query_contract_violation();
    };
    let Some(output) =
        php_runtime::api::native_mb_strimwidth(input, start, width, marker, encoding)
    else {
        return exact_query_contract_violation();
    };
    exact_mb_publish_bytes(fast, &output)
}

pub(crate) extern "C" fn jit_native_mb_convert_case_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(input) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(mode) = exact_native_integer(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    let Some(encoding) = exact_mb_encoding_argument(fast, argument_2) else {
        return exact_query_contract_violation();
    };
    let Some(output) = php_runtime::api::native_mb_convert_case(input, mode, encoding) else {
        return exact_query_contract_violation();
    };
    exact_mb_publish_bytes(fast, &output)
}

macro_rules! exact_mb_first_char_case_abi {
    ($abi:ident, $uppercase:literal) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
        ) -> php_jit::JitNativeControlResult {
            #[allow(unsafe_code)]
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            let fast = unsafe { &mut *runtime };
            let Some(input) = fast.native_string_view(argument_0) else {
                return exact_query_contract_violation();
            };
            let Some(encoding) = exact_mb_encoding_argument(fast, argument_1) else {
                return exact_query_contract_violation();
            };
            let Some(output) =
                php_runtime::api::native_mb_first_char_case(input, encoding, $uppercase)
            else {
                return exact_query_contract_violation();
            };
            exact_mb_publish_bytes(fast, &output)
        }
    };
}

exact_mb_first_char_case_abi!(jit_native_mb_ucfirst_abi, true);
exact_mb_first_char_case_abi!(jit_native_mb_lcfirst_abi, false);

pub(crate) extern "C" fn jit_native_mb_ord_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(input) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(encoding) = exact_mb_encoding_argument(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    match php_runtime::api::native_mb_ord(input, encoding) {
        Some(Some(value)) => fast.publish_direct_int(value).map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        ),
        Some(None) => exact_query_return_bool(false),
        None => exact_query_contract_violation(),
    }
}

pub(crate) extern "C" fn jit_native_mb_chr_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(codepoint) = exact_native_integer(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(encoding) = exact_mb_encoding_argument(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    let Some(output) = php_runtime::api::native_mb_chr(codepoint, encoding) else {
        return exact_query_contract_violation();
    };
    exact_mb_publish_bytes(fast, &output)
}

pub(crate) extern "C" fn jit_native_mb_parse_str_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    if !fast.direct_reference_accepts_native_replace(argument_1) {
        return exact_query_contract_violation();
    }
    let Some(array) = fast.native_parse_str_direct(argument_0) else {
        return exact_query_contract_violation();
    };
    if !fast.replace_direct_reference(argument_1, array) {
        return exact_query_contract_violation();
    }
    exact_query_return_bool(true)
}

fn exact_bcmath_scale(fast: &NativeRequestFastState, encoded: i64) -> Option<usize> {
    if encoded != php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        return usize::try_from(exact_native_integer(fast, encoded)?).ok();
    }
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    unsafe {
        fast.bcmath.scale.as_ref().copied()
    }
}

macro_rules! exact_bcmath_binary_abi {
    ($abi:ident, $native:path) => {
        pub(crate) extern "C" fn $abi(
            runtime: *mut NativeRequestFastState,
            argument_0: i64,
            argument_1: i64,
            argument_2: i64,
        ) -> php_jit::JitNativeControlResult {
            #[allow(unsafe_code)]
            // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
            let fast = unsafe { &mut *runtime };
            let Some(left) = fast.native_string_view(argument_0) else {
                return exact_query_contract_violation();
            };
            let Some(right) = fast.native_string_view(argument_1) else {
                return exact_query_contract_violation();
            };
            let Some(scale) = exact_bcmath_scale(fast, argument_2) else {
                return exact_query_contract_violation();
            };
            let Some(output) = $native(left, right, scale) else {
                return exact_query_contract_violation();
            };
            exact_mb_publish_bytes(fast, &output)
        }
    };
}

exact_bcmath_binary_abi!(jit_native_bcadd_abi, php_runtime::api::native_bcadd);
exact_bcmath_binary_abi!(jit_native_bcdiv_abi, php_runtime::api::native_bcdiv);
exact_bcmath_binary_abi!(jit_native_bcmod_abi, php_runtime::api::native_bcmod);
exact_bcmath_binary_abi!(jit_native_bcmul_abi, php_runtime::api::native_bcmul);
exact_bcmath_binary_abi!(jit_native_bcpow_abi, php_runtime::api::native_bcpow);
exact_bcmath_binary_abi!(jit_native_bcsub_abi, php_runtime::api::native_bcsub);

pub(crate) extern "C" fn jit_native_bccomp_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(left) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(right) = fast.native_string_view(argument_1) else {
        return exact_query_contract_violation();
    };
    let Some(scale) = exact_bcmath_scale(fast, argument_2) else {
        return exact_query_contract_violation();
    };
    let Some(result) = php_runtime::api::native_bccomp(left, right, scale) else {
        return exact_query_contract_violation();
    };
    fast.publish_direct_int(result).map_or_else(
        |_| exact_query_contract_violation(),
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_bcpowmod_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(base) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(exponent) = fast.native_string_view(argument_1) else {
        return exact_query_contract_violation();
    };
    let Some(modulus) = fast.native_string_view(argument_2) else {
        return exact_query_contract_violation();
    };
    let Some(scale) = exact_bcmath_scale(fast, argument_3) else {
        return exact_query_contract_violation();
    };
    let Some(output) = php_runtime::api::native_bcpowmod(base, exponent, modulus, scale) else {
        return exact_query_contract_violation();
    };
    exact_mb_publish_bytes(fast, &output)
}

pub(crate) extern "C" fn jit_native_bcscale_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let Some(scale) = (unsafe { fast.bcmath.scale.as_mut() }) else {
        return exact_query_contract_violation();
    };
    let previous = *scale;
    if argument_0 != php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING) {
        let Some(replacement) =
            exact_native_integer(fast, argument_0).and_then(|value| usize::try_from(value).ok())
        else {
            return exact_query_contract_violation();
        };
        *scale = replacement;
    }
    i64::try_from(previous)
        .ok()
        .and_then(|value| fast.publish_direct_int(value).ok())
        .map_or_else(
            exact_query_contract_violation,
            php_jit::JitNativeControlResult::returning,
        )
}

pub(crate) extern "C" fn jit_native_bcsqrt_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(input) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(scale) = exact_bcmath_scale(fast, argument_1) else {
        return exact_query_contract_violation();
    };
    let Some(output) = php_runtime::api::native_bcsqrt(input, scale) else {
        return exact_query_contract_violation();
    };
    exact_mb_publish_bytes(fast, &output)
}

fn exact_filter_source_root(fast: &NativeRequestFastState, source: i64) -> Option<Option<i64>> {
    let index = php_runtime::api::native_filter_input_source_index(source)?;
    Some((fast.filter.present & (1 << index) != 0).then_some(fast.filter.roots[index]))
}

fn exact_filter_publish_value(
    fast: &mut NativeRequestFastState,
    source: i64,
    value: php_runtime::api::NativeFilterValue,
) -> Option<i64> {
    match value {
        php_runtime::api::NativeFilterValue::Null => Some(php_jit::jit_encode_constant(u32::MAX)),
        php_runtime::api::NativeFilterValue::Bool(value) => {
            Some(php_jit::jit_encode_constant(if value {
                php_jit::JIT_VALUE_TRUE
            } else {
                php_jit::JIT_VALUE_FALSE
            }))
        }
        php_runtime::api::NativeFilterValue::Int(value) => fast.publish_direct_int(value).ok(),
        php_runtime::api::NativeFilterValue::Float(value) => fast.publish_direct_float(value).ok(),
        php_runtime::api::NativeFilterValue::InputString => {
            let source = fast.native_dereferenced_scalar_encoding(source)?;
            fast.retain_direct_encoded(source).ok()?;
            Some(source)
        }
        php_runtime::api::NativeFilterValue::Bytes(bytes) => {
            fast.publish_direct_string_bytes(&bytes).ok()
        }
    }
}

fn exact_filter_scalar_value(
    fast: &mut NativeRequestFastState,
    source: i64,
    filter: i64,
    flags: i64,
) -> Option<i64> {
    let scalar = fast.native_printf_scalar(source)?;
    let result = php_runtime::api::native_filter_scalar(scalar, filter, flags);
    match result {
        php_runtime::api::NativeFilterResult::Value(value) => {
            exact_filter_publish_value(fast, source, value)
        }
        php_runtime::api::NativeFilterResult::Unsupported
        | php_runtime::api::NativeFilterResult::UnknownFilter => None,
    }
}

fn exact_filter_recursive_array(
    fast: &mut NativeRequestFastState,
    source: i64,
    filter: i64,
    flags: i64,
) -> Option<i64> {
    let (source_entries, source_length) = fast.stable_native_array_range(source)?;
    fast.publish_owned_direct_array_with(source_length, |fast, index| {
        // SAFETY: `source` remains owned for this synchronous recursive
        // filter, and native publications reserve disjoint stable ranges.
        #[allow(unsafe_code)]
        let entry = unsafe { *source_entries.add(index) };
        fast.retain_direct_encoded(entry.key)?;
        let value = if fast.native_direct_array_entries(entry.value).is_some() {
            exact_filter_recursive_array(fast, entry.value, filter, flags)
        } else {
            exact_filter_scalar_value(fast, entry.value, filter, flags)
        };
        let Some(value) = value else {
            let _ = fast.discard_owned_direct_value(entry.key);
            return Err("native recursive filter could not publish an entry");
        };
        Ok(php_jit::JitNativeDirectArrayEntry {
            key: entry.key,
            value,
        })
    })
    .ok()
}

fn exact_filter_wrap_array(fast: &mut NativeRequestFastState, value: i64) -> Option<i64> {
    let key = match fast.publish_direct_int(0) {
        Ok(key) => key,
        Err(_) => {
            let _ = fast.discard_owned_direct_value(value);
            return None;
        }
    };
    fast.publish_owned_direct_array_from_iter(
        [php_jit::JitNativeDirectArrayEntry { key, value }].into_iter(),
    )
    .ok()
}

fn exact_filter_value(
    fast: &mut NativeRequestFastState,
    source: i64,
    filter: i64,
    flags: i64,
) -> Option<i64> {
    if fast.native_direct_array_entries(source).is_some() {
        if flags & php_runtime::api::FILTER_REQUIRE_SCALAR != 0
            || flags
                & (php_runtime::api::FILTER_REQUIRE_ARRAY | php_runtime::api::FILTER_FORCE_ARRAY)
                == 0
        {
            return exact_filter_publish_value(
                fast,
                source,
                if flags & php_runtime::api::FILTER_NULL_ON_FAILURE != 0 {
                    php_runtime::api::NativeFilterValue::Null
                } else {
                    php_runtime::api::NativeFilterValue::Bool(false)
                },
            );
        }
        return exact_filter_recursive_array(fast, source, filter, flags);
    }
    if flags & php_runtime::api::FILTER_REQUIRE_ARRAY != 0 {
        return exact_filter_publish_value(
            fast,
            source,
            if flags & php_runtime::api::FILTER_NULL_ON_FAILURE != 0 {
                php_runtime::api::NativeFilterValue::Null
            } else {
                php_runtime::api::NativeFilterValue::Bool(false)
            },
        );
    }
    let filtered = exact_filter_scalar_value(fast, source, filter, flags)?;
    if flags & php_runtime::api::FILTER_FORCE_ARRAY != 0 {
        exact_filter_wrap_array(fast, filtered)
    } else {
        Some(filtered)
    }
}

fn exact_filter_array_single(
    fast: &mut NativeRequestFastState,
    source: i64,
    filter: i64,
) -> Option<i64> {
    // Validate the PHP-visible filter ID before an empty array can hide it.
    match php_runtime::api::native_filter_scalar(
        php_runtime::api::NativePrintfScalar::String(&[]),
        filter,
        0,
    ) {
        php_runtime::api::NativeFilterResult::UnknownFilter
        | php_runtime::api::NativeFilterResult::Unsupported => return None,
        php_runtime::api::NativeFilterResult::Value(_) => {}
    }
    exact_filter_recursive_array(fast, source, filter, 0)
}

fn exact_filter_array_key_eq(fast: &NativeRequestFastState, left: i64, right: i64) -> bool {
    match (
        fast.native_string_view(left),
        fast.native_string_view(right),
    ) {
        (Some(left), Some(right)) => left == right,
        (None, None) => exact_native_integer(fast, left) == exact_native_integer(fast, right),
        _ => false,
    }
}

fn exact_filter_spec(
    fast: &NativeRequestFastState,
    spec: i64,
) -> Option<(i64, i64)> {
    let Some(entries) = fast.native_direct_array_entries(spec) else {
        return exact_native_integer(fast, spec).map(|filter| (filter, 0));
    };
    let mut filter = php_runtime::api::FILTER_DEFAULT;
    let mut flags = 0;
    for entry in entries {
        match fast.native_string_view(entry.key) {
            Some(b"filter") => filter = exact_native_integer(fast, entry.value)?,
            Some(b"flags") => flags = exact_native_integer(fast, entry.value)?,
            // Range, regexp, callback, and other option payloads require their
            // dedicated compiled implementations; do not silently erase them.
            Some(b"options") => return None,
            _ => {}
        }
    }
    Some((filter, flags))
}

fn exact_filter_array_specs(
    fast: &mut NativeRequestFastState,
    source: i64,
    specs: i64,
    add_empty: bool,
) -> Option<i64> {
    let (source_entries, source_length) = fast.stable_native_array_range(source)?;
    let (spec_entries, spec_length) = fast.stable_native_array_range(specs)?;
    let output_length = if add_empty {
        spec_length
    } else {
        let mut present = 0usize;
        for spec_index in 0..spec_length {
            #[allow(unsafe_code)]
            // Safety: the stable native array ranges remain request-owned for
            // this synchronous exact call.
            let spec = unsafe { *spec_entries.add(spec_index) };
            for source_index in 0..source_length {
                #[allow(unsafe_code)]
                // Safety: see the stable-range argument above.
                let input = unsafe { *source_entries.add(source_index) };
                if exact_filter_array_key_eq(fast, spec.key, input.key) {
                    present += 1;
                    break;
                }
            }
        }
        present
    };
    let mut spec_index = 0usize;
    fast.publish_owned_direct_array_with(output_length, |fast, _| loop {
        #[allow(unsafe_code)]
        // Safety: the stable specification range remains request-owned while
        // the output is published into a disjoint native arena range.
        let spec = unsafe { *spec_entries.add(spec_index) };
        spec_index += 1;
        let mut input_value = None;
        for source_index in 0..source_length {
            #[allow(unsafe_code)]
            // Safety: the stable source range remains request-owned here.
            let input = unsafe { *source_entries.add(source_index) };
            if exact_filter_array_key_eq(fast, spec.key, input.key) {
                input_value = Some(input.value);
                break;
            }
        }
        let value = match input_value {
            Some(input) => {
                let (filter, flags) = exact_filter_spec(fast, spec.value)
                    .ok_or("native filter array specification is unsupported")?;
                exact_filter_value(fast, input, filter, flags)
                    .ok_or("native filter array entry could not be published")?
            }
            None if add_empty => php_jit::jit_encode_constant(u32::MAX),
            None => continue,
        };
        fast.retain_direct_encoded(spec.key)?;
        return Ok(php_jit::JitNativeDirectArrayEntry {
            key: spec.key,
            value,
        });
    })
    .ok()
}

fn exact_filter_lookup_input(
    fast: &NativeRequestFastState,
    root: i64,
    name: &[u8],
) -> Option<Option<i64>> {
    let entries = fast.native_direct_array_entries(root)?;
    Some(entries.iter().find_map(|entry| {
        fast.native_string_view(entry.key)
            .is_some_and(|key| key == name)
            .then_some(entry.value)
    }))
}

pub(crate) extern "C" fn jit_native_filter_var_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let filter = if argument_1 != missing {
        let Some(filter) = exact_native_integer(fast, argument_1) else {
            return exact_query_contract_violation();
        };
        filter
    } else {
        php_runtime::api::FILTER_DEFAULT
    };
    let flags = if argument_2 != missing {
        let Some(flags) = exact_native_integer(fast, argument_2) else {
            return exact_query_contract_violation();
        };
        flags
    } else {
        0
    };
    exact_filter_value(fast, argument_0, filter, flags).map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_filter_id_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(name) = fast.native_string_view(argument_0) else {
        return exact_query_contract_violation();
    };
    match php_runtime::api::native_filter_id(name) {
        Some(id) => fast.publish_direct_int(id).map_or_else(
            |_| exact_query_contract_violation(),
            php_jit::JitNativeControlResult::returning,
        ),
        None => exact_query_return_bool(false),
    }
}

pub(crate) extern "C" fn jit_native_filter_list_abi(
    runtime: *mut NativeRequestFastState,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let mut names = php_runtime::api::native_filter_names();
    let length = names.len();
    fast.publish_owned_direct_array_with(length, |fast, index| {
        let (name, _) = names.next().ok_or("native filter name list truncated")?;
        let value = fast
            .publish_direct_string_bytes(name.as_bytes())
            .map_err(|_| "native filter name publication failed")?;
        Ok(php_jit::JitNativeDirectArrayEntry {
            key: i64::try_from(index).unwrap_or(i64::MAX),
            value,
        })
    })
    .ok()
    .map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_filter_has_var_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(source) = exact_native_integer(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(name) = fast.native_string_view(argument_1) else {
        return exact_query_contract_violation();
    };
    let Some(root) = exact_filter_source_root(fast, source) else {
        return exact_query_contract_violation();
    };
    let Some(root) = root else {
        return exact_query_return_bool(false);
    };
    let Some(value) = exact_filter_lookup_input(fast, root, name) else {
        return exact_query_contract_violation();
    };
    exact_query_return_bool(value.is_some())
}

pub(crate) extern "C" fn jit_native_filter_input_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
    argument_3: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(source) = exact_native_integer(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(name) = fast.native_string_view(argument_1) else {
        return exact_query_contract_violation();
    };
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let filter = if argument_2 != missing {
        let Some(filter) = exact_native_integer(fast, argument_2) else {
            return exact_query_contract_violation();
        };
        filter
    } else {
        php_runtime::api::FILTER_DEFAULT
    };
    let flags = if argument_3 != missing {
        let Some(flags) = exact_native_integer(fast, argument_3) else {
            return exact_query_contract_violation();
        };
        flags
    } else {
        0
    };
    let Some(root) = exact_filter_source_root(fast, source) else {
        return exact_query_contract_violation();
    };
    let value = match root {
        Some(root) => match exact_filter_lookup_input(fast, root, name) {
            Some(value) => value,
            None => return exact_query_contract_violation(),
        },
        None => None,
    };
    let Some(value) = value else {
        return php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(
            if flags & php_runtime::api::FILTER_NULL_ON_FAILURE != 0 {
                php_jit::JIT_VALUE_FALSE
            } else {
                u32::MAX
            },
        ));
    };
    exact_filter_value(fast, value, filter, flags).map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

fn exact_filter_array_abi_impl(
    fast: &mut NativeRequestFastState,
    source: i64,
    options: i64,
    add_empty: i64,
) -> php_jit::JitNativeControlResult {
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let add_empty = if add_empty != missing {
        let Some(add_empty) = exact_native_boolean_flag(fast, add_empty) else {
            return exact_query_contract_violation();
        };
        add_empty
    } else {
        true
    };
    let filtered = if options != missing && fast.native_direct_array_entries(options).is_some() {
        exact_filter_array_specs(fast, source, options, add_empty)
    } else {
        let filter = if options != missing {
            let Some(filter) = exact_native_integer(fast, options) else {
                return exact_query_contract_violation();
            };
            filter
        } else {
            php_runtime::api::FILTER_DEFAULT
        };
        exact_filter_array_single(fast, source, filter)
    };
    filtered.map_or_else(
        exact_query_contract_violation,
        php_jit::JitNativeControlResult::returning,
    )
}

pub(crate) extern "C" fn jit_native_filter_var_array_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    if fast.native_direct_array_entries(argument_0).is_none() {
        return exact_query_contract_violation();
    }
    exact_filter_array_abi_impl(fast, argument_0, argument_1, argument_2)
}

pub(crate) extern "C" fn jit_native_filter_input_array_abi(
    runtime: *mut NativeRequestFastState,
    argument_0: i64,
    argument_1: i64,
    argument_2: i64,
) -> php_jit::JitNativeControlResult {
    #[allow(unsafe_code)]
    // Safety: the compiled ABI passes the request-owned fast state for this synchronous call.
    let fast = unsafe { &mut *runtime };
    let Some(source) = exact_native_integer(fast, argument_0) else {
        return exact_query_contract_violation();
    };
    let Some(root) = exact_filter_source_root(fast, source) else {
        return exact_query_contract_violation();
    };
    let Some(root) = root else {
        return php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX));
    };
    let Some(entries) = fast.native_direct_array_entries(root) else {
        return exact_query_contract_violation();
    };
    if entries.is_empty() {
        return php_jit::JitNativeControlResult::returning(php_jit::jit_encode_constant(u32::MAX));
    }
    exact_filter_array_abi_impl(fast, root, argument_1, argument_2)
}
