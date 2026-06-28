//! mysqli builtin registry slice.

use super::core::{argument_type_error, expect_arity, int_arg, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{
    MYSQL_TEST_DSN_ENV, MYSQLI_ASSOC, MYSQLI_BOTH, MYSQLI_NUM, MysqlConnectOptions, ObjectRef,
    PhpString, Value,
};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "mysqli_close",
        builtin_mysqli_close,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_connect",
        builtin_mysqli_connect,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_connect_errno",
        builtin_mysqli_connect_errno,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_connect_error",
        builtin_mysqli_connect_error,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_errno",
        builtin_mysqli_errno,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_error",
        builtin_mysqli_error,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_escape_string",
        builtin_mysqli_real_escape_string,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_fetch_array",
        builtin_mysqli_fetch_array,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_fetch_assoc",
        builtin_mysqli_fetch_assoc,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_fetch_row",
        builtin_mysqli_fetch_row,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_free_result",
        builtin_mysqli_free_result,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_init",
        builtin_mysqli_init,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_num_fields",
        builtin_mysqli_num_fields,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_num_rows",
        builtin_mysqli_num_rows,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_prepare",
        builtin_mysqli_prepare,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_query",
        builtin_mysqli_query,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_real_connect",
        builtin_mysqli_real_connect,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_real_escape_string",
        builtin_mysqli_real_escape_string,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_report",
        builtin_mysqli_report,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_select_db",
        builtin_mysqli_select_db,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_set_charset",
        builtin_mysqli_set_charset,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_stmt_init",
        builtin_mysqli_prepare,
        BuiltinCompatibility::Php,
    ),
];

pub(in crate::builtins::modules) fn builtin_mysqli_connect(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_connect", args.len(), 0, 6)?;
    connect_from_test_dsn(context)
}

pub(in crate::builtins::modules) fn builtin_mysqli_init(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_init", &args, 0)?;
    Ok(Value::Object(mysqli_object(None)))
}

pub(in crate::builtins::modules) fn builtin_mysqli_real_connect(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_real_connect", args.len(), 1, 7)?;
    let object = mysqli_object_arg("mysqli_real_connect", args.first())?;
    let connection = connect_from_test_dsn(context)?;
    let Value::Object(connected) = connection else {
        return Ok(Value::Bool(false));
    };
    if let Some(id) = mysqli_connection_id(&connected) {
        set_mysqli_connection_id(&object, id);
        Ok(Value::Bool(true))
    } else {
        Ok(Value::Bool(false))
    }
}

