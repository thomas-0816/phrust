use super::native_builtins::format_native_php_diagnostic;
use super::{
    NativeStoredValue, dereference_native_callable_value, jit_native_call_dispatch_abi,
    jit_native_dynamic_code_abi, native_backtrace_frame, stored_value_view,
};

#[test]
fn stable_value_views_publish_only_versioned_abi_descriptors() {
    let string = stored_value_view(&NativeStoredValue::Php(php_runtime::api::Value::String(
        php_runtime::api::PhpString::from_bytes(b"phrust".to_vec()),
    )));
    assert_eq!(string.kind, php_jit::JIT_NATIVE_VALUE_VIEW_STRING);
    assert_eq!(string.length, 6);

    let array = stored_value_view(&NativeStoredValue::Php(
        php_runtime::api::Value::packed_array(vec![
            php_runtime::api::Value::Int(1),
            php_runtime::api::Value::Int(2),
        ]),
    ));
    assert_eq!(array.kind, php_jit::JIT_NATIVE_VALUE_VIEW_ARRAY);
    assert_eq!(array.length, 2);

    let scalar = stored_value_view(&NativeStoredValue::Php(php_runtime::api::Value::Int(1)));
    assert_eq!(scalar.kind, php_jit::JIT_NATIVE_VALUE_VIEW_NONE);
    assert_eq!(scalar.length, 0);

    let reference = php_runtime::api::ReferenceCell::new(php_runtime::api::Value::Int(1));
    let reference_view = stored_value_view(&NativeStoredValue::Php(
        php_runtime::api::Value::Reference(reference.clone()),
    ));
    assert_eq!(
        reference_view.kind,
        php_jit::JIT_NATIVE_VALUE_VIEW_REFERENCE_SCALAR
    );
    assert_eq!(
        reference_view.flags,
        php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
    );
    assert_eq!(
        reference_view.length,
        reference.native_scalar_view_address() as u64
    );
    assert_eq!(
        std::mem::size_of::<php_runtime::experimental::native_reference::NativeReferenceScalarView>(
        ),
        std::mem::size_of::<php_jit::JitNativeReferenceScalarView>()
    );
    assert_eq!(
        php_runtime::experimental::native_reference::NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION,
        php_jit::JIT_NATIVE_REFERENCE_SCALAR_VIEW_ABI_VERSION
    );
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
fn native_call_trampoline_requests_compile_without_interpreter_reentry() {
    let mut frame = php_jit::JitNativeCallFrame {
        function_id: 3,
        continuation_id: 7,
        ..php_jit::JitNativeCallFrame::default()
    };
    let mut out = php_jit::JitCallResult::default();
    assert_eq!(
        jit_native_call_dispatch_abi(0, &mut frame, &mut out),
        php_jit::JitCallStatus::COMPILE_REQUIRED.0 as i32
    );
    assert_eq!(out.status, php_jit::JitCallStatus::COMPILE_REQUIRED);

    frame.abi_version = frame.abi_version.saturating_add(1);
    assert_eq!(
        jit_native_call_dispatch_abi(0, &mut frame, &mut out),
        php_jit::JitCallStatus::ABI_MISMATCH.0 as i32
    );
    assert_eq!(out.status, php_jit::JitCallStatus::ABI_MISMATCH);
}

#[test]
fn native_dynamic_code_boundary_requires_an_active_execution_context() {
    let mut request = php_jit::JitNativeDynamicCodeRequest {
        kind: php_jit::JitNativeDynamicCodeKind::EVAL,
        ..php_jit::JitNativeDynamicCodeRequest::default()
    };
    let mut out = php_jit::JitCallResult::default();
    assert_eq!(
        jit_native_dynamic_code_abi(0, &mut request, &mut out),
        php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
    );
    assert_eq!(out.status, php_jit::JitCallStatus::RUNTIME_ERROR);
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
        Vec::new(),
    );
    assert_eq!(frame.file.as_deref(), Some(path.to_string_lossy().as_ref()));
    assert_eq!(frame.line, 3);

    let _ = std::fs::remove_dir_all(root);
}
