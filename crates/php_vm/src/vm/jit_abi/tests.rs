use super::baseline_native_builtins::format_native_php_diagnostic;
use super::{dereference_native_callable_value, native_backtrace_frame};

#[test]
#[allow(unsafe_code)]
fn fixed_callable_plan_publishes_walk_reference_and_return_contract() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrParam, IrReturnType, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_939));
    let file = builder.add_file("native-walk-callable-plan.php");
    let span = IrSpan::new(file, 0, 16);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let entry_block = builder.append_block(entry);
    builder.terminate_return(entry, entry_block, None, span);
    builder.set_entry(entry);

    let callback = builder.start_function("walk_callback", FunctionFlags::default(), span);
    for (index, by_ref, type_) in [
        (0, true, Some(IrReturnType::Int)),
        (1, false, None),
        (2, false, Some(IrReturnType::Int)),
    ] {
        let local = builder.intern_local(callback, format!("argument_{index}"));
        builder.push_param(
            callback,
            IrParam {
                name: format!("argument_{index}"),
                local,
                required: true,
                default: None,
                type_,
                by_ref,
                variadic: false,
                attributes: Vec::new(),
            },
        );
    }
    builder.set_return_type(callback, Some(IrReturnType::String));
    let callback_block = builder.append_block(callback);
    let returned = builder.intern_constant(php_ir::IrConstant::String("ok".to_owned()));
    builder.terminate_return(
        callback,
        callback_block,
        Some(php_ir::Operand::Constant(returned)),
        span,
    );
    builder.register_function_name("walk_callback", callback);

    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let plan = super::native_fixed_callable_plan(&compiled, callback, false)
        .expect("walk callback has one fixed native contract");
    assert_eq!(plan.visible_arity, 3);
    assert!(plan.first_parameter_by_reference);
    assert!(!plan.returns_int);
    assert!(plan.returns_string);
    assert!(plan.returns_releasable_scalar);
    let owner = super::NativePreparedCallableOwner::user_function(
        Box::from(&b"walk_callback"[..]),
        Some(plan),
    );
    assert_ne!(
        owner.native_view.flags
            & php_jit::JIT_NATIVE_PREPARED_CALLABLE_FIRST_PARAMETER_BY_REFERENCE,
        0
    );
    assert_ne!(
        owner.native_view.flags & php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_RELEASABLE_SCALAR,
        0
    );
    assert_ne!(
        owner.native_view.flags & php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_STRING,
        0
    );

    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );
    let runtime = context.fast_state;
    let _activation = super::activate_native_context(&mut context);
    let fast = unsafe { &mut *runtime };
    let name = fast
        .publish_direct_string_bytes(b"walk_callback")
        .expect("direct callback name");
    let callable = fast
        .acquire_direct_callable(name)
        .expect("exact callable acquisition")
        .expect("same-unit callable");
    let (_, slot) = fast.direct_slot(callable).expect("prepared callable slot");
    let acquired = unsafe { &*(slot.aux as usize as *const super::NativePreparedCallableOwner) };
    assert_eq!(acquired.native_view.function_id, callback.raw());
    assert_eq!(acquired.native_view.reserved, 3);
    assert_ne!(
        acquired.native_view.flags
            & php_jit::JIT_NATIVE_PREPARED_CALLABLE_FIRST_PARAMETER_BY_REFERENCE,
        0
    );
    assert_ne!(
        acquired.native_view.flags
            & php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_RELEASABLE_SCALAR,
        0
    );
    assert_ne!(
        acquired.native_view.flags & php_jit::JIT_NATIVE_PREPARED_CALLABLE_RETURNS_STRING,
        0
    );
}

fn native_serialized_test_bytes(
    fast: &super::NativeRequestFastState,
    encoded: i64,
) -> Option<Vec<u8>> {
    let length = fast.native_serialize_output_length(encoded)?;
    let mut output = vec![0_u8; length];
    fast.native_serialize_into(encoded, &mut output)
        .then_some(output)
}

#[test]
fn repeated_native_metadata_publication_keeps_current_exception_routes() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_940));
    let file = builder.add_file("native-exception-route-publication.php");
    let span = IrSpan::new(file, 0, 16);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );

    context
        .trusted_exception_route_entries
        .push(php_jit::JitNativeExceptionRouteEntry {
            layout_id: 71,
            resume_id: 72,
            pending_status: 73,
        });
    context
        .prepare_published_native_metadata()
        .expect("unchanged native metadata remains published");
    assert_eq!(context.trusted_exception_route_entries[0].layout_id, 71);

    context.external_signature_epoch = context.external_signature_epoch.saturating_add(1);
    context
        .prepare_published_native_metadata()
        .expect("a declaration epoch refreshes exception routes");
    assert!(context.trusted_exception_route_entries.is_empty());
    assert_eq!(
        context.trusted_exception_route_symbol_epoch,
        context.external_signature_epoch
    );
}

#[test]
fn exact_execution_poll_uses_only_published_deadline_capability() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_941));
    let file = builder.add_file("native-exact-deadline-poll.php");
    let span = IrSpan::new(file, 0, 16);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let options = super::super::VmOptions {
        runtime_context: php_runtime::api::RuntimeContext::controlled_cli(
            "native-exact-deadline-poll.php",
            Vec::new(),
        )
        .with_execution_time_limit(Some(std::time::Duration::ZERO)),
        ..super::super::VmOptions::default()
    };
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );
    let runtime = context.fast_state;
    let _activation = super::activate_native_context(&mut context);

    let status = super::baseline_runtime_ops::jit_native_execution_poll_abi(runtime);

    assert_eq!(status, php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32);
    assert_eq!(
        context
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.id()),
        Some("E_PHP_VM_EXECUTION_TIMEOUT")
    );
}

#[test]
fn exact_configuration_family_mutates_the_shared_request_state_without_value_materialization() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_949));
    let file = builder.add_file("native-exact-configuration.php");
    let span = IrSpan::new(file, 0, 16);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );
    let display_name = context
        .encode_direct_string_bytes(b"display_errors")
        .expect("configuration name fits the native arena");
    let disabled = context
        .encode_direct_string_bytes(b"0")
        .expect("configuration value fits the native arena");
    let include_path_bytes = b"/tmp/phrust-native-a:/tmp/phrust-native-b";
    let include_path = context
        .encode_direct_string_bytes(include_path_bytes)
        .expect("include path fits the native arena");
    let timezone_bytes = b"Europe/Berlin";
    let timezone = context
        .encode_direct_string_bytes(timezone_bytes)
        .expect("timezone fits the native arena");
    let runtime = context.fast_state;
    let _activation = super::activate_native_context(&mut context);

    let display_result = crate::native_exact::exact_call_dispatch::jit_native_ini_set_abi(
        runtime,
        display_name,
        disabled,
    );
    assert_eq!(display_result.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let previous_display = unsafe {
        (&*runtime)
            .native_string_view(display_result.value)
            .expect("ini_set returns the previous direct string")
            .to_vec()
    };
    assert_eq!(previous_display, b"1");
    assert_eq!(context.ini_registry.get("display_errors"), Some("0"));
    assert!(!context.display_errors);

    let include_result = crate::native_exact::exact_call_dispatch::jit_native_set_include_path_abi(
        runtime,
        include_path,
    );
    assert_eq!(include_result.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        context.ini_registry.get("include_path"),
        Some(std::str::from_utf8(include_path_bytes).expect("test path is UTF-8"))
    );
    assert_eq!(
        context.include_path.as_slice(),
        [
            std::path::PathBuf::from("/tmp/phrust-native-a"),
            std::path::PathBuf::from("/tmp/phrust-native-b"),
        ]
    );

    let timezone_result =
        crate::native_exact::exact_call_dispatch::jit_native_date_default_timezone_set_abi(
            runtime, timezone,
        );
    assert_eq!(timezone_result.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        timezone_result.value,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE)
    );
    assert_eq!(context.default_timezone, "Europe/Berlin");

    let timezone_get =
        crate::native_exact::exact_call_dispatch::jit_native_date_default_timezone_get_abi(runtime);
    assert_eq!(timezone_get.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let published_timezone = unsafe {
        (&*runtime)
            .native_string_view(timezone_get.value)
            .expect("timezone get publishes a direct native string")
            .to_vec()
    };
    assert_eq!(published_timezone, timezone_bytes);
    assert!(context.diagnostic.is_none());
}

#[test]
fn exact_http_response_family_mutates_and_projects_only_native_request_state() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_950));
    let file = builder.add_file("native-exact-http-response.php");
    let span = IrSpan::new(file, 0, 16);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );
    let first = context
        .encode_direct_string_bytes(b"X-Phrust: one")
        .expect("first header fits the native arena");
    let second = context
        .encode_direct_string_bytes(b"X-Phrust: two")
        .expect("second header fits the native arena");
    let header_name = context
        .encode_direct_string_bytes(b"X-Phrust")
        .expect("header name fits the native arena");
    let runtime = context.fast_state;
    let _activation = super::activate_native_context(&mut context);

    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let first_result = crate::native_exact::exact_call_dispatch::jit_native_header_abi(
        runtime, first, missing, missing,
    );
    assert_eq!(first_result.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(first_result.value, php_jit::jit_encode_constant(u32::MAX));
    let second_result = crate::native_exact::exact_call_dispatch::jit_native_header_abi(
        runtime,
        second,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE),
        201,
    );
    assert_eq!(second_result.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(context.http_response.status_code, 201);
    assert_eq!(context.http_response.headers.len(), 2);

    let listed = crate::native_exact::exact_call_dispatch::jit_native_headers_list_abi(runtime);
    assert_eq!(listed.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let serialized_headers = unsafe {
        native_serialized_test_bytes(&*runtime, listed.value)
            .expect("headers_list publishes one authoritative direct array")
    };
    assert_eq!(
        serialized_headers,
        b"a:2:{i:0;s:13:\"X-Phrust: one\";i:1;s:13:\"X-Phrust: two\";}"
    );

    let response_code =
        crate::native_exact::exact_call_dispatch::jit_native_http_response_code_abi(runtime, 204);
    assert_eq!(response_code.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(response_code.value, 201);
    assert_eq!(context.http_response.status_code, 204);

    let sent = crate::native_exact::exact_call_dispatch::jit_native_headers_sent_abi(runtime);
    assert_eq!(sent.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        sent.value,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE)
    );

    let removed = crate::native_exact::exact_call_dispatch::jit_native_header_remove_abi(
        runtime,
        header_name,
    );
    assert_eq!(removed.status, php_jit::JitCallStatus::RETURN);
    assert!(context.http_response.headers.is_empty());
    assert!(context.diagnostic.is_none());
}

#[test]
fn exact_cookie_family_consumes_seven_arguments_and_direct_options_arrays() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_951));
    let file = builder.add_file("native-exact-cookie.php");
    let span = IrSpan::new(file, 0, 16);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );
    let encoded_name = context
        .encode_direct_string_bytes(b"session")
        .expect("cookie name fits the native arena");
    let encoded_value = context
        .encode_direct_string_bytes(b"a b")
        .expect("cookie value fits the native arena");
    let path = context
        .encode_direct_string_bytes(b"/")
        .expect("cookie path fits the native arena");
    let empty = context
        .encode_direct_string_bytes(b"")
        .expect("empty cookie domain fits the native arena");
    let raw_name = context
        .encode_direct_string_bytes(b"raw")
        .expect("raw cookie name fits the native arena");
    let raw_value = context
        .encode_direct_string_bytes(b"a-b")
        .expect("raw cookie value fits the native arena");
    let samesite_key = context
        .encode_direct_string_bytes(b"samesite")
        .expect("SameSite key fits the native arena");
    let samesite_value = context
        .encode_direct_string_bytes(b"Lax")
        .expect("SameSite value fits the native arena");
    let secure_key = context
        .encode_direct_string_bytes(b"secure")
        .expect("secure key fits the native arena");
    let direct_options = context
        .publish_owned_direct_array_entries(vec![
            php_jit::JitNativeDirectArrayEntry {
                key: samesite_key,
                value: samesite_value,
            },
            php_jit::JitNativeDirectArrayEntry {
                key: secure_key,
                value: php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE),
            },
        ])
        .expect("cookie options fit the native array arena");
    let runtime = context.fast_state;
    let _activation = super::activate_native_context(&mut context);

    let encoded = crate::native_exact::exact_call_dispatch::jit_native_setcookie_abi(
        runtime,
        encoded_name,
        encoded_value,
        0,
        path,
        empty,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE),
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE),
    );
    assert_eq!(encoded.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        encoded.value,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE)
    );

    let raw = crate::native_exact::exact_call_dispatch::jit_native_setrawcookie_abi(
        runtime,
        raw_name,
        raw_value,
        direct_options,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING),
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING),
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING),
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING),
    );
    assert_eq!(raw.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        raw.value,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE)
    );
    assert_eq!(
        context.http_response.headers_list(),
        [
            "Set-Cookie: session=a%20b; Path=/; Secure; HttpOnly",
            "Set-Cookie: raw=a-b; Secure; SameSite=Lax",
        ]
    );
    assert!(context.diagnostic.is_none());
}

#[test]
fn exact_clock_family_publishes_only_authoritative_native_results() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_952));
    let file = builder.add_file("native-exact-clock.php");
    let span = IrSpan::new(file, 0, 16);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );
    let runtime = context.fast_state;
    let _activation = super::activate_native_context(&mut context);

    let time = crate::native_exact::exact_call_dispatch::jit_native_time_abi(runtime);
    assert_eq!(time.status, php_jit::JitCallStatus::RETURN);
    assert!(
        time.value > 1_700_000_000,
        "time() must remain an immediate native integer"
    );

    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let microtime =
        crate::native_exact::exact_call_dispatch::jit_native_microtime_abi(runtime, missing);
    assert_eq!(microtime.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let microtime_bytes = unsafe {
        (&*runtime)
            .native_string_view(microtime.value)
            .expect("microtime() publishes one native string")
    };
    assert!(microtime_bytes.starts_with(b"0."));
    assert!(microtime_bytes.contains(&b' '));

    let microtime_float = crate::native_exact::exact_call_dispatch::jit_native_microtime_abi(
        runtime,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE),
    );
    assert_eq!(microtime_float.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let microtime_float = unsafe {
        (&*runtime)
            .native_printf_scalar(microtime_float.value)
            .expect("microtime(true) publishes one native float")
    };
    assert!(matches!(
        microtime_float,
        php_runtime::api::NativePrintfScalar::Float(value) if value > 1_700_000_000.0
    ));

    let hrtime = crate::native_exact::exact_call_dispatch::jit_native_hrtime_abi(runtime, missing);
    assert_eq!(hrtime.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let serialized_hrtime = unsafe {
        native_serialized_test_bytes(&*runtime, hrtime.value)
            .expect("hrtime() publishes one authoritative packed array")
    };
    assert!(serialized_hrtime.starts_with(b"a:2:{i:0;i:"));

    let hrtime_number = crate::native_exact::exact_call_dispatch::jit_native_hrtime_abi(
        runtime,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE),
    );
    assert_eq!(hrtime_number.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let hrtime_number = unsafe {
        (&*runtime)
            .native_printf_scalar(hrtime_number.value)
            .expect("hrtime(true) publishes one native integer")
    };
    assert!(matches!(
        hrtime_number,
        php_runtime::api::NativePrintfScalar::Int(value) if value > 0
    ));
    assert!(context.diagnostic.is_none());
}

#[test]
fn root_deployment_attachment_publishes_its_dynamic_execution_scope() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_942));
    let file = builder.add_file("native-root-execution-scope.php");
    let span = IrSpan::new(file, 0, 16);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );

    context.attach_root_deployment_image(compiled.clone());

    assert_eq!(context.current_dynamic_unit, Some(0));
    let scope = context
        .native_execution_scopes
        .get(context.current_native_execution_scope as usize - 1)
        .expect("attached deployment keeps a published execution scope");
    assert_eq!(
        scope.unit,
        Some(0),
        "closures created by the root deployment must retain its unit-local function owner"
    );
}

