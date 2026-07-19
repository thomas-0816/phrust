//! Deterministic IMAP facade with explicit no-backend behavior.

use super::core::{argument_type_error, arity_error, int_arg, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult, ImapConnectionConfig,
    ImapMailboxSnapshot,
};
use crate::{
    ArrayKey, BuiltinError, ClassEntry, ClassFlags, ObjectRef, PhpArray, PhpString,
    RuntimeSourceSpan, Value, normalize_class_name,
};

const IMAP_CONNECTION_CLASS: &str = "IMAP\\Connection";
const IMAP_CONNECTION_ID_PROPERTY: &str = "__phrust_imap_connection_id";
const IMAP_BACKEND_ERROR: &str = "IMAP backend is not configured";

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "imap_8bit",
        builtin_imap_identity,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_alerts",
        builtin_imap_alerts,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_append",
        builtin_imap_append,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_base64",
        builtin_imap_identity,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_binary",
        builtin_imap_identity,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("imap_check", builtin_imap_check, BuiltinCompatibility::Php),
    BuiltinEntry::new("imap_close", builtin_imap_close, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "imap_delete",
        builtin_imap_delete,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_errors",
        builtin_imap_errors,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_expunge",
        builtin_imap_expunge,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_fetch_overview",
        builtin_imap_fetch_overview,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_fetchbody",
        builtin_imap_fetchbody,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_fetchheader",
        builtin_imap_fetchheader,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_fetchstructure",
        builtin_imap_fetchstructure,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("imap_gc", builtin_imap_gc, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "imap_headerinfo",
        builtin_imap_headerinfo,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_headers",
        builtin_imap_headers,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_last_error",
        builtin_imap_last_error,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("imap_list", builtin_imap_list, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "imap_listscan",
        builtin_imap_list,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_mailboxmsginfo",
        builtin_imap_mailboxmsginfo,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_mail_copy",
        builtin_imap_mail_copy,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_mail_move",
        builtin_imap_mail_copy,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_num_msg",
        builtin_imap_num_msg,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_num_recent",
        builtin_imap_num_recent,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("imap_open", builtin_imap_open, BuiltinCompatibility::Php),
    BuiltinEntry::new("imap_ping", builtin_imap_ping, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "imap_qprint",
        builtin_imap_identity,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_reopen",
        builtin_imap_reopen,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_search",
        builtin_imap_search,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("imap_sort", builtin_imap_search, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "imap_status",
        builtin_imap_status,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_undelete",
        builtin_imap_undelete,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_utf8",
        builtin_imap_identity,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_utf7_decode",
        builtin_imap_identity,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "imap_utf7_encode",
        builtin_imap_identity,
        BuiltinCompatibility::Php,
    ),
];

fn builtin_imap_open(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=6).contains(&args.len()) {
        return Err(arity_error("imap_open", "three to six arguments"));
    }
    let mailbox = string_arg("imap_open", &args[0])?
        .to_string_lossy()
        .to_owned();
    let user = string_arg("imap_open", &args[1])?
        .to_string_lossy()
        .to_owned();
    let password = string_arg("imap_open", &args[2])?
        .to_string_lossy()
        .to_owned();
    let flags = optional_int("imap_open", args.get(3), 0)?;
    if let Some(value) = args.get(4) {
        let _ = int_arg("imap_open", value)?;
    }
    if let Some(value) = args.get(5)
        && !matches!(value, Value::Array(_))
    {
        return Err(argument_type_error(
            "imap_open",
            "#6 ($options)",
            "array",
            value,
        ));
    }
    let id = context.imap_state().open(mailbox, flags);
    if let Some(config) =
        parse_mailbox_connection(&context.imap_state().mailbox(id).unwrap_or_default())
        && live_imap_endpoint_enabled(context, &config)
        && !context
            .imap_state()
            .open_backend(id, &config, &user, &password)
    {
        context.imap_state().close(id);
        return Ok(Value::Bool(false));
    }
    Ok(Value::Object(imap_connection_object(id)))
}

fn builtin_imap_close(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error("imap_close", "one or two arguments"));
    }
    let id = imap_connection_id_arg("imap_close", &args[0])?;
    if let Some(value) = args.get(1) {
        let _ = int_arg("imap_close", value)?;
    }
    Ok(Value::Bool(context.imap_state().close(id)))
}

