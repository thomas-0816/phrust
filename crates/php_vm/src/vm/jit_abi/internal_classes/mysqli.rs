use super::*;

fn mysqli_internal_object(class_name: &str, display_name: &str) -> php_runtime::api::ObjectRef {
    let class = php_runtime::api::ClassEntry {
        name: std::sync::Arc::from(class_name),
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
    php_runtime::api::ObjectRef::new_with_display_name(&class, display_name)
}

/// Constructs the property-only `mysqli_driver` object used by mysqli's own
/// capability probes. Network and result objects continue to be created by
/// the owning typed mysqli builtins; this does not invent a second DB path.
pub(in crate::vm::jit_abi) fn construct_native_mysqli_class(
    context: &mut NativeRequestColdState<'_>,
    class_name: &str,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    if normalize_class_name(class_name) != "mysqli_driver" {
        return None;
    }
    let result = decode_arguments(context, arguments).and_then(|arguments| {
        expect_arity("mysqli_driver::__construct", arguments.len(), 0, 0)?;
        let object = mysqli_internal_object("mysqli_driver", "mysqli_driver");
        object.set_property(
            "client_info",
            Value::String(PhpString::from_bytes(
                php_runtime::api::MYSQLND_CLIENT_INFO.as_bytes().to_vec(),
            )),
        );
        object.set_property(
            "client_version",
            Value::Int(php_runtime::api::MYSQLND_CLIENT_VERSION),
        );
        object.set_property(
            "driver_version",
            Value::Int(php_runtime::api::MYSQLND_CLIENT_VERSION),
        );
        object.set_property("embedded", Value::Bool(false));
        object.set_property("reconnect", Value::Bool(false));
        object.set_property(
            "report_mode",
            Value::Int(context.mysql_state.borrow().report_flags()),
        );
        context.encode(Value::Object(object))
    });
    Some(result)
}

pub(in crate::vm::jit_abi) fn execute_native_mysqli_instruction(
    context: &mut NativeRequestColdState<'_>,
    instruction: &php_ir::Instruction,
    arguments: &[i64],
) -> Option<Result<i64, String>> {
    match &instruction.kind {
        php_ir::InstructionKind::NewObject { class_name, .. } => {
            construct_native_mysqli_class(context, class_name, arguments)
        }
        _ => None,
    }
}
