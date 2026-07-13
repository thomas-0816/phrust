//! Procedural PostgreSQL (`pgsql`) builtin slice.

use super::core::{argument_type_error, expect_arity, int_arg, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use crate::{
    ArrayKey, ClassEntry, ClassFlags, ObjectRef, PGSQL_ASSOC, PGSQL_BOTH, PGSQL_NUM,
    PostgresConnectOptions, PostgresError, PostgresField, Value, normalize_class_name,
};

const CONNECTION_CLASS: &str = "PgSql\\Connection";
const RESULT_CLASS: &str = "PgSql\\Result";
const CONNECTION_ID_PROPERTY: &str = "__pgsql_connection";
const RESULT_ID_PROPERTY: &str = "__pgsql_result";

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "pg_affected_rows",
        builtin_pg_affected_rows,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("pg_close", builtin_pg_close, BuiltinCompatibility::Php),
    BuiltinEntry::new("pg_connect", builtin_pg_connect, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "pg_escape_bytea",
        builtin_pg_escape_bytea,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pg_escape_identifier",
        builtin_pg_escape_identifier,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pg_escape_literal",
        builtin_pg_escape_literal,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pg_escape_string",
        builtin_pg_escape_string,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("pg_execute", builtin_pg_execute, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "pg_fetch_array",
        builtin_pg_fetch_array,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pg_fetch_assoc",
        builtin_pg_fetch_assoc,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pg_fetch_object",
        builtin_pg_fetch_object,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pg_fetch_result",
        builtin_pg_fetch_result,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pg_fetch_row",
        builtin_pg_fetch_row,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pg_free_result",
        builtin_pg_free_result,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pg_last_error",
        builtin_pg_last_error,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pg_num_fields",
        builtin_pg_num_fields,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pg_num_rows",
        builtin_pg_num_rows,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pg_pconnect",
        builtin_pg_pconnect,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("pg_prepare", builtin_pg_prepare, BuiltinCompatibility::Php),
    BuiltinEntry::new("pg_query", builtin_pg_query, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "pg_query_params",
        builtin_pg_query_params,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "pg_result_error",
        builtin_pg_result_error,
        BuiltinCompatibility::Php,
    ),
];

fn builtin_pg_connect(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    pg_connect_impl(context, "pg_connect", args)
}

fn builtin_pg_pconnect(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    pg_connect_impl(context, "pg_pconnect", args)
}

fn pg_connect_impl(
    context: &mut BuiltinContext<'_>,
    name: &str,
    args: Vec<Value>,
) -> BuiltinResult {
    expect_pgsql_arity(name, args.len(), 1, 2)?;
    let dsn = string_arg(name, &args[0])?.to_string_lossy();
    let options = match PostgresConnectOptions::from_dsn(dsn) {
        Ok(options) => options,
        Err(_) => return Ok(Value::Bool(false)),
    };
    let Some(state) = context.postgres_state() else {
        return Ok(Value::Bool(false));
    };
    match state.connect(&options) {
        Ok(id) => Ok(Value::Object(pgsql_connection_object(id))),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_pg_close(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_pgsql_arity("pg_close", args.len(), 0, 1)?;
    let Some(state) = context.postgres_state() else {
        return Ok(Value::Bool(false));
    };
    let Some(id) =
        connection_id_from_optional_arg("pg_close", state.default_connection(), args.first())?
    else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool(state.close(id)))
}

fn builtin_pg_query(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_pgsql_arity("pg_query", args.len(), 1, 2)?;
    let Some(state) = context.postgres_state() else {
        return Ok(Value::Bool(false));
    };
    let (connection_id, sql_arg) = if args.len() == 1 {
        let Some(connection_id) = state.default_connection() else {
            return Ok(Value::Bool(false));
        };
        (connection_id, &args[0])
    } else {
        (pgsql_connection_id_arg("pg_query", &args[0])?, &args[1])
    };
    let sql = string_arg("pg_query", sql_arg)?.to_string_lossy();
    let result = state.query(connection_id, &sql);
    pgsql_query_result(state, connection_id, result)
}

fn builtin_pg_query_params(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_pgsql_arity("pg_query_params", args.len(), 2, 3)?;
    let Some(state) = context.postgres_state() else {
        return Ok(Value::Bool(false));
    };
    let (connection_id, sql_arg, params_arg) = if args.len() == 2 {
        let Some(connection_id) = state.default_connection() else {
            return Ok(Value::Bool(false));
        };
        (connection_id, &args[0], &args[1])
    } else {
        (
            pgsql_connection_id_arg("pg_query_params", &args[0])?,
            &args[1],
            &args[2],
        )
    };
    let sql = string_arg("pg_query_params", sql_arg)?.to_string_lossy();
    let params = array_values("pg_query_params", params_arg)?;
    let result = state.query_params(connection_id, &sql, &params);
    pgsql_query_result(state, connection_id, result)
}

fn builtin_pg_prepare(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("pg_prepare", &args, 3)?;
    let connection_id = pgsql_connection_id_arg("pg_prepare", &args[0])?;
    let name = string_arg("pg_prepare", &args[1])?.to_string_lossy();
    let sql = string_arg("pg_prepare", &args[2])?.to_string_lossy();
    let Some(state) = context.postgres_state() else {
        return Ok(Value::Bool(false));
    };
    match state.prepare_named(connection_id, &name, &sql) {
        Ok(()) => Ok(Value::Object(pgsql_result_object(
            state.empty_result(connection_id, 0),
        ))),
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn builtin_pg_execute(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_pgsql_arity("pg_execute", args.len(), 2, 3)?;
    let Some(state) = context.postgres_state() else {
        return Ok(Value::Bool(false));
    };
    let (connection_id, name_arg, params_arg) = if args.len() == 2 {
        let Some(connection_id) = state.default_connection() else {
            return Ok(Value::Bool(false));
        };
        (connection_id, &args[0], &args[1])
    } else {
        (
            pgsql_connection_id_arg("pg_execute", &args[0])?,
            &args[1],
            &args[2],
        )
    };
    let name = string_arg("pg_execute", name_arg)?.to_string_lossy();
    let params = array_values("pg_execute", params_arg)?;
    let result = state.execute_named(connection_id, &name, &params);
    pgsql_query_result(state, connection_id, result)
}

fn builtin_pg_fetch_array(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_pgsql_arity("pg_fetch_array", args.len(), 1, 3)?;
    let row = optional_row_offset("pg_fetch_array", args.get(1))?;
    let mode = args
        .get(2)
        .map(|value| int_arg("pg_fetch_array", value))
        .transpose()?
        .unwrap_or(PGSQL_BOTH);
    fetch_array(context, "pg_fetch_array", args.first(), row, mode)
}

fn builtin_pg_fetch_assoc(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_pgsql_arity("pg_fetch_assoc", args.len(), 1, 2)?;
    let row = optional_row_offset("pg_fetch_assoc", args.get(1))?;
    fetch_array(context, "pg_fetch_assoc", args.first(), row, PGSQL_ASSOC)
}

fn builtin_pg_fetch_row(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_pgsql_arity("pg_fetch_row", args.len(), 1, 2)?;
    let row = optional_row_offset("pg_fetch_row", args.get(1))?;
    fetch_array(context, "pg_fetch_row", args.first(), row, PGSQL_NUM)
}

fn builtin_pg_fetch_object(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_pgsql_arity("pg_fetch_object", args.len(), 1, 4)?;
    let row = optional_row_offset("pg_fetch_object", args.get(1))?;
    let value = fetch_array(context, "pg_fetch_object", args.first(), row, PGSQL_ASSOC)?;
    let Value::Array(array) = value else {
        return Ok(Value::Bool(false));
    };
    let class_name = args
        .get(2)
        .map(|value| string_arg("pg_fetch_object", value).map(|value| value.to_string_lossy()))
        .transpose()?
        .unwrap_or_else(|| "stdClass".to_owned());
    let object = ObjectRef::new_with_display_name(
        &runtime_class(&class_name),
        class_name.trim_start_matches('\\'),
    );
    for (key, value) in array.iter() {
        if let ArrayKey::String(name) = key {
            object.set_property(name.to_string_lossy(), value.clone());
        }
    }
    Ok(Value::Object(object))
}

fn builtin_pg_fetch_result(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_pgsql_arity("pg_fetch_result", args.len(), 2, 3)?;
    let result_id = pgsql_result_id_arg("pg_fetch_result", &args[0])?;
    let (row, field_arg) = if args.len() == 2 {
        (0, &args[1])
    } else {
        (
            usize::try_from(int_arg("pg_fetch_result", &args[1])?).unwrap_or(usize::MAX),
            &args[2],
        )
    };
    let field = match field_arg {
        Value::String(value) => PostgresField::Name(value.to_string_lossy()),
        _ => PostgresField::Index(
            usize::try_from(int_arg("pg_fetch_result", field_arg)?).unwrap_or(usize::MAX),
        ),
    };
    Ok(context
        .postgres_state()
        .map_or(Value::Bool(false), |state| {
            state.result_value(result_id, row, field)
        }))
}

fn builtin_pg_num_rows(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("pg_num_rows", &args, 1)?;
    let result_id = pgsql_result_id_arg("pg_num_rows", &args[0])?;
    Ok(Value::Int(
        context
            .postgres_state()
            .map_or(0, |state| state.num_rows(result_id)),
    ))
}

fn builtin_pg_num_fields(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("pg_num_fields", &args, 1)?;
    let result_id = pgsql_result_id_arg("pg_num_fields", &args[0])?;
    Ok(Value::Int(
        context
            .postgres_state()
            .map_or(0, |state| state.num_fields(result_id)),
    ))
}

fn builtin_pg_affected_rows(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("pg_affected_rows", &args, 1)?;
    let result_id = pgsql_result_id_arg("pg_affected_rows", &args[0])?;
    Ok(Value::Int(
        context
            .postgres_state()
            .map_or(-1, |state| state.affected_result_rows(result_id)),
    ))
}

fn builtin_pg_free_result(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("pg_free_result", &args, 1)?;
    let result_id = pgsql_result_id_arg("pg_free_result", &args[0])?;
    Ok(Value::Bool(
        context
            .postgres_state()
            .is_some_and(|state| state.free_result(result_id)),
    ))
}

fn builtin_pg_last_error(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_pgsql_arity("pg_last_error", args.len(), 0, 1)?;
    let Some(state) = context.postgres_state() else {
        return Ok(Value::string("not an open PostgreSQL connection"));
    };
    let Some(id) =
        connection_id_from_optional_arg("pg_last_error", state.default_connection(), args.first())?
    else {
        return Ok(Value::string("not an open PostgreSQL connection"));
    };
    Ok(Value::string(state.error(id)))
}

fn builtin_pg_result_error(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("pg_result_error", &args, 1)?;
    let result_id = pgsql_result_id_arg("pg_result_error", &args[0])?;
    Ok(context
        .postgres_state()
        .and_then(|state| state.result_error(result_id))
        .map_or(Value::Bool(false), Value::string))
}

fn builtin_pg_escape_string(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let value = escape_input_arg("pg_escape_string", &args)?;
    Ok(Value::string(pg_escape_string_bytes(value.as_bytes())))
}

fn builtin_pg_escape_literal(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let value = escape_input_arg("pg_escape_literal", &args)?;
    Ok(Value::string(format!(
        "'{}'",
        pg_escape_string_bytes(value.as_bytes())
    )))
}

fn builtin_pg_escape_identifier(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let value = escape_input_arg("pg_escape_identifier", &args)?;
    Ok(Value::string(format!("\"{}\"", value.replace('"', "\"\""))))
}

fn builtin_pg_escape_bytea(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let value = escape_input_arg("pg_escape_bytea", &args)?;
    let mut out = String::from("\\\\x");
    for byte in value.as_bytes() {
        out.push(hex_digit(byte >> 4));
        out.push(hex_digit(byte & 0x0f));
    }
    Ok(Value::string(out))
}

fn pgsql_query_result(
    state: &mut crate::PostgresState,
    connection_id: i64,
    result: Result<Option<i64>, PostgresError>,
) -> BuiltinResult {
    match result {
        Ok(Some(result_id)) => Ok(Value::Object(pgsql_result_object(result_id))),
        Ok(None) => {
            let affected_rows = state.affected_rows(connection_id);
            Ok(Value::Object(pgsql_result_object(
                state.empty_result(connection_id, affected_rows),
            )))
        }
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn fetch_array(
    context: &mut BuiltinContext<'_>,
    name: &str,
    value: Option<&Value>,
    row: Option<usize>,
    mode: i64,
) -> BuiltinResult {
    let result_id = pgsql_result_id_arg(
        name,
        value.ok_or_else(|| {
            BuiltinError::new(
                "E_PHP_RUNTIME_BUILTIN_ARITY",
                format!("builtin {name} expects PgSql\\Result argument"),
            )
        })?,
    )?;
    Ok(context
        .postgres_state()
        .map_or(Value::Bool(false), |state| {
            state.fetch_array_at(result_id, row, mode)
        }))
}

fn array_values(name: &str, value: &Value) -> Result<Vec<Value>, BuiltinError> {
    let Value::Array(array) = value else {
        return Err(argument_type_error(name, "#3 ($params)", "array", value));
    };
    Ok(array.iter().map(|(_, value)| value.clone()).collect())
}

fn optional_row_offset(name: &str, value: Option<&Value>) -> Result<Option<usize>, BuiltinError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => Ok(Some(
            usize::try_from(int_arg(name, value)?).unwrap_or(usize::MAX),
        )),
    }
}

fn connection_id_from_optional_arg(
    name: &str,
    default: Option<i64>,
    value: Option<&Value>,
) -> Result<Option<i64>, BuiltinError> {
    match value {
        None | Some(Value::Null) => Ok(default),
        Some(value) => pgsql_connection_id_arg(name, value).map(Some),
    }
}

fn pgsql_connection_id_arg(name: &str, value: &Value) -> Result<i64, BuiltinError> {
    let Value::Object(object) = value else {
        return Err(argument_type_error(
            name,
            "#1 ($connection)",
            CONNECTION_CLASS,
            value,
        ));
    };
    if normalize_class_name(&object.class_name()) != "pgsql\\connection" {
        return Err(argument_type_error(
            name,
            "#1 ($connection)",
            CONNECTION_CLASS,
            value,
        ));
    }
    match object.get_property(CONNECTION_ID_PROPERTY) {
        Some(Value::Int(id)) if id > 0 => Ok(id),
        _ => Err(BuiltinError::new(
            "E_PHP_RUNTIME_PGSQL_INVALID_CONNECTION",
            format!("{name}(): PgSql\\Connection object is no longer valid"),
        )),
    }
}

fn pgsql_result_id_arg(name: &str, value: &Value) -> Result<i64, BuiltinError> {
    let Value::Object(object) = value else {
        return Err(argument_type_error(
            name,
            "#1 ($result)",
            RESULT_CLASS,
            value,
        ));
    };
    if normalize_class_name(&object.class_name()) != "pgsql\\result" {
        return Err(argument_type_error(
            name,
            "#1 ($result)",
            RESULT_CLASS,
            value,
        ));
    }
    match object.get_property(RESULT_ID_PROPERTY) {
        Some(Value::Int(id)) if id > 0 => Ok(id),
        _ => Err(BuiltinError::new(
            "E_PHP_RUNTIME_PGSQL_INVALID_RESULT",
            format!("{name}(): PgSql\\Result object is no longer valid"),
        )),
    }
}

fn escape_input_arg(name: &str, args: &[Value]) -> Result<String, BuiltinError> {
    expect_pgsql_arity(name, args.len(), 1, 2)?;
    let value = if args.len() == 1 {
        &args[0]
    } else {
        pgsql_connection_id_arg(name, &args[0])?;
        &args[1]
    };
    Ok(string_arg(name, value)?.to_string_lossy())
}

fn pgsql_connection_object(connection_id: i64) -> ObjectRef {
    let object =
        ObjectRef::new_with_display_name(&runtime_class(CONNECTION_CLASS), CONNECTION_CLASS);
    object.set_property(CONNECTION_ID_PROPERTY, Value::Int(connection_id));
    object
}

fn pgsql_result_object(result_id: i64) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&runtime_class(RESULT_CLASS), RESULT_CLASS);
    object.set_property(RESULT_ID_PROPERTY, Value::Int(result_id));
    object
}

fn runtime_class(name: &str) -> ClassEntry {
    ClassEntry {
        name: normalize_class_name(name).into(),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: ClassFlags::default(),
    }
}

fn pg_escape_string_bytes(value: &[u8]) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value {
        match byte {
            b'\'' => out.push_str("''"),
            b'\\' => out.push_str("\\\\"),
            _ => out.push(char::from(*byte)),
        }
    }
    out
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => char::from(b'0' + value),
        10..=15 => char::from(b'a' + value - 10),
        _ => '0',
    }
}

fn expect_pgsql_arity(
    name: &str,
    actual: usize,
    min: usize,
    max: usize,
) -> Result<(), BuiltinError> {
    if (min..=max).contains(&actual) {
        return Ok(());
    }
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_BUILTIN_ARITY",
        format!("builtin {name} expects between {min} and {max} arguments, got {actual}"),
    ))
}