fn builtin_imap_ping(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let id = single_connection_arg("imap_ping", &args)?;
    Ok(Value::Bool(context.imap_state().is_open(id)))
}

fn builtin_imap_reopen(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(arity_error("imap_reopen", "two to four arguments"));
    }
    let id = imap_connection_id_arg("imap_reopen", &args[0])?;
    let mailbox = string_arg("imap_reopen", &args[1])?
        .to_string_lossy()
        .to_owned();
    let flags = optional_int("imap_reopen", args.get(2), 0)?;
    if let Some(value) = args.get(3) {
        let _ = int_arg("imap_reopen", value)?;
    }
    if !context.imap_state().is_open(id) {
        return Ok(Value::Bool(false));
    }
    context.imap_state().close(id);
    let new_id = context.imap_state().open(mailbox, flags);
    if let Value::Object(object) = &args[0] {
        object.set_property(IMAP_CONNECTION_ID_PROPERTY, Value::Int(new_id));
    }
    Ok(Value::Bool(true))
}

fn builtin_imap_headers(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let id = single_connection_arg("imap_headers", &args)?;
    if !context.imap_state().is_open(id) {
        return Ok(Value::Bool(false));
    }
    if context.imap_state().has_backend(id) {
        return Ok(context
            .imap_state()
            .backend_headers(id)
            .map(|headers| {
                Value::Array(PhpArray::from_packed(
                    headers.into_iter().map(Value::string).collect(),
                ))
            })
            .unwrap_or(Value::Bool(false)));
    }
    Ok(Value::Array(PhpArray::new()))
}

fn builtin_imap_fetch_overview(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("imap_fetch_overview", "two or three arguments"));
    }
    let id = imap_connection_id_arg("imap_fetch_overview", &args[0])?;
    let _sequence = string_arg("imap_fetch_overview", &args[1])?;
    optional_int("imap_fetch_overview", args.get(2), 0)?;
    if !context.imap_state().is_open(id) {
        return Ok(Value::Bool(false));
    }
    Ok(Value::Array(PhpArray::new()))
}

fn builtin_imap_fetchbody(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=4).contains(&args.len()) {
        return Err(arity_error("imap_fetchbody", "three or four arguments"));
    }
    let id = imap_connection_id_arg("imap_fetchbody", &args[0])?;
    let message = int_arg("imap_fetchbody", &args[1])?;
    let _section = string_arg("imap_fetchbody", &args[2])?;
    optional_int("imap_fetchbody", args.get(3), 0)?;
    if !context.imap_state().is_open(id) || message <= 0 {
        return Ok(Value::Bool(false));
    }
    if context.imap_state().has_backend(id) {
        return Ok(context
            .imap_state()
            .backend_fetch_body(id, message)
            .map(|bytes| Value::String(PhpString::from_bytes(bytes)))
            .unwrap_or(Value::Bool(false)));
    }
    Ok(Value::string(""))
}

fn builtin_imap_fetchheader(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("imap_fetchheader", "two or three arguments"));
    }
    let id = imap_connection_id_arg("imap_fetchheader", &args[0])?;
    let message = int_arg("imap_fetchheader", &args[1])?;
    optional_int("imap_fetchheader", args.get(2), 0)?;
    if !context.imap_state().is_open(id) || message <= 0 {
        return Ok(Value::Bool(false));
    }
    if context.imap_state().has_backend(id) {
        return Ok(context
            .imap_state()
            .backend_fetch_header(id, message)
            .map(|bytes| Value::String(PhpString::from_bytes(bytes)))
            .unwrap_or(Value::Bool(false)));
    }
    Ok(Value::string(""))
}

fn builtin_imap_fetchstructure(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("imap_fetchstructure", "two or three arguments"));
    }
    let id = imap_connection_id_arg("imap_fetchstructure", &args[0])?;
    let message = int_arg("imap_fetchstructure", &args[1])?;
    optional_int("imap_fetchstructure", args.get(2), 0)?;
    if !context.imap_state().is_open(id) || message <= 0 {
        return Ok(Value::Bool(false));
    }
    Ok(Value::Object(std_object(&[("type", Value::Int(0))])))
}

