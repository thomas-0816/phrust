//! mysqli builtin registry slice.

use super::core::{argument_type_error, expect_arity, int_arg, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{
    ArrayKey, MYSQL_TEST_DSN_ENV, MYSQLI_ASSOC, MYSQLI_BOTH, MYSQLI_NUM, MYSQLI_REPORT_ERROR,
    MYSQLI_REPORT_OFF, MYSQLI_REPORT_STRICT, MYSQLI_SQLITE_COMPAT_ENV, MYSQLI_STORE_RESULT,
    MYSQLI_USE_RESULT, MYSQLND_CLIENT_INFO, MYSQLND_CLIENT_VERSION, MysqlConnectOptions,
    MysqlError, ObjectRef, PhpArray, PhpString, ReferenceCell, RuntimeBringupDiagnosticContext,
    RuntimeDiagnostic, RuntimeDiagnosticPayload, RuntimeSeverity, Value,
};
use std::env;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "mysqli_affected_rows",
        builtin_mysqli_affected_rows,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_autocommit",
        builtin_mysqli_autocommit,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_begin_transaction",
        builtin_mysqli_begin_transaction,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_character_set_name",
        builtin_mysqli_character_set_name,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_close",
        builtin_mysqli_close,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_commit",
        builtin_mysqli_commit,
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
        "mysqli_data_seek",
        builtin_mysqli_data_seek,
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
        "mysqli_fetch_fields",
        builtin_mysqli_fetch_fields,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_fetch_object",
        builtin_mysqli_fetch_object,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_fetch_row",
        builtin_mysqli_fetch_row,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_field_count",
        builtin_mysqli_field_count,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_free_result",
        builtin_mysqli_free_result,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_get_charset",
        builtin_mysqli_character_set_name,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_get_client_info",
        builtin_mysqli_get_client_info,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_get_client_stats",
        builtin_mysqli_get_client_stats,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_get_client_version",
        builtin_mysqli_get_client_version,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_get_connection_stats",
        builtin_mysqli_get_connection_stats,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_get_host_info",
        builtin_mysqli_get_host_info,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_get_server_info",
        builtin_mysqli_get_server_info,
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
        "mysqli_more_results",
        builtin_mysqli_more_results,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_multi_query",
        builtin_mysqli_multi_query,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_next_result",
        builtin_mysqli_next_result,
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
        "mysqli_options",
        builtin_mysqli_options,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_ping",
        builtin_mysqli_ping,
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
        "mysqli_rollback",
        builtin_mysqli_rollback,
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
        "mysqli_store_result",
        builtin_mysqli_store_result,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_stmt_affected_rows",
        builtin_mysqli_stmt_affected_rows,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_stmt_bind_param",
        builtin_mysqli_stmt_bind_param,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_stmt_bind_result",
        builtin_mysqli_stmt_bind_result,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_stmt_close",
        builtin_mysqli_stmt_close,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_stmt_errno",
        builtin_mysqli_stmt_errno,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_stmt_error",
        builtin_mysqli_stmt_error,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_stmt_execute",
        builtin_mysqli_stmt_execute,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_stmt_fetch",
        builtin_mysqli_stmt_fetch,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_stmt_free_result",
        builtin_mysqli_stmt_free_result,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_stmt_get_result",
        builtin_mysqli_stmt_get_result,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_stmt_init",
        builtin_mysqli_stmt_init,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_stmt_insert_id",
        builtin_mysqli_stmt_insert_id,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_stmt_num_rows",
        builtin_mysqli_stmt_num_rows,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_stmt_prepare",
        builtin_mysqli_stmt_prepare,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_stmt_result_metadata",
        builtin_mysqli_stmt_result_metadata,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_stmt_sqlstate",
        builtin_mysqli_stmt_sqlstate,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "mysqli_use_result",
        builtin_mysqli_use_result,
        BuiltinCompatibility::Php,
    ),
];

pub(in crate::builtins::modules) fn builtin_mysqli_connect(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_connect", args.len(), 0, 6)?;
    connect_from_mysqli_args(context, "mysqli_connect", &args, span)
}

pub(in crate::builtins::modules) fn builtin_mysqli_init(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_init", &args, 0)?;
    Ok(Value::Object(mysqli_object(None)))
}

pub(in crate::builtins::modules) fn builtin_mysqli_get_client_info(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_get_client_info", args.len(), 0, 1)?;
    Ok(Value::string(MYSQLND_CLIENT_INFO))
}

pub(in crate::builtins::modules) fn builtin_mysqli_get_client_version(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_get_client_version", &args, 0)?;
    Ok(Value::Int(MYSQLND_CLIENT_VERSION))
}

pub(in crate::builtins::modules) fn builtin_mysqli_get_client_stats(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_get_client_stats", &args, 0)?;
    Ok(mysqli_stats_array(context.mysql_state().is_some(), None))
}

pub(in crate::builtins::modules) fn builtin_mysqli_get_connection_stats(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_get_connection_stats", &args, 1)?;
    let object = mysqli_object_arg("mysqli_get_connection_stats", args.first())?;
    Ok(mysqli_stats_array(
        context.mysql_state().is_some(),
        mysqli_connection_id(&object),
    ))
}

