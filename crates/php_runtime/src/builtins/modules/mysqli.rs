//! mysqli builtin registry slice.

use super::core::{argument_type_error, expect_arity, int_arg, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{
    MYSQL_TEST_DSN_ENV, MYSQLI_ASSOC, MYSQLI_BOTH, MYSQLI_NUM, MYSQLI_SQLITE_COMPAT_ENV,
    MysqlConnectOptions, ObjectRef, PhpString, Value,
};
use std::env;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "mysqli_affected_rows",
        builtin_mysqli_affected_rows,
        BuiltinCompatibility::Php,
    ),
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
        "mysqli_get_server_info",
        builtin_mysqli_get_server_info,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_get_client_info",
        builtin_mysqli_get_client_info,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_get_client_version",
        builtin_mysqli_get_client_version,
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
        "mysqli_insert_id",
        builtin_mysqli_insert_id,
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

const MYSQLND_CLIENT_INFO: &str = "mysqlnd 8.5.7";
const MYSQLND_CLIENT_VERSION: i64 = 80507;

pub(in crate::builtins::modules) fn builtin_mysqli_connect(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_connect", args.len(), 0, 6)?;
    connect_from_mysqli_args(context, &args)
}

pub(in crate::builtins::modules) fn builtin_mysqli_init(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_init", &args, 0)?;
    Ok(Value::Object(mysqli_object(None)))
}

pub(in crate::builtins::modules) fn builtin_mysqli_get_client_info(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_get_client_info", args.len(), 0, 1)?;
    Ok(Value::string(MYSQLND_CLIENT_INFO))
}

pub(in crate::builtins::modules) fn builtin_mysqli_get_client_version(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_get_client_version", &args, 0)?;
    Ok(Value::Int(MYSQLND_CLIENT_VERSION))
}