fn builtin_imap_headerinfo(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=5).contains(&args.len()) {
        return Err(arity_error("imap_headerinfo", "two to five arguments"));
    }
    let id = imap_connection_id_arg("imap_headerinfo", &args[0])?;
    let message = int_arg("imap_headerinfo", &args[1])?;
    for value in args.iter().skip(2) {
        let _ = int_arg("imap_headerinfo", value)?;
    }
    if !context.imap_state().is_open(id) || message <= 0 {
        return Ok(Value::Bool(false));
    }
    Ok(Value::Object(std_object(&[
        ("subject", Value::string("")),
        ("fromaddress", Value::string("")),
        ("date", Value::string("")),
        ("Msgno", Value::Int(message)),
    ])))
}

fn builtin_imap_search(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(arity_error("imap_search", "two to four arguments"));
    }
    let id = imap_connection_id_arg("imap_search", &args[0])?;
    let criteria = string_arg("imap_search", &args[1])?
        .to_string_lossy()
        .to_owned();
    for value in args.iter().skip(2) {
        let _ = int_arg("imap_search", value)?;
    }
    if !context.imap_state().is_open(id) {
        return Ok(Value::Bool(false));
    }
    if context.imap_state().has_backend(id) {
        let Some(matches) = context.imap_state().backend_search(id, &criteria) else {
            return Ok(Value::Bool(false));
        };
        if matches.is_empty() {
            return Ok(Value::Bool(false));
        }
        return Ok(Value::Array(PhpArray::from_packed(
            matches.into_iter().map(Value::Int).collect(),
        )));
    }
    Ok(Value::Bool(false))
}

fn builtin_imap_list(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("imap_list", "exactly three arguments"));
    }
    let id = imap_connection_id_arg("imap_list", &args[0])?;
    let _reference = string_arg("imap_list", &args[1])?;
    let _pattern = string_arg("imap_list", &args[2])?;
    if !context.imap_state().is_open(id) {
        return Ok(Value::Bool(false));
    }
    Ok(Value::Array(PhpArray::new()))
}

fn builtin_imap_status(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("imap_status", "exactly three arguments"));
    }
    let id = imap_connection_id_arg("imap_status", &args[0])?;
    let mailbox = string_arg("imap_status", &args[1])?
        .to_string_lossy()
        .to_owned();
    let _flags = int_arg("imap_status", &args[2])?;
    if !context.imap_state().is_open(id) {
        return Ok(Value::Bool(false));
    }
    if let Some(snapshot) = context.imap_state().backend_mailbox(id) {
        return Ok(Value::Object(status_object_from_snapshot(
            mailbox, &snapshot,
        )));
    }
    Ok(Value::Object(status_object(mailbox)))
}

fn builtin_imap_check(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let id = single_connection_arg("imap_check", &args)?;
    let state = context.imap_state();
    if let Some(snapshot) = state.backend_mailbox(id) {
        let mailbox = state.mailbox(id).unwrap_or_default();
        return Ok(Value::Object(check_object(mailbox, &snapshot)));
    }
    let Some(mailbox) = state.mailbox(id) else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Object(std_object(&[
        ("Date", Value::string("")),
        ("Driver", Value::string("phrust-imap")),
        ("Mailbox", Value::string(mailbox)),
        ("Nmsgs", Value::Int(0)),
        ("Recent", Value::Int(0)),
    ])))
}

fn builtin_imap_mailboxmsginfo(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let id = single_connection_arg("imap_mailboxmsginfo", &args)?;
    let state = context.imap_state();
    if let Some(snapshot) = state.backend_mailbox(id) {
        let mailbox = state.mailbox(id).unwrap_or_default();
        return Ok(Value::Object(mailbox_info_object(mailbox, &snapshot)));
    }
    let Some(mailbox) = state.mailbox(id) else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Object(std_object(&[
        ("Date", Value::string("")),
        ("Driver", Value::string("phrust-imap")),
        ("Mailbox", Value::string(mailbox)),
        ("Nmsgs", Value::Int(0)),
        ("Recent", Value::Int(0)),
        ("Unread", Value::Int(0)),
        ("Deleted", Value::Int(0)),
        ("Size", Value::Int(0)),
    ])))
}