#[test]
fn transition_runtime_view_disambiguates_equal_cross_unit_function_ids() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let compiled_unit = |unit_id, source: &str| {
        let mut builder = IrBuilder::new(UnitId::new(unit_id));
        let file = builder.add_file(source);
        let span = IrSpan::new(file, 0, 16);
        let entry = builder.start_function("main", FunctionFlags::default(), span);
        let block = builder.append_block(entry);
        builder.terminate_return(entry, block, None, span);
        builder.set_entry(entry);
        crate::compiled_unit::CompiledUnit::new(builder.finish())
    };
    let root = compiled_unit(9_946, "native-transition-root.php");
    let linked = compiled_unit(9_947, "native-transition-linked.php");
    assert_eq!(root.unit().entry.raw(), linked.unit().entry.raw());
    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &root,
        root.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );
    context.attach_root_deployment_image(root.clone());
    let linked_unit = super::cold_dynamic_units::register_native_dynamic_unit(
        &mut context,
        linked,
        super::NativeIncludeExports::default(),
    )
    .expect("linked unit registration");
    let linked_view = *context.dynamic_units[linked_unit].published_runtime_view;
    let state = php_jit::JitDeoptState {
        function_id: 0,
        runtime_view: linked_view,
        ..php_jit::JitDeoptState::default()
    };
    let fallback = super::NativeExecutionTarget {
        unit: Some(0),
        function: php_ir::FunctionId::new(0),
        called_class: None,
        scope_class: None,
    };

    let target = context
        .native_execution_target_from_state(&state, Some(&fallback))
        .expect("captured linked runtime view resolves its unit");

    assert_eq!(
        target.unit,
        Some(linked_unit),
        "equal dense FunctionIds must not redirect a linked continuation into the caller unit"
    );
}

#[test]
fn tier_transition_reconciles_borrowed_and_owned_snapshot_values() {
    let transition = |owned_locals, owned_registers| php_jit::JitNativeTransitionMetadata {
        function: php_ir::FunctionId::new(3),
        native_version: 0,
        continuation_id: 7,
        resume_id: 7,
        span: php_ir::IrSpan::new(php_ir::FileId::new(0), 0, 0),
        live_locals: vec![php_ir::LocalId::new(2)],
        live_registers: vec![php_ir::RegId::new(11)],
        owned_locals,
        owned_registers,
        result_register: None,
    };
    let source = transition(Vec::new(), Vec::new());
    let target = transition(vec![php_ir::LocalId::new(2)], vec![php_ir::RegId::new(11)]);
    let mut state = php_jit::JitDeoptState::default();
    state.mark_local_initialized(php_ir::LocalId::new(2));
    state.slots[2] = 0x1_0000_0042;
    state.initialized_register_mask = 1;
    state.register_ids[0] = 11;
    state.registers[0] = 0x1_0000_0043;

    let (retain, release) = super::native_transition_owner_adjustments(&source, &target, &state);
    assert_eq!(retain, [0x1_0000_0042, 0x1_0000_0043]);
    assert!(release.is_empty());

    let (retain, release) = super::native_transition_owner_adjustments(&target, &source, &state);
    assert!(retain.is_empty());
    assert_eq!(release, [0x1_0000_0042, 0x1_0000_0043]);
}

#[test]
fn tier_transition_remaps_registers_by_identity() {
    let target = php_jit::JitNativeTransitionMetadata {
        function: php_ir::FunctionId::new(4),
        native_version: 0,
        continuation_id: 103,
        resume_id: 103,
        span: php_ir::IrSpan::new(php_ir::FileId::new(0), 0, 0),
        live_locals: Vec::new(),
        live_registers: vec![php_ir::RegId::new(2191), php_ir::RegId::new(2192)],
        owned_locals: Vec::new(),
        owned_registers: Vec::new(),
        result_register: Some(php_ir::RegId::new(2192)),
    };
    let mut state = php_jit::JitDeoptState {
        function_id: 4,
        continuation_id: 103,
        initialized_register_mask: 0b11,
        ..php_jit::JitDeoptState::default()
    };
    state.register_ids[0] = 2192;
    state.registers[0] = 92;
    state.register_ids[1] = 2191;
    state.registers[1] = 91;

    let remapped = super::remap_native_transition_registers(&target, &state).unwrap();

    assert_eq!(remapped.initialized_register_mask, 0b11);
    assert_eq!(&remapped.register_ids[..2], &[2191, 2192]);
    assert_eq!(&remapped.registers[..2], &[91, 92]);
}

#[test]
fn native_root_mutation_invalidates_cross_unit_graph_cache() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_943));
    let file = builder.add_file("native-root-mutation.php");
    let span = IrSpan::new(file, 0, 16);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );

    context.cross_unit_stable_values.extend([7, 11]);
    *context.native_root_mutation_pending = 1;
    context.consume_native_root_mutation();

    assert!(
        context.cross_unit_stable_values.is_empty(),
        "a native store may have inserted a new unit-local literal"
    );
    assert_eq!(*context.native_root_mutation_pending, 0);
}

#[test]
fn cold_reference_materialization_is_republished_before_native_reentry() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_945));
    let file = builder.add_file("native-reference-reentry.php");
    let span = IrSpan::new(file, 0, 16);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );
    let reference = php_runtime::api::ReferenceCell::new(php_runtime::api::Value::Int(41));
    let encoded = context
        .encode_native_reference_owner(reference)
        .expect("reference enters the authoritative native plane");
    let index =
        super::NativeRequestColdState::direct_value_index(encoded).expect("direct reference slot");

    let runtime = context.fast_state;
    let _activation = super::activate_native_context(&mut context);
    super::with_baseline_native_context_for(runtime, "reference_reentry_test", |context| {
        let php_runtime::api::Value::Reference(materialized) = context
            .decode_baseline_value(encoded)
            .expect("explicit cold boundary materializes reference identity")
        else {
            panic!("cold boundary lost reference identity");
        };
        materialized.set(php_runtime::api::Value::Int(42));
        assert_eq!(
            context.direct_value_slots[index].kind,
            php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
        );
        assert!(
            context
                .baseline_values
                .materialized_direct_references
                .contains(&index)
        );
    })
    .expect("baseline request context");

    assert!(
        context
            .baseline_values
            .materialized_direct_references
            .is_empty()
    );
    assert_eq!(
        context.direct_value_slots[index].kind,
        php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR
    );
    assert_eq!(context.direct_reference_payload(encoded), Some(42));
}

#[test]
fn shutdown_object_sweep_balances_native_receiver_ownership() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_944));
    let file = builder.add_file("native-shutdown-object-sweep.php");
    let span = IrSpan::new(file, 0, 16);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );
    let class = php_runtime::api::ClassEntry {
        name: "PlainShutdownObject".to_owned().into(),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: php_runtime::api::ClassFlags {
            has_complete_method_table: true,
            ..php_runtime::api::ClassFlags::default()
        },
    };
    let objects = (0..3)
        .map(|_| php_runtime::api::ObjectRef::new(&class))
        .collect::<Vec<_>>();
    let encoded = objects
        .iter()
        .cloned()
        .map(|object| {
            context
                .encode_native_object_owner(object)
                .expect("plain object enters the authoritative native plane")
        })
        .collect::<Vec<_>>();
    let indices = encoded
        .iter()
        .map(|value| {
            php_jit::jit_decode_runtime_value(*value)
                .expect("direct object runtime index")
                .checked_sub(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
                .expect("direct object slot index") as usize
        })
        .collect::<Vec<_>>();
    assert!(
        indices
            .iter()
            .all(|index| context.direct_value_slots[*index].refcount == 1)
    );

    context
        .run_shutdown_callbacks()
        .expect("native shutdown object sweep");

    assert!(
        indices
            .iter()
            .all(|index| context.direct_value_slots[*index].refcount == 1),
        "shutdown must release each temporary destructor receiver"
    );
    assert!(
        context.destroyed_objects.is_empty(),
        "objects without __destruct need no shutdown publication"
    );
    assert!(context.shutdown_destructor_queue.is_none());

    context
        .run_shutdown_callbacks()
        .expect("repeated shutdown sweep is idempotent");
    assert!(
        indices
            .iter()
            .all(|index| context.direct_value_slots[*index].refcount == 1)
    );

    for value in encoded {
        context
            .release(value)
            .expect("test releases the original native object owner");
    }
}