pub(in crate::builtins::modules) fn builtin_mysqli_query(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_query", &args, 2)?;
    let object = mysqli_object_arg("mysqli_query", args.first())?;
    let Some(id) = mysqli_connection_id(&object) else {
        return Ok(Value::Bool(false));
    };
    let sql = string_arg("mysqli_query", &args[1])?.to_string_lossy();
    let Some(state) = context.mysql_state() else {
        return Ok(Value::Bool(false));
    };
    match state.query(id, &sql) {
        Ok(Some(result_id)) => {
            let result = mysqli_result_object(result_id);
            result.set_property("num_rows", Value::Int(state.num_rows(result_id)));
            Ok(Value::Object(result))
        }
        Ok(None) => Ok(Value::Bool(true)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

pub(in crate::builtins::modules) fn builtin_mysqli_fetch_assoc(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_fetch_assoc", &args, 1)?;
    fetch_array(context, args.first(), MYSQLI_ASSOC)
}

pub(in crate::builtins::modules) fn builtin_mysqli_fetch_row(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_fetch_row", &args, 1)?;
    fetch_array(context, args.first(), MYSQLI_NUM)
}

pub(in crate::builtins::modules) fn builtin_mysqli_fetch_array(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_fetch_array", args.len(), 1, 2)?;
    let mode = args
        .get(1)
        .map(|value| int_arg("mysqli_fetch_array", value))
        .transpose()?
        .unwrap_or(MYSQLI_BOTH);
    fetch_array(context, args.first(), mode)
}

pub(in crate::builtins::modules) fn builtin_mysqli_num_rows(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_num_rows", &args, 1)?;
    let result = mysqli_result_object_arg("mysqli_num_rows", args.first())?;
    let Some(id) = mysqli_result_id(&result) else {
        return Ok(Value::Int(0));
    };
    Ok(Value::Int(
        context.mysql_state().map_or(0, |state| state.num_rows(id)),
    ))
}

pub(in crate::builtins::modules) fn builtin_mysqli_num_fields(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_num_fields", &args, 1)?;
    let result = mysqli_result_object_arg("mysqli_num_fields", args.first())?;
    let Some(id) = mysqli_result_id(&result) else {
        return Ok(Value::Int(0));
    };
    Ok(Value::Int(
        context
            .mysql_state()
            .map_or(0, |state| state.num_fields(id)),
    ))
}

pub(in crate::builtins::modules) fn builtin_mysqli_free_result(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_free_result", &args, 1)?;
    let result = mysqli_result_object_arg("mysqli_free_result", args.first())?;
    let Some(id) = mysqli_result_id(&result) else {
        return Ok(Value::Bool(false));
    };
    result.unset_property("__mysqli_result");
    Ok(Value::Bool(
        context
            .mysql_state()
            .is_some_and(|state| state.free_result(id)),
    ))
}

pub(in crate::builtins::modules) fn builtin_mysqli_close(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_close", &args, 1)?;
    let object = mysqli_object_arg("mysqli_close", args.first())?;
    let Some(id) = mysqli_connection_id(&object) else {
        return Ok(Value::Bool(false));
    };
    object.unset_property("__mysqli_connection");
    Ok(Value::Bool(
        context.mysql_state().is_some_and(|state| state.close(id)),
    ))
}

pub(in crate::builtins::modules) fn builtin_mysqli_errno(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_errno", &args, 1)?;
    let object = mysqli_object_arg("mysqli_errno", args.first())?;
    let errno = mysqli_connection_id(&object).map_or(1, |id| {
        context.mysql_state().map_or(1, |state| state.errno(id))
    });
    Ok(Value::Int(errno))
}

pub(in crate::builtins::modules) fn builtin_mysqli_error(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_error", &args, 1)?;
    let object = mysqli_object_arg("mysqli_error", args.first())?;
    let error = mysqli_connection_id(&object).map_or_else(
        || "not an open MySQL connection".to_owned(),
        |id| {
            context.mysql_state().map_or_else(
                || "not an open MySQL connection".to_owned(),
                |state| state.error(id),
            )
        },
    );
    Ok(Value::String(PhpString::from(error.into_bytes())))
}

pub(in crate::builtins::modules) fn builtin_mysqli_connect_errno(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_connect_errno", &args, 0)?;
    Ok(Value::Int(
        context
            .mysql_state()
            .map_or(2002, |state| state.connect_errno()),
    ))
}

pub(in crate::builtins::modules) fn builtin_mysqli_connect_error(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_connect_error", &args, 0)?;
    Ok(Value::String(PhpString::from(
        context
            .mysql_state()
            .map_or_else(
                || "mysqli runtime state is unavailable".to_owned(),
                |state| state.connect_error(),
            )
            .into_bytes(),
    )))
}

pub(in crate::builtins::modules) fn builtin_mysqli_real_escape_string(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_real_escape_string", &args, 2)?;
    let _object = mysqli_object_arg("mysqli_real_escape_string", args.first())?;
    let value = string_arg("mysqli_real_escape_string", &args[1])?;
    Ok(Value::string(mysql_escape_string(value.as_bytes())))
}

pub(in crate::builtins::modules) fn builtin_mysqli_report(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_report", &args, 1)?;
    let _flags = int_arg("mysqli_report", &args[0])?;
    Ok(Value::Bool(true))
}

pub(in crate::builtins::modules) fn builtin_mysqli_select_db(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_select_db", &args, 2)?;
    let object = mysqli_object_arg("mysqli_select_db", args.first())?;
    let Some(id) = mysqli_connection_id(&object) else {
        return Ok(Value::Bool(false));
    };
    let database = string_arg("mysqli_select_db", &args[1])?.to_string_lossy();
    Ok(Value::Bool(context.mysql_state().is_some_and(|state| {
        state.select_db(id, &database).is_ok()
    })))
}

pub(in crate::builtins::modules) fn builtin_mysqli_set_charset(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_set_charset", &args, 2)?;
    let object = mysqli_object_arg("mysqli_set_charset", args.first())?;
    let Some(id) = mysqli_connection_id(&object) else {
        return Ok(Value::Bool(false));
    };
    let charset = string_arg("mysqli_set_charset", &args[1])?.to_string_lossy();
    Ok(Value::Bool(context.mysql_state().is_some_and(|state| {
        state.set_charset(id, &charset).is_ok()
    })))
}

pub(in crate::builtins::modules) fn builtin_mysqli_prepare(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_prepare", args.len(), 1, 2)?;
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_MYSQLI_PREPARE_UNSUPPORTED",
        "mysqli prepared statements are not implemented in the mysqli MVP",
    ))
}

fn connect_from_test_dsn(context: &mut BuiltinContext<'_>) -> BuiltinResult {
    let Some(state) = context.mysql_state() else {
        return Ok(Value::Bool(false));
    };
    let Some(options) = MysqlConnectOptions::from_test_env() else {
        state.record_connect_error(
            2002,
            format!("live mysqli connections require {MYSQL_TEST_DSN_ENV}"),
        );
        return Ok(Value::Bool(false));
    };
    match options {
        Ok(options) => match state.connect(&options) {
            Ok(id) => Ok(Value::Object(mysqli_object(Some(id)))),
            Err(_) => Ok(Value::Bool(false)),
        },
        Err(error) => {
            state.record_connect_error(2005, error.message);
            Ok(Value::Bool(false))
        }
    }
}

fn fetch_array(
    context: &mut BuiltinContext<'_>,
    value: Option<&Value>,
    mode: i64,
) -> BuiltinResult {
    let result = mysqli_result_object_arg("mysqli_fetch_array", value)?;
    let Some(id) = mysqli_result_id(&result) else {
        return Ok(Value::Bool(false));
    };
    Ok(context
        .mysql_state()
        .map_or(Value::Bool(false), |state| state.fetch_array(id, mode)))
}

pub fn mysqli_object(connection_id: Option<i64>) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&mysqli_runtime_class("mysqli"), "mysqli");
    if let Some(id) = connection_id {
        set_mysqli_connection_id(&object, id);
    }
    object.set_property("connect_errno", Value::Int(0));
    object.set_property("connect_error", Value::String(PhpString::from("")));
    object.set_property("errno", Value::Int(0));
    object.set_property("error", Value::String(PhpString::from("")));
    object
}