fn builtin_imap_num_msg(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let id = single_connection_arg("imap_num_msg", &args)?;
    if !context.imap_state().is_open(id) {
        return Ok(Value::Bool(false));
    }
    if let Some(snapshot) = context.imap_state().backend_mailbox(id) {
        return Ok(Value::Int(snapshot.exists));
    }
    Ok(Value::Int(0))
}

fn builtin_imap_num_recent(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let id = single_connection_arg("imap_num_recent", &args)?;
    if !context.imap_state().is_open(id) {
        return Ok(Value::Bool(false));
    }
    if let Some(snapshot) = context.imap_state().backend_mailbox(id) {
        return Ok(Value::Int(snapshot.recent));
    }
    Ok(Value::Int(0))
}

fn builtin_imap_delete(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("imap_delete", "two or three arguments"));
    }
    let id = imap_connection_id_arg("imap_delete", &args[0])?;
    let message = int_arg("imap_delete", &args[1])?;
    optional_int("imap_delete", args.get(2), 0)?;
    Ok(Value::Bool(context.imap_state().mark_deleted(id, message)))
}

fn builtin_imap_undelete(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("imap_undelete", "two or three arguments"));
    }
    let id = imap_connection_id_arg("imap_undelete", &args[0])?;
    let _message = int_arg("imap_undelete", &args[1])?;
    optional_int("imap_undelete", args.get(2), 0)?;
    Ok(Value::Bool(context.imap_state().is_open(id)))
}

fn builtin_imap_expunge(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let id = single_connection_arg("imap_expunge", &args)?;
    Ok(Value::Bool(context.imap_state().expunge(id)))
}

fn builtin_imap_append(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=6).contains(&args.len()) {
        return Err(arity_error("imap_append", "three to six arguments"));
    }
    let id = imap_connection_id_arg("imap_append", &args[0])?;
    for (index, value) in args.iter().enumerate().skip(1) {
        if index == 5 {
            if !matches!(value, Value::Null | Value::String(_)) {
                return Err(argument_type_error(
                    "imap_append",
                    "#6 ($options)",
                    "string|null",
                    value,
                ));
            }
        } else {
            let _ = string_arg("imap_append", value)?;
        }
    }
    context.imap_state().push_error(IMAP_BACKEND_ERROR);
    let _ = id;
    Ok(Value::Bool(false))
}

fn builtin_imap_mail_copy(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(3..=4).contains(&args.len()) {
        return Err(arity_error("imap_mail_copy", "three or four arguments"));
    }
    let id = imap_connection_id_arg("imap_mail_copy", &args[0])?;
    let _sequence = string_arg("imap_mail_copy", &args[1])?;
    let _mailbox = string_arg("imap_mail_copy", &args[2])?;
    optional_int("imap_mail_copy", args.get(3), 0)?;
    if !context.imap_state().is_open(id) {
        return Ok(Value::Bool(false));
    }
    context.imap_state().push_error(IMAP_BACKEND_ERROR);
    Ok(Value::Bool(false))
}

fn builtin_imap_gc(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("imap_gc", "exactly two arguments"));
    }
    let id = imap_connection_id_arg("imap_gc", &args[0])?;
    let _flags = int_arg("imap_gc", &args[1])?;
    Ok(Value::Bool(context.imap_state().is_open(id)))
}

fn builtin_imap_errors(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !args.is_empty() {
        return Err(arity_error("imap_errors", "no arguments"));
    }
    let errors = context.imap_state().take_errors();
    if errors.is_empty() {
        return Ok(Value::Bool(false));
    }
    let mut array = PhpArray::new();
    for (index, error) in errors.into_iter().enumerate() {
        array.insert(ArrayKey::Int(index as i64), Value::string(error));
    }
    Ok(Value::Array(array))
}

fn builtin_imap_last_error(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !args.is_empty() {
        return Err(arity_error("imap_last_error", "no arguments"));
    }
    Ok(context
        .imap_state()
        .last_error()
        .map(Value::string)
        .unwrap_or(Value::Bool(false)))
}