#[test]
#[allow(unsafe_code)]
fn object_cast_maps_authoritative_array_properties_and_preserves_identity() {
    let mut slots =
        vec![php_jit::JitNativeValueSlot::default(); php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut owners = vec![0_u64; php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let key = b"first";
    let value = b"A";
    let array_value = php_jit::jit_encode_typed_runtime_value(
        php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG,
    );
    let key_value = php_jit::jit_encode_typed_runtime_value(
        php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1,
        php_jit::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let string_value = php_jit::jit_encode_typed_runtime_value(
        php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 2,
        php_jit::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let entries = [
        php_jit::JitNativeDirectArrayEntry {
            key: key_value,
            value: string_value,
        },
        php_jit::JitNativeDirectArrayEntry { key: 7, value: 42 },
    ];
    slots[0] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: php_jit::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
        payload: entries.len() as u64,
        aux: entries.as_ptr() as usize as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    slots[1] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: key.len() as u64,
        aux: key.as_ptr() as usize as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    slots[2] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: value.len() as u64,
        aux: value.as_ptr() as usize as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    let mut next = 3_u32;
    let mut free_head = php_jit::JIT_NATIVE_DIRECT_ARRAY_FREE_NONE;
    let mut reused_bytes = 0_u64;
    let mut fast_state = super::NativeRequestFastState {
        header: php_jit::JitNativeFastStateHeader {
            abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
            flags: 0,
            runtime_view_pointer: 0,
            runtime_view: php_jit::JitNativeRuntimeView {
                abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
                direct_value_slots: slots.as_mut_ptr() as usize as u64,
                direct_value_next: std::ptr::from_mut(&mut next) as usize as u64,
                direct_value_free_head: std::ptr::from_mut(&mut free_head) as usize as u64,
                direct_value_reused_bytes: std::ptr::from_mut(&mut reused_bytes) as usize as u64,
                direct_object_owners: owners.as_mut_ptr() as usize as u64,
                ..php_jit::JitNativeRuntimeView::default()
            },
        },
        ..super::NativeRequestFastState::default()
    };

    let cast = crate::native_exact::exact_runtime_ops::jit_native_object_cast_abi(
        std::ptr::from_mut(&mut fast_state),
        array_value,
    );
    assert_eq!(cast.status, php_jit::JitCallStatus::RETURN);
    let object = fast_state
        .direct_object(cast.value)
        .expect("cast result owns a direct object")
        .clone();
    let layout_id = object.class_layout_epoch();
    assert_eq!(
        object.native_dynamic_property_slot(layout_id, "first"),
        Some(Some(php_runtime::api::NativeDeclaredPropertySlot {
            initialized: 1,
            reserved: 0,
            value: string_value,
        }))
    );
    assert_eq!(
        object.native_dynamic_property_slot(layout_id, "7"),
        Some(Some(php_runtime::api::NativeDeclaredPropertySlot {
            initialized: 1,
            reserved: 0,
            value: 42,
        }))
    );
    let order = object
        .with_native_comparison_view(layout_id, |_, _, _, dynamic_order, _| {
            dynamic_order.to_vec()
        })
        .expect("cast object keeps authoritative native properties");
    assert_eq!(order, ["first", "7"]);
    assert_eq!(slots[2].refcount, 2);

    let identity = crate::native_exact::exact_runtime_ops::jit_native_object_cast_abi(
        std::ptr::from_mut(&mut fast_state),
        cast.value,
    );
    assert_eq!(identity.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(identity.value, cast.value);
    let object_index = php_jit::jit_decode_runtime_value(cast.value)
        .expect("object runtime index")
        .checked_sub(php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE)
        .expect("direct object index") as usize;
    assert_eq!(slots[object_index].refcount, 2);

    for owner in owners.into_iter().filter(|owner| *owner != 0) {
        unsafe {
            drop(Box::from_raw(
                owner as usize as *mut php_runtime::api::ObjectRef,
            ));
        }
    }
}

#[test]
#[allow(unsafe_code)]
fn dynamic_property_slot_resolver_reserves_one_stable_stdclass_tombstone() {
    let class = php_runtime::api::ClassEntry {
        name: "stdClass".to_owned().into(),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: php_runtime::api::ClassFlags {
            has_complete_method_table: true,
            ..php_runtime::api::ClassFlags::default()
        },
    };
    let object = php_runtime::api::ObjectRef::new(&class);
    let layout_id = object.class_layout_epoch();
    let _ = object
        .take_property_slots_for_native(layout_id)
        .expect("fresh stdClass enters native storage");
    object
        .install_native_property_slots(layout_id, Box::new([]), Default::default())
        .expect("stdClass native slots install");

    let property = b"created";
    let mut slots = vec![php_jit::JitNativeValueSlot::default(); 2];
    let (declared_slots, declared_count) = object
        .native_declared_slots_view(layout_id)
        .expect("native declared view");
    slots[0] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT,
        flags: php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION,
        reserved: u32::try_from(declared_count).expect("declared count"),
        payload: layout_id,
        aux: declared_slots as usize as u64,
    };
    slots[1] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: property.len() as u64,
        aux: property.as_ptr() as usize as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    let mut owners = vec![0_u64; slots.len()];
    owners[0] = std::ptr::from_ref(&object) as usize as u64;
    let mut fast_state = super::NativeRequestFastState {
        header: php_jit::JitNativeFastStateHeader {
            abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
            flags: 0,
            runtime_view_pointer: 0,
            runtime_view: php_jit::JitNativeRuntimeView {
                abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
                direct_value_slots: slots.as_mut_ptr() as usize as u64,
                direct_object_owners: owners.as_mut_ptr() as usize as u64,
                ..php_jit::JitNativeRuntimeView::default()
            },
        },
        ..super::NativeRequestFastState::default()
    };
    let object_value = php_jit::jit_encode_typed_runtime_value(
        php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        php_jit::JIT_VALUE_RUNTIME_OBJECT_TAG,
    );
    let property_value = php_jit::jit_encode_typed_runtime_value(
        php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1,
        php_jit::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let absent = crate::native_exact::exact_runtime_ops::jit_native_dynamic_property_test_slot_abi(
        std::ptr::from_mut(&mut fast_state),
        object_value,
        property_value,
    );
    assert_eq!(absent.status, php_jit::JitCallStatus::RETURN);
    let absent_cell = absent.value as usize as *mut php_runtime::api::NativeDeclaredPropertySlot;
    assert!(!absent_cell.is_null());
    assert_eq!(unsafe { *absent_cell }, Default::default());
    let repeated_absent =
        crate::native_exact::exact_runtime_ops::jit_native_dynamic_property_test_slot_abi(
            std::ptr::from_mut(&mut fast_state),
            object_value,
            property_value,
        );
    assert_eq!(repeated_absent.value, absent.value);
    assert_eq!(
        object.native_dynamic_property_slot_location(layout_id, "created"),
        Some(None),
        "non-mutating tests must not reserve a dynamic-property tombstone"
    );

    let first = crate::native_exact::exact_runtime_ops::jit_native_dynamic_property_slot_abi(
        std::ptr::from_mut(&mut fast_state),
        object_value,
        property_value,
    );
    assert_eq!(first.status, php_jit::JitCallStatus::RETURN);
    let cell = first.value as usize as *mut php_runtime::api::NativeDeclaredPropertySlot;
    assert!(!cell.is_null());
    assert_eq!(unsafe { (*cell).initialized }, 0);
    let second = crate::native_exact::exact_runtime_ops::jit_native_dynamic_property_slot_abi(
        std::ptr::from_mut(&mut fast_state),
        object_value,
        property_value,
    );
    assert_eq!(second.value, first.value);
}

#[test]
fn dynamic_property_test_slot_rejects_unpublished_magic_and_visibility_shapes() {
    let magic_class = php_runtime::api::ClassEntry {
        name: "magic_box".to_owned().into(),
        parent: None,
        interfaces: Vec::new(),
        methods: vec![php_runtime::api::ClassMethodEntry {
            name: "__isset".to_owned(),
            origin_class: "magic_box".to_owned(),
            function_id: 1,
            flags: php_runtime::api::ClassMethodFlags::default(),
            attributes: Vec::new(),
        }],
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: php_runtime::api::ClassFlags {
            has_complete_method_table: true,
            ..php_runtime::api::ClassFlags::default()
        },
    };
    let declared_class = php_runtime::api::ClassEntry {
        name: "declared_box".to_owned().into(),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: vec![php_runtime::api::ClassPropertyEntry {
            name: "known".to_owned(),
            default: php_runtime::api::Value::Null,
            type_: None,
            flags: php_runtime::api::ClassPropertyFlags::default(),
            hooks: php_runtime::api::ClassPropertyHooks::default(),
            attributes: Vec::new(),
        }],
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: php_runtime::api::ClassFlags {
            has_complete_method_table: true,
            ..php_runtime::api::ClassFlags::default()
        },
    };
    let magic = php_runtime::api::ObjectRef::new(&magic_class);
    let declared = php_runtime::api::ObjectRef::new(&declared_class);
    let magic_layout = magic.class_layout_epoch();
    let declared_layout = declared.class_layout_epoch();
    let (magic_values, magic_dynamic) = magic
        .take_property_slots_for_native(magic_layout)
        .expect("magic object enters native storage");
    assert!(magic_values.is_empty());
    assert!(magic_dynamic.is_empty());
    magic
        .install_native_property_slots(magic_layout, Box::new([]), Default::default())
        .expect("magic native storage installs");
    let (declared_values, declared_dynamic) = declared
        .take_property_slots_for_native(declared_layout)
        .expect("declared object enters native storage");
    assert_eq!(declared_values, [Some(php_runtime::api::Value::Null)]);
    assert!(declared_dynamic.is_empty());
    declared
        .install_native_property_slots(
            declared_layout,
            Box::new([php_runtime::api::NativeDeclaredPropertySlot {
                initialized: 1,
                reserved: 0,
                value: 0,
            }]),
            Default::default(),
        )
        .expect("declared native storage installs");

    let missing = b"missing";
    let known = b"known";
    let mut slots = vec![php_jit::JitNativeValueSlot::default(); 4];
    let (magic_slots, magic_count) = magic
        .native_declared_slots_view(magic_layout)
        .expect("magic native view");
    let (declared_slots, declared_count) = declared
        .native_declared_slots_view(declared_layout)
        .expect("declared native view");
    slots[0] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT,
        flags: php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION,
        reserved: u32::try_from(magic_count).expect("magic slot count"),
        payload: magic_layout,
        aux: magic_slots as usize as u64,
    };
    slots[1] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT,
        flags: php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION,
        reserved: u32::try_from(declared_count).expect("declared slot count"),
        payload: declared_layout,
        aux: declared_slots as usize as u64,
    };
    slots[2] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: missing.len() as u64,
        aux: missing.as_ptr() as usize as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    slots[3] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: known.len() as u64,
        aux: known.as_ptr() as usize as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    let mut owners = vec![0_u64; slots.len()];
    owners[0] = std::ptr::from_ref(&magic) as usize as u64;
    owners[1] = std::ptr::from_ref(&declared) as usize as u64;
    let mut fast_state = super::NativeRequestFastState {
        header: php_jit::JitNativeFastStateHeader {
            abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
            flags: 0,
            runtime_view_pointer: 0,
            runtime_view: php_jit::JitNativeRuntimeView {
                abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
                direct_value_slots: slots.as_mut_ptr() as usize as u64,
                direct_object_owners: owners.as_mut_ptr() as usize as u64,
                ..php_jit::JitNativeRuntimeView::default()
            },
        },
        ..super::NativeRequestFastState::default()
    };
    let encoded = |index: u32, tag: u64| {
        php_jit::jit_encode_typed_runtime_value(
            php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + index,
            tag,
        )
    };
    let magic_value = encoded(0, php_jit::JIT_VALUE_RUNTIME_OBJECT_TAG);
    let declared_value = encoded(1, php_jit::JIT_VALUE_RUNTIME_OBJECT_TAG);
    let missing_value = encoded(2, php_jit::JIT_VALUE_RUNTIME_STRING_TAG);
    let known_value = encoded(3, php_jit::JIT_VALUE_RUNTIME_STRING_TAG);

    let magic_result =
        crate::native_exact::exact_runtime_ops::jit_native_dynamic_property_test_slot_abi(
            std::ptr::from_mut(&mut fast_state),
            magic_value,
            missing_value,
        );
    assert_eq!(magic_result.status, php_jit::JitCallStatus::ABI_MISMATCH);
    let declared_result =
        crate::native_exact::exact_runtime_ops::jit_native_dynamic_property_test_slot_abi(
            std::ptr::from_mut(&mut fast_state),
            declared_value,
            known_value,
        );
    assert_eq!(declared_result.status, php_jit::JitCallStatus::ABI_MISMATCH);
    let ordinary_missing =
        crate::native_exact::exact_runtime_ops::jit_native_dynamic_property_test_slot_abi(
            std::ptr::from_mut(&mut fast_state),
            declared_value,
            missing_value,
        );
    assert_eq!(ordinary_missing.status, php_jit::JitCallStatus::RETURN);
}

#[test]
fn native_http_build_query_reads_recursive_direct_arrays() {
    let mut slots = vec![php_jit::JitNativeValueSlot::default(); 6];
    let array_value = |index: u32| {
        php_jit::jit_encode_typed_runtime_value(
            php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + index,
            php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG,
        )
    };
    let string_value = |index: u32| {
        php_jit::jit_encode_typed_runtime_value(
            php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + index,
            php_jit::JIT_VALUE_RUNTIME_STRING_TAG,
        )
    };
    let hello = b"hello world";
    let nested_key = b"nested key";
    let skipped_key = b"skip";
    for (index, bytes) in [(2, hello.as_slice()), (3, nested_key), (4, skipped_key)] {
        slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
            flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
            payload: bytes.len() as u64,
            aux: bytes.as_ptr() as usize as u64,
            ..php_jit::JitNativeValueSlot::default()
        };
    }
    let nested = [php_jit::JitNativeDirectArrayEntry {
        key: 1,
        value: php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE),
    }];
    slots[1] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: php_jit::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
        payload: nested.len() as u64,
        aux: nested.as_ptr() as usize as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    let root = [
        php_jit::JitNativeDirectArrayEntry {
            key: 0,
            value: string_value(2),
        },
        php_jit::JitNativeDirectArrayEntry {
            key: string_value(3),
            value: array_value(1),
        },
        php_jit::JitNativeDirectArrayEntry {
            key: string_value(4),
            value: php_jit::jit_encode_constant(u32::MAX),
        },
    ];
    slots[0] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: php_jit::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
        payload: root.len() as u64,
        aux: root.as_ptr() as usize as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    let mut fast_state = super::NativeRequestFastState {
        header: php_jit::JitNativeFastStateHeader {
            abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
            flags: 0,
            runtime_view_pointer: 0,
            runtime_view: php_jit::JitNativeRuntimeView {
                abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
                direct_value_slots: slots.as_mut_ptr() as usize as u64,
                ..php_jit::JitNativeRuntimeView::default()
            },
        },
        ..super::NativeRequestFastState::default()
    };
    let encoded = fast_state
        .native_http_build_query(array_value(0), Some(b"n_"), b";", true)
        .expect("direct query encoding");
    assert_eq!(
        fast_state
            .native_string_view(encoded)
            .expect("direct query result remains authoritative native data"),
        b"n_0=hello%20world;nested%20key%5B1%5D=1"
    );
}

#[test]
fn exact_parse_str_publishes_keyed_native_array_through_direct_reference() {
    let query = b"plain=value&list[]=a&list[]=b&12=numeric&nested[x]=old&nested[x]=new&flip=scalar&flip[child]=nested&collapse[child]=nested&collapse=scalar&extra1=1&extra2=2";
    let replacement_query = b"plain=replaced&next=owner";
    let mut buffers = super::NativeRequestBuffers::default();
    *buffers.direct_value_next = 3;
    buffers.direct_value_slots[0] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: query.len() as u64,
        aux: query.as_ptr() as usize as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    buffers.direct_value_slots[1] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
        flags: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
        payload: php_jit::jit_encode_constant(u32::MAX) as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    buffers.direct_value_slots[2] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: replacement_query.len() as u64,
        aux: replacement_query.as_ptr() as usize as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    let mut ini = php_runtime::api::IniRegistry::default();
    let runtime_view = php_jit::JitNativeRuntimeView {
        abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: buffers.direct_value_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(buffers.direct_value_next.as_mut()) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(buffers.direct_value_free_head.as_mut()) as usize
            as u64,
        direct_value_reused_bytes: std::ptr::from_mut(buffers.direct_value_reused_bytes.as_mut())
            as usize as u64,
        direct_array_states: buffers.direct_array_states.as_mut_ptr() as usize as u64,
        direct_array_entries: buffers.direct_array_entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(buffers.direct_array_next.as_mut()) as usize as u64,
        direct_array_free_heads: buffers.direct_array_free_heads.as_mut_ptr() as usize as u64,
        direct_array_reused_bytes: std::ptr::from_mut(buffers.direct_array_reused_bytes.as_mut())
            as usize as u64,
        direct_string_bytes: buffers.direct_string_bytes.as_mut_ptr() as usize as u64,
        direct_string_next: std::ptr::from_mut(buffers.direct_string_next.as_mut()) as usize as u64,
        direct_string_free_heads: buffers.direct_string_free_heads.as_mut_ptr() as usize as u64,
        direct_string_reused_bytes: std::ptr::from_mut(buffers.direct_string_reused_bytes.as_mut())
            as usize as u64,
        ..php_jit::JitNativeRuntimeView::default()
    };
    let mut fast_state = super::NativeRequestFastState {
        header: php_jit::JitNativeFastStateHeader {
            abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
            flags: 0,
            runtime_view_pointer: 0,
            runtime_view,
        },
        configuration: super::NativeConfigurationCapability {
            ini_registry: std::ptr::from_mut(&mut ini),
            ..super::NativeConfigurationCapability::default()
        },
        ..super::NativeRequestFastState::default()
    };
    let input = php_jit::jit_encode_typed_runtime_value(
        php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        php_jit::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let output_reference = php_jit::jit_encode_typed_runtime_value(
        php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1,
        php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG,
    );
    let replacement_input = php_jit::jit_encode_typed_runtime_value(
        php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 2,
        php_jit::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let result = crate::native_exact::exact_call_dispatch::jit_native_parse_str_abi(
        std::ptr::from_mut(&mut fast_state),
        input,
        output_reference,
    );
    assert_eq!(result.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(result.value, php_jit::jit_encode_constant(u32::MAX));
    let published = buffers.direct_value_slots[1].payload as i64;
    let encoded = fast_state
        .native_http_build_query(published, None, b"&", false)
        .expect("published parse_str array remains authoritative native data");
    assert_eq!(
        fast_state
            .native_string_view(encoded)
            .expect("query result remains authoritative native data"),
        b"plain=value&list%5B0%5D=a&list%5B1%5D=b&12=numeric&nested%5Bx%5D=new&flip%5Bchild%5D=nested&collapse=scalar&extra1=1&extra2=2"
    );

    let replaced = crate::native_exact::exact_call_dispatch::jit_native_parse_str_abi(
        std::ptr::from_mut(&mut fast_state),
        replacement_input,
        output_reference,
    );
    assert_eq!(
        replaced.status,
        php_jit::JitCallStatus::RETURN,
        "an exact out-parameter must replace its previous direct array owner"
    );
    let published = buffers.direct_value_slots[1].payload as i64;
    let encoded = fast_state
        .native_http_build_query(published, None, b"&", false)
        .expect("replacement array remains authoritative native data");
    assert_eq!(
        fast_state
            .native_string_view(encoded)
            .expect("replacement query result remains authoritative native data"),
        b"plain=replaced&next=owner"
    );
}

#[test]
fn exact_string_rewrite_and_json_consume_authoritative_native_arrays() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_954));
    let file = builder.add_file("native-exact-array-builtins.php");
    let span = IrSpan::new(file, 0, 16);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );
    let subject = context
        .encode_direct_string_bytes(b"hello $slug")
        .expect("subject fits the native string arena");
    let key = context
        .encode_direct_string_bytes(b"$slug")
        .expect("replacement key fits the native string arena");
    let value = context
        .encode_direct_string_bytes(b"native")
        .expect("replacement value fits the native string arena");
    let replacements = context
        .publish_owned_direct_array_entries(vec![php_jit::JitNativeDirectArrayEntry { key, value }])
        .expect("replacement map fits the native array arena");
    let flagged_string = context
        .encode_direct_string_bytes("ä/x".as_bytes())
        .expect("flagged value fits the native string arena");
    let numeric_string = context
        .encode_direct_string_bytes(b"1.0")
        .expect("numeric JSON string fits the native string arena");
    let invalid_string = context
        .encode_direct_string_bytes(b"a\xffb")
        .expect("invalid UTF-8 JSON string fits the native byte arena");
    let whole_float = context
        .encode_native_float_owner(php_runtime::api::FloatValue::from_f64(1.0))
        .expect("float fits the native value arena");
    let packed = context
        .publish_owned_direct_array_entries(vec![
            php_jit::JitNativeDirectArrayEntry {
                key: 0,
                value: flagged_string,
            },
            php_jit::JitNativeDirectArrayEntry {
                key: 1,
                value: whole_float,
            },
        ])
        .expect("packed JSON input fits the native array arena");
    let object_class = php_runtime::api::ClassEntry {
        name: "JsonNativeObject".to_owned().into(),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: php_runtime::api::ClassFlags::default(),
    };
    let object = php_runtime::api::ObjectRef::new(&object_class);
    object.set_property(
        "path",
        php_runtime::api::Value::String(php_runtime::api::PhpString::from_test_str("ä/x")),
    );
    object.set_property("count", php_runtime::api::Value::Int(2));
    let object = context
        .encode_native_object_owner(object)
        .expect("ordinary JSON object enters the native property plane");
    let runtime = context.fast_state;
    let _activation = super::activate_native_context(&mut context);

    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let rewritten = crate::native_exact::exact_call_dispatch::jit_native_strtr_abi(
        runtime,
        subject,
        replacements,
        missing,
    );
    assert_eq!(rewritten.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let rewritten_bytes = unsafe {
        (&*runtime)
            .native_string_view(rewritten.value)
            .expect("strtr publishes native bytes")
    };
    assert_eq!(rewritten_bytes, b"hello native");

    let encoded = crate::native_exact::exact_call_dispatch::jit_native_json_encode_abi(
        runtime,
        replacements,
        0,
        512,
    );
    assert_eq!(encoded.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let encoded_bytes = unsafe {
        (&*runtime)
            .native_string_view(encoded.value)
            .expect("json_encode publishes native bytes")
    };
    assert_eq!(encoded_bytes, br#"{"$slug":"native"}"#);

    let flags = php_runtime::api::NATIVE_JSON_UNESCAPED_SLASHES
        | php_runtime::api::NATIVE_JSON_UNESCAPED_UNICODE
        | php_runtime::api::NATIVE_JSON_PRETTY_PRINT
        | php_runtime::api::NATIVE_JSON_PRESERVE_ZERO_FRACTION;
    let encoded = crate::native_exact::exact_call_dispatch::jit_native_json_encode_abi(
        runtime, packed, flags, 512,
    );
    assert_eq!(encoded.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let encoded_bytes = unsafe {
        (&*runtime)
            .native_string_view(encoded.value)
            .expect("flagged json_encode publishes native bytes")
    };
    assert_eq!(encoded_bytes, "[\n    \"ä/x\",\n    1.0\n]".as_bytes());

    let encoded = crate::native_exact::exact_call_dispatch::jit_native_json_encode_abi(
        runtime,
        packed,
        flags | php_runtime::api::NATIVE_JSON_FORCE_OBJECT,
        512,
    );
    assert_eq!(encoded.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let encoded_bytes = unsafe {
        (&*runtime)
            .native_string_view(encoded.value)
            .expect("forced-object json_encode publishes native bytes")
    };
    assert_eq!(
        encoded_bytes,
        "{\n    \"0\": \"ä/x\",\n    \"1\": 1.0\n}".as_bytes()
    );

    let encoded = crate::native_exact::exact_call_dispatch::jit_native_json_encode_abi(
        runtime, object, flags, 512,
    );
    assert_eq!(encoded.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let encoded_bytes = unsafe {
        (&*runtime)
            .native_string_view(encoded.value)
            .expect("ordinary native object json_encode publishes bytes")
    };
    assert_eq!(
        encoded_bytes,
        "{\n    \"path\": \"ä/x\",\n    \"count\": 2\n}".as_bytes()
    );

    let encoded = crate::native_exact::exact_call_dispatch::jit_native_json_encode_abi(
        runtime,
        numeric_string,
        php_runtime::api::NATIVE_JSON_NUMERIC_CHECK,
        512,
    );
    assert_eq!(encoded.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let encoded_bytes = unsafe {
        (&*runtime)
            .native_string_view(encoded.value)
            .expect("numeric-check json_encode publishes bytes")
    };
    assert_eq!(encoded_bytes, b"1.0");

    let encoded = crate::native_exact::exact_call_dispatch::jit_native_json_encode_abi(
        runtime,
        invalid_string,
        php_runtime::api::NATIVE_JSON_INVALID_UTF8_SUBSTITUTE,
        512,
    );
    assert_eq!(encoded.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let encoded_bytes = unsafe {
        (&*runtime)
            .native_string_view(encoded.value)
            .expect("invalid UTF-8 substitution publishes bytes")
    };
    assert_eq!(encoded_bytes, b"\"a\\ufffdb\"");
}

#[test]
fn exact_by_value_readers_follow_authoritative_native_references() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_960));
    let file = builder.add_file("native-exact-reference-values.php");
    let span = IrSpan::new(file, 0, 1);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );
    let known = context
        .encode_direct_string_bytes(b"same")
        .expect("known string fits the native arena");
    let known = context
        .encode_direct_reference_payload_owned(known)
        .expect("known reference fits the native arena");
    let user = context
        .encode_direct_string_bytes(b"same")
        .expect("user string fits the native arena");
    let user = context
        .encode_direct_reference_payload_owned(user)
        .expect("user reference fits the native arena");
    let array = context
        .publish_owned_direct_array_entries(vec![php_jit::JitNativeDirectArrayEntry {
            key: 0,
            value: 42,
        }])
        .expect("array fits the native arena");
    let array = context
        .encode_direct_reference_payload_owned(array)
        .expect("array reference fits the native arena");
    let runtime = context.fast_state;
    let _activation = super::activate_native_context(&mut context);

    let equal =
        crate::native_exact::exact_call_dispatch::jit_native_hash_equals_abi(runtime, known, user);
    assert_eq!(equal.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        equal.value,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE)
    );
    #[allow(unsafe_code)]
    let fast = unsafe { &*runtime };
    assert_eq!(fast.native_string_view(known), Some(b"same".as_slice()));
    assert_eq!(
        fast.native_direct_array_entries(array)
            .expect("by-value array reference stays on the native plane"),
        &[php_jit::JitNativeDirectArrayEntry { key: 0, value: 42 }]
    );
}

#[test]
fn exact_preg_match_publishes_and_replaces_named_native_captures() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_955));
    let file = builder.add_file("native-exact-named-preg-match.php");
    let span = IrSpan::new(file, 0, 16);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );
    let pattern = context
        .encode_direct_string_bytes(br#"~(?<word>[a-z]+)~"#)
        .expect("pattern fits the native string arena");
    let subject = context
        .encode_direct_string_bytes(b"native")
        .expect("subject fits the native string arena");
    let output_reference = context
        .encode_direct_reference_payload_owned(php_jit::jit_encode_constant(u32::MAX))
        .expect("capture reference fits the native value arena");
    let runtime = context.fast_state;
    let _activation = super::activate_native_context(&mut context);

    for _ in 0..2 {
        let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
        let result = crate::native_exact::exact_call_dispatch::jit_native_preg_match_abi(
            runtime,
            pattern,
            subject,
            output_reference,
            missing,
            missing,
        );
        assert_eq!(result.status, php_jit::JitCallStatus::RETURN);
        assert_eq!(result.value, 1);
        let published = context
            .direct_reference_payload(output_reference)
            .expect("capture reference remains direct");
        #[allow(unsafe_code)]
        let serialized = unsafe {
            native_serialized_test_bytes(&*runtime, published)
                .expect("named captures remain authoritative native data")
        };
        assert_eq!(
            serialized,
            br#"a:3:{i:0;s:6:"native";s:4:"word";s:6:"native";i:1;s:6:"native";}"#
        );
    }
}