pub fn mysqli_result_object(result_id: i64) -> ObjectRef {
    let object =
        ObjectRef::new_with_display_name(&mysqli_runtime_class("mysqli_result"), "mysqli_result");
    object.set_property("__mysqli_result", Value::Int(result_id));
    object.set_property("num_rows", Value::Int(0));
    object
}

fn mysqli_runtime_class(name: &str) -> crate::ClassEntry {
    crate::ClassEntry {
        name: crate::normalize_class_name(name),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: crate::ClassFlags::default(),
    }
}

pub fn set_mysqli_connection_id(object: &ObjectRef, id: i64) {
    object.set_property("__mysqli_connection", Value::Int(id));
}

pub fn mysqli_connection_id(object: &ObjectRef) -> Option<i64> {
    match object.get_property("__mysqli_connection") {
        Some(Value::Int(id)) => Some(id),
        _ => None,
    }
}

pub fn mysqli_result_id(object: &ObjectRef) -> Option<i64> {
    match object.get_property("__mysqli_result") {
        Some(Value::Int(id)) => Some(id),
        _ => None,
    }
}

fn mysqli_object_arg(name: &str, value: Option<&Value>) -> Result<ObjectRef, BuiltinError> {
    match value {
        Some(Value::Object(object)) if object.class_name() == "mysqli" => Ok(object.clone()),
        Some(value) => Err(argument_type_error(name, "1", "mysqli", value)),
        None => Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            format!("builtin {name} expects mysqli argument"),
        )),
    }
}

fn mysqli_result_object_arg(name: &str, value: Option<&Value>) -> Result<ObjectRef, BuiltinError> {
    match value {
        Some(Value::Object(object)) if object.class_name() == "mysqli_result" => Ok(object.clone()),
        Some(value) => Err(argument_type_error(name, "1", "mysqli_result", value)),
        None => Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            format!("builtin {name} expects mysqli_result argument"),
        )),
    }
}

fn mysql_escape_string(value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    for byte in value {
        match byte {
            0 => out.extend_from_slice(b"\\0"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\'' => out.extend_from_slice(b"\\'"),
            b'"' => out.extend_from_slice(b"\\\""),
            0x1a => out.extend_from_slice(b"\\Z"),
            other => out.push(*other),
        }
    }
    out
}

fn expect_mysqli_arity(
    name: &str,
    actual: usize,
    min: usize,
    max: usize,
) -> Result<(), BuiltinError> {
    if actual >= min && actual <= max {
        return Ok(());
    }
    let expected = if min == max {
        format!("exactly {min} argument(s)")
    } else {
        format!("{min} to {max} argument(s)")
    };
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_ARITY",
        format!("builtin {name} expects {expected}"),
    ))
}