fn builtin_imap_alerts(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !args.is_empty() {
        return Err(arity_error("imap_alerts", "no arguments"));
    }
    let alerts = context.imap_state().take_alerts();
    if alerts.is_empty() {
        return Ok(Value::Bool(false));
    }
    let mut array = PhpArray::new();
    for (index, alert) in alerts.into_iter().enumerate() {
        array.insert(ArrayKey::Int(index as i64), Value::string(alert));
    }
    Ok(Value::Array(array))
}

fn builtin_imap_identity(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("imap string helper", "exactly one argument"));
    }
    Ok(Value::String(string_arg("imap string helper", &args[0])?))
}

fn single_connection_arg(function: &'static str, args: &[Value]) -> Result<i64, BuiltinError> {
    if args.len() != 1 {
        return Err(arity_error(function, "exactly one argument"));
    }
    imap_connection_id_arg(function, &args[0])
}

fn optional_int(
    function: &'static str,
    value: Option<&Value>,
    default: i64,
) -> Result<i64, BuiltinError> {
    match value {
        Some(value) => int_arg(function, value),
        None => Ok(default),
    }
}

fn imap_connection_id_arg(function: &'static str, value: &Value) -> Result<i64, BuiltinError> {
    let Value::Object(object) = value else {
        return Err(argument_type_error(
            function,
            "#1",
            IMAP_CONNECTION_CLASS,
            value,
        ));
    };
    match object.get_property(IMAP_CONNECTION_ID_PROPERTY) {
        Some(Value::Int(id)) => Ok(id),
        _ => Err(argument_type_error(
            function,
            "#1",
            IMAP_CONNECTION_CLASS,
            value,
        )),
    }
}

fn parse_mailbox_connection(mailbox: &str) -> Option<ImapConnectionConfig> {
    let rest = mailbox.strip_prefix('{')?;
    let (server, mailbox_name) = rest.split_once('}')?;
    let mut parts = server.split('/');
    let authority = parts.next()?.trim();
    if authority.is_empty() {
        return None;
    }
    let flags = parts
        .map(|part| part.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let ssl = flags.iter().any(|flag| flag == "ssl" || flag == "tls");
    let novalidate_cert = flags.iter().any(|flag| flag == "novalidate-cert");
    let (host, port) = authority
        .rsplit_once(':')
        .and_then(|(host, port)| port.parse::<u16>().ok().map(|port| (host, port)))
        .map_or_else(
            || (authority.to_owned(), if ssl { 993 } else { 143 }),
            |(host, port)| (host.to_owned(), port),
        );
    Some(ImapConnectionConfig {
        host,
        port,
        ssl,
        novalidate_cert,
        mailbox: if mailbox_name.is_empty() {
            "INBOX".to_owned()
        } else {
            mailbox_name.to_owned()
        },
    })
}

fn live_imap_endpoint_enabled(context: &BuiltinContext<'_>, config: &ImapConnectionConfig) -> bool {
    context.network_requests_enabled()
        && context
            .env_value("PHRUST_IMAP_LIVE_ENDPOINT")
            .is_some_and(|endpoint| endpoint == format!("{}:{}", config.host, config.port))
}

fn check_object(mailbox: String, snapshot: &ImapMailboxSnapshot) -> ObjectRef {
    std_object(&[
        ("Date", Value::string("")),
        ("Driver", Value::string("phrust-imap")),
        ("Mailbox", Value::string(mailbox)),
        ("Nmsgs", Value::Int(snapshot.exists)),
        ("Recent", Value::Int(snapshot.recent)),
    ])
}

fn mailbox_info_object(mailbox: String, snapshot: &ImapMailboxSnapshot) -> ObjectRef {
    std_object(&[
        ("Date", Value::string("")),
        ("Driver", Value::string("phrust-imap")),
        ("Mailbox", Value::string(mailbox)),
        ("Nmsgs", Value::Int(snapshot.exists)),
        ("Recent", Value::Int(snapshot.recent)),
        ("Unread", Value::Int(snapshot.unseen)),
        ("Deleted", Value::Int(0)),
        ("Size", Value::Int(0)),
    ])
}

fn status_object(mailbox: String) -> ObjectRef {
    std_object(&[
        ("flags", Value::Int(31)),
        ("messages", Value::Int(0)),
        ("recent", Value::Int(0)),
        ("unseen", Value::Int(0)),
        ("uidnext", Value::Int(1)),
        ("uidvalidity", Value::Int(1)),
        ("mailbox", Value::string(mailbox)),
    ])
}

fn status_object_from_snapshot(mailbox: String, snapshot: &ImapMailboxSnapshot) -> ObjectRef {
    std_object(&[
        ("flags", Value::Int(31)),
        ("messages", Value::Int(snapshot.exists)),
        ("recent", Value::Int(snapshot.recent)),
        ("unseen", Value::Int(snapshot.unseen)),
        ("uidnext", Value::Int(snapshot.uid_next)),
        ("uidvalidity", Value::Int(snapshot.uid_validity)),
        ("mailbox", Value::string(mailbox)),
    ])
}

fn std_object(properties: &[(&str, Value)]) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(&runtime_class("stdclass"), "stdClass");
    for (name, value) in properties {
        object.set_property(*name, value.clone());
    }
    object
}