#[test]
fn exact_serialization_roundtrip_never_materializes_the_value_plane() {
    let key = b"x";
    let mut buffers = super::NativeRequestBuffers::default();
    *buffers.direct_value_next = 2;
    *buffers.direct_array_next = 4;
    buffers.direct_array_entries[0] = php_jit::JitNativeDirectArrayEntry { key: 0, value: 7 };
    buffers.direct_array_entries[1] = php_jit::JitNativeDirectArrayEntry {
        key: php_jit::jit_encode_typed_runtime_value(
            php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1,
            php_jit::JIT_VALUE_RUNTIME_STRING_TAG,
        ),
        value: php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE),
    };
    buffers.direct_value_slots[0] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: php_jit::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
        reserved: 4,
        payload: 2,
        aux: buffers.direct_array_entries.as_ptr() as usize as u64,
    };
    buffers.direct_value_slots[1] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: key.len() as u64,
        aux: key.as_ptr() as usize as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    let mut ini = php_runtime::api::IniRegistry::default();
    let runtime_view = php_jit::JitNativeRuntimeView {
        abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: buffers.direct_value_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(buffers.direct_value_next.as_mut()) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(buffers.direct_value_free_head.as_mut()) as usize
            as u64,
        direct_value_reused_bytes: std::ptr::from_mut(buffers.direct_value_reused_bytes.as_mut())
            as usize as u64,
        direct_array_states: buffers.direct_array_states.as_mut_ptr() as usize as u64,
        direct_array_entries: buffers.direct_array_entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(buffers.direct_array_next.as_mut()) as usize as u64,
        direct_array_free_heads: buffers.direct_array_free_heads.as_mut_ptr() as usize as u64,
        direct_array_reused_bytes: std::ptr::from_mut(buffers.direct_array_reused_bytes.as_mut())
            as usize as u64,
        direct_string_bytes: buffers.direct_string_bytes.as_mut_ptr() as usize as u64,
        direct_string_next: std::ptr::from_mut(buffers.direct_string_next.as_mut()) as usize as u64,
        direct_string_free_heads: buffers.direct_string_free_heads.as_mut_ptr() as usize as u64,
        direct_string_reused_bytes: std::ptr::from_mut(buffers.direct_string_reused_bytes.as_mut())
            as usize as u64,
        ..php_jit::JitNativeRuntimeView::default()
    };
    let mut fast = super::NativeRequestFastState {
        header: php_jit::JitNativeFastStateHeader {
            abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
            flags: 0,
            runtime_view_pointer: 0,
            runtime_view,
        },
        configuration: super::NativeConfigurationCapability {
            ini_registry: std::ptr::from_mut(&mut ini),
            ..super::NativeConfigurationCapability::default()
        },
        ..super::NativeRequestFastState::default()
    };
    let input = php_jit::jit_encode_typed_runtime_value(
        php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG,
    );
    let serialized = crate::native_exact::exact_call_dispatch::jit_native_serialize_abi(
        std::ptr::from_mut(&mut fast),
        input,
    );
    assert_eq!(serialized.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        fast.native_string_view(serialized.value)
            .expect("serialize publishes a direct native string"),
        b"a:2:{i:0;i:7;s:1:\"x\";b:1;}"
    );

    let decoded = crate::native_exact::exact_call_dispatch::jit_native_unserialize_abi(
        std::ptr::from_mut(&mut fast),
        serialized.value,
    );
    assert_eq!(decoded.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        native_serialized_test_bytes(&fast, decoded.value)
            .expect("unserialize publishes an authoritative direct array"),
        b"a:2:{i:0;i:7;s:1:\"x\";b:1;}"
    );
}

#[test]
fn exact_tokenizer_publishes_lexer_records_directly_into_native_slots() {
    let source = b"<?php echo 1;";
    let expected = php_runtime::api::tokenize(
        std::str::from_utf8(source).expect("fixture source is UTF-8"),
        0,
    )
    .expect("fixture tokenizes without baseline diagnostics");
    let mut buffers = super::NativeRequestBuffers::default();
    *buffers.direct_value_next = 1;
    buffers.direct_value_slots[0] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: source.len() as u64,
        aux: source.as_ptr() as usize as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    let runtime_view = php_jit::JitNativeRuntimeView {
        abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: buffers.direct_value_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(buffers.direct_value_next.as_mut()) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(buffers.direct_value_free_head.as_mut()) as usize
            as u64,
        direct_value_reused_bytes: std::ptr::from_mut(buffers.direct_value_reused_bytes.as_mut())
            as usize as u64,
        direct_array_states: buffers.direct_array_states.as_mut_ptr() as usize as u64,
        direct_array_entries: buffers.direct_array_entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(buffers.direct_array_next.as_mut()) as usize as u64,
        direct_array_free_heads: buffers.direct_array_free_heads.as_mut_ptr() as usize as u64,
        direct_array_reused_bytes: std::ptr::from_mut(buffers.direct_array_reused_bytes.as_mut())
            as usize as u64,
        direct_string_bytes: buffers.direct_string_bytes.as_mut_ptr() as usize as u64,
        direct_string_next: std::ptr::from_mut(buffers.direct_string_next.as_mut()) as usize as u64,
        direct_string_free_heads: buffers.direct_string_free_heads.as_mut_ptr() as usize as u64,
        direct_string_reused_bytes: std::ptr::from_mut(buffers.direct_string_reused_bytes.as_mut())
            as usize as u64,
        ..php_jit::JitNativeRuntimeView::default()
    };
    let mut fast = super::NativeRequestFastState {
        header: php_jit::JitNativeFastStateHeader {
            abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
            runtime_view,
            ..php_jit::JitNativeFastStateHeader::default()
        },
        ..super::NativeRequestFastState::default()
    };
    let source = php_jit::jit_encode_typed_runtime_value(
        php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        php_jit::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let result = crate::native_exact::exact_call_dispatch::jit_native_token_get_all_abi(
        std::ptr::from_mut(&mut fast),
        source,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING),
    );
    assert_eq!(result.status, php_jit::JitCallStatus::RETURN);
    let outer = fast
        .native_direct_array_entries(result.value)
        .expect("token_get_all publishes one direct native array")
        .to_vec();
    assert_eq!(outer.len(), expected.len());
    for (index, (entry, token)) in outer.iter().zip(&expected).enumerate() {
        assert_eq!(entry.key, index as i64);
        if token.named {
            let fields = fast
                .native_direct_array_entries(entry.value)
                .expect("named token publishes a direct native tuple");
            assert_eq!(fields.len(), 3);
            assert_eq!(fields[0].value, token.id);
            assert_eq!(
                fast.native_string_view(fields[1].value)
                    .expect("named token text remains a native string"),
                token.text.as_bytes()
            );
            assert_eq!(fields[2].value, i64::from(token.line));
        } else {
            assert_eq!(
                fast.native_string_view(entry.value)
                    .expect("symbol token remains a native string"),
                token.text.as_bytes()
            );
        }
    }

    let named = expected
        .iter()
        .find(|token| token.named)
        .expect("fixture has a named token");
    let name = crate::native_exact::exact_call_dispatch::jit_native_token_name_abi(
        std::ptr::from_mut(&mut fast),
        named.id,
    );
    assert_eq!(name.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        fast.native_string_view(name.value)
            .expect("token_name publishes a native string"),
        php_runtime::api::token_name_for_id(named.id)
            .expect("named token has a public name")
            .as_bytes()
    );
}

#[test]
fn exact_mbstring_family_keeps_strings_and_request_encoding_native() {
    let source_bytes = "Grüße".as_bytes();
    let binary = b"8bit";
    let mut buffers = super::NativeRequestBuffers::default();
    *buffers.direct_value_next = 2;
    for (index, bytes) in [source_bytes, binary.as_slice()].into_iter().enumerate() {
        buffers.direct_value_slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
            flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
            payload: bytes.len() as u64,
            aux: bytes.as_ptr() as usize as u64,
            ..php_jit::JitNativeValueSlot::default()
        };
    }
    let runtime_view = php_jit::JitNativeRuntimeView {
        abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: buffers.direct_value_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(buffers.direct_value_next.as_mut()) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(buffers.direct_value_free_head.as_mut()) as usize
            as u64,
        direct_value_reused_bytes: std::ptr::from_mut(buffers.direct_value_reused_bytes.as_mut())
            as usize as u64,
        direct_array_states: buffers.direct_array_states.as_mut_ptr() as usize as u64,
        direct_array_entries: buffers.direct_array_entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(buffers.direct_array_next.as_mut()) as usize as u64,
        direct_array_free_heads: buffers.direct_array_free_heads.as_mut_ptr() as usize as u64,
        direct_array_reused_bytes: std::ptr::from_mut(buffers.direct_array_reused_bytes.as_mut())
            as usize as u64,
        direct_string_bytes: buffers.direct_string_bytes.as_mut_ptr() as usize as u64,
        direct_string_next: std::ptr::from_mut(buffers.direct_string_next.as_mut()) as usize as u64,
        direct_string_free_heads: buffers.direct_string_free_heads.as_mut_ptr() as usize as u64,
        direct_string_reused_bytes: std::ptr::from_mut(buffers.direct_string_reused_bytes.as_mut())
            as usize as u64,
        ..php_jit::JitNativeRuntimeView::default()
    };
    let mut internal_encoding = "UTF-8".to_owned();
    let mut substitute = php_runtime::api::MbSubstituteCharacter::default();
    let mut fast = super::NativeRequestFastState {
        header: php_jit::JitNativeFastStateHeader {
            abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
            runtime_view,
            ..php_jit::JitNativeFastStateHeader::default()
        },
        mbstring: super::NativeMbstringCapability {
            internal_encoding: std::ptr::from_mut(&mut internal_encoding),
            substitute_character: std::ptr::from_mut(&mut substitute),
        },
        ..super::NativeRequestFastState::default()
    };
    let source = php_jit::jit_encode_typed_runtime_value(
        php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        php_jit::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let binary = php_jit::jit_encode_typed_runtime_value(
        php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1,
        php_jit::JIT_VALUE_RUNTIME_STRING_TAG,
    );
    let runtime = std::ptr::from_mut(&mut fast);

    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let length = crate::native_exact::exact_call_dispatch::jit_native_mb_strlen_abi(
        runtime, source, missing,
    );
    assert_eq!(length.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(length.value, 5);

    let uppercase = crate::native_exact::exact_call_dispatch::jit_native_mb_strtoupper_abi(
        runtime, source, missing,
    );
    assert_eq!(uppercase.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        fast.native_string_view(uppercase.value)
            .expect("mb_strtoupper publishes an authoritative native string"),
        "GRÜSSE".as_bytes()
    );

    let changed = crate::native_exact::exact_call_dispatch::jit_native_mb_internal_encoding_abi(
        runtime, binary,
    );
    assert_eq!(changed.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(internal_encoding, "8BIT");
    let byte_length = crate::native_exact::exact_call_dispatch::jit_native_mb_strlen_abi(
        runtime, source, missing,
    );
    assert_eq!(byte_length.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(byte_length.value, source_bytes.len() as i64);
}

#[test]
fn exact_bcmath_family_shares_scale_and_publishes_native_decimal_strings() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_952));
    let file = builder.add_file("native-exact-bcmath.php");
    let span = IrSpan::new(file, 0, 16);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );
    let left = context
        .encode_direct_string_bytes(b"1.2")
        .expect("left decimal fits native arena");
    let right = context
        .encode_direct_string_bytes(b"3.45")
        .expect("right decimal fits native arena");
    let scale = context
        .encode_native_int(3)
        .expect("scale fits native arena");
    let runtime = context.fast_state;
    let _activation = super::activate_native_context(&mut context);

    let changed = crate::native_exact::exact_call_dispatch::jit_native_bcscale_abi(runtime, scale);
    assert_eq!(changed.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(changed.value, 0);

    let sum = crate::native_exact::exact_call_dispatch::jit_native_bcadd_abi(
        runtime,
        left,
        right,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING),
    );
    assert_eq!(sum.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let sum_bytes = unsafe { &*runtime }
        .native_string_view(sum.value)
        .expect("bcadd publishes an authoritative native decimal string");
    assert_eq!(sum_bytes, b"4.650");
}