pub(in crate::builtins::modules) fn builtin_mysqli_real_connect(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_real_connect", args.len(), 1, 8)?;
    let object = mysqli_object_arg("mysqli_real_connect", args.first())?;
    let connection = connect_from_mysqli_args(context, &args[1..])?;
    let Value::Object(connected) = connection else {
        if let Some(state) = context.mysql_state() {
            sync_mysqli_status_properties(&object, state);
        }
        return Ok(Value::Bool(false));
    };
    if let Some(id) = mysqli_connection_id(&connected) {
        set_mysqli_connection_id(&object, id);
        if let Some(state) = context.mysql_state() {
            sync_mysqli_status_properties(&object, state);
        }
        Ok(Value::Bool(true))
    } else {
        if let Some(state) = context.mysql_state() {
            sync_mysqli_status_properties(&object, state);
        }
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
            sync_mysqli_status_properties(&object, state);
            let result = mysqli_result_object(result_id);
            result.set_property("num_rows", Value::Int(state.num_rows(result_id)));
            Ok(Value::Object(result))
        }
        Ok(None) => {
            sync_mysqli_status_properties(&object, state);
            Ok(Value::Bool(true))
        }
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

pub(in crate::builtins::modules) fn builtin_mysqli_get_server_info(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_get_server_info", &args, 1)?;
    let object = mysqli_object_arg("mysqli_get_server_info", args.first())?;
    let Some(id) = mysqli_connection_id(&object) else {
        return Ok(Value::String(PhpString::from("")));
    };
    let info = context
        .mysql_state()
        .map_or_else(String::new, |state| state.server_info(id));
    Ok(Value::String(PhpString::from(info.into_bytes())))
}

pub(in crate::builtins::modules) fn builtin_mysqli_affected_rows(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_affected_rows", &args, 1)?;
    let object = mysqli_object_arg("mysqli_affected_rows", args.first())?;
    let Some(id) = mysqli_connection_id(&object) else {
        return Ok(Value::Int(-1));
    };
    Ok(Value::Int(
        context
            .mysql_state()
            .map_or(-1, |state| state.affected_rows(id)),
    ))
}

pub(in crate::builtins::modules) fn builtin_mysqli_insert_id(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_insert_id", &args, 1)?;
    let object = mysqli_object_arg("mysqli_insert_id", args.first())?;
    let Some(id) = mysqli_connection_id(&object) else {
        return Ok(Value::Int(0));
    };
    Ok(Value::Int(
        context
            .mysql_state()
            .map_or(0, |state| state.last_insert_id(id)),
    ))
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

fn connect_from_mysqli_args(context: &mut BuiltinContext<'_>, args: &[Value]) -> BuiltinResult {
    let Some(state) = context.mysql_state() else {
        return Ok(Value::Bool(false));
    };
    if mysqli_sqlite_compat_enabled() {
        return match state.connect_sqlite_compat() {
            Ok(id) => Ok(Value::Object(mysqli_object(Some(id)))),
            Err(_) => Ok(Value::Bool(false)),
        };
    }
    let options = if args.is_empty() {
        let Some(options) = MysqlConnectOptions::from_test_env() else {
            state.record_connect_error(
                2002,
                format!(
                    "live mysqli connections require {MYSQL_TEST_DSN_ENV} or mysqli connection arguments; selected SQLite compatibility fixtures require {MYSQLI_SQLITE_COMPAT_ENV}=1"
                ),
            );
            return Ok(Value::Bool(false));
        };
        options
    } else {
        mysqli_options_from_args(args)?
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

fn mysqli_options_from_args(
    args: &[Value],
) -> Result<Result<MysqlConnectOptions, crate::MysqlError>, BuiltinError> {
    let host = args
        .first()
        .map(|value| string_arg("mysqli_real_connect", value).map(|value| value.to_string_lossy()))
        .transpose()?
        .unwrap_or_else(|| "localhost".to_owned());
    let user = args
        .get(1)
        .map(|value| string_arg("mysqli_real_connect", value).map(|value| value.to_string_lossy()))
        .transpose()?
        .unwrap_or_default();
    let password = args
        .get(2)
        .map(|value| string_arg("mysqli_real_connect", value).map(|value| value.to_string_lossy()))
        .transpose()?
        .unwrap_or_default();
    let database = args
        .get(3)
        .and_then(|value| {
            if matches!(value, Value::Null) {
                None
            } else {
                Some(value)
            }
        })
        .map(|value| string_arg("mysqli_real_connect", value).map(|value| value.to_string_lossy()))
        .transpose()?;
    let port = args
        .get(4)
        .and_then(|value| {
            if matches!(value, Value::Null) {
                None
            } else {
                Some(value)
            }
        })
        .map(|value| int_arg("mysqli_real_connect", value))
        .transpose()?
        .and_then(|port| u16::try_from(port).ok());

    Ok(MysqlConnectOptions::from_parts(
        &host,
        &user,
        &password,
        database.as_deref(),
        port,
    ))
}

fn mysqli_sqlite_compat_enabled() -> bool {
    env::var(MYSQLI_SQLITE_COMPAT_ENV).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
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
    object.set_property("affected_rows", Value::Int(0));
    object.set_property("insert_id", Value::Int(0));
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

fn sync_mysqli_status_properties(object: &ObjectRef, state: &crate::MysqlState) {
    object.set_property("connect_errno", Value::Int(state.connect_errno()));
    object.set_property(
        "connect_error",
        Value::String(PhpString::from(state.connect_error().into_bytes())),
    );
    if let Some(id) = mysqli_connection_id(object) {
        object.set_property("errno", Value::Int(state.errno(id)));
        object.set_property(
            "error",
            Value::String(PhpString::from(state.error(id).into_bytes())),
        );
        object.set_property("affected_rows", Value::Int(state.affected_rows(id)));
        object.set_property("insert_id", Value::Int(state.last_insert_id(id)));
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputBuffer;

    #[test]
    fn mysqli_real_connect_accepts_php_flags_argument() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let result = builtin_mysqli_real_connect(
            &mut context,
            vec![
                Value::Object(mysqli_object(None)),
                Value::string("127.0.0.1"),
                Value::string("wordpress"),
                Value::string("secret"),
                Value::string("wordpress"),
                Value::Int(3306),
                Value::Null,
                Value::Int(0),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("eight-argument mysqli_real_connect should pass arity");

        assert!(matches!(result, Value::Bool(false)));
    }

    #[test]
    fn mysqli_get_server_info_is_available_for_capability_checks() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let result = builtin_mysqli_get_server_info(
            &mut context,
            vec![Value::Object(mysqli_object(None))],
            RuntimeSourceSpan::default(),
        )
        .expect("mysqli_get_server_info should be available");

        assert!(matches!(result, Value::String(_)));
    }
}