fn imap_connection_object(id: i64) -> ObjectRef {
    let object =
        ObjectRef::new_with_display_name(&runtime_class("imap\\connection"), IMAP_CONNECTION_CLASS);
    object.set_property(IMAP_CONNECTION_ID_PROPERTY, Value::Int(id));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputBuffer;

    #[test]
    fn imap_facade_opens_empty_mailbox_and_tracks_errors() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let connection = builtin_imap_open(
            &mut context,
            vec![
                Value::string("{127.0.0.1:143/imap}INBOX"),
                Value::string("user"),
                Value::string("secret"),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("open succeeds");
        assert!(matches!(connection, Value::Object(_)));
        assert_eq!(
            builtin_imap_num_msg(
                &mut context,
                vec![connection.clone()],
                RuntimeSourceSpan::default()
            )
            .expect("num msg"),
            Value::Int(0)
        );
        assert_eq!(
            builtin_imap_search(
                &mut context,
                vec![connection.clone(), Value::string("ALL")],
                RuntimeSourceSpan::default()
            )
            .expect("search"),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_imap_append(
                &mut context,
                vec![
                    connection.clone(),
                    Value::string("{127.0.0.1}INBOX"),
                    Value::string("Subject: test\r\n\r\nbody"),
                ],
                RuntimeSourceSpan::default()
            )
            .expect("append"),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_imap_last_error(&mut context, Vec::new(), RuntimeSourceSpan::default())
                .expect("last error"),
            Value::string(IMAP_BACKEND_ERROR)
        );
    }

    #[test]
    fn imap_delete_and_expunge_track_request_local_state() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);
        let connection = builtin_imap_open(
            &mut context,
            vec![
                Value::string("INBOX"),
                Value::string("u"),
                Value::string("p"),
            ],
            RuntimeSourceSpan::default(),
        )
        .expect("open succeeds");
        assert_eq!(
            builtin_imap_delete(
                &mut context,
                vec![connection.clone(), Value::Int(1)],
                RuntimeSourceSpan::default(),
            )
            .expect("delete"),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_imap_expunge(
                &mut context,
                vec![connection.clone()],
                RuntimeSourceSpan::default(),
            )
            .expect("expunge"),
            Value::Bool(true)
        );
    }

    #[test]
    fn imap_mailbox_parser_extracts_endpoint_tls_flags_and_name() {
        assert_eq!(
            parse_mailbox_connection("{mail.example.test:993/imap/ssl/novalidate-cert}Archive"),
            Some(ImapConnectionConfig {
                host: "mail.example.test".to_owned(),
                port: 993,
                ssl: true,
                novalidate_cert: true,
                mailbox: "Archive".to_owned(),
            })
        );
        assert_eq!(
            parse_mailbox_connection("{127.0.0.1/imap}INBOX"),
            Some(ImapConnectionConfig {
                host: "127.0.0.1".to_owned(),
                port: 143,
                ssl: false,
                novalidate_cert: false,
                mailbox: "INBOX".to_owned(),
            })
        );
        assert_eq!(parse_mailbox_connection("INBOX"), None);
    }
}