#[test]
fn exact_filter_family_reads_prepublished_request_roots_and_native_values() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_953));
    let file = builder.add_file("native-exact-filter.php");
    let span = IrSpan::new(file, 0, 16);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let mut options = super::super::VmOptions::default();
    options.runtime_context.env = std::sync::Arc::new(vec![(
        "QUERY_STRING".to_owned(),
        "age=42&name=Alice".to_owned(),
    )]);
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );
    let age = context
        .encode_direct_string_bytes(b"age")
        .expect("input name fits native arena");
    let email = context
        .encode_direct_string_bytes(b" person @example.com ")
        .expect("filter source fits native arena");
    let int_name = context
        .encode_direct_string_bytes(b"int")
        .expect("filter name fits native arena");
    let runtime = context.fast_state;
    let _activation = super::activate_native_context(&mut context);

    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let present =
        crate::native_exact::exact_call_dispatch::jit_native_filter_has_var_abi(runtime, 1, age);
    assert_eq!(present.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        present.value,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE)
    );

    let validated = crate::native_exact::exact_call_dispatch::jit_native_filter_input_abi(
        runtime, 1, age, 257, missing,
    );
    assert_eq!(validated.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(validated.value, 42);

    let sanitized = crate::native_exact::exact_call_dispatch::jit_native_filter_var_abi(
        runtime, email, 517, missing,
    );
    assert_eq!(sanitized.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let sanitized = unsafe { &*runtime }
        .native_string_view(sanitized.value)
        .expect("filter_var publishes native string bytes");
    assert_eq!(sanitized, b"person@example.com");

    let input_array = crate::native_exact::exact_call_dispatch::jit_native_filter_input_array_abi(
        runtime, 1, missing, missing,
    );
    assert_eq!(input_array.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let entries = unsafe { &*runtime }
        .native_direct_array_entries(input_array.value)
        .expect("filter_input_array publishes a native array");
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|entry| {
        #[allow(unsafe_code)]
        let fast = unsafe { &*runtime };
        fast.native_string_view(entry.key) == Some(b"age")
            && fast.native_string_view(entry.value) == Some(b"42")
    }));

    let required_array = crate::native_exact::exact_call_dispatch::jit_native_filter_var_abi(
        runtime,
        input_array.value,
        257,
        php_runtime::api::FILTER_REQUIRE_ARRAY,
    );
    assert_eq!(required_array.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let required_entries = unsafe { &*runtime }
        .native_direct_array_entries(required_array.value)
        .expect("FILTER_REQUIRE_ARRAY keeps the native array shape");
    assert!(required_entries.iter().any(|entry| {
        #[allow(unsafe_code)]
        let fast = unsafe { &*runtime };
        fast.native_string_view(entry.key) == Some(b"age") && entry.value == 42
    }));

    let filtered_array = crate::native_exact::exact_call_dispatch::jit_native_filter_var_array_abi(
        runtime,
        input_array.value,
        php_runtime::api::FILTER_DEFAULT,
        missing,
    );
    assert_eq!(filtered_array.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let filtered_entries = unsafe { &*runtime }
        .native_direct_array_entries(filtered_array.value)
        .expect("filter_var_array publishes a native array");
    assert_eq!(filtered_entries.len(), 2);

    let filter_id =
        crate::native_exact::exact_call_dispatch::jit_native_filter_id_abi(runtime, int_name);
    assert_eq!(filter_id.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(filter_id.value, 257);

    let names = crate::native_exact::exact_call_dispatch::jit_native_filter_list_abi(runtime);
    assert_eq!(names.status, php_jit::JitCallStatus::RETURN);
    #[allow(unsafe_code)]
    let names = unsafe { &*runtime }
        .native_direct_array_entries(names.value)
        .expect("filter_list publishes a native packed array");
    assert_eq!(names.len(), php_runtime::api::native_filter_names().len());
}

#[test]
#[allow(unsafe_code)]
fn native_unserialize_publishes_nested_duplicate_keys_in_place() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_953));
    let file = builder.add_file("native-unserialize-direct.php");
    let span = IrSpan::new(file, 0, 16);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );
    let runtime = context.fast_state;
    let _activation = super::activate_native_context(&mut context);
    let serialized = b"a:3:{s:1:\"x\";i:1;s:1:\"y\";a:1:{i:0;s:3:\"old\";}s:1:\"x\";a:2:{i:0;s:3:\"new\";i:0;s:5:\"final\";}}";
    let parsed = crate::native_exact::NativeSerializedParser {
        bytes: serialized,
        offset: 0,
        parsed_items: 0,
    }
    .parse(unsafe { &mut *runtime })
    .expect("native unserialize publishes its direct result");

    let fast = unsafe { &*runtime };
    let entries = fast
        .native_direct_array_entries(parsed)
        .expect("top-level result is an authoritative native array");
    assert_eq!(entries.len(), 2);
    assert_eq!(
        fast.native_string_view(entries[0].key),
        Some(b"x".as_slice())
    );
    assert_eq!(
        fast.native_string_view(entries[1].key),
        Some(b"y".as_slice())
    );
    let nested = fast
        .native_direct_array_entries(entries[0].value)
        .expect("duplicate x keeps the final nested array value");
    assert_eq!(nested.len(), 1);
    assert_eq!(
        fast.native_string_view(nested[0].value),
        Some(b"final".as_slice())
    );
}

#[test]
#[allow(unsafe_code)]
fn exact_session_family_keeps_lifecycle_payload_and_commit_native() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_954));
    let file = builder.add_file("native-exact-session.php");
    let span = IrSpan::new(file, 0, 16);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );
    let app_name = context
        .encode_direct_string_bytes(b"APPSESSID")
        .expect("session name fits native arena");
    let session_id = context
        .encode_direct_string_bytes(b"request-id")
        .expect("session id fits native arena");
    let limiter = context
        .encode_direct_string_bytes(b"private")
        .expect("cache limiter fits native arena");
    let save_path = context
        .encode_direct_string_bytes(b"/tmp/session")
        .expect("save path fits native arena");
    let files = context
        .encode_direct_string_bytes(b"files")
        .expect("module name fits native arena");
    let cookie_path = context
        .encode_direct_string_bytes(b"/app")
        .expect("cookie path fits native arena");
    let cookie_domain = context
        .encode_direct_string_bytes(b"example.test")
        .expect("cookie domain fits native arena");
    let empty = context
        .encode_direct_string_bytes(b"")
        .expect("empty path fits native arena");
    let serialized = context
        .encode_direct_string_bytes(b"foo|s:3:\"bar\";")
        .expect("serialized session fits native arena");
    let runtime = context.fast_state;
    let _activation = super::activate_native_context(&mut context);
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);

    let status = crate::native_exact::exact_call_dispatch::jit_native_session_status_abi(runtime);
    assert_eq!(status.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(status.value, php_runtime::api::PHP_SESSION_NONE);

    let old_name =
        crate::native_exact::exact_call_dispatch::jit_native_session_name_abi(runtime, app_name);
    assert_eq!(old_name.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        unsafe { &*runtime }.native_string_view(old_name.value),
        Some(b"PHPSESSID".as_slice())
    );

    let old_id =
        crate::native_exact::exact_call_dispatch::jit_native_session_id_abi(runtime, session_id);
    assert_eq!(old_id.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        unsafe { &*runtime }.native_string_view(old_id.value),
        Some(b"".as_slice())
    );

    let old_expire =
        crate::native_exact::exact_call_dispatch::jit_native_session_cache_expire_abi(runtime, 60);
    assert_eq!(old_expire.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(old_expire.value, 180);

    let old_limiter =
        crate::native_exact::exact_call_dispatch::jit_native_session_cache_limiter_abi(
            runtime, limiter,
        );
    assert_eq!(old_limiter.status, php_jit::JitCallStatus::RETURN);

    let old_path = crate::native_exact::exact_call_dispatch::jit_native_session_save_path_abi(
        runtime, save_path,
    );
    assert_eq!(old_path.status, php_jit::JitCallStatus::RETURN);

    let old_module = crate::native_exact::exact_call_dispatch::jit_native_session_module_name_abi(
        runtime, files,
    );
    assert_eq!(old_module.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        unsafe { &*runtime }.native_string_view(old_module.value),
        Some(b"files".as_slice())
    );

    let set_cookie =
        crate::native_exact::exact_call_dispatch::jit_native_session_set_cookie_params_abi(
            runtime,
            3_600,
            cookie_path,
            cookie_domain,
            php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE),
            php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE),
        );
    assert_eq!(set_cookie.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        set_cookie.value,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE)
    );

    let cookie_params =
        crate::native_exact::exact_call_dispatch::jit_native_session_get_cookie_params_abi(runtime);
    assert_eq!(cookie_params.status, php_jit::JitCallStatus::RETURN);
    let cookie_entries = unsafe { &*runtime }
        .native_direct_array_entries(cookie_params.value)
        .expect("cookie parameters publish a native array");
    assert_eq!(cookie_entries.len(), 7);
    assert!(cookie_entries.iter().any(|entry| {
        let fast = unsafe { &*runtime };
        fast.native_string_view(entry.key) == Some(b"lifetime") && entry.value == 3_600
    }));

    let shutdown =
        crate::native_exact::exact_call_dispatch::jit_native_session_register_shutdown_abi(runtime);
    assert_eq!(shutdown.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(shutdown.value, php_jit::jit_encode_constant(u32::MAX));

    let _ =
        crate::native_exact::exact_call_dispatch::jit_native_session_save_path_abi(runtime, empty);

    let lifecycle =
        crate::native_exact::exact_call_dispatch::jit_native_session_start_abi(runtime, missing);
    assert_eq!(lifecycle.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        lifecycle.value,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE)
    );

    let decoded = crate::native_exact::exact_call_dispatch::jit_native_session_decode_abi(
        runtime, serialized,
    );
    assert_eq!(decoded.status, php_jit::JitCallStatus::RETURN);
    let encoded = crate::native_exact::exact_call_dispatch::jit_native_session_encode_abi(runtime);
    assert_eq!(encoded.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        unsafe { &*runtime }.native_string_view(encoded.value),
        Some(b"foo|s:3:\"bar\";".as_slice())
    );

    let committed =
        crate::native_exact::exact_call_dispatch::jit_native_session_commit_abi(runtime);
    assert_eq!(committed.status, php_jit::JitCallStatus::RETURN);
    let restarted =
        crate::native_exact::exact_call_dispatch::jit_native_session_start_abi(runtime, missing);
    assert_eq!(restarted.status, php_jit::JitCallStatus::RETURN);
    let unset = crate::native_exact::exact_call_dispatch::jit_native_session_unset_abi(runtime);
    assert_eq!(unset.status, php_jit::JitCallStatus::RETURN);
    let aborted = crate::native_exact::exact_call_dispatch::jit_native_session_abort_abi(runtime);
    assert_eq!(aborted.status, php_jit::JitCallStatus::RETURN);

    let payload = unsafe { &*runtime }
        .native_session_payload()
        .expect("session reference remains native");
    let payload = unsafe { &*runtime }
        .native_direct_array_entries(payload)
        .expect("restored commit is an authoritative native array");
    assert_eq!(payload.len(), 1);
    assert_eq!(
        unsafe { &*runtime }.native_string_view(payload[0].key),
        Some(b"foo".as_slice())
    );
    assert_eq!(
        unsafe { &*runtime }.native_string_view(payload[0].value),
        Some(b"bar".as_slice())
    );
}

#[test]
fn exact_key_preserving_sorts_reorder_authoritative_entries_in_place() {
    let mut slots = vec![php_jit::JitNativeValueSlot::default(); 8];
    let bytes: [&[u8]; 6] = [b"a", b"c", b"b", b"item10", b"item2", b"item1"];
    for (index, value) in bytes.iter().enumerate() {
        slots[index + 2] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
            flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
            payload: value.len() as u64,
            aux: value.as_ptr() as usize as u64,
            ..php_jit::JitNativeValueSlot::default()
        };
    }
    let string_value = |index: u32| {
        php_jit::jit_encode_typed_runtime_value(
            php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + index,
            php_jit::JIT_VALUE_RUNTIME_STRING_TAG,
        )
    };
    let mut entries = vec![
        php_jit::JitNativeDirectArrayEntry {
            key: string_value(2),
            value: string_value(5),
        },
        php_jit::JitNativeDirectArrayEntry {
            key: string_value(3),
            value: string_value(6),
        },
        php_jit::JitNativeDirectArrayEntry {
            key: string_value(4),
            value: string_value(7),
        },
    ];
    let array = php_jit::jit_encode_typed_runtime_value(
        php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG,
    );
    slots[0] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: php_jit::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
        reserved: 4,
        payload: entries.len() as u64,
        aux: entries.as_mut_ptr() as usize as u64,
    };
    slots[1] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
        flags: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
        reserved: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED,
        payload: array as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    let reference = php_jit::jit_encode_typed_runtime_value(
        php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + 1,
        php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG,
    );
    let mut fast_state = super::NativeRequestFastState {
        header: php_jit::JitNativeFastStateHeader {
            abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
            flags: 0,
            runtime_view_pointer: 0,
            runtime_view: php_jit::JitNativeRuntimeView {
                abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
                direct_value_slots: slots.as_mut_ptr() as usize as u64,
                ..php_jit::JitNativeRuntimeView::default()
            },
        },
        ..super::NativeRequestFastState::default()
    };
    let runtime = std::ptr::from_mut(&mut fast_state);
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);
    let result = crate::native_exact::exact_call_dispatch::jit_native_natsort_abi(
        runtime, reference, missing,
    );
    assert_eq!(result.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        entries
            .iter()
            .map(|entry| fast_state
                .native_string_view(entry.key)
                .expect("string key"))
            .collect::<Vec<_>>(),
        [b"b".as_slice(), b"c".as_slice(), b"a".as_slice()]
    );
    assert_eq!(
        php_jit::jit_native_direct_array_cursor(slots[0].flags),
        Some(0)
    );

    let result = crate::native_exact::exact_call_dispatch::jit_native_krsort_abi(
        runtime, reference, missing,
    );
    assert_eq!(result.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        entries
            .iter()
            .map(|entry| fast_state
                .native_string_view(entry.key)
                .expect("string key"))
            .collect::<Vec<_>>(),
        [b"c".as_slice(), b"b".as_slice(), b"a".as_slice()]
    );
}