pub(in crate::builtins::modules) fn builtin_mysqli_real_connect(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_real_connect", args.len(), 1, 8)?;
    let object = mysqli_object_arg("mysqli_real_connect", args.first())?;
    let connection = connect_from_mysqli_args(context, "mysqli_real_connect", &args[1..], span)?;
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
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_query", &args, 2)?;
    let object = mysqli_object_arg("mysqli_query", args.first())?;
    let Some(id) = mysqli_connection_id(&object) else {
        record_mysqli_diagnostic(
            context,
            mysqli_diagnostic_meta(
                "E_PHP_MYSQLI_QUERY_FAILED",
                "mysqli_query",
                "query",
                "invalid_handle",
                MysqliDiagnosticTarget::default(),
            )
            .with_mysql_error(1, "HY000", "not an open MySQL connection"),
            span,
        );
        return Ok(Value::Bool(false));
    };
    let sql = string_arg("mysqli_query", &args[1])?.to_string_lossy();
    let Some(state) = context.mysql_state() else {
        record_mysqli_diagnostic(
            context,
            mysqli_diagnostic_meta(
                "E_PHP_MYSQLI_CAPABILITY_DISABLED",
                "mysqli_query",
                "query",
                "runtime_state_unavailable",
                MysqliDiagnosticTarget::default(),
            )
            .with_mysql_error(2002, "HY000", "mysqli runtime state is unavailable"),
            span,
        );
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
        Err(error) => {
            sync_mysqli_status_properties(&object, state);
            let error = error.clone();
            let _ = state;
            record_mysqli_error_diagnostic(
                context,
                mysqli_diagnostic_meta(
                    "E_PHP_MYSQLI_QUERY_FAILED",
                    "mysqli_query",
                    "query",
                    "enabled",
                    MysqliDiagnosticTarget::default_enabled(),
                ),
                &error,
                span,
            );
            Ok(Value::Bool(false))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_mysqli_fetch_assoc(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_fetch_assoc", &args, 1)?;
    fetch_array(context, args.first(), MYSQLI_ASSOC)
}

pub(in crate::builtins::modules) fn builtin_mysqli_fetch_row(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_fetch_row", &args, 1)?;
    fetch_array(context, args.first(), MYSQLI_NUM)
}

pub(in crate::builtins::modules) fn builtin_mysqli_fetch_array(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
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

pub(in crate::builtins::modules) fn builtin_mysqli_fetch_object(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_fetch_object", args.len(), 1, 3)?;
    let row = fetch_array(context, args.first(), MYSQLI_ASSOC)?;
    let Value::Array(row) = row else {
        return Ok(Value::Bool(false));
    };
    let class_name = args
        .get(1)
        .map(|value| string_arg("mysqli_fetch_object", value).map(|value| value.to_string_lossy()))
        .transpose()?
        .unwrap_or_else(|| "stdClass".to_owned());
    let object = ObjectRef::new_with_display_name(&mysqli_runtime_class(&class_name), &class_name);
    for (key, value) in row.iter() {
        if let ArrayKey::String(name) = key {
            object.set_property(name.to_string_lossy(), value.clone());
        }
    }
    Ok(Value::Object(object))
}

pub(in crate::builtins::modules) fn builtin_mysqli_data_seek(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_data_seek", &args, 2)?;
    let result = mysqli_result_object_arg("mysqli_data_seek", args.first())?;
    let offset = usize::try_from(int_arg("mysqli_data_seek", &args[1])?).unwrap_or(usize::MAX);
    let Some(id) = mysqli_result_id(&result) else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool(
        context
            .mysql_state()
            .is_some_and(|state| state.data_seek(id, offset)),
    ))
}

pub(in crate::builtins::modules) fn builtin_mysqli_num_rows(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
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
    args: crate::builtins::BuiltinArgs,
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

pub(in crate::builtins::modules) fn builtin_mysqli_field_count(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_field_count", &args, 1)?;
    let object = mysqli_object_arg("mysqli_field_count", args.first())?;
    let Some(id) = mysqli_connection_id(&object) else {
        return Ok(Value::Int(0));
    };
    Ok(Value::Int(
        context
            .mysql_state()
            .map_or(0, |state| state.field_count(id)),
    ))
}

pub(in crate::builtins::modules) fn builtin_mysqli_fetch_fields(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_fetch_fields", &args, 1)?;
    let result = mysqli_result_object_arg("mysqli_fetch_fields", args.first())?;
    let Some(id) = mysqli_result_id(&result) else {
        return Ok(Value::packed_array(Vec::new()));
    };
    let mut fields = PhpArray::new();
    if let Some(state) = context.mysql_state() {
        for name in state.field_names(id) {
            let field =
                ObjectRef::new_with_display_name(&mysqli_runtime_class("stdClass"), "stdClass");
            field.set_property("name", Value::String(PhpString::from(name.into_bytes())));
            fields.append(Value::Object(field));
        }
    }
    Ok(Value::Array(fields))
}

pub(in crate::builtins::modules) fn builtin_mysqli_free_result(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
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
    args: crate::builtins::BuiltinArgs,
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
    args: crate::builtins::BuiltinArgs,
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
    args: crate::builtins::BuiltinArgs,
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
    args: crate::builtins::BuiltinArgs,
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

pub(in crate::builtins::modules) fn builtin_mysqli_character_set_name(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_character_set_name", &args, 1)?;
    let _object = mysqli_object_arg("mysqli_character_set_name", args.first())?;
    Ok(Value::string("utf8mb4"))
}

pub(in crate::builtins::modules) fn builtin_mysqli_get_host_info(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_get_host_info", &args, 1)?;
    let _object = mysqli_object_arg("mysqli_get_host_info", args.first())?;
    Ok(Value::string("localhost via TCP/IP"))
}

pub(in crate::builtins::modules) fn builtin_mysqli_affected_rows(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
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
    args: crate::builtins::BuiltinArgs,
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

pub(in crate::builtins::modules) fn builtin_mysqli_more_results(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_more_results", &args, 1)?;
    let object = mysqli_object_arg("mysqli_more_results", args.first())?;
    let Some(id) = mysqli_connection_id(&object) else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool(
        context
            .mysql_state()
            .is_some_and(|state| state.more_results(id)),
    ))
}

pub(in crate::builtins::modules) fn builtin_mysqli_connect_errno(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
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
    args: crate::builtins::BuiltinArgs,
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
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_real_escape_string", &args, 2)?;
    let _object = mysqli_object_arg("mysqli_real_escape_string", args.first())?;
    let value = string_arg("mysqli_real_escape_string", &args[1])?;
    Ok(Value::string(mysql_escape_string(value.as_bytes())))
}

pub(in crate::builtins::modules) fn builtin_mysqli_report(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_report", &args, 1)?;
    let flags = int_arg("mysqli_report", &args[0])?;
    if let Some(state) = context.mysql_state() {
        state.set_report_flags(flags);
    }
    Ok(Value::Bool(true))
}

pub(in crate::builtins::modules) fn builtin_mysqli_options(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_options", &args, 3)?;
    let object = mysqli_object_arg("mysqli_options", args.first())?;
    let option = int_arg("mysqli_options", &args[1])?;
    set_mysqli_option(&object, option, args[2].clone());
    Ok(Value::Bool(true))
}

pub(in crate::builtins::modules) fn builtin_mysqli_ping(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_ping", &args, 1)?;
    mysqli_connection_operation(
        context,
        args.first(),
        "mysqli_ping",
        "ping",
        span,
        |state, id| state.ping(id),
    )
}

pub(in crate::builtins::modules) fn builtin_mysqli_autocommit(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_autocommit", &args, 2)?;
    let mode = mysqli_bool_arg("mysqli_autocommit", &args[1])?;
    mysqli_connection_operation(
        context,
        args.first(),
        "mysqli_autocommit",
        "autocommit",
        span,
        |state, id| state.autocommit(id, mode),
    )
}

pub(in crate::builtins::modules) fn builtin_mysqli_begin_transaction(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_begin_transaction", args.len(), 1, 3)?;
    mysqli_connection_operation(
        context,
        args.first(),
        "mysqli_begin_transaction",
        "begin_transaction",
        span,
        |state, id| state.begin_transaction(id),
    )
}

pub(in crate::builtins::modules) fn builtin_mysqli_commit(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_commit", args.len(), 1, 3)?;
    mysqli_connection_operation(
        context,
        args.first(),
        "mysqli_commit",
        "commit",
        span,
        |state, id| state.commit(id),
    )
}

pub(in crate::builtins::modules) fn builtin_mysqli_rollback(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_rollback", args.len(), 1, 3)?;
    mysqli_connection_operation(
        context,
        args.first(),
        "mysqli_rollback",
        "rollback",
        span,
        |state, id| state.rollback(id),
    )
}

pub(in crate::builtins::modules) fn builtin_mysqli_multi_query(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_multi_query", &args, 2)?;
    let sql = string_arg("mysqli_multi_query", &args[1])?.to_string_lossy();
    mysqli_connection_operation(
        context,
        args.first(),
        "mysqli_multi_query",
        "multi_query",
        span,
        |state, id| state.multi_query(id, &sql).map(|_| ()),
    )
}

pub(in crate::builtins::modules) fn builtin_mysqli_next_result(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_next_result", &args, 1)?;
    let object = mysqli_object_arg("mysqli_next_result", args.first())?;
    let Some(id) = mysqli_connection_id(&object) else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool(
        context
            .mysql_state()
            .is_some_and(|state| state.next_result(id)),
    ))
}

pub(in crate::builtins::modules) fn builtin_mysqli_store_result(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_store_result", args.len(), 1, 2)?;
    if args
        .get(1)
        .map(|value| int_arg("mysqli_store_result", value))
        .transpose()?
        .is_some_and(|mode| mode != MYSQLI_STORE_RESULT && mode != MYSQLI_USE_RESULT)
    {
        return Ok(Value::Bool(false));
    }
    mysqli_buffered_result_from_connection(context, "mysqli_store_result", args.first())
}

pub(in crate::builtins::modules) fn builtin_mysqli_use_result(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_use_result", &args, 1)?;
    mysqli_buffered_result_from_connection(context, "mysqli_use_result", args.first())
}

pub(in crate::builtins::modules) fn builtin_mysqli_select_db(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_select_db", &args, 2)?;
    let object = mysqli_object_arg("mysqli_select_db", args.first())?;
    let Some(id) = mysqli_connection_id(&object) else {
        return Ok(Value::Bool(false));
    };
    let database = string_arg("mysqli_select_db", &args[1])?.to_string_lossy();
    let Some(state) = context.mysql_state() else {
        record_mysqli_diagnostic(
            context,
            mysqli_diagnostic_meta(
                "E_PHP_MYSQLI_CAPABILITY_DISABLED",
                "mysqli_select_db",
                "select_db",
                "runtime_state_unavailable",
                MysqliDiagnosticTarget::default(),
            )
            .with_mysql_error(2002, "HY000", "mysqli runtime state is unavailable"),
            span,
        );
        return Ok(Value::Bool(false));
    };
    match state.select_db(id, &database) {
        Ok(()) => Ok(Value::Bool(true)),
        Err(error) => {
            let error = error.clone();
            let _ = state;
            record_mysqli_error_diagnostic(
                context,
                mysqli_diagnostic_meta(
                    "E_PHP_MYSQLI_QUERY_FAILED",
                    "mysqli_select_db",
                    "select_db",
                    "enabled",
                    MysqliDiagnosticTarget::default_enabled(),
                ),
                &error,
                span,
            );
            Ok(Value::Bool(false))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_mysqli_set_charset(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_set_charset", &args, 2)?;
    let object = mysqli_object_arg("mysqli_set_charset", args.first())?;
    let Some(id) = mysqli_connection_id(&object) else {
        return Ok(Value::Bool(false));
    };
    let charset = string_arg("mysqli_set_charset", &args[1])?.to_string_lossy();
    let Some(state) = context.mysql_state() else {
        record_mysqli_diagnostic(
            context,
            mysqli_diagnostic_meta(
                "E_PHP_MYSQLI_CAPABILITY_DISABLED",
                "mysqli_set_charset",
                "set_charset",
                "runtime_state_unavailable",
                MysqliDiagnosticTarget::default(),
            )
            .with_mysql_error(2002, "HY000", "mysqli runtime state is unavailable"),
            span,
        );
        return Ok(Value::Bool(false));
    };
    match state.set_charset(id, &charset) {
        Ok(()) => Ok(Value::Bool(true)),
        Err(error) => {
            let error = error.clone();
            let _ = state;
            record_mysqli_error_diagnostic(
                context,
                mysqli_diagnostic_meta(
                    "E_PHP_MYSQLI_QUERY_FAILED",
                    "mysqli_set_charset",
                    "set_charset",
                    "enabled",
                    MysqliDiagnosticTarget::default_enabled(),
                ),
                &error,
                span,
            );
            Ok(Value::Bool(false))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_mysqli_prepare(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_prepare", &args, 2)?;
    let object = mysqli_object_arg("mysqli_prepare", args.first())?;
    let Some(connection_id) = mysqli_connection_id(&object) else {
        return Ok(Value::Bool(false));
    };
    let sql = string_arg("mysqli_prepare", &args[1])?.to_string_lossy();
    let Some(state) = context.mysql_state() else {
        record_mysqli_diagnostic(
            context,
            mysqli_diagnostic_meta(
                "E_PHP_MYSQLI_CAPABILITY_DISABLED",
                "mysqli_prepare",
                "prepare",
                "runtime_state_unavailable",
                MysqliDiagnosticTarget::default(),
            )
            .with_mysql_error(2002, "HY000", "mysqli runtime state is unavailable"),
            span,
        );
        return Ok(Value::Bool(false));
    };
    match state.prepare_statement(connection_id, &sql) {
        Ok(statement_id) => Ok(Value::Object(mysqli_stmt_object(statement_id))),
        Err(error) => {
            sync_mysqli_status_properties(&object, state);
            let error = error.clone();
            let _ = state;
            record_mysqli_error_diagnostic(
                context,
                mysqli_diagnostic_meta(
                    "E_PHP_MYSQLI_PREPARE_FAILED",
                    "mysqli_prepare",
                    "prepare",
                    "enabled",
                    MysqliDiagnosticTarget::default_enabled(),
                ),
                &error,
                span,
            );
            Ok(Value::Bool(false))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_mysqli_stmt_init(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_stmt_init", &args, 1)?;
    let object = mysqli_object_arg("mysqli_stmt_init", args.first())?;
    let Some(connection_id) = mysqli_connection_id(&object) else {
        return Ok(Value::Bool(false));
    };
    let Some(state) = context.mysql_state() else {
        return Ok(Value::Bool(false));
    };
    match state.stmt_init(connection_id) {
        Ok(statement_id) => Ok(Value::Object(mysqli_stmt_object(statement_id))),
        Err(_) => Ok(Value::Bool(false)),
    }
}

pub(in crate::builtins::modules) fn builtin_mysqli_stmt_prepare(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_stmt_prepare", &args, 2)?;
    let stmt = mysqli_stmt_object_arg("mysqli_stmt_prepare", args.first())?;
    let Some(statement_id) = mysqli_stmt_id(&stmt) else {
        return Ok(Value::Bool(false));
    };
    let sql = string_arg("mysqli_stmt_prepare", &args[1])?.to_string_lossy();
    let Some(state) = context.mysql_state() else {
        record_mysqli_diagnostic(
            context,
            mysqli_diagnostic_meta(
                "E_PHP_MYSQLI_CAPABILITY_DISABLED",
                "mysqli_stmt_prepare",
                "stmt_prepare",
                "runtime_state_unavailable",
                MysqliDiagnosticTarget::default(),
            )
            .with_mysql_error(2002, "HY000", "mysqli runtime state is unavailable"),
            span,
        );
        return Ok(Value::Bool(false));
    };
    match state.stmt_prepare(statement_id, &sql) {
        Ok(()) => {
            sync_mysqli_stmt_properties(&stmt, state);
            Ok(Value::Bool(true))
        }
        Err(error) => {
            sync_mysqli_stmt_properties(&stmt, state);
            let error = error.clone();
            let _ = state;
            record_mysqli_error_diagnostic(
                context,
                mysqli_diagnostic_meta(
                    "E_PHP_MYSQLI_PREPARE_FAILED",
                    "mysqli_stmt_prepare",
                    "stmt_prepare",
                    "enabled",
                    MysqliDiagnosticTarget::default_enabled(),
                ),
                &error,
                span,
            );
            Ok(Value::Bool(false))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_mysqli_stmt_bind_param(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_stmt_bind_param", args.len(), 3, usize::MAX)?;
    let stmt = mysqli_stmt_object_arg("mysqli_stmt_bind_param", args.first())?;
    let types = string_arg("mysqli_stmt_bind_param", &args[1])?.to_string_lossy();
    if types.chars().count() != args.len().saturating_sub(2) {
        record_mysqli_diagnostic(
            context,
            mysqli_diagnostic_meta(
                "E_PHP_MYSQLI_STMT_BIND_FAILED",
                "mysqli_stmt_bind_param",
                "stmt_bind_param",
                "enabled",
                MysqliDiagnosticTarget::default_enabled(),
            )
            .with_mysql_error(
                1,
                "HY000",
                "mysqli_stmt_bind_param type string length does not match bound parameters",
            ),
            span,
        );
        return Ok(Value::Bool(false));
    }
    let refs = collect_reference_args("mysqli_stmt_bind_param", &args[2..])?;
    set_stmt_refs(&stmt, "__mysqli_stmt_param_refs", refs);
    stmt.set_property(
        "__mysqli_stmt_param_types",
        Value::String(PhpString::from(types.into_bytes())),
    );
    Ok(Value::Bool(true))
}

pub(in crate::builtins::modules) fn builtin_mysqli_stmt_execute(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_stmt_execute", args.len(), 1, 2)?;
    let stmt = mysqli_stmt_object_arg("mysqli_stmt_execute", args.first())?;
    let Some(statement_id) = mysqli_stmt_id(&stmt) else {
        return Ok(Value::Bool(false));
    };
    let params: Vec<Value> = if let Some(Value::Array(params)) = args.get(1) {
        params.iter().map(|(_, value)| value.clone()).collect()
    } else {
        stmt_refs(&stmt, "__mysqli_stmt_param_refs")
            .into_iter()
            .map(|cell| cell.get())
            .collect()
    };
    let Some(state) = context.mysql_state() else {
        record_mysqli_diagnostic(
            context,
            mysqli_diagnostic_meta(
                "E_PHP_MYSQLI_CAPABILITY_DISABLED",
                "mysqli_stmt_execute",
                "stmt_execute",
                "runtime_state_unavailable",
                MysqliDiagnosticTarget::default(),
            )
            .with_mysql_error(2002, "HY000", "mysqli runtime state is unavailable"),
            span,
        );
        return Ok(Value::Bool(false));
    };
    match state.stmt_execute(statement_id, &params) {
        Ok(ok) => {
            sync_mysqli_stmt_properties(&stmt, state);
            Ok(Value::Bool(ok))
        }
        Err(error) => {
            sync_mysqli_stmt_properties(&stmt, state);
            let error = error.clone();
            let _ = state;
            record_mysqli_error_diagnostic(
                context,
                mysqli_diagnostic_meta(
                    "E_PHP_MYSQLI_STMT_EXECUTE_FAILED",
                    "mysqli_stmt_execute",
                    "stmt_execute",
                    "enabled",
                    MysqliDiagnosticTarget::default_enabled(),
                ),
                &error,
                span,
            );
            Ok(Value::Bool(false))
        }
    }
}

pub(in crate::builtins::modules) fn builtin_mysqli_stmt_get_result(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_stmt_get_result", &args, 1)?;
    let stmt = mysqli_stmt_object_arg("mysqli_stmt_get_result", args.first())?;
    let Some(statement_id) = mysqli_stmt_id(&stmt) else {
        return Ok(Value::Bool(false));
    };
    let Some(state) = context.mysql_state() else {
        return Ok(Value::Bool(false));
    };
    let Some(result_id) = state.stmt_result(statement_id) else {
        return Ok(Value::Bool(false));
    };
    let result = mysqli_result_object(result_id);
    result.set_property("num_rows", Value::Int(state.num_rows(result_id)));
    Ok(Value::Object(result))
}

pub(in crate::builtins::modules) fn builtin_mysqli_stmt_result_metadata(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_stmt_result_metadata", &args, 1)?;
    let stmt = mysqli_stmt_object_arg("mysqli_stmt_result_metadata", args.first())?;
    let Some(statement_id) = mysqli_stmt_id(&stmt) else {
        return Ok(Value::Bool(false));
    };
    let Some(state) = context.mysql_state() else {
        return Ok(Value::Bool(false));
    };
    let Some(result_id) = state.stmt_result(statement_id) else {
        return Ok(Value::Bool(false));
    };
    let result = mysqli_result_object(result_id);
    result.set_property("num_rows", Value::Int(state.num_rows(result_id)));
    Ok(Value::Object(result))
}

pub(in crate::builtins::modules) fn builtin_mysqli_stmt_bind_result(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_mysqli_arity("mysqli_stmt_bind_result", args.len(), 2, usize::MAX)?;
    let stmt = mysqli_stmt_object_arg("mysqli_stmt_bind_result", args.first())?;
    let refs = collect_reference_args("mysqli_stmt_bind_result", &args[1..])?;
    set_stmt_refs(&stmt, "__mysqli_stmt_result_refs", refs);
    Ok(Value::Bool(true))
}

pub(in crate::builtins::modules) fn builtin_mysqli_stmt_fetch(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_stmt_fetch", &args, 1)?;
    let stmt = mysqli_stmt_object_arg("mysqli_stmt_fetch", args.first())?;
    let Some(statement_id) = mysqli_stmt_id(&stmt) else {
        return Ok(Value::Bool(false));
    };
    let refs = stmt_refs(&stmt, "__mysqli_stmt_result_refs");
    let Some(state) = context.mysql_state() else {
        return Ok(Value::Bool(false));
    };
    let Some(row) = state.stmt_fetch_row(statement_id) else {
        return Ok(Value::Bool(false));
    };
    for (cell, value) in refs.into_iter().zip(row) {
        cell.set(value);
    }
    Ok(Value::Bool(true))
}

pub(in crate::builtins::modules) fn builtin_mysqli_stmt_num_rows(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_stmt_num_rows", &args, 1)?;
    stmt_int_status(
        context,
        args.first(),
        "mysqli_stmt_num_rows",
        |state, id| state.stmt_num_rows(id),
    )
}

pub(in crate::builtins::modules) fn builtin_mysqli_stmt_affected_rows(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_stmt_affected_rows", &args, 1)?;
    stmt_int_status(
        context,
        args.first(),
        "mysqli_stmt_affected_rows",
        |state, id| state.stmt_affected_rows(id),
    )
}

pub(in crate::builtins::modules) fn builtin_mysqli_stmt_insert_id(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_stmt_insert_id", &args, 1)?;
    stmt_int_status(
        context,
        args.first(),
        "mysqli_stmt_insert_id",
        |state, id| state.stmt_insert_id(id),
    )
}

pub(in crate::builtins::modules) fn builtin_mysqli_stmt_errno(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_stmt_errno", &args, 1)?;
    stmt_int_status(context, args.first(), "mysqli_stmt_errno", |state, id| {
        state.stmt_errno(id)
    })
}

pub(in crate::builtins::modules) fn builtin_mysqli_stmt_error(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_stmt_error", &args, 1)?;
    stmt_string_status(context, args.first(), "mysqli_stmt_error", |state, id| {
        state.stmt_error(id)
    })
}

pub(in crate::builtins::modules) fn builtin_mysqli_stmt_sqlstate(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_stmt_sqlstate", &args, 1)?;
    stmt_string_status(
        context,
        args.first(),
        "mysqli_stmt_sqlstate",
        |state, id| state.stmt_sqlstate(id),
    )
}

pub(in crate::builtins::modules) fn builtin_mysqli_stmt_close(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_stmt_close", &args, 1)?;
    let stmt = mysqli_stmt_object_arg("mysqli_stmt_close", args.first())?;
    let Some(statement_id) = mysqli_stmt_id(&stmt) else {
        return Ok(Value::Bool(false));
    };
    stmt.unset_property("__mysqli_stmt");
    Ok(Value::Bool(
        context
            .mysql_state()
            .is_some_and(|state| state.stmt_close(statement_id)),
    ))
}

pub(in crate::builtins::modules) fn builtin_mysqli_stmt_free_result(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("mysqli_stmt_free_result", &args, 1)?;
    let stmt = mysqli_stmt_object_arg("mysqli_stmt_free_result", args.first())?;
    let Some(statement_id) = mysqli_stmt_id(&stmt) else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool(
        context
            .mysql_state()
            .is_some_and(|state| state.stmt_free_result(statement_id)),
    ))
}

fn connect_from_mysqli_args(
    context: &mut BuiltinContext<'_>,
    function_name: &'static str,
    args: &[Value],
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    let target = MysqliDiagnosticTarget::from_args(args);
    if context.mysql_state().is_none() {
        record_mysqli_diagnostic(
            context,
            mysqli_diagnostic_meta(
                "E_PHP_MYSQLI_CAPABILITY_DISABLED",
                function_name,
                "connect",
                "runtime_state_unavailable",
                target,
            )
            .with_mysql_error(2002, "HY000", "mysqli runtime state is unavailable"),
            span,
        );
        return Ok(Value::Bool(false));
    }
    if mysqli_sqlite_compat_enabled() {
        let result = {
            let state = context.mysql_state().expect("mysql state checked above");
            state.connect_sqlite_compat()
        };
        return match result {
            Ok(id) => Ok(Value::Object(mysqli_object(Some(id)))),
            Err(error) => {
                record_mysqli_error_diagnostic(
                    context,
                    mysqli_diagnostic_meta(
                        "E_PHP_MYSQLI_CONNECTION_FAILED",
                        function_name,
                        "connect",
                        "sqlite_compat_enabled",
                        target,
                    ),
                    &error,
                    span,
                );
                Ok(Value::Bool(false))
            }
        };
    }
    let options = if args.is_empty() {
        let Some(options) = MysqlConnectOptions::from_test_env() else {
            let message = format!(
                "live mysqli connections require {MYSQL_TEST_DSN_ENV} or mysqli connection arguments; selected SQLite compatibility fixtures require {MYSQLI_SQLITE_COMPAT_ENV}=1"
            );
            if let Some(state) = context.mysql_state() {
                state.record_connect_error(2002, message.clone());
            }
            record_mysqli_diagnostic(
                context,
                mysqli_diagnostic_meta(
                    "E_PHP_MYSQLI_CAPABILITY_DISABLED",
                    function_name,
                    "connect",
                    "disabled",
                    target,
                )
                .with_mysql_error(2002, "HY000", message),
                span,
            );
            return Ok(Value::Bool(false));
        };
        options
    } else {
        mysqli_options_from_args(args)?
    };
    match options {
        Ok(options) => {
            let result = {
                let state = context.mysql_state().expect("mysql state checked above");
                state.connect(&options)
            };
            match result {
                Ok(id) => Ok(Value::Object(mysqli_object(Some(id)))),
                Err(error) => {
                    record_mysqli_error_diagnostic(
                        context,
                        mysqli_diagnostic_meta(
                            "E_PHP_MYSQLI_CONNECTION_FAILED",
                            function_name,
                            "connect",
                            "enabled",
                            target,
                        ),
                        &error,
                        span,
                    );
                    Ok(Value::Bool(false))
                }
            }
        }
        Err(error) => {
            if let Some(state) = context.mysql_state() {
                state.record_connect_error(error.mysql_errno(), error.message.clone());
            }
            record_mysqli_error_diagnostic(
                context,
                mysqli_diagnostic_meta(
                    "E_PHP_MYSQLI_CONNECTION_FAILED",
                    function_name,
                    "connect",
                    "enabled",
                    target,
                ),
                &error,
                span,
            );
            Ok(Value::Bool(false))
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct MysqliDiagnosticTarget {
    host: String,
    port: Option<u16>,
    database: Option<String>,
    dsn_present: bool,
}

impl MysqliDiagnosticTarget {
    fn from_args(args: &[Value]) -> Self {
        if args.is_empty() {
            return Self {
                host: String::new(),
                port: None,
                database: None,
                dsn_present: env::var(MYSQL_TEST_DSN_ENV).is_ok(),
            };
        }
        Self {
            host: diagnostic_string_arg(args.first()).unwrap_or_else(|| "localhost".to_owned()),
            port: args
                .get(4)
                .and_then(diagnostic_int_arg)
                .and_then(|port| u16::try_from(port).ok()),
            database: args
                .get(3)
                .and_then(|value| diagnostic_string_arg(Some(value))),
            dsn_present: true,
        }
    }

    fn default_enabled() -> Self {
        Self {
            dsn_present: true,
            ..Self::default()
        }
    }
}

fn diagnostic_string_arg(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(value)) if !value.is_empty() => Some(value.to_string_lossy()),
        Some(Value::Int(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn diagnostic_int_arg(value: &Value) -> Option<i64> {
    match value {
        Value::Int(value) => Some(*value),
        Value::String(value) => value.to_string_lossy().parse().ok(),
        _ => None,
    }
}

struct MysqliDiagnosticMeta {
    diagnostic_id: &'static str,
    function_name: &'static str,
    operation: &'static str,
    capability_state: &'static str,
    target: MysqliDiagnosticTarget,
}

impl MysqliDiagnosticMeta {
    fn with_mysql_error(
        self,
        mysql_error_code: i64,
        mysql_sqlstate: impl Into<String>,
        mysql_error_message: impl Into<String>,
    ) -> MysqliDiagnostic {
        MysqliDiagnostic {
            meta: self,
            mysql_error_code,
            mysql_sqlstate: mysql_sqlstate.into(),
            mysql_error_message: mysql_error_message.into(),
        }
    }

    fn with_error_detail(self, error: &MysqlError) -> MysqliDiagnostic {
        self.with_mysql_error(
            error.mysql_errno(),
            error.mysql_sqlstate(),
            error.message.clone(),
        )
    }
}

fn mysqli_diagnostic_meta(
    diagnostic_id: &'static str,
    function_name: &'static str,
    operation: &'static str,
    capability_state: &'static str,
    target: MysqliDiagnosticTarget,
) -> MysqliDiagnosticMeta {
    MysqliDiagnosticMeta {
        diagnostic_id,
        function_name,
        operation,
        capability_state,
        target,
    }
}

struct MysqliDiagnostic {
    meta: MysqliDiagnosticMeta,
    mysql_error_code: i64,
    mysql_sqlstate: String,
    mysql_error_message: String,
}

fn record_mysqli_error_diagnostic(
    context: &mut BuiltinContext<'_>,
    diagnostic: MysqliDiagnosticMeta,
    error: &MysqlError,
    span: RuntimeSourceSpan,
) {
    record_mysqli_diagnostic(context, diagnostic.with_error_detail(error), span);
}

fn record_mysqli_diagnostic(
    context: &mut BuiltinContext<'_>,
    diagnostic: MysqliDiagnostic,
    span: RuntimeSourceSpan,
) {
    let MysqliDiagnostic {
        meta,
        mysql_error_code,
        mysql_sqlstate,
        mysql_error_message,
    } = diagnostic;
    let MysqliDiagnosticMeta {
        diagnostic_id,
        function_name,
        operation,
        capability_state,
        target,
    } = meta;
    let report_flags = context
        .mysql_state()
        .map_or(0, |state| state.report_flags());
    if report_flags == MYSQLI_REPORT_OFF && capability_state == "enabled" {
        return;
    }
    let severity = if report_flags & (MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT) != 0 {
        RuntimeSeverity::RecoverableError
    } else {
        RuntimeSeverity::Warning
    };
    let mysql_error_message = sanitize_mysql_error(&mysql_error_message);
    let payload = RuntimeBringupDiagnosticContext::new("db_network")
        .with_field("diagnostic_id", diagnostic_id)
        .with_field("function_name", function_name)
        .with_field("operation", operation)
        .with_field("capability_state", capability_state)
        .with_field("mysqli_report_flags", report_flags.to_string())
        .with_field("dsn_present_boolean", target.dsn_present.to_string())
        .with_field("host", target.host)
        .with_field(
            "port",
            target.port.map(|port| port.to_string()).unwrap_or_default(),
        )
        .with_field(
            "database_name_if_nonsecret",
            target.database.unwrap_or_default(),
        )
        .with_field("mysql_error_code", mysql_error_code.to_string())
        .with_field("mysql_sqlstate", mysql_sqlstate)
        .with_field("mysql_error_message", mysql_error_message.clone());
    context.record_diagnostic(
        RuntimeDiagnostic::new(
            diagnostic_id,
            severity,
            mysql_error_message,
            span,
            Vec::new(),
            Some(crate::PhpReferenceClassification::Warning),
        )
        .with_diagnostic_payload(RuntimeDiagnosticPayload::Bringup(payload)),
    );
}

fn sanitize_mysql_error(message: &str) -> String {
    let sanitized = if message.contains("mysql://") || message.contains('@') {
        "MySQL connection failed; connection credentials were redacted"
    } else {
        message
    };
    sanitized.chars().take(512).collect()
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
    let socket = args
        .get(5)
        .and_then(|value| {
            if matches!(value, Value::Null) {
                None
            } else {
                Some(value)
            }
        })
        .map(|value| string_arg("mysqli_real_connect", value).map(|value| value.to_string_lossy()))
        .transpose()?;

    Ok(MysqlConnectOptions::from_parts_with_socket(
        &host,
        &user,
        &password,
        database.as_deref(),
        port,
        socket.as_deref(),
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

fn mysqli_connection_operation(
    context: &mut BuiltinContext<'_>,
    value: Option<&Value>,
    function_name: &'static str,
    operation: &'static str,
    span: RuntimeSourceSpan,
    f: impl FnOnce(&mut crate::MysqlState, i64) -> Result<(), MysqlError>,
) -> BuiltinResult {
    let object = mysqli_object_arg(function_name, value)?;
    let Some(id) = mysqli_connection_id(&object) else {
        record_mysqli_diagnostic(
            context,
            mysqli_diagnostic_meta(
                "E_PHP_MYSQLI_QUERY_FAILED",
                function_name,
                operation,
                "invalid_handle",
                MysqliDiagnosticTarget::default(),
            )
            .with_mysql_error(1, "HY000", "not an open MySQL connection"),
            span,
        );
        return Ok(Value::Bool(false));
    };
    let Some(state) = context.mysql_state() else {
        record_mysqli_diagnostic(
            context,
            mysqli_diagnostic_meta(
                "E_PHP_MYSQLI_CAPABILITY_DISABLED",
                function_name,
                operation,
                "runtime_state_unavailable",
                MysqliDiagnosticTarget::default(),
            )
            .with_mysql_error(2002, "HY000", "mysqli runtime state is unavailable"),
            span,
        );
        return Ok(Value::Bool(false));
    };
    match f(state, id) {
        Ok(()) => {
            sync_mysqli_status_properties(&object, state);
            Ok(Value::Bool(true))
        }
        Err(error) => {
            sync_mysqli_status_properties(&object, state);
            let error = error.clone();
            let _ = state;
            record_mysqli_error_diagnostic(
                context,
                mysqli_diagnostic_meta(
                    "E_PHP_MYSQLI_QUERY_FAILED",
                    function_name,
                    operation,
                    "enabled",
                    MysqliDiagnosticTarget::default_enabled(),
                ),
                &error,
                span,
            );
            Ok(Value::Bool(false))
        }
    }
}

fn mysqli_buffered_result_from_connection(
    context: &mut BuiltinContext<'_>,
    function_name: &'static str,
    value: Option<&Value>,
) -> BuiltinResult {
    let object = mysqli_object_arg(function_name, value)?;
    let Some(id) = mysqli_connection_id(&object) else {
        return Ok(Value::Bool(false));
    };
    let Some(result_id) = context
        .mysql_state()
        .and_then(|state| state.store_result(id))
    else {
        return Ok(Value::Bool(false));
    };
    let result = mysqli_result_object(result_id);
    if let Some(state) = context.mysql_state() {
        result.set_property("num_rows", Value::Int(state.num_rows(result_id)));
    }
    Ok(Value::Object(result))
}

fn mysqli_bool_arg(name: &str, value: &Value) -> Result<bool, BuiltinError> {
    Ok(match value {
        Value::Bool(value) => *value,
        Value::Int(value) => *value != 0,
        Value::Null | Value::Uninitialized => false,
        Value::String(value) => !value.is_empty() && value.as_bytes() != b"0",
        other => {
            return Err(argument_type_error(name, "2", "bool", other));
        }
    })
}

fn set_mysqli_option(object: &ObjectRef, option: i64, value: Value) {
    let mut options = match object.get_property("__mysqli_options") {
        Some(Value::Array(options)) => options,
        _ => PhpArray::new(),
    };
    options.insert(ArrayKey::Int(option), value);
    object.set_property("__mysqli_options", Value::Array(options));
}

fn mysqli_stats_array(has_runtime_state: bool, connection_id: Option<i64>) -> Value {
    let mut stats = PhpArray::new();
    let active_connections = i64::from(has_runtime_state && connection_id.is_some());
    for (name, value) in [
        ("bytes_sent", 0),
        ("bytes_received", 0),
        ("packets_sent", 0),
        ("packets_received", 0),
        ("protocol_overhead_in", 0),
        ("protocol_overhead_out", 0),
        ("connect_success", active_connections),
        ("connect_failure", 0),
        ("active_connections", active_connections),
        ("active_persistent_connections", 0),
        ("explicit_close", 0),
        ("implicit_close", 0),
        ("disconnect_close", 0),
        ("in_middle_of_command_close", 0),
        ("init_command_executed_count", 0),
        ("rows_fetched_from_server_normal", 0),
        ("rows_fetched_from_server_ps", 0),
        ("rows_buffered_from_client_normal", 0),
        ("rows_buffered_from_client_ps", 0),
        ("rows_fetched_from_client_normal_buffered", 0),
        ("rows_fetched_from_client_ps_buffered", 0),
        ("rows_fetched_from_client_normal_unbuffered", 0),
        ("rows_fetched_from_client_ps_unbuffered", 0),
        ("rows_fetched_from_client_ps_cursor", 0),
        ("rows_skipped_normal", 0),
        ("rows_skipped_ps", 0),
        ("copy_on_write_saved", 0),
        ("copy_on_write_performed", 0),
        ("command_buffer_too_small", 0),
    ] {
        stats.insert(
            ArrayKey::String(PhpString::from_bytes(name.as_bytes().to_vec())),
            Value::Int(value),
        );
    }
    Value::Array(stats)
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
    object.set_property(
        "client_info",
        Value::String(PhpString::from(MYSQLND_CLIENT_INFO)),
    );
    object.set_property("client_version", Value::Int(MYSQLND_CLIENT_VERSION));
    object
}

pub fn mysqli_result_object(result_id: i64) -> ObjectRef {
    let object =
        ObjectRef::new_with_display_name(&mysqli_runtime_class("mysqli_result"), "mysqli_result");
    object.set_property("__mysqli_result", Value::Int(result_id));
    object.set_property("num_rows", Value::Int(0));
    object
}

pub fn mysqli_stmt_object(statement_id: i64) -> ObjectRef {
    let object =
        ObjectRef::new_with_display_name(&mysqli_runtime_class("mysqli_stmt"), "mysqli_stmt");
    object.set_property("__mysqli_stmt", Value::Int(statement_id));
    object.set_property("affected_rows", Value::Int(0));
    object.set_property("insert_id", Value::Int(0));
    object.set_property("num_rows", Value::Int(0));
    object.set_property("errno", Value::Int(0));
    object.set_property("error", Value::String(PhpString::from("")));
    object.set_property("sqlstate", Value::String(PhpString::from("00000")));
    object
}

fn mysqli_runtime_class(name: &str) -> crate::ClassEntry {
    crate::ClassEntry {
        name: crate::normalize_class_name(name).into(),
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

pub fn mysqli_stmt_id(object: &ObjectRef) -> Option<i64> {
    match object.get_property("__mysqli_stmt") {
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

fn mysqli_stmt_object_arg(name: &str, value: Option<&Value>) -> Result<ObjectRef, BuiltinError> {
    match value {
        Some(Value::Object(object)) if object.class_name() == "mysqli_stmt" => Ok(object.clone()),
        Some(value) => Err(argument_type_error(name, "1", "mysqli_stmt", value)),
        None => Err(BuiltinError::new(
            "E_PHP_RUNTIME_BUILTIN_ARITY",
            format!("builtin {name} expects mysqli_stmt argument"),
        )),
    }
}

fn collect_reference_args(
    name: &str,
    values: &[Value],
) -> Result<Vec<ReferenceCell>, BuiltinError> {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| match value {
            Value::Reference(cell) => Ok(cell.clone()),
            other => Err(argument_type_error(
                name,
                &(index + 1).to_string(),
                "reference",
                other,
            )),
        })
        .collect()
}

fn set_stmt_refs(object: &ObjectRef, property: &str, refs: Vec<ReferenceCell>) {
    let mut array = PhpArray::new();
    for cell in refs {
        array.append(Value::Reference(cell));
    }
    object.set_property(property, Value::Array(array));
}

pub fn stmt_refs(object: &ObjectRef, property: &str) -> Vec<ReferenceCell> {
    let Some(Value::Array(array)) = object.get_property(property) else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|(_, value)| match value {
            Value::Reference(cell) => Some(cell.clone()),
            _ => None,
        })
        .collect()
}

fn stmt_int_status(
    context: &mut BuiltinContext<'_>,
    value: Option<&Value>,
    name: &str,
    f: impl FnOnce(&mut crate::MysqlState, i64) -> i64,
) -> BuiltinResult {
    let stmt = mysqli_stmt_object_arg(name, value)?;
    let Some(statement_id) = mysqli_stmt_id(&stmt) else {
        return Ok(Value::Int(-1));
    };
    Ok(Value::Int(context.mysql_state().map_or(-1, |state| {
        let value = f(state, statement_id);
        sync_mysqli_stmt_properties(&stmt, state);
        value
    })))
}

fn stmt_string_status(
    context: &mut BuiltinContext<'_>,
    value: Option<&Value>,
    name: &str,
    f: impl FnOnce(&mut crate::MysqlState, i64) -> String,
) -> BuiltinResult {
    let stmt = mysqli_stmt_object_arg(name, value)?;
    let Some(statement_id) = mysqli_stmt_id(&stmt) else {
        return Ok(Value::String(PhpString::from("")));
    };
    let text = context.mysql_state().map_or_else(String::new, |state| {
        let value = f(state, statement_id);
        sync_mysqli_stmt_properties(&stmt, state);
        value
    });
    Ok(Value::String(PhpString::from(text.into_bytes())))
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

pub fn sync_mysqli_stmt_properties(object: &ObjectRef, state: &crate::MysqlState) {
    if let Some(id) = mysqli_stmt_id(object) {
        object.set_property("affected_rows", Value::Int(state.stmt_affected_rows(id)));
        object.set_property("insert_id", Value::Int(state.stmt_insert_id(id)));
        object.set_property("num_rows", Value::Int(state.stmt_num_rows(id)));
        object.set_property("errno", Value::Int(state.stmt_errno(id)));
        object.set_property(
            "error",
            Value::String(PhpString::from(state.stmt_error(id).into_bytes())),
        );
        object.set_property(
            "sqlstate",
            Value::String(PhpString::from(state.stmt_sqlstate(id).into_bytes())),
        );
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
                Value::string("app"),
                Value::string("secret"),
                Value::string("app"),
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
    fn mysqli_connection_failure_records_db_network_payload() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let result = builtin_mysqli_connect(&mut context, Vec::new(), RuntimeSourceSpan::default())
            .expect("connect failure remains a PHP false return");

        assert_eq!(result, Value::Bool(false));
        let diagnostics = context.take_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].id(), "E_PHP_MYSQLI_CAPABILITY_DISABLED");
        let Some(RuntimeDiagnosticPayload::Bringup(payload)) = diagnostics[0].payload() else {
            panic!("expected db/network diagnostic payload");
        };
        assert_eq!(
            payload.fields().get("diagnostic_id").map(String::as_str),
            Some("E_PHP_MYSQLI_CAPABILITY_DISABLED")
        );
        assert_eq!(
            payload.fields().get("function_name").map(String::as_str),
            Some("mysqli_connect")
        );
        assert_eq!(
            payload
                .fields()
                .get("dsn_present_boolean")
                .map(String::as_str),
            Some("false")
        );
        assert!(payload.fields().contains_key("mysql_error_message"));
    }

    #[test]
    fn mysqli_report_error_changes_failure_diagnostic_severity() {
        let mut output = OutputBuffer::default();
        let mut mysql = crate::MysqlState::default();
        let mut context = BuiltinContext::new(&mut output);
        context.set_mysql_state(&mut mysql);

        assert_eq!(
            builtin_mysqli_report(
                &mut context,
                vec![Value::Int(MYSQLI_REPORT_ERROR)],
                RuntimeSourceSpan::default(),
            )
            .expect("report flags should be accepted"),
            Value::Bool(true)
        );
        let result = builtin_mysqli_connect(&mut context, Vec::new(), RuntimeSourceSpan::default())
            .expect("connect failure remains a PHP false return");

        assert_eq!(result, Value::Bool(false));
        let diagnostics = context.take_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity(), RuntimeSeverity::RecoverableError);
        let Some(RuntimeDiagnosticPayload::Bringup(payload)) = diagnostics[0].payload() else {
            panic!("expected db/network diagnostic payload");
        };
        assert_eq!(
            payload
                .fields()
                .get("mysqli_report_flags")
                .map(String::as_str),
            Some("1")
        );
    }

    #[test]
    fn mysqli_report_off_query_failure_updates_error_state_without_diagnostic() {
        let mut output = OutputBuffer::default();
        let mut mysql = crate::MysqlState::default();
        let id = mysql
            .connect_sqlite_compat()
            .expect("SQLite compatibility connection should open");
        let object = mysqli_object(Some(id));
        let mut context = BuiltinContext::new(&mut output);
        context.set_mysql_state(&mut mysql);

        let result = builtin_mysqli_query(
            &mut context,
            vec![
                Value::Object(object.clone()),
                Value::string("SELECT missing"),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("query failure remains a PHP false return");

        assert_eq!(result, Value::Bool(false));
        assert!(context.take_diagnostics().is_empty());
        assert!(matches!(object.get_property("errno"), Some(Value::Int(errno)) if errno > 0));
        assert!(
            matches!(object.get_property("error"), Some(Value::String(error)) if !error.is_empty())
        );
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

    #[test]
    fn mysqli_init_populates_client_properties() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let result = builtin_mysqli_init(&mut context, Vec::new(), RuntimeSourceSpan::default())
            .expect("mysqli_init should create a handle");
        let Value::Object(object) = result else {
            panic!("mysqli_init should return an object");
        };

        assert_eq!(
            object.get_property("client_info"),
            Some(Value::string(MYSQLND_CLIENT_INFO))
        );
        assert_eq!(
            object.get_property("client_version"),
            Some(Value::Int(MYSQLND_CLIENT_VERSION))
        );
    }

    #[test]
    fn mysqli_more_results_reports_no_pending_multi_result_set() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let result = builtin_mysqli_more_results(
            &mut context,
            vec![Value::Object(mysqli_object(None))],
            RuntimeSourceSpan::default(),
        )
        .expect("mysqli_more_results should be available for wpdb flush");

        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn mysqli_stats_functions_return_mysqlnd_shaped_arrays() {
        let mut output = OutputBuffer::default();
        let mut mysql = crate::MysqlState::default();
        let id = mysql
            .connect_sqlite_compat()
            .expect("SQLite compatibility connection should open");
        let object = mysqli_object(Some(id));
        let mut context = BuiltinContext::new(&mut output);
        context.set_mysql_state(&mut mysql);

        let client_stats =
            builtin_mysqli_get_client_stats(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("client stats should return an array");
        let connection_stats = builtin_mysqli_get_connection_stats(
            &mut context,
            vec![Value::Object(object)],
            RuntimeSourceSpan::default(),
        )
        .expect("connection stats should return an array");

        for value in [client_stats, connection_stats] {
            let Value::Array(stats) = value else {
                panic!("mysqli stats should return arrays");
            };
            assert!(
                stats
                    .get(&ArrayKey::String(PhpString::from("active_connections")))
                    .is_some()
            );
            assert!(
                stats
                    .get(&ArrayKey::String(PhpString::from("bytes_sent")))
                    .is_some()
            );
        }
    }

    #[test]
    fn mysqli_ping_and_transactions_use_runtime_backend() {
        let mut output = OutputBuffer::default();
        let mut mysql = crate::MysqlState::default();
        let id = mysql
            .connect_sqlite_compat()
            .expect("SQLite compatibility connection should open");
        let object = mysqli_object(Some(id));
        let mut context = BuiltinContext::new(&mut output);
        context.set_mysql_state(&mut mysql);

        assert_eq!(
            builtin_mysqli_ping(
                &mut context,
                vec![Value::Object(object.clone())],
                RuntimeSourceSpan::default(),
            )
            .expect("ping should run"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_mysqli_autocommit(
                &mut context,
                vec![Value::Object(object.clone()), Value::Bool(false)],
                RuntimeSourceSpan::default(),
            )
            .expect("autocommit should run"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_mysqli_begin_transaction(
                &mut context,
                vec![Value::Object(object.clone())],
                RuntimeSourceSpan::default(),
            )
            .expect("begin should run"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_mysqli_commit(
                &mut context,
                vec![Value::Object(object)],
                RuntimeSourceSpan::default(),
            )
            .expect("commit should run"),
            Value::Bool(true)
        );
    }

    #[test]
    fn mysqli_multi_query_store_and_next_result_use_buffered_results() {
        let mut output = OutputBuffer::default();
        let mut mysql = crate::MysqlState::default();
        let id = mysql
            .connect_sqlite_compat()
            .expect("SQLite compatibility connection should open");
        let object = mysqli_object(Some(id));
        let mut context = BuiltinContext::new(&mut output);
        context.set_mysql_state(&mut mysql);

        assert_eq!(
            builtin_mysqli_multi_query(
                &mut context,
                vec![
                    Value::Object(object.clone()),
                    Value::string("SELECT 1 AS one; SELECT 2 AS two"),
                ],
                RuntimeSourceSpan::default(),
            )
            .expect("multi query should run"),
            Value::Bool(true)
        );
        let first = builtin_mysqli_store_result(
            &mut context,
            vec![Value::Object(object.clone())],
            RuntimeSourceSpan::default(),
        )
        .expect("first result should be available");
        assert!(matches!(first, Value::Object(_)));
        assert_eq!(
            builtin_mysqli_more_results(
                &mut context,
                vec![Value::Object(object.clone())],
                RuntimeSourceSpan::default(),
            )
            .expect("more results should run"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_mysqli_next_result(
                &mut context,
                vec![Value::Object(object.clone())],
                RuntimeSourceSpan::default(),
            )
            .expect("next result should run"),
            Value::Bool(true)
        );
        let second = builtin_mysqli_use_result(
            &mut context,
            vec![Value::Object(object)],
            RuntimeSourceSpan::default(),
        )
        .expect("second result should be available");
        assert!(matches!(second, Value::Object(_)));
    }
}
