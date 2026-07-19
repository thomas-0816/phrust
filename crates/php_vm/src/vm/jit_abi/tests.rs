use super::native_builtins::format_native_php_diagnostic;
use super::{
    NativeResolvedEntryCache, dereference_native_callable_value, jit_native_call_dispatch_abi,
    jit_native_dynamic_code_abi, jit_native_value_lifecycle_abi, native_backtrace_frame,
};

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

    assert!(!super::call_support::native_builtin_arguments_require_binding(None));
    assert!(!super::call_support::native_builtin_arguments_require_binding(Some(&positional)));
    assert!(super::call_support::native_builtin_arguments_require_binding(Some(&named)));
    assert!(super::call_support::native_builtin_arguments_require_binding(Some(&unpacked)));
}

#[test]
fn normalized_builtin_names_borrow_the_common_lowercase_form() {
    use std::borrow::Cow;

    assert!(matches!(
        super::native_builtins::normalized_native_builtin_name("array_key_exists"),
        Cow::Borrowed("array_key_exists")
    ));
    assert!(matches!(
        super::native_builtins::normalized_native_builtin_name("\\strlen"),
        Cow::Borrowed("strlen")
    ));
    assert_eq!(
        super::native_builtins::normalized_native_builtin_name("StrLen"),
        Cow::<str>::Owned("strlen".to_owned())
    );
}

#[test]
fn plain_local_fetch_fast_path_keeps_observable_values_on_the_slow_path() {
    let null = php_jit::jit_encode_constant(u32::MAX);
    let uninitialized = php_jit::jit_encode_constant(php_jit::JIT_VALUE_UNINITIALIZED);

    assert_eq!(
        super::runtime_ops::fast_plain_local_fetch(42, false),
        Some(42)
    );
    assert_eq!(
        super::runtime_ops::fast_plain_local_fetch(null, false),
        Some(null)
    );
    assert_eq!(
        super::runtime_ops::fast_plain_local_fetch(uninitialized, false),
        None
    );
    assert_eq!(
        super::runtime_ops::fast_plain_local_fetch(uninitialized, true),
        Some(null)
    );
    assert_eq!(
        super::runtime_ops::fast_plain_local_fetch(php_jit::jit_encode_constant(3), true),
        None
    );
    assert_eq!(
        super::runtime_ops::fast_plain_local_fetch(php_jit::jit_encode_runtime_value(3), true),
        None
    );
}

#[test]
fn immediate_scalar_fast_paths_preserve_native_slot_encoding() {
    use super::runtime_ops::{
        fast_native_binary, fast_native_cast, fast_native_compare, fast_native_truthy,
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
    assert_eq!(fast_native_binary(0, 20, 22), Some(42));
    assert_eq!(fast_native_binary(0, i64::MAX, 1), None);
    assert_eq!(fast_native_binary(3, 8, 2), Some(4));
    assert_eq!(fast_native_binary(3, 7, 2), None);
    assert_eq!(fast_native_binary(10, 1, -1), None);

    assert_eq!(fast_native_compare(4, 2, 3), Some(true_value));
    assert_eq!(fast_native_compare(8, 3, 2), Some(1));
    assert_eq!(fast_native_compare(0, runtime, 1), None);
    assert_eq!(fast_native_cast(0, 0), Some(false_value));
    assert_eq!(fast_native_cast(1, true_value), Some(1));
    assert_eq!(fast_native_cast(6, runtime), Some(null));
}

#[test]
fn resolved_native_entry_cache_validates_identity_epoch_and_collisions() {
    let mut cache = NativeResolvedEntryCache::default();
    let first = php_ir::FunctionId::new(3);
    let collision = php_ir::FunctionId::new(3 + 4_096);

    assert_eq!(cache.get(None, first, 7), None);
    cache.insert(None, first, 7, 0x1234);
    assert_eq!(cache.get(None, first, 7), Some(0x1234));
    assert_eq!(cache.get(Some(0), first, 7), None);
    assert_eq!(cache.get(None, first, 8), None);

    cache.insert(None, collision, 7, 0x5678);
    assert_eq!(cache.get(None, first, 7), None);
    assert_eq!(cache.get(None, collision, 7), Some(0x5678));
}

#[test]
fn immediate_value_lifecycle_bypasses_request_context() {
    for encoded in [
        42,
        php_jit::jit_encode_constant(u32::MAX),
        php_jit::jit_encode_constant(php_jit::JIT_VALUE_TRUE),
    ] {
        for operation in [0, 1, 0x8000_0000, 0x8000_0001] {
            let mut out = 0_i64;
            assert_eq!(
                jit_native_value_lifecycle_abi(operation, encoded, &mut out),
                0
            );
            assert_eq!(out, encoded);
        }
    }

    let mut out = 0_i64;
    assert_eq!(
        jit_native_value_lifecycle_abi(2, 42, &mut out),
        php_jit::JitCallStatus::RUNTIME_ERROR.0 as i32
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
fn native_call_trampoline_trusts_the_published_internal_abi() {
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
        php_jit::JitCallStatus::COMPILE_REQUIRED.0 as i32
    );
    assert_eq!(out.status, php_jit::JitCallStatus::COMPILE_REQUIRED);
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
fn native_value_slots_keep_iterator_state_out_of_line() {
    let value_bytes = std::mem::size_of::<php_runtime::api::Value>();
    let slot_bytes = std::mem::size_of::<super::NativeStoredValue>();
    assert!(
        slot_bytes <= value_bytes.saturating_add(std::mem::size_of::<usize>()),
        "native value arena slot grew to {slot_bytes} bytes for a {value_bytes}-byte PHP value"
    );
}