#[test]
fn exact_count_family_traverses_only_authoritative_direct_arrays() {
    let array = |index: u32| {
        php_jit::jit_encode_typed_runtime_value(
            php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + index,
            php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG,
        )
    };
    let mut slots = vec![php_jit::JitNativeValueSlot::default(); 2];
    let mut nested_entries = vec![
        php_jit::JitNativeDirectArrayEntry { key: 0, value: 10 },
        php_jit::JitNativeDirectArrayEntry { key: 1, value: 11 },
        php_jit::JitNativeDirectArrayEntry { key: 2, value: 12 },
    ];
    let mut outer_entries = vec![
        php_jit::JitNativeDirectArrayEntry { key: 0, value: 9 },
        php_jit::JitNativeDirectArrayEntry {
            key: 1,
            value: array(1),
        },
    ];
    slots[0] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: php_jit::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
        payload: outer_entries.len() as u64,
        aux: outer_entries.as_mut_ptr() as usize as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    slots[1] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: php_jit::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
        payload: nested_entries.len() as u64,
        aux: nested_entries.as_mut_ptr() as usize as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    let mut fast_state = super::NativeRequestFastState {
        header: php_jit::JitNativeFastStateHeader {
            abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
            runtime_view: php_jit::JitNativeRuntimeView {
                abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
                direct_value_slots: slots.as_mut_ptr() as usize as u64,
                ..php_jit::JitNativeRuntimeView::default()
            },
            ..php_jit::JitNativeFastStateHeader::default()
        },
        ..super::NativeRequestFastState::default()
    };
    let runtime = std::ptr::from_mut(&mut fast_state);
    let missing = php_jit::jit_encode_constant(php_jit::JIT_VALUE_ARGUMENT_MISSING);

    let shallow =
        crate::native_exact::exact_runtime_ops::jit_native_count_abi(runtime, array(0), missing);
    assert_eq!(shallow.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(shallow.value, 2);

    let recursive =
        crate::native_exact::exact_runtime_ops::jit_native_sizeof_abi(runtime, array(0), 1);
    assert_eq!(recursive.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(recursive.value, 5);

    let invalid_mode =
        crate::native_exact::exact_runtime_ops::jit_native_count_abi(runtime, array(0), 2);
    assert_eq!(invalid_mode.status, php_jit::JitCallStatus::ABI_MISMATCH);
    let scalar = crate::native_exact::exact_runtime_ops::jit_native_count_abi(runtime, 42, missing);
    assert_eq!(scalar.status, php_jit::JitCallStatus::ABI_MISMATCH);

    nested_entries[0].value = array(0);
    let recursive_cycle =
        crate::native_exact::exact_runtime_ops::jit_native_count_abi(runtime, array(0), 1);
    assert_eq!(recursive_cycle.status, php_jit::JitCallStatus::ABI_MISMATCH);
}

#[test]
fn baseline_count_scalar_failure_remains_typed_throw_control() {
    use php_ir::builder::IrBuilder;
    use php_ir::{FunctionFlags, IrSpan, UnitId};

    let mut builder = IrBuilder::new(UnitId::new(9_955));
    let file = builder.add_file("baseline-count-scalar.php");
    let span = IrSpan::new(file, 0, 1);
    let entry = builder.start_function("main", FunctionFlags::default(), span);
    let block = builder.append_block(entry);
    builder.terminate_return(entry, block, None, span);
    builder.set_entry(entry);
    let compiled = crate::compiled_unit::CompiledUnit::new(builder.finish());
    let options = super::super::VmOptions::default();
    let worker = super::super::VmWorkerState::default();
    let mut context = super::NativeRequestOwner::new(
        &compiled,
        compiled.artifact_identity(),
        &options,
        &worker,
        php_runtime::api::OutputBuffer::new(),
        std::sync::Arc::new(std::collections::BTreeMap::new()),
    );
    let source = php_ir::Instruction {
        id: php_ir::InstrId::new(0),
        span,
        kind: php_ir::InstructionKind::Nop,
    };

    let outcome = super::baseline_native_builtins::execute_baseline_native_builtin_control(
        &mut context,
        "sizeof",
        &[42],
        &source,
        None,
        None,
    );
    assert!(matches!(
        outcome,
        Err(super::NativeCallControl::Throw { ref class, .. }) if class == "TypeError"
    ));

    let sizeof_entry = php_runtime::api::BuiltinRegistry::new()
        .get("sizeof")
        .expect("sizeof builtin");
    let prepared = crate::compiled_unit::PreparedNativeBuiltin::for_entry(sizeof_entry, 1, true);
    let prepared_outcome = super::baseline_native_builtins::execute_baseline_native_builtin_control(
        &mut context,
        "sizeof",
        &[42],
        &source,
        None,
        Some(prepared),
    );
    assert!(matches!(
        prepared_outcome,
        Err(super::NativeCallControl::Throw { ref class, .. }) if class == "TypeError"
    ));

    let runtime = context.fast_state;
    let _activation = super::activate_native_context(&mut context);
    let mut out = php_jit::JitCallResult::default();
    let status = super::baseline_call_dispatch::finish_native_dispatch_outcome(
        runtime,
        Some(outcome),
        Some(span),
        std::ptr::null_mut(),
        std::ptr::from_mut(&mut out),
    );
    assert_eq!(status, php_jit::JitCallStatus::THROW.0 as i32);
    assert_ne!(out.value.payload, 0);
    assert!(super::baseline_call_support::native_catch_matches(
        &mut context,
        &["TypeError".to_owned()],
        out.value.payload as i64,
    ));

    let mut direct_out = php_jit::JitCallResult::default();
    let mut transition_state = php_jit::JitDeoptState::default();
    let direct_status = super::baseline_call_dispatch::jit_baseline_native_builtin_dispatch_abi(
        runtime,
        sizeof_entry.dense_id(),
        entry.raw(),
        span.file.raw(),
        span.start,
        span.end,
        [42_i64].as_ptr(),
        1,
        std::ptr::null(),
        0,
        std::ptr::from_mut(&mut transition_state),
        std::ptr::from_mut(&mut direct_out),
    );
    assert_eq!(
        direct_status,
        php_jit::JitCallStatus::THROW.0 as i32,
        "{direct_out:#?}"
    );
    assert_ne!(direct_out.value.payload, 0);
}

#[test]
fn exact_array_multisort_applies_one_permutation_to_all_native_arrays() {
    let mut slots = vec![php_jit::JitNativeValueSlot::default(); 4];
    let mut primary_entries = vec![
        php_jit::JitNativeDirectArrayEntry { key: 9, value: 2 },
        php_jit::JitNativeDirectArrayEntry { key: 10, value: 1 },
        php_jit::JitNativeDirectArrayEntry { key: 12, value: 2 },
    ];
    let mut secondary_entries = vec![
        php_jit::JitNativeDirectArrayEntry { key: 4, value: 1 },
        php_jit::JitNativeDirectArrayEntry { key: 7, value: 3 },
        php_jit::JitNativeDirectArrayEntry { key: 8, value: 0 },
    ];
    let array = |index: u32| {
        php_jit::jit_encode_typed_runtime_value(
            php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + index,
            php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG,
        )
    };
    let reference = |index: u32| {
        php_jit::jit_encode_typed_runtime_value(
            php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + index,
            php_jit::JIT_VALUE_RUNTIME_REFERENCE_TAG,
        )
    };
    slots[0] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: php_jit::jit_native_direct_array_flags(Some(13)),
        reserved: 4,
        payload: primary_entries.len() as u64,
        aux: primary_entries.as_mut_ptr() as usize as u64,
    };
    slots[1] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: php_jit::jit_native_direct_array_flags(Some(9)),
        reserved: 4,
        payload: secondary_entries.len() as u64,
        aux: secondary_entries.as_mut_ptr() as usize as u64,
    };
    for (index, array_index) in [(2, 0), (3, 1)] {
        slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_REFERENCE_SCALAR,
            flags: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
            reserved: php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_PUBLISHED,
            payload: array(array_index) as u64,
            ..php_jit::JitNativeValueSlot::default()
        };
    }
    let mut fast_state = super::NativeRequestFastState {
        header: php_jit::JitNativeFastStateHeader {
            abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
            flags: 0,
            runtime_view_pointer: 0,
            runtime_view: php_jit::JitNativeRuntimeView {
                abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
                direct_value_slots: slots.as_mut_ptr() as usize as u64,
                ..php_jit::JitNativeRuntimeView::default()
            },
        },
        ..super::NativeRequestFastState::default()
    };
    let arguments = [reference(2), 4, 1, reference(3), 3, 1];
    let result = crate::native_exact::exact_call_dispatch::jit_native_array_multisort_abi(
        std::ptr::from_mut(&mut fast_state),
        arguments.len() as u32,
        arguments.as_ptr(),
    );
    assert_eq!(result.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        primary_entries,
        [
            php_jit::JitNativeDirectArrayEntry { key: 0, value: 1 },
            php_jit::JitNativeDirectArrayEntry { key: 1, value: 2 },
            php_jit::JitNativeDirectArrayEntry { key: 2, value: 2 },
        ]
    );
    assert_eq!(
        secondary_entries,
        [
            php_jit::JitNativeDirectArrayEntry { key: 0, value: 3 },
            php_jit::JitNativeDirectArrayEntry { key: 1, value: 1 },
            php_jit::JitNativeDirectArrayEntry { key: 2, value: 0 },
        ]
    );
    assert_eq!(
        php_jit::jit_native_direct_array_cursor(slots[0].flags),
        Some(3)
    );
    assert_eq!(
        php_jit::jit_native_direct_array_cursor(slots[1].flags),
        Some(3)
    );
}

#[test]
fn exact_frame_introspection_keeps_arguments_in_the_native_plane() {
    let arguments = [11_i64, 22_i64];
    let current_fixed_arguments = [33_i64, 44_i64];
    let mut buffers = super::NativeRequestBuffers::default();
    let runtime_view = php_jit::JitNativeRuntimeView {
        abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: buffers.direct_value_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(buffers.direct_value_next.as_mut()) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(buffers.direct_value_free_head.as_mut()) as usize
            as u64,
        direct_value_reused_bytes: std::ptr::from_mut(buffers.direct_value_reused_bytes.as_mut())
            as usize as u64,
        direct_array_states: buffers.direct_array_states.as_mut_ptr() as usize as u64,
        direct_array_entries: buffers.direct_array_entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(buffers.direct_array_next.as_mut()) as usize as u64,
        direct_array_free_heads: buffers.direct_array_free_heads.as_mut_ptr() as usize as u64,
        direct_array_reused_bytes: std::ptr::from_mut(buffers.direct_array_reused_bytes.as_mut())
            as usize as u64,
        direct_string_bytes: buffers.direct_string_bytes.as_mut_ptr() as usize as u64,
        direct_string_next: std::ptr::from_mut(buffers.direct_string_next.as_mut()) as usize as u64,
        direct_string_free_heads: buffers.direct_string_free_heads.as_mut_ptr() as usize as u64,
        direct_string_reused_bytes: std::ptr::from_mut(buffers.direct_string_reused_bytes.as_mut())
            as usize as u64,
        active_call_arguments: arguments.as_ptr() as usize as u64,
        active_call_argument_count: arguments.len() as u32,
        active_call_fixed_argument_count: current_fixed_arguments.len() as u32,
        active_call_fixed_arguments: current_fixed_arguments.as_ptr() as usize as u64,
        ..php_jit::JitNativeRuntimeView::default()
    };
    let mut fast = super::NativeRequestFastState {
        header: php_jit::JitNativeFastStateHeader {
            abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
            runtime_view,
            ..php_jit::JitNativeFastStateHeader::default()
        },
        ..super::NativeRequestFastState::default()
    };
    let runtime = std::ptr::from_mut(&mut fast);

    let count = crate::native_exact::exact_call_dispatch::jit_native_func_num_args_abi(runtime);
    assert_eq!(count.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(count.value, 2);

    let argument =
        crate::native_exact::exact_call_dispatch::jit_native_func_get_arg_abi(runtime, 1);
    assert_eq!(argument.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(argument.value, 44);

    let all = crate::native_exact::exact_call_dispatch::jit_native_func_get_args_abi(runtime);
    assert_eq!(all.status, php_jit::JitCallStatus::RETURN);
    let entries = fast
        .native_direct_array_entries(all.value)
        .expect("func_get_args publishes a direct native array");
    assert_eq!(
        entries,
        [
            php_jit::JitNativeDirectArrayEntry { key: 0, value: 33 },
            php_jit::JitNativeDirectArrayEntry { key: 1, value: 44 },
        ]
    );
}

#[test]
fn exact_frame_introspection_reads_segmented_unpack_tail_arguments() {
    let fixed_arguments = [33_i64];
    let variadic_entries = [
        php_jit::JitNativeDirectArrayEntry { key: 0, value: 44 },
        php_jit::JitNativeDirectArrayEntry { key: 1, value: 55 },
        php_jit::JitNativeDirectArrayEntry { key: 2, value: 66 },
    ];
    let variadic_array = php_jit::jit_encode_typed_runtime_value(
        php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE,
        php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG,
    );
    let mut buffers = super::NativeRequestBuffers::default();
    *buffers.direct_value_next = 1;
    buffers.direct_value_slots[0] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: php_jit::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
        payload: variadic_entries.len() as u64,
        aux: variadic_entries.as_ptr() as usize as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    let runtime_view = php_jit::JitNativeRuntimeView {
        abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
        direct_value_slots: buffers.direct_value_slots.as_mut_ptr() as usize as u64,
        direct_value_next: std::ptr::from_mut(buffers.direct_value_next.as_mut()) as usize as u64,
        direct_value_free_head: std::ptr::from_mut(buffers.direct_value_free_head.as_mut()) as usize
            as u64,
        direct_value_reused_bytes: std::ptr::from_mut(buffers.direct_value_reused_bytes.as_mut())
            as usize as u64,
        direct_array_states: buffers.direct_array_states.as_mut_ptr() as usize as u64,
        direct_array_entries: buffers.direct_array_entries.as_mut_ptr() as usize as u64,
        direct_array_next: std::ptr::from_mut(buffers.direct_array_next.as_mut()) as usize as u64,
        direct_array_free_heads: buffers.direct_array_free_heads.as_mut_ptr() as usize as u64,
        direct_array_reused_bytes: std::ptr::from_mut(buffers.direct_array_reused_bytes.as_mut())
            as usize as u64,
        direct_string_bytes: buffers.direct_string_bytes.as_mut_ptr() as usize as u64,
        direct_string_next: std::ptr::from_mut(buffers.direct_string_next.as_mut()) as usize as u64,
        direct_string_free_heads: buffers.direct_string_free_heads.as_mut_ptr() as usize as u64,
        direct_string_reused_bytes: std::ptr::from_mut(buffers.direct_string_reused_bytes.as_mut())
            as usize as u64,
        active_call_arguments: fixed_arguments.as_ptr() as usize as u64,
        active_call_argument_count: 4,
        active_call_fixed_argument_count: fixed_arguments.len() as u32,
        active_call_tail_arguments: variadic_array,
        ..php_jit::JitNativeRuntimeView::default()
    };
    let mut fast = super::NativeRequestFastState {
        header: php_jit::JitNativeFastStateHeader {
            abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
            runtime_view,
            ..php_jit::JitNativeFastStateHeader::default()
        },
        ..super::NativeRequestFastState::default()
    };
    let runtime = std::ptr::from_mut(&mut fast);

    let count = crate::native_exact::exact_call_dispatch::jit_native_func_num_args_abi(runtime);
    assert_eq!(count.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(count.value, 4);

    let argument =
        crate::native_exact::exact_call_dispatch::jit_native_func_get_arg_abi(runtime, 2);
    assert_eq!(argument.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(argument.value, 55);

    let all = crate::native_exact::exact_call_dispatch::jit_native_func_get_args_abi(runtime);
    assert_eq!(all.status, php_jit::JitCallStatus::RETURN);
    let entries = fast
        .native_direct_array_entries(all.value)
        .expect("segmented func_get_args publishes one direct native array");
    assert_eq!(
        entries,
        [
            php_jit::JitNativeDirectArrayEntry { key: 0, value: 33 },
            php_jit::JitNativeDirectArrayEntry { key: 1, value: 44 },
            php_jit::JitNativeDirectArrayEntry { key: 2, value: 55 },
            php_jit::JitNativeDirectArrayEntry { key: 3, value: 66 },
        ]
    );
}

#[test]
fn native_request_pool_reuses_only_reset_worker_owned_buffers() {
    fn assert_send<T: Send>() {}
    assert_send::<super::NativeRequestBuffers>();

    let mut pool = super::NativeRequestPool::default();
    let mut first = pool.checkout(37);
    let direct_value_slots = first.direct_value_slots.as_mut_ptr() as usize;
    let fiber_states = first.fiber_suspension_states.as_mut_ptr() as usize;
    let static_properties = first.static_property_slots.as_mut_ptr() as usize;
    assert!(first.native_call_encoded_scratch.capacity() >= 37);
    first
        .native_call_encoded_scratch
        .extend_from_slice(&[11, 13, 17]);
    first.diagnostic_telemetry.counters.runtime_helper_calls = 23;

    pool.recycle(first);
    assert_eq!(pool.available.len(), 1);

    let mut second = pool.checkout(37);
    assert_eq!(
        second.direct_value_slots.as_mut_ptr() as usize,
        direct_value_slots
    );
    assert_eq!(
        second.fiber_suspension_states.as_mut_ptr() as usize,
        fiber_states
    );
    assert_eq!(
        second.static_property_slots.as_mut_ptr() as usize,
        static_properties
    );
    assert!(second.native_call_encoded_scratch.is_empty());
    assert!(second.native_call_encoded_scratch.capacity() >= 37);
    assert_eq!(*second.direct_value_next, 0);
    assert_eq!(*second.direct_array_next, 0);
    assert_eq!(*second.direct_string_next, 0);
    assert_eq!(*second.fiber_suspension_next, 0);
    assert_eq!(*second.static_property_next, 0);
    assert_eq!(second.native_frame_arena.high_water_bytes(), 0);
    assert_eq!(second.diagnostic_telemetry.counters.runtime_helper_calls, 0);
}

#[test]
fn nested_native_activation_restores_the_outer_fast_state_view() {
    let outer_view = php_jit::JitNativeRuntimeView {
        trusted_function_entries: 0x1110,
        trusted_function_entry_count: 30,
        ..php_jit::JitNativeRuntimeView::default()
    };
    let inner_view = php_jit::JitNativeRuntimeView {
        trusted_function_entries: 0x2220,
        trusted_function_entry_count: 64,
        ..php_jit::JitNativeRuntimeView::default()
    };
    let outer_header = php_jit::JitNativeFastStateHeader {
        abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
        flags: 0,
        runtime_view_pointer: 0,
        runtime_view: outer_view,
    };
    let mut fast_state = super::NativeRequestFastState {
        header: outer_header,
        ..super::NativeRequestFastState::default()
    };
    let outer_cold = super::ActiveBaselineContext {
        cold: 0x1110usize as *mut std::ffi::c_void,
    };
    let inner_cold = super::ActiveBaselineContext {
        cold: 0x2220usize as *mut std::ffi::c_void,
    };
    let previous_baseline_context =
        super::ACTIVE_BASELINE_CONTEXT.with(|active| active.replace(outer_cold));
    let _outer_runtime_view = php_jit::activate_native_runtime_view(outer_view);
    fast_state.header.runtime_view = inner_view;
    super::ACTIVE_BASELINE_CONTEXT.with(|active| active.set(inner_cold));

    let inner = super::NativeRequestActivationGuard {
        _runtime_view: php_jit::activate_native_runtime_view(inner_view),
        fast_state: std::ptr::from_mut(&mut fast_state),
        previous_header: outer_header,
        previous_execution_scope: std::ptr::null(),
        previous_baseline_context: outer_cold,
    };
    drop(inner);

    assert_eq!(
        fast_state.header.runtime_view.trusted_function_entries,
        outer_view.trusted_function_entries
    );
    assert_eq!(
        fast_state.header.runtime_view.trusted_function_entry_count,
        outer_view.trusted_function_entry_count
    );
    assert_eq!(
        super::ACTIVE_BASELINE_CONTEXT
            .with(std::cell::Cell::get)
            .cold,
        outer_cold.cold
    );
    super::ACTIVE_BASELINE_CONTEXT.with(|active| active.set(previous_baseline_context));
}

#[test]
fn exact_native_array_comparison_handlers_traverse_authoritative_entries() {
    let mut slots =
        vec![php_jit::JitNativeValueSlot::default(); php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let array_value = |index: u32| {
        php_jit::jit_encode_typed_runtime_value(
            php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + index,
            php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG,
        )
    };
    let string_value = |index: u32| {
        php_jit::jit_encode_typed_runtime_value(
            php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + index,
            php_jit::JIT_VALUE_RUNTIME_STRING_TAG,
        )
    };
    let float_value = |index: u32| {
        php_jit::jit_encode_typed_runtime_value(
            php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + index,
            php_jit::JIT_VALUE_RUNTIME_FLOAT_TAG,
        )
    };
    let string_two = b"2";
    slots[7] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: string_two.len() as u64,
        aux: string_two.as_ptr() as usize as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    let string_key = b"key";
    slots[8] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_STRING,
        flags: php_jit::JIT_NATIVE_STRING_VIEW_ABI_VERSION,
        payload: string_key.len() as u64,
        aux: string_key.as_ptr() as usize as u64,
        ..php_jit::JitNativeValueSlot::default()
    };
    let ordered = vec![
        php_jit::JitNativeDirectArrayEntry { key: 0, value: 1 },
        php_jit::JitNativeDirectArrayEntry {
            key: 1,
            value: string_value(7),
        },
    ]
    .into_boxed_slice();
    let same = ordered.clone();
    let reordered = vec![
        php_jit::JitNativeDirectArrayEntry {
            key: 1,
            value: string_value(7),
        },
        php_jit::JitNativeDirectArrayEntry { key: 0, value: 1 },
    ]
    .into_boxed_slice();
    let coercive = vec![
        php_jit::JitNativeDirectArrayEntry { key: 0, value: 1 },
        php_jit::JitNativeDirectArrayEntry { key: 1, value: 2 },
    ]
    .into_boxed_slice();
    let lower = vec![php_jit::JitNativeDirectArrayEntry {
        key: string_value(8),
        value: 1,
    }]
    .into_boxed_slice();
    let greater = vec![php_jit::JitNativeDirectArrayEntry {
        key: string_value(8),
        value: 2,
    }]
    .into_boxed_slice();
    let disjoint = vec![
        php_jit::JitNativeDirectArrayEntry { key: 2, value: 1 },
        php_jit::JitNativeDirectArrayEntry { key: 3, value: 2 },
    ]
    .into_boxed_slice();
    for (index, entries) in [
        ordered.as_ref(),
        same.as_ref(),
        reordered.as_ref(),
        coercive.as_ref(),
        lower.as_ref(),
        greater.as_ref(),
        disjoint.as_ref(),
    ]
    .into_iter()
    .enumerate()
    {
        slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
            flags: php_jit::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
            payload: entries.len() as u64,
            aux: entries.as_ptr() as usize as u64,
            ..php_jit::JitNativeValueSlot::default()
        };
    }
    slots[11] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT,
        flags: 0,
        payload: f64::NAN.to_bits(),
        ..php_jit::JitNativeValueSlot::default()
    };
    slots[12] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_FLOAT,
        flags: 0,
        payload: 0.0_f64.to_bits(),
        ..php_jit::JitNativeValueSlot::default()
    };
    let nan_entries = [php_jit::JitNativeDirectArrayEntry {
        key: 0,
        value: float_value(11),
    }];
    let zero_entries = [php_jit::JitNativeDirectArrayEntry {
        key: 0,
        value: float_value(12),
    }];
    for (index, entries) in [(9, nan_entries.as_slice()), (10, zero_entries.as_slice())] {
        slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
            flags: php_jit::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
            payload: entries.len() as u64,
            aux: entries.as_ptr() as usize as u64,
            ..php_jit::JitNativeValueSlot::default()
        };
    }
    let mut fast_state = super::NativeRequestFastState {
        header: php_jit::JitNativeFastStateHeader {
            abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
            flags: 0,
            runtime_view_pointer: 0,
            runtime_view: php_jit::JitNativeRuntimeView {
                abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
                direct_value_slots: slots.as_mut_ptr() as usize as u64,
                ..php_jit::JitNativeRuntimeView::default()
            },
        },
        ..super::NativeRequestFastState::default()
    };
    let runtime = std::ptr::from_mut(&mut fast_state);
    let identical = crate::native_exact::exact_runtime_ops::jit_native_identical_abi(
        runtime,
        array_value(0),
        array_value(1),
    );
    assert_eq!(identical.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        identical.value,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE)
    );
    let reordered_identity = crate::native_exact::exact_runtime_ops::jit_native_identical_abi(
        runtime,
        array_value(0),
        array_value(2),
    );
    assert_eq!(reordered_identity.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        reordered_identity.value,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE)
    );
    for right in [2, 3] {
        let equal = crate::native_exact::exact_runtime_ops::jit_native_equal_abi(
            runtime,
            array_value(0),
            array_value(right),
        );
        assert_eq!(equal.status, php_jit::JitCallStatus::RETURN);
        assert_eq!(
            equal.value,
            php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE)
        );
    }
    let unequal = crate::native_exact::exact_runtime_ops::jit_native_equal_abi(
        runtime,
        array_value(0),
        array_value(6),
    );
    assert_eq!(unequal.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        unequal.value,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE)
    );
    let compared = crate::native_exact::exact_runtime_ops::jit_native_spaceship_abi(
        runtime,
        array_value(4),
        array_value(5),
    );
    assert_eq!(compared.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(compared.value, -1);
    for compare in [
        crate::native_exact::exact_runtime_ops::jit_native_less_abi,
        crate::native_exact::exact_runtime_ops::jit_native_less_equal_abi,
        crate::native_exact::exact_runtime_ops::jit_native_greater_abi,
        crate::native_exact::exact_runtime_ops::jit_native_greater_equal_abi,
    ] {
        let result = compare(runtime, array_value(9), array_value(10));
        assert_eq!(result.status, php_jit::JitCallStatus::RETURN);
        assert_eq!(
            result.value,
            php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE)
        );
    }
    let compared = crate::native_exact::exact_runtime_ops::jit_native_spaceship_abi(
        runtime,
        array_value(9),
        array_value(10),
    );
    assert_eq!(compared.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(compared.value, 1);
}

#[test]
fn exact_native_object_comparison_uses_identity_and_authoritative_slots() {
    fn class_entry(name: &str) -> php_runtime::api::ClassEntry {
        php_runtime::api::ClassEntry {
            name: name.to_owned().into(),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: vec![php_runtime::api::ClassPropertyEntry {
                name: "value".to_owned(),
                default: php_runtime::api::Value::Null,
                type_: None,
                flags: php_runtime::api::ClassPropertyFlags::default(),
                hooks: php_runtime::api::ClassPropertyHooks::default(),
                attributes: Vec::new(),
            }],
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: php_runtime::api::ClassFlags::default(),
        }
    }
    fn object(class: &php_runtime::api::ClassEntry, value: i64) -> php_runtime::api::ObjectRef {
        php_runtime::api::ObjectRef::from_layout_native_slots(
            class,
            class.name.to_string(),
            vec![php_runtime::api::NativeDeclaredPropertySlot {
                initialized: 1,
                reserved: 0,
                value,
            }]
            .into_boxed_slice(),
        )
    }

    let class = class_entry("comparison_box");
    let other_class = class_entry("comparison_other");
    let left = Box::new(object(&class, 1));
    let same_properties = Box::new(object(&class, 1));
    let greater = Box::new(object(&class, 2));
    let other = Box::new(object(&other_class, 1));
    let dynamic = Box::new(object(&class, 1));
    dynamic.set_property("dynamic", php_runtime::api::Value::Int(9));

    let mut slots =
        vec![php_jit::JitNativeValueSlot::default(); php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let mut owners = vec![0_u64; php_jit::JIT_NATIVE_DIRECT_VALUE_CAPACITY];
    let object_value = |index: u32| {
        php_jit::jit_encode_typed_runtime_value(
            php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + index,
            php_jit::JIT_VALUE_RUNTIME_OBJECT_TAG,
        )
    };
    for (index, object) in [
        left.as_ref(),
        same_properties.as_ref(),
        greater.as_ref(),
        other.as_ref(),
        dynamic.as_ref(),
    ]
    .into_iter()
    .enumerate()
    {
        let layout_id = object.class_layout_epoch();
        let (properties, property_count) = object
            .native_declared_slots_view(layout_id)
            .expect("test object native slots");
        slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_OBJECT,
            flags: php_jit::JIT_NATIVE_OBJECT_PROPERTY_VIEW_ABI_VERSION,
            reserved: u32::try_from(property_count).expect("property count"),
            payload: layout_id,
            aux: properties as usize as u64,
        };
        owners[index] = object as *const php_runtime::api::ObjectRef as usize as u64;
    }
    let left_array = [php_jit::JitNativeDirectArrayEntry {
        key: 0,
        value: object_value(0),
    }];
    let same_array = [php_jit::JitNativeDirectArrayEntry {
        key: 0,
        value: object_value(1),
    }];
    for (index, entries) in [(5, left_array.as_slice()), (6, same_array.as_slice())] {
        slots[index] = php_jit::JitNativeValueSlot {
            refcount: 1,
            kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
            flags: php_jit::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
            payload: entries.len() as u64,
            aux: entries.as_ptr() as usize as u64,
            ..php_jit::JitNativeValueSlot::default()
        };
    }
    slots[7] = php_jit::JitNativeValueSlot {
        refcount: 1,
        kind: php_jit::JIT_NATIVE_VALUE_VIEW_DIRECT_ARRAY,
        flags: php_jit::JIT_NATIVE_DIRECT_ARRAY_ABI_VERSION,
        ..php_jit::JitNativeValueSlot::default()
    };
    let array_value = |index: u32| {
        php_jit::jit_encode_typed_runtime_value(
            php_jit::JIT_NATIVE_DIRECT_VALUE_INDEX_BASE + index,
            php_jit::JIT_VALUE_RUNTIME_ARRAY_TAG,
        )
    };
    let mut fast_state = super::NativeRequestFastState {
        header: php_jit::JitNativeFastStateHeader {
            abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
            flags: 0,
            runtime_view_pointer: 0,
            runtime_view: php_jit::JitNativeRuntimeView {
                abi_version: php_jit::JIT_RUNTIME_ABI_VERSION,
                direct_value_slots: slots.as_mut_ptr() as usize as u64,
                direct_object_owners: owners.as_mut_ptr() as usize as u64,
                ..php_jit::JitNativeRuntimeView::default()
            },
        },
        ..super::NativeRequestFastState::default()
    };
    let runtime = std::ptr::from_mut(&mut fast_state);

    let nested_identity = crate::native_exact::exact_runtime_ops::jit_native_identical_abi(
        runtime,
        array_value(5),
        array_value(6),
    );
    assert_eq!(nested_identity.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        nested_identity.value,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE),
        "different objects sharing one layout must not become identical"
    );

    let equal = crate::native_exact::exact_runtime_ops::jit_native_equal_abi(
        runtime,
        object_value(0),
        object_value(1),
    );
    assert_eq!(equal.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        equal.value,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE)
    );
    let unequal_class = crate::native_exact::exact_runtime_ops::jit_native_equal_abi(
        runtime,
        object_value(0),
        object_value(3),
    );
    assert_eq!(unequal_class.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        unequal_class.value,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE)
    );
    let compared = crate::native_exact::exact_runtime_ops::jit_native_spaceship_abi(
        runtime,
        object_value(0),
        object_value(2),
    );
    assert_eq!(compared.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(compared.value, -1);

    let object_boolean = crate::native_exact::exact_runtime_ops::jit_native_equal_abi(
        runtime,
        object_value(0),
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE),
    );
    assert_eq!(object_boolean.status, php_jit::JitCallStatus::RETURN);
    assert_eq!(
        object_boolean.value,
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE)
    );
    for (left, right) in [
        (
            array_value(5),
            php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE),
        ),
        (array_value(7), php_jit::jit_encode_constant(u32::MAX)),
        (
            array_value(7),
            php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE),
        ),
    ] {
        let equal =
            crate::native_exact::exact_runtime_ops::jit_native_equal_abi(runtime, left, right);
        assert_eq!(equal.status, php_jit::JitCallStatus::RETURN);
        assert_eq!(
            equal.value,
            php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE)
        );
    }

    let cold_dynamic = crate::native_exact::exact_runtime_ops::jit_native_equal_abi(
        runtime,
        object_value(0),
        object_value(4),
    );
    assert_eq!(cold_dynamic.status, php_jit::JitCallStatus::ABI_MISMATCH);
}

#[test]
fn positional_builtin_arguments_do_not_require_rebinding() {
    use php_ir::instruction::{IrCallArg, IrCallArgValueKind};

    let argument = |name, unpack| IrCallArg {
        name,
        value: php_ir::Operand::Constant(php_ir::ConstId::new(0)),
        unpack,
        value_kind: IrCallArgValueKind::Direct,
        by_ref_local: None,
        by_ref_dim: None,
        by_ref_property: None,
        by_ref_property_dim: None,
    };
    let positional = [argument(None, false)];
    let named = [argument(Some("value".to_owned()), false)];
    let unpacked = [argument(None, true)];

    assert!(!super::baseline_call_support::native_builtin_arguments_require_binding(None));
    assert!(
        !super::baseline_call_support::native_builtin_arguments_require_binding(Some(&positional))
    );
    assert!(super::baseline_call_support::native_builtin_arguments_require_binding(Some(&named)));
    assert!(
        super::baseline_call_support::native_builtin_arguments_require_binding(Some(&unpacked))
    );
}

#[test]
fn normalized_builtin_names_borrow_the_common_lowercase_form() {
    use std::borrow::Cow;

    assert!(matches!(
        super::baseline_native_builtins::normalized_native_builtin_name("array_key_exists"),
        Cow::Borrowed("array_key_exists")
    ));
    assert!(matches!(
        super::baseline_native_builtins::normalized_native_builtin_name("\\strlen"),
        Cow::Borrowed("strlen")
    ));
    assert_eq!(
        super::baseline_native_builtins::normalized_native_builtin_name("StrLen"),
        Cow::<str>::Owned("strlen".to_owned())
    );
}

#[test]
fn plain_local_fetch_fast_path_keeps_observable_values_on_the_slow_path() {
    let null = php_jit::jit_encode_constant(u32::MAX);
    let uninitialized = php_jit::jit_encode_constant(php_jit::JIT_VALUE_UNINITIALIZED);

    assert_eq!(
        super::baseline_runtime_ops::fast_plain_local_fetch(42, false),
        Some(42)
    );
    assert_eq!(
        super::baseline_runtime_ops::fast_plain_local_fetch(null, false),
        Some(null)
    );
    assert_eq!(
        super::baseline_runtime_ops::fast_plain_local_fetch(uninitialized, false),
        None
    );
    assert_eq!(
        super::baseline_runtime_ops::fast_plain_local_fetch(uninitialized, true),
        Some(null)
    );
    assert_eq!(
        super::baseline_runtime_ops::fast_plain_local_fetch(php_jit::jit_encode_constant(3), true),
        None
    );
    assert_eq!(
        super::baseline_runtime_ops::fast_plain_local_fetch(
            php_jit::jit_encode_runtime_value(3),
            true
        ),
        None
    );
}

#[test]
fn immediate_scalar_fast_paths_preserve_native_slot_encoding() {
    use super::baseline_runtime_ops::{
        fast_baseline_binary, fast_native_cast, fast_native_compare, fast_native_truthy,
        fast_native_unary,
    };

    let null = php_jit::jit_encode_constant(u32::MAX);
    let false_value = php_jit::jit_encode_constant(php_jit::JIT_VALUE_FALSE);
    let true_value = php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE);
    let runtime = php_jit::jit_encode_runtime_value(7);

    assert_eq!(fast_native_truthy(0), Some(false));
    assert_eq!(fast_native_truthy(-7), Some(true));
    assert_eq!(fast_native_truthy(null), Some(false));
    assert_eq!(fast_native_truthy(true_value), Some(true));
    assert_eq!(fast_native_truthy(runtime), None);

    assert_eq!(fast_native_unary(1, 7), Some(-7));
    assert_eq!(fast_native_unary(1, i64::MIN), None);
    assert_eq!(fast_native_unary(2, false_value), Some(true_value));
    assert_eq!(fast_baseline_binary(0, 20, 22), Some(42));
    assert_eq!(fast_baseline_binary(0, i64::MAX, 1), None);
    assert_eq!(fast_baseline_binary(0, 0x7ff0_ffff_ffff_ffff, 1), None);
    assert_eq!(fast_native_unary(3, !0x7ff1_0000_0000_0000), None);
    assert_eq!(fast_baseline_binary(3, 8, 2), Some(4));
    assert_eq!(fast_baseline_binary(3, 7, 2), None);
    assert_eq!(fast_baseline_binary(10, 1, -1), None);

    assert_eq!(fast_native_compare(4, 2, 3), Some(true_value));
    assert_eq!(fast_native_compare(8, 3, 2), Some(1));
    assert_eq!(fast_native_compare(0, runtime, 1), None);
    assert_eq!(fast_native_cast(0, 0), Some(false_value));
    assert_eq!(fast_native_cast(1, true_value), Some(1));
    assert_eq!(fast_native_cast(6, runtime), Some(null));
}

#[test]
fn callable_resolution_dereferences_nested_php_references() {
    let inner = php_runtime::api::ReferenceCell::new(php_runtime::api::Value::String(
        php_runtime::api::PhpString::from_bytes(b"Fixture::run".to_vec()),
    ));
    let outer = php_runtime::api::ReferenceCell::new(php_runtime::api::Value::Reference(inner));
    let value = dereference_native_callable_value(php_runtime::api::Value::Reference(outer));

    assert!(matches!(
        value,
        php_runtime::api::Value::String(name) if name.as_bytes() == b"Fixture::run"
    ));
}

#[test]
fn native_php_diagnostics_match_cli_and_http_rendering() {
    let cli = format_native_php_diagnostic(
        "Deprecated",
        "Using null as an array offset is deprecated, use an empty string instead",
        "/srv/index.php",
        17,
        true,
        false,
    );
    assert_eq!(
        cli,
        "\nDeprecated: Using null as an array offset is deprecated, use an empty string instead in /srv/index.php on line 17\n"
    );

    let http = format_native_php_diagnostic(
        "Deprecated",
        "Using null as an array offset is deprecated, use an empty string instead",
        "/srv/index.php",
        17,
        true,
        true,
    );
    assert_eq!(
        http,
        "<br />\n<b>Deprecated</b>:  Using null as an array offset is deprecated, use an empty string instead in <b>/srv/index.php</b> on line <b>17</b><br />\n"
    );
}

#[test]
fn native_backtrace_lines_use_the_retained_source_index() {
    let root = std::env::temp_dir().join(format!(
        "phrust-native-backtrace-lines-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temporary source root should be created");
    let path = root.join("fixture.php");
    std::fs::write(&path, "<?php\nline2\nfunction traced() {}\n")
        .expect("source fixture should be written");

    let span = php_ir::IrSpan::new(php_ir::FileId::new(0), 12, 32);
    let mut unit = php_ir::IrUnit::new(php_ir::UnitId::new(0));
    unit.files.push(php_ir::module::FileEntry {
        id: php_ir::FileId::new(0),
        path: path.to_string_lossy().into_owned(),
    });
    unit.functions.push(php_ir::IrFunction::new(
        "traced",
        php_ir::FunctionFlags::default(),
        span,
    ));
    let compiled = crate::compiled_unit::CompiledUnit::new(unit);

    std::fs::write(&path, "replaced without the original line structure")
        .expect("source fixture should be replaceable");
    let frame = native_backtrace_frame(
        &compiled,
        php_ir::FunctionId::new(0),
        None,
        None,
        Vec::new().into(),
    );
    let metadata = frame
        .metadata
        .expect("backtrace metadata should be prepared");
    assert_eq!(
        metadata.trace_file.as_deref(),
        Some(path.to_string_lossy().as_ref())
    );
    assert_eq!(metadata.trace_line, 3);

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn direct_value_slots_keep_cold_iterator_state_out_of_line() {
    let value_bytes = std::mem::size_of::<php_runtime::api::Value>();
    let slot_bytes = std::mem::size_of::<super::NativeColdIterator>();
    assert!(
        slot_bytes <= value_bytes.saturating_add(std::mem::size_of::<usize>()),
        "native value arena slot grew to {slot_bytes} bytes for a {value_bytes}-byte PHP value"
    );
}

#[test]
fn common_and_exact_native_sources_cannot_import_the_rust_value_plane() {
    let sources = [
        ("jit_abi.rs", include_str!("../jit_abi.rs")),
        (
            "exact_call_dispatch.rs",
            include_str!("native/exact_call_dispatch.rs"),
        ),
        (
            "exact_call_dispatch/array_multisort.rs",
            include_str!("native/exact_call_dispatch/array_multisort.rs"),
        ),
        (
            "exact_call_dispatch/recursive_array_family.rs",
            include_str!("native/exact_call_dispatch/recursive_array_family.rs"),
        ),
        (
            "exact_call_dispatch/scalar_and_filter_families.rs",
            include_str!("native/exact_call_dispatch/scalar_and_filter_families.rs"),
        ),
        (
            "exact_runtime_ops.rs",
            include_str!("native/exact_runtime_ops.rs"),
        ),
        (
            "native_request_fast_state.rs",
            include_str!("native/fast_state_impl.rs"),
        ),
        ("frame_arena.rs", include_str!("frame_arena.rs")),
        ("request_state.rs", include_str!("request_state.rs")),
        (
            "diagnostic_helpers.rs",
            include_str!("diagnostic_helpers.rs"),
        ),
        (
            "diagnostic_telemetry.rs",
            include_str!("diagnostic_telemetry.rs"),
        ),
    ];
    let forbidden = [
        "php_runtime::api::Value",
        "php_runtime::api::PhpArray",
        "php_runtime::api::PhpString",
        "php_runtime::api::ReferenceCell",
        "decode_baseline_value",
        "encode_baseline_value",
        "NativeStoredValue::Php",
        "duplicate_native_call_argument",
    ];
    for (name, source) in sources {
        for symbol in forbidden {
            assert!(
                !source.contains(symbol),
                "{name} imported forbidden Rust value-plane symbol {symbol}"
            );
        }
    }

    let exact_runtime = include_str!("native/exact_runtime_ops.rs");
    for deleted in [
        "native_compound_comparison_baseline",
        "native_object_cast_baseline",
        "native_array_cast_baseline",
        "native_int_cast_baseline",
        "native_float_cast_baseline",
        "native_string_cast_baseline",
        "native_unary_baseline",
        "native_array_count_baseline",
    ] {
        assert!(
            !exact_runtime.contains(deleted),
            "exact runtime retained deleted warm fallback {deleted}"
        );
    }

    for (name, source) in [
        (
            "exact_call_dispatch.rs",
            include_str!("native/exact_call_dispatch.rs"),
        ),
        (
            "exact_call_dispatch/array_multisort.rs",
            include_str!("native/exact_call_dispatch/array_multisort.rs"),
        ),
        (
            "exact_call_dispatch/recursive_array_family.rs",
            include_str!("native/exact_call_dispatch/recursive_array_family.rs"),
        ),
        (
            "exact_call_dispatch/scalar_and_filter_families.rs",
            include_str!("native/exact_call_dispatch/scalar_and_filter_families.rs"),
        ),
    ] {
        assert!(
            !source.contains("exact_query_baseline"),
            "{name} retained the deleted exact-query warm fallback"
        );
        assert!(
            !source.contains("RECOMPILE_REQUESTED"),
            "{name} retained a runtime retry result"
        );
    }
}

#[test]
fn exact_native_tree_has_a_compiler_enforced_cold_state_firewall() {
    let native_sources = [
        ("native/mod.rs", include_str!("native/mod.rs")),
        (
            "native/fast_state_impl.rs",
            include_str!("native/fast_state_impl.rs"),
        ),
        (
            "native/exact_call_dispatch.rs",
            include_str!("native/exact_call_dispatch.rs"),
        ),
        (
            "native/exact_call_dispatch/array_multisort.rs",
            include_str!("native/exact_call_dispatch/array_multisort.rs"),
        ),
        (
            "native/exact_call_dispatch/recursive_array_family.rs",
            include_str!("native/exact_call_dispatch/recursive_array_family.rs"),
        ),
        (
            "native/exact_call_dispatch/scalar_and_filter_families.rs",
            include_str!("native/exact_call_dispatch/scalar_and_filter_families.rs"),
        ),
        (
            "native/exact_runtime_ops.rs",
            include_str!("native/exact_runtime_ops.rs"),
        ),
    ];
    for (name, source) in native_sources {
        for forbidden in [
            "NativeRequestColdState",
            "baseline_value_plane",
            "decode_baseline_value",
            "encode_baseline_value",
            "use php_runtime::api::Value",
        ] {
            assert!(
                !source.contains(forbidden),
                "{name} crossed the exact-native visibility firewall through {forbidden}"
            );
        }
    }

    let crate_root = include_str!("../../lib.rs");
    let vm_root = include_str!("../mod.rs");
    let common = include_str!("../jit_abi.rs");
    assert!(crate_root.contains("mod native_exact;"));
    assert!(vm_root.contains("pub(crate) mod jit_abi;"));
    assert!(common.contains("pub(in crate::vm) use cold_request_state::NativeRequestColdState;"));
}

#[test]
fn cold_request_state_is_physically_outside_the_common_native_abi_source() {
    let common = include_str!("../jit_abi.rs");
    let cold = include_str!("cold_request_state.rs");
    assert!(!common.contains("struct NativeRequestColdState"));
    assert!(cold.contains("struct NativeRequestColdState"));
    assert!(!cold.contains("php_runtime::api::Value"));
    assert!(!cold.contains("decode_baseline_value"));
    assert!(!cold.contains("encode_baseline_value"));
    let value_plane = include_str!("baseline_value_plane.rs");
    assert!(value_plane.contains("fn decode_baseline_value"));
    assert!(value_plane.contains("fn encode_baseline_value"));
}
